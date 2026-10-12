use std::time::Duration;

use anyhow::{bail, Context, Result};
use chrono::NaiveDateTime;
use reqwest::Client;
use serde::{Deserialize, Serialize};

const SBDB_URL: &str = "https://ssd-api.jpl.nasa.gov/sbdb.api";
const HORIZONS_URL: &str = "https://ssd.jpl.nasa.gov/api/horizons.api";
const WINDOW_HOURS: i64 = 6;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrajectoryPoint {
    pub hours_from_approach: f64,
    pub x_km: f64,
    pub y_km: f64,
    pub z_km: f64,
    pub distance_km: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trajectory {
    pub target: String,
    pub approach_time: String,
    pub points: Vec<TrajectoryPoint>,
}

#[derive(Debug, Deserialize)]
struct SbdbResponse {
    object: Option<SbdbObject>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SbdbObject {
    spkid: String,
    fullname: String,
}

#[derive(Debug, Deserialize)]
struct HorizonsResponse {
    result: Option<String>,
    error: Option<String>,
}

pub struct JplHorizons {
    http: Client,
    sbdb_url: String,
    horizons_url: String,
}

impl JplHorizons {
    pub fn new() -> Result<Self> {
        Self::with_endpoints(SBDB_URL, HORIZONS_URL)
    }

    fn with_endpoints(sbdb_url: &str, horizons_url: &str) -> Result<Self> {
        Ok(Self {
            http: Client::builder()
                .http1_only()
                .user_agent("NASA-AsteroidTracker/0.1")
                .timeout(Duration::from_secs(40))
                .build()
                .context("Не удалось создать HTTP-клиент JPL Horizons")?,
            sbdb_url: sbdb_url.to_owned(),
            horizons_url: horizons_url.to_owned(),
        })
    }

    pub async fn trajectory(&self, name: &str, approach_time: &str) -> Result<Trajectory> {
        validate_name(name)?;
        let approach = parse_approach_time(approach_time)?;

        let sbdb_response = self
            .http
            .get(&self.sbdb_url)
            .query(&[("sstr", normalized_name(name)), ("no-orbit", "true")])
            .send()
            .await
            .context("Не удалось связаться с JPL Small-Body Database")?;
        let sbdb_response = ensure_success(
            sbdb_response,
            "JPL Small-Body Database вернула ошибочный HTTP-статус",
        )
        .await?;
        let sbdb: SbdbResponse = sbdb_response
            .json()
            .await
            .context("Не удалось разобрать ответ JPL Small-Body Database")?;
        if let Some(error) = sbdb.error {
            bail!("JPL Small-Body Database: {error}");
        }
        let object = sbdb
            .object
            .context("JPL не смогла однозначно найти этот астероид")?;

        let start = approach - chrono::Duration::hours(WINDOW_HOURS);
        let stop = approach + chrono::Duration::hours(WINDOW_HOURS);
        let response = self
            .http
            .get(&self.horizons_url)
            .query(&[
                ("format", "json"),
                ("COMMAND", &format!("'DES={};'", object.spkid)),
                ("OBJ_DATA", "NO"),
                ("MAKE_EPHEM", "YES"),
                ("EPHEM_TYPE", "VECTORS"),
                ("CENTER", "500@399"),
                ("START_TIME", &start.format("%Y-%m-%d %H:%M").to_string()),
                ("STOP_TIME", &stop.format("%Y-%m-%d %H:%M").to_string()),
                ("STEP_SIZE", "30 m"),
                ("VEC_TABLE", "2"),
                ("CSV_FORMAT", "YES"),
                ("OUT_UNITS", "KM-S"),
            ])
            .send()
            .await
            .context("Не удалось связаться с NASA/JPL Horizons")?;
        let response =
            ensure_success(response, "NASA/JPL Horizons вернул ошибочный HTTP-статус").await?;
        let response: HorizonsResponse = response
            .json()
            .await
            .context("Не удалось разобрать ответ NASA/JPL Horizons")?;

        if let Some(error) = response.error {
            bail!("NASA/JPL Horizons: {error}");
        }
        let result = response
            .result
            .context("В ответе NASA/JPL Horizons отсутствуют эфемериды")?;

        Ok(Trajectory {
            target: object.fullname,
            approach_time: approach_time.to_owned(),
            points: parse_vector_table(&result, approach)?,
        })
    }
}

async fn ensure_success(response: reqwest::Response, context: &str) -> Result<reqwest::Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let url = response.url().clone();
    let body = response
        .text()
        .await
        .unwrap_or_else(|error| format!("Не удалось прочитать тело ответа: {error}"));
    bail!("{context} ({url}): HTTP {status}: {body}");
}

fn normalized_name(name: &str) -> &str {
    let name = name.trim();
    if let Some(start) = name.rfind('(') {
        if let Some(end) = name[start..].find(')') {
            let designation = name[start + 1..start + end].trim();
            if !designation.is_empty() {
                return designation;
            }
        }
    }
    name.trim_start_matches('(').trim_end_matches(')').trim()
}

fn validate_name(name: &str) -> Result<()> {
    let name = normalized_name(name);
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b" ()-/".contains(&byte))
    {
        bail!("Имя астероида содержит неподдерживаемые символы.");
    }
    Ok(())
}

fn parse_approach_time(value: &str) -> Result<NaiveDateTime> {
    NaiveDateTime::parse_from_str(value.trim(), "%Y-%b-%d %H:%M")
        .or_else(|_| NaiveDateTime::parse_from_str(value.trim(), "%Y-%b-%d %H:%M:%S"))
        .map_err(|_| anyhow::anyhow!("Для объекта нет точного времени сближения NASA."))
}

fn parse_vector_table(result: &str, approach: NaiveDateTime) -> Result<Vec<TrajectoryPoint>> {
    let table = result
        .split_once("$$SOE")
        .and_then(|(_, remainder)| remainder.split_once("$$EOE").map(|(table, _)| table))
        .context("В ответе NASA/JPL Horizons отсутствует таблица эфемерид")?;
    let points: Vec<_> = table
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let values: Vec<_> = line.split(',').map(str::trim).collect();
            if values.len() < 8 {
                return Some(Err(anyhow::anyhow!("Неверная строка в таблице эфемерид")));
            }
            let parse = |index: usize| {
                values[index].parse::<f64>().with_context(|| {
                    format!("Неверная координата в таблице эфемерид: {}", values[index])
                })
            };
            Some((|| {
                let julian_date = parse(0)?;
                let x_km = parse(2)?;
                let y_km = parse(3)?;
                let z_km = parse(4)?;
                let distance_km = x_km.hypot(y_km).hypot(z_km);
                if !distance_km.is_finite() || !julian_date.is_finite() {
                    bail!("NASA/JPL Horizons вернул нечисловые координаты");
                }
                let approach_julian =
                    approach.and_utc().timestamp() as f64 / 86_400.0 + 2_440_587.5;
                Ok(TrajectoryPoint {
                    hours_from_approach: (julian_date - approach_julian) * 24.0,
                    x_km,
                    y_km,
                    z_km,
                    distance_km,
                })
            })())
        })
        .collect::<Result<_>>()?;
    if points.len() < 2 {
        bail!("NASA/JPL Horizons вернул слишком мало точек траектории");
    }

    Ok(points)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread::JoinHandle,
    };

    fn mock_jpl() -> (String, JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 1024];
                loop {
                    let read = stream.read(&mut buffer).unwrap();
                    request.extend_from_slice(&buffer[..read]);
                    if read == 0 || request.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }
                let request = String::from_utf8_lossy(&request).into_owned();
                let body = if request.starts_with("GET /sbdb.api") {
                    r#"{"object":{"spkid":"50843661","fullname":"(2019 QC4)"}}"#
                } else {
                    r#"{"result":"$$SOE\n2461325.250000000, A.D. 2026-Oct-11 18:00:00.0000, 3000.0, 4000.0, 0.0, 1.0, 2.0, 3.0,\n2461325.500000000, A.D. 2026-Oct-12 00:00:00.0000, 0.0, 13000.0, 0.0, 1.0, 2.0, 3.0,\n$$EOE"}"#
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).unwrap();
                requests.push(request);
            }
            requests
        });
        (format!("http://{address}"), worker)
    }

    #[test]
    fn parses_vector_table_as_earth_centered_distances() {
        let result = r#"
$$SOE
2461325.250000000, A.D. 2026-Oct-11 18:00:00.0000, 3000.0, 4000.0, 0.0, 1.0, 2.0, 3.0,
2461325.500000000, A.D. 2026-Oct-12 00:00:00.0000, 0.0, 13000.0, 0.0, 1.0, 2.0, 3.0,
$$EOE
"#;
        let approach =
            NaiveDateTime::parse_from_str("2026-Oct-12 00:00", "%Y-%b-%d %H:%M").unwrap();
        let points = parse_vector_table(result, approach).unwrap();

        assert_eq!(points.len(), 2);
        assert!((points[0].distance_km - 5000.0).abs() < f64::EPSILON);
        assert!((points[1].distance_km - 13000.0).abs() < f64::EPSILON);
        assert!((points[1].hours_from_approach - 0.0).abs() < 0.001);
    }

    #[test]
    fn rejects_empty_or_malformed_horizons_tables() {
        let approach =
            NaiveDateTime::parse_from_str("2026-Oct-12 00:00", "%Y-%b-%d %H:%M").unwrap();
        assert!(parse_vector_table("No ephemeris", approach).is_err());
        assert!(parse_vector_table("$$SOE\n$$EOE", approach).is_err());
        assert!(parse_vector_table("$$SOE\nbad row\n$$EOE", approach).is_err());
    }

    #[test]
    fn validates_designations_and_approach_times() {
        assert_eq!(normalized_name("(2019 QC4)"), "2019 QC4");
        assert_eq!(normalized_name("164207 Cardea (2004 GU9)"), "2004 GU9");
        assert!(validate_name("2019 QC4").is_ok());
        assert!(validate_name("bad&COMMAND=1").is_err());
        assert!(validate_name(&"a".repeat(65)).is_err());
        assert!(parse_approach_time("2050-Oct-12 05:45").is_ok());
        assert!(parse_approach_time("not a time").is_err());
    }

    #[tokio::test]
    async fn resolves_spkid_and_requests_earth_centered_vectors() {
        let (endpoint, worker) = mock_jpl();
        let client = JplHorizons::with_endpoints(
            &format!("{endpoint}/sbdb.api"),
            &format!("{endpoint}/api/horizons.api"),
        )
        .unwrap();
        let trajectory = client
            .trajectory("(2019 QC4)", "2026-Oct-12 00:00")
            .await
            .unwrap();
        let requests = worker.join().unwrap();

        assert_eq!(trajectory.target, "(2019 QC4)");
        assert_eq!(trajectory.points.len(), 2);
        assert_eq!(trajectory.points[0].hours_from_approach, -6.0);
        assert_eq!(trajectory.points[1].distance_km, 13_000.0);
        assert!(requests[0].starts_with("GET /sbdb.api?sstr=2019+QC4"));
        assert!(requests[1].contains("COMMAND=%27DES%3D50843661%3B%27"));
        assert!(requests[1].contains("CENTER=500%40399"));
        assert!(requests[1].contains("VEC_TABLE=2"));
    }
}
