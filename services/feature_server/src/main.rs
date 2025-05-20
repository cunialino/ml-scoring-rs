use std::time::Duration;

use actix_web::{web, App, HttpResponse, HttpServer};
use actix_web_prometheus::PrometheusMetricsBuilder;
use rocksdb::DB;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn, Level};
use xgb_rs::{booster::Booster, dmatrix::DMatrix};

struct AppState {
    booster: Booster,
    number_of_features: usize,
    rocksdb: std::sync::Arc<DB>,
}

#[derive(Deserialize, Serialize)]
struct UpdateRequest {
    path: String,
}

#[derive(Serialize, Deserialize)]
struct ScoringRequest {
    #[serde(rename = "f1")]
    id: String,
    #[serde(rename = "f2")]
    num_var: f32,
}
#[derive(Serialize, Deserialize)]
struct ScoringResponse {
    score: f32,
}

#[derive(Serialize, Deserialize)]
struct IdFeatures {
    features: std::vec::Vec<f32>,
}

#[derive(Serialize, Deserialize)]
struct CreateRequest {
    id: String,
    features: std::vec::Vec<f32>,
}

async fn score(req: web::Json<ScoringRequest>, app_data: web::Data<AppState>) -> HttpResponse {
    debug!("Feature Request {}", req.id.as_str());
    let start = std::time::Instant::now();

    let rb = app_data.rocksdb.clone();
    let id = req.id.clone();

    let ff = web::block(move || rb.get(id)).await;

    let rocksdb_res = match ff {
        Ok(val) => val,
        Err(e) => return HttpResponse::InternalServerError().body(format!("Error awaiting {}", e)),
    };

    let feats: IdFeatures = match rocksdb_res {
        Ok(Some(value)) => match serde_json::from_slice(&value) {
            Ok(feats) => feats,
            Err(e) => {
                warn!("Cannot deserialize file {}", e);
                return HttpResponse::NotAcceptable().body("Cannot deserialize feats");
            }
        },
        Err(e) => return HttpResponse::InternalServerError().body(format!("DB error: {}", e)),
        Ok(None) => {
            warn!("Cannot find feature {}", req.id);
            return HttpResponse::NotFound().body("Feature not found");
        }
    };

    let features = feats.features;
    let booster = &app_data.booster;
    if features.len() != app_data.number_of_features {
        return HttpResponse::BadRequest().body(format!(
            "Id {} does not have right amount of features",
            req.id.as_str()
        ));
    }
    let dmat = DMatrix::try_from_data(features.as_ref(), 1, features.len() as u64)
        .expect("Cannot create dmatrix");
    let predict = booster.predict(&dmat);
    let vec = predict.expect("Cannot compute score");
    let score = vec.first().unwrap();
    debug!(
        "Computed score {} for id {} with features {}",
        score,
        req.id.as_str(),
        serde_json::to_string(&features).unwrap()
    );
    let duration = start.elapsed().as_millis();
    info!(duration = duration, response_code = 200);
    HttpResponse::Ok().json(ScoringResponse { score: *score })
}

async fn get_health_check() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("application/json")
        .body("Heathly!")
}

async fn update_db(app_data: web::Data<AppState>) -> HttpResponse {
    let rocksdb = app_data.rocksdb.clone();
    let res = rocksdb.try_catch_up_with_primary();
    match res {
        Ok(_) => HttpResponse::Ok().body("Db Updated"),
        Err(e) => HttpResponse::InternalServerError().body(format!("Could not update db: {}", e)),
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let host = std::env::var("FEATURES_HOST").unwrap_or("127.0.0.1".to_string());
    let port = std::env::var("FEATURES_PORT").unwrap_or("8080".to_string());
    let num_workers: usize = std::env::var("FEATURES_WORKERS")
        .unwrap_or("2".to_string())
        .parse()
        .expect("Cannot converto num workers to usize");
    if std::env::var("ECS_TASK").is_ok() {
        tracing_subscriber::fmt()
            .json()
            .with_current_span(false)
            .with_ansi(false)
            .with_max_level(Level::DEBUG)
            .with_target(false)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_max_level(Level::DEBUG)
            .with_target(false)
            .init();
    }

    let prom = PrometheusMetricsBuilder::new("api")
        .endpoint("/metrics")
        .build()
        .unwrap();
    // Optimize for read-heavy workload on network storage
    let mut opts = rocksdb::Options::default();
    opts.set_max_background_jobs(2); // Reduce background I/O
    opts.set_max_subcompactions(1);
    opts.set_disable_auto_compactions(true); // Critical for read-only instances
    opts.set_use_direct_reads(false); // Better with EFS
    opts.set_use_direct_io_for_flush_and_compaction(false);

    // Increase cache sizes
    let cache = rocksdb::Cache::new_lru_cache(512 * 1024 * 1024); // 512MB
    opts.set_row_cache(&cache);
    let mut block_opts = rocksdb::BlockBasedOptions::default();
    block_opts.set_block_cache(&cache);
    opts.set_block_based_table_factory(&block_opts);

    let rocksdb_handle = std::sync::Arc::new(
        DB::open_for_read_only(
            &opts,
            std::env::var("FEATURES_PATH").unwrap_or("rocksdb".to_owned()),
            false,
        )
        .expect("Could not open rocksdb"),
    );

    let tokio_clone = rocksdb_handle.clone();

    tokio::spawn(async move {
        let interval = Duration::from_secs(600); // Adjust the interval as needed
        loop {
            tokio::time::sleep(interval).await;
            info!("Updating db");
            if let Err(e) = tokio_clone.try_catch_up_with_primary() {
                eprintln!("Error catching up with primary: {}", e);
            }
        }
    });

    HttpServer::new(move || {
        let booster = Booster::new().expect("Cannot load model");
        booster
            .load_model("assets/silly_model.json")
            .expect("Cannot load model");
        let number_of_features = booster
            .get_number_of_features()
            .expect("Cannot extract models num feats");

        let shared_data = web::Data::new(AppState {
            booster,
            number_of_features,
            rocksdb: rocksdb_handle.clone(),
        });
        App::new()
            .app_data(shared_data.clone())
            .wrap(prom.clone())
            .route("/score", web::to(score))
            .route("/health", web::to(get_health_check))
            .route("/update_db", web::to(update_db))
    })
    .bind(format!("{}:{}", host.as_str(), port.as_str()))?
    .workers(num_workers)
    .run()
    .await
}
