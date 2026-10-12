use std::collections::HashMap;

use anyhow::{Context, Result};
use chrono::NaiveDate;
use serde::Deserialize;

use crate::models::Asteroid;

const API_URL: &str = "https://api.nasa.gov/neo/rest/v1/feed";
const DEMO_KEY: &str = "DEMO_KEY";

#[derive(Debug, Deserialize)]
struct FeedResponse {
    near_earth_objects: HashMap<String, Vec<NeoObject>>,
}

#[derive(Debug, Deserialize)]
struct NeoObject {
    name: String,
    #[serde(default)]
    estimated_diameter: Option<EstimatedDiameter>,
    #[serde(default)]
    close_approach_data: Vec<CloseApproach>,
    #[serde(default)]
    is_potentially_hazardous_asteroid: bool,
}

#[derive(Debug, Deserialize)]
struct EstimatedDiameter {
    kilometers: Option<KilometerDiameter>,
}

#[derive(Debug, Deserialize)]
struct KilometerDiameter {
    estimated_diameter_min: Option<f64>,
    estimated_diameter_max: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct CloseApproach {
    #[serde(default)]
    close_approach_date_full: Option<String>,
    #[serde(default)]
    close_approach_date: Option<String>,
    relative_velocity: Option<Velocity>,
    miss_distance: Option<MissDistance>,
}

#[derive(Debug, Deserialize)]
struct Velocity {
    kilometers_per_second: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MissDistance {
    kilometers: Option<String>,
}

pub struct NasaClient {
    http: reqwest::blocking::Client,
    endpoint: String,
    api_key: String,
}

impl NasaClient {
    pub fn new() -> Result<Self> {
        Self::with_endpoint(API_URL, api_key())
    }

    fn with_endpoint(endpoint: &str, api_key: String) -> Result<Self> {
        Self::with_timeout(endpoint, api_key, std::time::Duration::from_secs(20))
    }

    fn with_timeout(endpoint: &str, api_key: String, timeout: std::time::Duration) -> Result<Self> {
        let http = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .build()
            .context("Не удалось создать HTTP-клиент NASA")?;
        Ok(Self {
            http,
            endpoint: endpoint.to_owned(),
            api_key,
        })
    }

    pub fn fetch(&self, date: NaiveDate) -> Result<Vec<Asteroid>> {
        let date = date.format("%Y-%m-%d").to_string();
        let response = self
            .http
            .get(&self.endpoint)
            .query(&[
                ("start_date", date.as_str()),
                ("end_date", date.as_str()),
                ("api_key", self.api_key.as_str()),
            ])
            .send()
            .context("Не удалось связаться с NASA NeoWs")?
            .error_for_status()
            .context("NASA NeoWs вернул ошибочный HTTP-статус")?;

        let feed: FeedResponse = response
            .json()
            .context("Не удалось разобрать JSON-ответ NASA NeoWs")?;
        parse_feed(feed, &date)
    }
}

fn api_key() -> String {
    std::env::var("NASA_API_KEY")
        .ok()
        .filter(|key| !key.trim().is_empty())
        .unwrap_or_else(|| DEMO_KEY.to_owned())
}

fn parse_feed(feed: FeedResponse, date: &str) -> Result<Vec<Asteroid>> {
    let objects = feed
        .near_earth_objects
        .get(date)
        .with_context(|| format!("В ответе NASA отсутствуют данные за {date}"))?;

    Ok(objects
        .iter()
        .map(|object| {
            let diameter = object
                .estimated_diameter
                .as_ref()
                .and_then(|diameter| diameter.kilometers.as_ref());
            let approach = object.close_approach_data.first();

            Asteroid {
                name: object.name.clone(),
                diameter_min_km: diameter.and_then(|value| value.estimated_diameter_min),
                diameter_max_km: diameter.and_then(|value| value.estimated_diameter_max),
                velocity_km_s: approach
                    .and_then(|value| value.relative_velocity.as_ref())
                    .and_then(|value| value.kilometers_per_second.clone()),
                miss_distance_km: approach
                    .and_then(|value| value.miss_distance.as_ref())
                    .and_then(|value| value.kilometers.clone()),
                hazardous: object.is_potentially_hazardous_asteroid,
                approach_time_utc: approach.and_then(|value| {
                    value
                        .close_approach_date_full
                        .clone()
                        .or_else(|| value.close_approach_date.clone())
                }),
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread::JoinHandle,
        time::Duration,
    };

    fn mock_server(
        status: &'static str,
        body: &'static str,
        delay: Duration,
    ) -> (String, JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 1024];
            loop {
                let bytes_read = stream.read(&mut buffer).unwrap();
                request.extend_from_slice(&buffer[..bytes_read]);
                if bytes_read == 0 || request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                    break;
                }
            }
            if !delay.is_zero() {
                std::thread::sleep(delay);
            }
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
            String::from_utf8_lossy(&request).into_owned()
        });
        (format!("http://{address}"), worker)
    }

    #[test]
    fn parses_empty_day_and_rejects_missing_day() {
        let feed: FeedResponse =
            serde_json::from_str(r#"{"near_earth_objects":{"2026-10-12":[]}}"#).unwrap();
        assert!(parse_feed(feed, "2026-10-12").unwrap().is_empty());

        let feed: FeedResponse = serde_json::from_str(r#"{"near_earth_objects":{}}"#).unwrap();
        assert!(parse_feed(feed, "2026-10-12").is_err());
    }

    #[test]
    fn maps_known_and_missing_optional_fields() {
        let feed: FeedResponse = serde_json::from_str(
            r#"{
                "near_earth_objects": {
                    "2026-10-12": [{
                        "name": "Example",
                        "estimated_diameter": {"kilometers": {
                            "estimated_diameter_min": 0.1,
                            "estimated_diameter_max": 0.2
                        }},
                        "close_approach_data": [{
                            "close_approach_date": "2026-Oct-12",
                            "relative_velocity": {"kilometers_per_second": "12.5"},
                            "miss_distance": {"kilometers": "1000"}
                        }],
                        "is_potentially_hazardous_asteroid": true
                    }, {
                        "name": "Partial",
                        "is_potentially_hazardous_asteroid": false
                    }]
                }
            }"#,
        )
        .unwrap();

        let asteroids = parse_feed(feed, "2026-10-12").unwrap();
        assert_eq!(asteroids.len(), 2);
        assert_eq!(asteroids[0].name, "Example");
        assert_eq!(asteroids[0].diameter_max_km, Some(0.2));
        assert_eq!(asteroids[0].velocity_km_s.as_deref(), Some("12.5"));
        assert!(asteroids[0].hazardous);
        assert_eq!(asteroids[1].diameter_min_km, None);
        assert_eq!(asteroids[1].approach_time_utc, None);
    }

    #[test]
    fn uses_demo_key_for_missing_or_blank_environment_value() {
        assert_eq!(api_key_for_test(None), DEMO_KEY);
        assert_eq!(api_key_for_test(Some("  ")), DEMO_KEY);
        assert_eq!(api_key_for_test(Some("private")), "private");
    }

    fn api_key_for_test(value: Option<&str>) -> String {
        value
            .filter(|key| !key.trim().is_empty())
            .unwrap_or(DEMO_KEY)
            .to_owned()
    }

    #[test]
    fn client_endpoint_can_be_replaced_for_local_http_tests() {
        let client = NasaClient::with_endpoint("http://127.0.0.1:1", DEMO_KEY.to_owned());
        assert!(client.is_ok());
    }

    #[test]
    fn fetch_sends_selected_day_and_maps_local_feed_response() {
        let body = r#"{"near_earth_objects":{"2026-10-12":[{"name":"Local test"}]}}"#;
        let (endpoint, server) = mock_server("200 OK", body, Duration::ZERO);
        let client = NasaClient::with_endpoint(&endpoint, "local-key".to_owned()).unwrap();
        let asteroids = client
            .fetch(NaiveDate::from_ymd_opt(2026, 10, 12).unwrap())
            .unwrap();
        let request = server.join().unwrap();

        assert_eq!(asteroids.len(), 1);
        assert_eq!(asteroids[0].name, "Local test");
        assert!(request.contains("start_date=2026-10-12"));
        assert!(request.contains("end_date=2026-10-12"));
        assert!(request.contains("api_key=local-key"));
    }

    #[test]
    fn fetch_reports_http_status_and_timeout_errors() {
        let (endpoint, server) = mock_server("500 Internal Server Error", "{}", Duration::ZERO);
        let client = NasaClient::with_endpoint(&endpoint, "local-key".to_owned()).unwrap();
        let error = client
            .fetch(NaiveDate::from_ymd_opt(2026, 10, 12).unwrap())
            .unwrap_err();
        assert!(format!("{error:#}").contains("500"));
        server.join().unwrap();

        let (endpoint, server) = mock_server("200 OK", "{}", Duration::from_millis(150));
        let client =
            NasaClient::with_timeout(&endpoint, "local-key".to_owned(), Duration::from_millis(25))
                .unwrap();
        let error = client
            .fetch(NaiveDate::from_ymd_opt(2026, 10, 12).unwrap())
            .unwrap_err();
        assert!(format!("{error:#}").contains("NASA NeoWs"));
        server.join().unwrap();
    }
}
