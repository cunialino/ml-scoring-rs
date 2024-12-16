use std::{
    collections::HashMap,
    io::Write,
    sync::{Arc, Mutex},
};

use feature_server::FeatureStore;

use actix_web::{web, App, Error, HttpResponse, HttpServer};
use actix_web_prometheus::PrometheusMetricsBuilder;
use serde::{Deserialize, Serialize};
use tracing::{info, Level};
use xgb_rs::{booster::Booster, dmatrix::DMatrix};

struct AppState {
    store: Arc<FeatureStore>,
    lock: Arc<Mutex<u8>>,
    booster: Booster,
    number_of_features: usize,
    features_path: String,
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
    info!("Feature Request {}", req.id.as_str());
    let features_file = format!(
        "{}/{}.json",
        app_data.features_path.as_str(),
        req.id.as_str()
    );
    info!("Getting features from {}", features_file.as_str());
    let feats_str = match std::fs::read_to_string(features_file.as_str()) {
        Ok(feats_str) => feats_str,
        Err(_) => {
            info!("Could not read file {}", features_file.as_str());
            return HttpResponse::NoContent().body("Cannot get features");
        }
    };
    info!("Get features {}", req.id.as_str());
    let id_features: IdFeatures = match serde_json::from_str(feats_str.as_str()) {
        Ok(idf) => idf,
        Err(_) => return HttpResponse::NotAcceptable().body("Cannot deserialize feats"),
    };
    let features = id_features.features;
    let booster = &app_data.booster;
    if features.len() != app_data.number_of_features {
        return HttpResponse::NotFound().body(format!(
            "Id {} does not have right amount of features",
            req.id.as_str()
        ));
    }
    let dmat = DMatrix::try_from_data(features.as_ref(), 1, features.len() as u64)
        .expect("Cannot create dmatrix");
    let predict = booster.predict(&dmat);
    let vec = predict.expect("Cannot compute score");
    let score = vec.first().unwrap();
    info!(
        "Computed score {} for id {} with features {}",
        score,
        req.id.as_str(),
        serde_json::to_string(&features).unwrap()
    );
    HttpResponse::Ok().json(ScoringResponse { score: *score })
}

async fn batch_update(
    update_req: web::Json<UpdateRequest>,
    app_data: web::Data<AppState>,
) -> Result<HttpResponse, Error> {
    let _lock = app_data.lock.lock();
    info!("Updating features from path: {}", update_req.path);
    let file_content = std::fs::read_to_string(update_req.path.as_str()).map_err(|e| {
        actix_web::error::ErrorBadRequest(format!("Cannot read json at {}: {}", update_req.path, e))
    })?;
    let batch_update: HashMap<String, Vec<f32>> = serde_json::from_str(file_content.as_str())
        .map_err(|e| actix_web::error::ErrorBadRequest(format!("Ill formed json: {}", e)))?;
    app_data.store.batch_update_features(batch_update);
    info!("Updated features from path: {}", update_req.path);
    Ok(HttpResponse::Ok().into())
}

async fn create_feature(
    create_req: web::Json<CreateRequest>,
    app_data: web::Data<AppState>,
) -> HttpResponse {
    info!("Creating feature {}", create_req.id.as_str());
    if create_req.features.len() != app_data.number_of_features {
        return HttpResponse::BadRequest().body("Not enough features");
    }
    let file = match std::fs::File::create(
        format!(
            "{}/{}.json",
            app_data.features_path.as_str(),
            create_req.id.as_str()
        )
        .as_str(),
    ) {
        Ok(f) => f,
        Err(_) => return HttpResponse::BadRequest().body("Cannot open file"),
    };
    let feats = IdFeatures {
        features: create_req.features.clone(),
    };
    let mut writer = std::io::BufWriter::new(file);
    match serde_json::to_writer(&mut writer, &feats) {
        Ok(_) => (),
        Err(_) => return HttpResponse::BadRequest().body("Cannot write file"),
    };
    match writer.flush() {
        Ok(_) => {
            info!("Created feature {}", create_req.id.as_str());
            HttpResponse::Ok().into()
        }
        Err(_) => HttpResponse::BadRequest().body("Cannot write file"),
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
    let feature_store = Arc::new(FeatureStore::default());
    let lock = Arc::new(Mutex::new(0));
    HttpServer::new(move || {
        let booster = Booster::new().expect("Cannot load model");
        booster
            .load_model("assets/silly_model.json")
            .expect("Cannot load model");
        let number_of_features = booster
            .get_number_of_features()
            .expect("Cannot extract models num feats");
        let shared_data = web::Data::new(AppState {
            store: feature_store.clone(),
            lock: lock.clone(),
            booster,
            number_of_features,
            features_path: if let Ok(p) = std::env::var("FEATURES_PATH") {
                p
            } else {
                "stupid_json".to_string()
            },
        });
        App::new()
            .app_data(shared_data.clone())
            .wrap(prom.clone())
            .route("/score", web::to(score))
            .route("/batch_update", web::to(batch_update))
            .route("/create_feature", web::to(create_feature))
    })
    .bind(format!("{}:{}", host.as_str(), port.as_str()))?
    .workers(num_workers)
    .run()
    .await
}
