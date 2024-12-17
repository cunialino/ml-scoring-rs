use rand::Rng;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

const N_IDS: usize = 500000;
const N_COLS: usize = 30;

#[derive(Serialize, Deserialize)]
struct IdFeatures {
    features: std::vec::Vec<f32>,
}

fn main() {
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
    let json_path: String = std::env::var("FEATURES_PATH").unwrap_or("stupid_json".to_string());
    (0..N_IDS).into_par_iter().for_each(|i| {
        info!("Building feature {}", i);
        let data: Vec<f32> = (0..N_COLS)
            .map(|_| rand::thread_rng().gen::<f32>() * 100.)
            .collect();
        info!("Data Created feature {}", i);
        let feats = IdFeatures { features: data };
        let file = match std::fs::File::create(format!("{}/feature_{}.json", json_path.as_str(), i))
        {
            Ok(f) => f,
            _ => {
                warn!("Could not open file for feature {}", i);
                return;
            }
        };
        let mut writer = std::io::BufWriter::new(file);
        match serde_json::to_writer(&mut writer, &feats) {
            Ok(_) => info!("Wrote feature {}", i),
            Err(_) => warn!("Could not write feature {}", i),
        };
    });
    info!("Done");
}
