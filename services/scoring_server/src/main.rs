use actix_web::{web, App, HttpResponse, HttpServer, ResponseError};
use actix_web_prometheus::PrometheusMetricsBuilder;
use thiserror::Error;
use tracing::{debug, info, Level};
use xgb_rs::{booster::Booster, dmatrix::DMatrix};

#[derive(Debug, Error)]
pub enum AppError {
    #[error("KvStore error: {0}")]
    Kv(#[from] kv_store::KvStoreError),
    // Add other errors as needed here
    #[error("Error while blocking")]
    BlockingErr(#[from] actix_web::error::BlockingError),
    #[error("Id {0} does not have the right amount of features")]
    WrongFeaturesCount(String),
}

impl ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        match self {
            AppError::Kv(e) => match e {
                kv_store::KvStoreError::NotFound => {
                    HttpResponse::NotFound().body("Feature ID not found")
                }
                kv_store::KvStoreError::IllFormed(e) => {
                    HttpResponse::BadRequest().body(format!("Invalid JSON: {}", e))
                }
                kv_store::KvStoreError::DbError(e) => {
                    HttpResponse::InternalServerError().body(format!("Database error: {}", e))
                }
            },
            AppError::BlockingErr(e) => {
                HttpResponse::InternalServerError().body(format!("Actix Block error: {}", e))
            }
            AppError::WrongFeaturesCount(id) => HttpResponse::BadRequest().body(format!(
                "Id {} does not have the right amount of features",
                id
            )),
        }
    }
}

struct AppState {
    booster: std::sync::Arc<Booster>,
    number_of_features: usize,
    rocksdb: std::sync::Arc<kv_store::KvStore<kv_store::Secondary>>,
}

async fn score(
    req: web::Json<common::ScoringRequest>,
    app_data: web::Data<AppState>,
) -> Result<HttpResponse, AppError> {
    debug!("Feature Request {}", req.id.as_str());
    let start = std::time::Instant::now();

    let rb = app_data.rocksdb.clone();
    let id = req.id.clone();

    let ff = web::block(move || rb.get_feature(&id)).await??;

    let features = ff.features;
    let booster = app_data.booster.clone();
    if features.len() != app_data.number_of_features {
        return Err(AppError::WrongFeaturesCount(req.id.clone()));
    }
    let score = web::block(move || {
        let dmat = DMatrix::try_from_data(features.as_ref(), 1, features.len() as u64)
            .expect("Cannot create dmatrix");
        let predict = booster.predict(&dmat);
        let vec = predict.expect("Cannot compute score");
        let score = vec.first().unwrap().to_owned();
        debug!(
            "Computed score {} for id {} with features {}",
            score,
            req.id.as_str(),
            serde_json::to_string(&features).unwrap()
        );
        score
    })
    .await?;
    let duration = start.elapsed().as_millis();
    info!(duration = duration, response_code = 200);
    Ok(HttpResponse::Ok().json(common::ScoringResponse { score }))
}

async fn get_health_check() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("application/json")
        .body("Heathly!")
}

async fn update_features(app_data: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    app_data.rocksdb.catchup()?;
    Ok(HttpResponse::Ok().body("Feature updated"))
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
    let cache_size = 8 * 1024 * 1024; // 512MB

    let rocksdb_handle = std::sync::Arc::new(
        kv_store::KvStore::try_new_secondary(
            &std::env::var("FEATURES_PATH").unwrap_or("rocksdb".to_owned()),
            &std::env::var("SECONDARY_FEATURES_PATH").unwrap_or("rocks_db_secondary".to_owned()),
            cache_size,
        )
        .unwrap(),
    );

    let mut booster = Booster::new().expect("Cannot load model");
    booster
        .load_model("assets/silly_model.json")
        .expect("Cannot load model");
    booster
        .set_conf("nthread", "1")
        .expect("Cannot set threads");
    let number_of_features = booster
        .get_number_of_features()
        .expect("Cannot extract models num feats");

    let booster = std::sync::Arc::new(booster);
    HttpServer::new(move || {
        let shared_data = web::Data::new(AppState {
            booster: booster.clone(),
            number_of_features,
            rocksdb: rocksdb_handle.clone(),
        });
        App::new()
            .app_data(shared_data.clone())
            .wrap(prom.clone())
            .route("/score", web::to(score))
            .route("/health", web::to(get_health_check))
            .route("/update_features", web::to(update_features))
    })
    .bind(format!("{}:{}", host.as_str(), port.as_str()))?
    .workers(num_workers)
    .worker_max_blocking_threads(num_workers * 2)
    .run()
    .await
}
