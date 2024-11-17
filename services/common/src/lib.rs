use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct UpdateRequest {
    pub path: String,
}

#[derive(Serialize, Deserialize)]
pub struct ScoringRequest {
    #[serde(rename = "f1")]
    pub id: String,
    #[serde(rename = "f2")]
    pub num_var: f32,
}
#[derive(Serialize, Deserialize)]
pub struct ScoringResponse {
    pub score: f32,
}

#[derive(Serialize, Deserialize)]
pub struct IdFeatures {
    pub features: std::vec::Vec<f32>,
}

#[derive(Serialize, Deserialize)]
struct CreateRequest {
    id: String,
    features: std::vec::Vec<f32>,
}
