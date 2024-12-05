use std::{collections::HashMap, sync::Arc};

use feature_server::FeatureStore;

use actix_web::{web, App, Error, HttpResponse, HttpServer};
use actix_web_prometheus::PrometheusMetricsBuilder;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tracing::{info, Level};

#[derive(Clone, Default)]
struct AppState {
    store: Arc<FeatureStore>,
    lock: Arc<Mutex<u8>>,
}

#[derive(Deserialize)]
struct FeatureRequest {
    id: String,
}

#[derive(Deserialize, Serialize)]
struct FeatureResponse {
    features: std::vec::Vec<f32>,
}

#[derive(Deserialize, Serialize)]
struct UpdateRequest {
    path: String,
}

async fn get_feature(
    feature_req: web::Json<FeatureRequest>,
    app_data: web::Data<AppState>,
) -> HttpResponse {
    info!("Features requests for id: {}", feature_req.id);
    let feats_id = app_data.store.get_feature(feature_req.id.as_str());
    info!("Features requests completed for id: {}", feature_req.id);
    match feats_id {
        Ok(feats) => HttpResponse::Ok().json(FeatureResponse { features: feats }),
        Err(_) => HttpResponse::NoContent().json(FeatureResponse { features: vec![] }),
    }
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

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let host = std::env::var("FEATURES_HOST").unwrap_or("127.0.0.1".to_string());
    let port = std::env::var("FEATURES_PORT").unwrap_or("8080".to_string());
    let num_workers: usize = std::env::var("FEATURES_WORKERS")
        .unwrap_or("2".to_string())
        .parse()
        .expect("Cannote converto num workers to usize");
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .with_target(false)
        .init();
    let shared_data = web::Data::new(AppState::default());
    let prom = PrometheusMetricsBuilder::new("api")
        .endpoint("/metrics")
        .build()
        .unwrap();
    HttpServer::new(move || {
        App::new()
            .app_data(shared_data.clone())
            .wrap(prom.clone())
            .route("/feature", web::to(get_feature))
            .route("/batch_update", web::to(batch_update))
    })
    .bind(format!("{}:{}", host.as_str(), port.as_str()))?
    .workers(num_workers)
    .run()
    .await
}
