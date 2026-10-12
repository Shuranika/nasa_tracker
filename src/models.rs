use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Asteroid {
    pub name: String,
    pub diameter_min_km: Option<f64>,
    pub diameter_max_km: Option<f64>,
    pub velocity_km_s: Option<String>,
    pub miss_distance_km: Option<String>,
    pub hazardous: bool,
    pub approach_time_utc: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CacheFile {
    pub version: u32,
    pub dates: std::collections::HashMap<String, Vec<Asteroid>>,
}
