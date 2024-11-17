use feature_server::FeatureStore;

use actix_web::{web, App, HttpResponse, HttpServer};
use actix_web_prometheus::PrometheusMetricsBuilder;
use serde::{Deserialize, Serialize};
use tracing::{info, Level};

#[derive(Deserialize)]
struct FeatureRequest {
    id: String,
}

#[derive(Deserialize, Serialize)]
struct FeatureResponse {
    features: std::vec::Vec<f32>,
}

async fn get_feature(
    feature_req: web::Json<FeatureRequest>,
    feature_store: web::Data<FeatureStore>,
) -> HttpResponse {
    info!("Features requests for id: {}", feature_req.id);
    let feats_id = feature_store.get_feature(feature_req.id.as_str());
    match feats_id {
        Ok(feats) => HttpResponse::Ok().json(FeatureResponse { features: feats }),
        Err(_) => HttpResponse::NoContent().json(FeatureResponse { features: vec![] }),
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .init();
    let feature_store = web::Data::new(FeatureStore::default());
    let prom = PrometheusMetricsBuilder::new("api")
        .endpoint("/metrics")
        .build()
        .unwrap();
    HttpServer::new(move || {
        App::new()
            .app_data(feature_store.clone())
            .wrap(prom.clone())
            .route("/feature", web::to(get_feature))
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}
