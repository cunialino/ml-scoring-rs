use actix_web::{web, App, HttpResponse, HttpServer, ResponseError};
use actix_web_prom::PrometheusMetricsBuilder;
use prometheus::{Histogram, HistogramOpts};
use thiserror::Error;
use tokio::time::{timeout, Duration};
use tracing::{debug, info, Level};
use tracing_actix_web::TracingLogger;
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
    #[error("No score returned")]
    EmptyScore,
    #[error("XGBoost Error {0}")]
    XGBErr(#[from] xgb_rs::booster::XGBoostError),
    #[error("DMatrix Error {0}")]
    DMATErr(#[from] xgb_rs::dmatrix::DMatrixError),
    #[error("Request timed out")]
    Timeout,
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
            AppError::EmptyScore => HttpResponse::InternalServerError().body("Empty score"),
            AppError::XGBErr(e) => {
                HttpResponse::InternalServerError().body(format!("XGBoost Error: {}", e))
            }
            AppError::DMATErr(e) => {
                HttpResponse::InternalServerError().body(format!("DMatrix Error: {}", e))
            }
            AppError::Timeout => HttpResponse::RequestTimeout().body("Request timeout"),
        }
    }
}

struct AppState {
    booster: std::sync::Arc<Booster>,
    number_of_features: usize,
    rocksdb: std::sync::Arc<kv_store::KvStore<kv_store::Secondary>>,
}

struct PromHist {
    xgboost_hist: Histogram,
    rocksdb_hist: Histogram,
}

async fn score(
    req: web::Json<common::ScoringRequest>,
    prom_hist: web::Data<PromHist>,
    app_data: web::Data<AppState>,
) -> Result<HttpResponse, AppError> {
    let response_future = async {
        debug!("Feature Request {}", req.id.as_str());
        let start = std::time::Instant::now();

        let rb = app_data.rocksdb.clone();
        let id = req.id.clone();

        let timer1 = prom_hist.rocksdb_hist.start_timer();
        let ff = web::block(move || rb.get_feature(&id)).await??;
        timer1.observe_duration();

        let features = ff.features;
        let booster = app_data.booster.clone();
        if features.len() != app_data.number_of_features {
            return Err(AppError::WrongFeaturesCount(req.id.clone()));
        }

        let timer2 = prom_hist.xgboost_hist.start_timer();
        let score = web::block(move || -> Result<f32, AppError> {
            let dmat = DMatrix::try_from_data(features.as_ref(), 1, features.len() as u64)?;
            let predict = booster.predict(&dmat)?;
            Ok(predict.first().ok_or(AppError::EmptyScore)?.to_owned())
        })
        .await??;
        timer2.observe_duration();
        let duration = start.elapsed().as_millis();
        info!(
            feature_id = req.id,
            duration = duration,
            response_code = 200
        );
        Ok(score)
    };
    let score = timeout(Duration::from_millis(31), response_future)
        .await
        .map_err(|_| AppError::Timeout)??;
    Ok(HttpResponse::Ok().json(common::ScoringResponse { score }))
}

async fn get_health_check() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("application/json")
        .body("Heathly!")
}

async fn update_features(app_data: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    info!("Updating features");
    app_data.rocksdb.catchup()?;
    info!("Featuers updated");
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
            .with_max_level(Level::ERROR)
            .with_target(false)
            .init();
    }

    let prometheus = PrometheusMetricsBuilder::new("api_score")
        .endpoint("/metrics")
        .build()
        .unwrap();

    let registry = prometheus.registry.clone();

    let rocksdb_opts = HistogramOpts::new(
        "api_score_rocksdb_duration_seconds",
        "Latency of RocksDB lookup in /score endpoint",
    );
    let rocksdb_hist = Histogram::with_opts(rocksdb_opts).unwrap();
    registry.register(Box::new(rocksdb_hist.clone())).unwrap();

    let xgb_opts = HistogramOpts::new(
        "api_score_xbg_duration_seconds",
        "Latency of XGBoost predict in /score endpoint",
    );
    let xgb_hist = Histogram::with_opts(xgb_opts).unwrap();
    registry.register(Box::new(xgb_hist.clone())).unwrap();
    let prom_hists = PromHist {
        rocksdb_hist,
        xgboost_hist: xgb_hist,
    };
    let prom_hists_data = web::Data::new(prom_hists);

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
            .app_data(prom_hists_data.clone())
            .wrap(TracingLogger::default())
            .wrap(prometheus.clone())
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
