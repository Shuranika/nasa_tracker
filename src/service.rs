use std::future::Future;

use chrono::NaiveDate;

use crate::{cache, models::Asteroid, nasa::NasaClient};

#[derive(Debug)]
pub struct Loaded {
    pub asteroids: Vec<Asteroid>,
    pub source: String,
}

pub async fn load_data(date: NaiveDate) -> anyhow::Result<Loaded> {
    let cache_path = cache::app_cache_path();
    let used_demo_key = std::env::var("NASA_API_KEY")
        .ok()
        .filter(|key| !key.trim().is_empty())
        .is_none();
    let client = NasaClient::new()?;

    fetch_and_cache(date, cache_path, used_demo_key, move |date| async move {
        client.fetch(date).await
    })
    .await
}

async fn fetch_and_cache<F, Fut>(
    date: NaiveDate,
    cache_path: anyhow::Result<std::path::PathBuf>,
    used_demo_key: bool,
    fetch: F,
) -> anyhow::Result<Loaded>
where
    F: FnOnce(NaiveDate) -> Fut,
    Fut: Future<Output = anyhow::Result<Vec<Asteroid>>>,
{
    match fetch(date).await {
        Ok(asteroids) => {
            let mut source = if used_demo_key {
                "Источник: NASA API (DEMO_KEY; действуют ограничения квоты).".to_owned()
            } else {
                "Источник: NASA API.".to_owned()
            };
            match cache_path {
                Ok(path) => {
                    let cached_asteroids = asteroids.clone();
                    let result = tokio::task::spawn_blocking(move || {
                        cache::write(&path, date, &cached_asteroids)
                    })
                    .await
                    .map_err(anyhow::Error::from)
                    .and_then(|result| result);
                    if let Err(error) = result {
                        source.push_str(&format!(" Не удалось сохранить кэш: {error:#}"));
                    }
                }
                Err(error) => {
                    source.push_str(&format!(" Не удалось определить папку кэша: {error:#}"));
                }
            }
            Ok(Loaded { asteroids, source })
        }
        Err(api_error) => match cache_path {
            Ok(path) => match tokio::task::spawn_blocking(move || cache::read(&path, date))
                .await
                .map_err(anyhow::Error::from)
                .and_then(|result| result)
            {
                Ok(Some(asteroids)) => Ok(Loaded {
                    asteroids,
                    source: format!(
                        "Нет связи с NASA ({api_error:#}). Показан локальный кэш для этой даты."
                    ),
                }),
                Ok(None) => Err(api_error.context("Данных для выбранной даты в кэше нет")),
                Err(cache_error) => Err(anyhow::anyhow!(
                    "Не удалось получить данные NASA ({api_error:#}); также не удалось прочитать кэш ({cache_error:#})"
                )),
            },
            Err(cache_error) => Err(anyhow::anyhow!(
                "Не удалось получить данные NASA ({api_error:#}); также не удалось определить папку кэша ({cache_error:#})"
            )),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
    }

    fn asteroid(name: &str) -> Asteroid {
        Asteroid {
            name: name.to_owned(),
            diameter_min_km: None,
            diameter_max_km: None,
            velocity_km_s: None,
            miss_distance_km: None,
            hazardous: false,
            approach_time_utc: None,
        }
    }

    #[tokio::test]
    async fn uses_cache_only_for_the_requested_date_after_fetch_failure() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("cache.json");
        cache::write(&path, date(12), &[asteroid("Cached")]).unwrap();

        let loaded = fetch_and_cache(date(12), Ok(path.clone()), false, |_| async {
            anyhow::bail!("network unavailable")
        })
        .await
        .unwrap();
        assert_eq!(loaded.asteroids, vec![asteroid("Cached")]);
        assert!(loaded.source.contains("локальный кэш"));

        let error = fetch_and_cache(date(13), Ok(path), false, |_| async {
            anyhow::bail!("network unavailable")
        })
        .await
        .unwrap_err()
        .to_string();
        assert!(error.contains("кэше нет"));
    }

    #[tokio::test]
    async fn empty_network_result_is_success_and_cache_write_failure_is_reported() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("cache.json");
        let loaded = fetch_and_cache(date(12), Ok(path.clone()), false, |_| async {
            Ok(Vec::new())
        })
        .await
        .unwrap();
        assert!(loaded.asteroids.is_empty());
        assert_eq!(cache::read(&path, date(12)).unwrap(), Some(Vec::new()));

        let blocker = directory.path().join("file");
        std::fs::write(&blocker, "not a directory").unwrap();
        let loaded = fetch_and_cache(date(12), Ok(blocker.join("cache.json")), false, |_| async {
            Ok(vec![asteroid("Online")])
        })
        .await
        .unwrap();
        assert_eq!(loaded.asteroids, vec![asteroid("Online")]);
        assert!(loaded.source.contains("Не удалось сохранить кэш"));
    }

    #[tokio::test]
    async fn fetch_success_is_not_blocked_by_unavailable_cache_directory() {
        let directory = tempfile::tempdir().unwrap();
        let blocker = directory.path().join("file");
        std::fs::write(&blocker, "not a directory").unwrap();

        let loaded = fetch_and_cache(
            date(12),
            Err(anyhow::anyhow!("data directory unavailable")),
            false,
            |_| async { Ok(vec![asteroid("Online")]) },
        )
        .await
        .unwrap();
        assert_eq!(loaded.asteroids, vec![asteroid("Online")]);
        assert!(loaded.source.contains("Не удалось определить папку кэша"));
    }
}
