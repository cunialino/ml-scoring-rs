use futures::future::join_all;
use rand::Rng;
use std::sync::Arc;
use std::time::Instant;

const N_IDS: usize = 15_000_000;
const N_COLS: usize = 30;
const BATCH_SIZE: usize = 500000; // number of puts per batch

#[tokio::main]
async fn main() {
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
            .with_max_level(tracing::Level::DEBUG)
            .with_target(false)
            .init();
    }

    // Get RocksDB path from env
    let db_path = std::env::var("FEATURES_PATH").unwrap_or_else(|_| "rocksdb".to_string());
    let kv = kv_store::KvStore::try_new_primary(&db_path, 8 * 1024 * 1024).unwrap();
    let db = Arc::new(kv);

    // Concurrency limiter
    let semaphore = Arc::new(tokio::sync::Semaphore::new(10));

    // Partition IDs into batches
    let mut batch_tasks = vec![];
    for chunk in (0..N_IDS).collect::<Vec<_>>().chunks(BATCH_SIZE) {
        let slice = chunk.to_vec();
        let db_clone = db.clone();
        let sem = semaphore.clone();
        // Spawn a task per batch
        let fut = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
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

            let duration = batch_start.elapsed();
            println!(
                "Batch of {} features written in {:?}",
                slice.len(),
                duration
            );
        });
        batch_tasks.push(fut);
    }

    // Wait for all batches
    let _ = join_all(batch_tasks).await;

    db.finalize_writes();
    let total_duration = start_time.elapsed();
    println!("All {} features written in {:?}", N_IDS, total_duration);
    println!("Ending at: {:?}", Instant::now());
}
