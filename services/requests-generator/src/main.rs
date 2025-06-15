use rand::Rng;
use serde_json::json;
use std::{env, io};

// Import the ScoringRequest struct from your common crate
use common::ScoringRequest; // <--- This line is the key change

fn main() -> Result<(), io::Error> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <target_url>", args[0]);
        std::process::exit(1);
    }
    let target_url = &args[1];

    let mut rng = rand::rng();

    // Loop indefinitely to generate targets
    loop {
        // Generate random values for the ScoringRequest fields
        let id = format!("feature_{}", rng.random_range(0..15_000_000)); // Random ID string
        let num_var: f32 = 1.0;

        // Create an instance of your common::ScoringRequest struct
        let scoring_request = ScoringRequest {
            id,
            num_var,
        };

        let raw_body_json = serde_json::to_string(&scoring_request)?;
        let encoded_body = base64::encode(&raw_body_json);

        let vegeta_target = json!({
            "method": "POST",
            "url": target_url,
            "body": encoded_body,
            "header": {
                "Content-Type": ["application/json"]
            }
        });

        println!("{}", vegeta_target);
    }
}
