use k8s_openapi::api::core::v1::{Pod, Service};
use kube::{
    api::{Api, ListParams},
    Client,
};
use rand::Rng;
use reqwest::Client as HttpClient;
use std::sync::Arc;
use std::time::Instant;
use tracing::info;

const N_IDS: usize = 15_000_000;
const N_COLS: usize = 30;
const BATCH_SIZE: usize = 500000;

async fn update_features_all_pods() -> anyhow::Result<()> {
    let k8s_client = Client::try_default().await?;
    let http_client = HttpClient::new();

    // Get the Service
    let services: Api<Service> = Api::namespaced(k8s_client.clone(), "scoring");
    let service_name = "scoring";
    let svc = services.get(service_name).await?;

    // Get the selector
    let selector = svc
        .spec
        .and_then(|spec| spec.selector)
        .ok_or_else(|| anyhow::anyhow!("Service has no selector"))?;

    // Build label selector string
    let selector_string = selector
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join(",");

    // List Pods with that selector
    let pods: Api<Pod> = Api::namespaced(k8s_client.clone(), "scoring");
    let pod_list = pods
        .list(&ListParams::default().labels(&selector_string))
        .await?;

    // For each Pod, send HTTP request
    for pod in pod_list.items {
        if let Some(pod_ip) = pod.status.and_then(|status| status.pod_ip) {
            let url = format!("http://{}:8080/update_features", pod_ip);
            println!("Sending request to {}", url);
            let res = http_client.post(&url).send().await?;
            println!("Response: {}", res.status());
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let start_time = Instant::now();
    println!("Starting at: {:?}", start_time);

    // Initialize logging
    if std::env::var("ECS_TASK").is_ok() {
        tracing_subscriber::fmt()
            .json()
            .with_current_span(false)
            .with_ansi(false)
            .with_max_level(tracing::Level::DEBUG)
            .with_target(false)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_target(false)
            .init();
    }

    // Get RocksDB path from env
    let db_path = std::env::var("FEATURES_PATH").unwrap_or_else(|_| "rocksdb".to_string());
    let kv = kv_store::KvStore::try_new_primary(&db_path, 8 * 1024 * 1024).unwrap();
    let db = Arc::new(kv);

    // Partition IDs into batches
    for chunk in (0..N_IDS).collect::<Vec<_>>().chunks(BATCH_SIZE) {
        info!("Writing batch");
        let slice = chunk.to_vec();
        let db_clone = db.clone();
        let batch_start = Instant::now();

        let mut my_stuff = Vec::with_capacity(slice.len());
        let mut keys = Vec::with_capacity(slice.len());
        for &i in &slice {
            // generate features
            let features: Vec<f32> = (0..N_COLS)
                .map(|_| rand::thread_rng().gen::<f32>() * 100.)
                .collect();

            keys.push(format!("feature_{}", i));

            // SAFETY: Because we reserved capacity, `keys` will never move
            //         or reallocate, so this raw pointer stays valid.
            let last_ptr: *const String = keys.last().unwrap();
            let key_str: &str = unsafe { (*last_ptr).as_str() };
            my_stuff.push((key_str, common::IdFeatures { features }));
        }
        db_clone.write_features(&my_stuff).unwrap();

        db.finalize_writes();

        tokio::spawn(async {
            match update_features_all_pods().await {
                Ok(_) => info!("Updated all features"),
                Err(e) => info!("Err updating pods: {:?}", e),
            }
        });

        println!("Updating pods");
        let duration = batch_start.elapsed();
        println!(
            "Batch of {} features written in {:?}",
            slice.len(),
            duration
        );
    }

    let total_duration = start_time.elapsed();
    println!("All {} features written in {:?}", N_IDS, total_duration);
    println!("Ending at: {:?}", Instant::now());
    Ok(())
}
