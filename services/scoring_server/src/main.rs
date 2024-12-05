use actix_web::{web, App, HttpResponse, HttpServer};
use actix_web_prometheus::PrometheusMetricsBuilder;
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn, Level};
use xgb_rs::{booster::Booster, dmatrix::DMatrix};

struct AppState {
    booster: Booster,
    features_url: String,
    number_of_features: usize,
}

#[derive(Serialize, Deserialize)]
struct ScoringResponse {
    score: f32,
}

#[derive(Serialize, Deserialize)]
struct ScoringRequest {
    #[serde(rename = "f1")]
    id: String,
    #[serde(rename = "f2")]
    num_var: f32,
}

#[derive(Serialize, Deserialize)]
struct FeatureRequest {
    id: String,
}
#[derive(Serialize, Deserialize)]
struct FeatureResponse {
    features: std::vec::Vec<f32>,
}

async fn score(req: web::Json<ScoringRequest>, app_data: web::Data<AppState>) -> HttpResponse {
    let feature_req = FeatureRequest { id: req.id.clone() };
    info!("Feature Request {}", feature_req.id.as_str());
    let reqw = reqwest::Client::new()
        .get(app_data.features_url.as_str())
        .json(&feature_req);
    let features: FeatureResponse = match reqw.send().await {
        Ok(response) => match response.json::<FeatureResponse>().await {
            Ok(feats) => {
                info!("Got features {:?}", feats.features);
                feats
            }
            Err(e) => {
                warn!("Could not deserialize {}", e);
                return HttpResponse::BadGateway().body("Cannot deserialize features");
            }
        },
        Err(e) => {
            error!("Feature server did not respond");
            return HttpResponse::GatewayTimeout()
                .body(format!("Feature server did not respond: {}", e));
        }
    };
    let booster = &app_data.booster;
    if features.features.len() != app_data.number_of_features {
        return HttpResponse::NotFound().body(format!(
            "Id {} does not have right amount of features",
            req.id.as_str()
        ));
    }
    let dmat = DMatrix::try_from_data(
        features.features.as_ref(),
        1,
        features.features.len() as u64,
    )
    .expect("Cannot create dmatrix");
    let predict = booster.predict(&dmat);
    let vec = predict.expect("Cannot compute score");
    let score = vec.first().unwrap();
    info!(
        "Computed score {} for id {} with features {}",
        score,
        feature_req.id.as_str(),
        serde_json::to_string(&features).unwrap()
    );
    HttpResponse::Ok().json(ScoringResponse { score: *score })
}

#[actix_web::main]
async fn main() -> Result<(), std::io::Error> {
    let host = std::env::var("SCORING_HOST").unwrap_or("127.0.0.1".to_string());
    let port = std::env::var("SCORING_PORT").unwrap_or("8080".to_string());
    let num_workers: usize = std::env::var("SCORING_WORKERS")
        .unwrap_or("2".to_string())
        .parse()
        .expect("Cannot convert num workers to usize");
    let host_feats = std::env::var("FEATURES_HOST").unwrap_or("127.0.0.1".to_string());
    let port_feats = std::env::var("FEATURES_PORT").unwrap_or("8081".to_string());
    let prom = PrometheusMetricsBuilder::new("api")
        .endpoint("/metrics")
        .build()
        .unwrap();
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .with_target(false)
        .init();
    HttpServer::new(move || {
        let booster = Booster::new().expect("Cannot load model");
        booster
            .load_model("assets/silly_model.json")
            .expect("Cannot load model");
        let number_of_features = booster
            .get_number_of_features()
            .expect("Cannot extract models num feats");
        let app_data = web::Data::new(AppState {
            booster,
            features_url: format!("http://{}:{}/feature", host_feats, port_feats),
            number_of_features,
        });
        App::new()
            .app_data(app_data)
            .wrap(prom.clone())
            .route("/score", web::to(score))
    })
    .bind(format!("{}:{}", host.as_str(), port.as_str()))?
    .workers(num_workers)
    .run()
    .await
}
