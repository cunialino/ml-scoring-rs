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

async fn get_feature(feature_req: web::Json<FeatureRequest>) -> HttpResponse {
    info!("Features requests for id: {}", feature_req.id);
    HttpResponse::Ok().json(FeatureResponse { features: vec![] })
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_target(false)
        .init();
    let prom = PrometheusMetricsBuilder::new("api")
        .endpoint("/metrics")
        .build()
        .unwrap();
    HttpServer::new(move || {
        App::new()
            .wrap(prom.clone())
            .route("/feature", web::to(get_feature))
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
