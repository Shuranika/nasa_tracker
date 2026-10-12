use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use atomicwrites::{AllowOverwrite, AtomicFile};
use chrono::NaiveDate;

use crate::models::{Asteroid, CacheFile};

const CACHE_VERSION: u32 = 1;

pub fn app_cache_path() -> Result<PathBuf> {
    let project_dirs = directories::ProjectDirs::from("org", "Shuranika", "NASA Asteroid Tracker")
        .context("Не удалось определить системную папку данных приложения")?;
    Ok(project_dirs.data_dir().join("cache.json"))
}

pub fn read(path: &Path, date: NaiveDate) -> Result<Option<Vec<Asteroid>>> {
    let contents = match std::fs::read(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("Не удалось прочитать JSON-кэш"),
    };
    let cache: CacheFile = serde_json::from_slice(&contents)
        .context("JSON-кэш повреждён или имеет неверный формат")?;
    if cache.version != CACHE_VERSION {
        anyhow::bail!(
            "Неподдерживаемая версия кэша: {} (ожидается {})",
            cache.version,
            CACHE_VERSION
        );
    }
    Ok(cache
        .dates
        .get(&date.format("%Y-%m-%d").to_string())
        .cloned())
}

pub fn write(path: &Path, date: NaiveDate, asteroids: &[Asteroid]) -> Result<()> {
    let mut dates = if path.exists() {
        let contents = std::fs::read(path).context("Не удалось прочитать JSON-кэш")?;
        let cache: CacheFile = serde_json::from_slice(&contents)
            .context("JSON-кэш повреждён или имеет неверный формат")?;
        if cache.version != CACHE_VERSION {
            anyhow::bail!("Неподдерживаемая версия JSON-кэша: {}", cache.version);
        }
        cache.dates
    } else {
        HashMap::new()
    };
    dates.insert(date.format("%Y-%m-%d").to_string(), asteroids.to_vec());

    let contents = serde_json::to_vec_pretty(&CacheFile {
        version: CACHE_VERSION,
        dates,
    })
    .context("Не удалось сериализовать JSON-кэш")?;

    let parent = path
        .parent()
        .context("У пути JSON-кэша отсутствует родительская папка")?;
    std::fs::create_dir_all(parent).context("Не удалось создать папку JSON-кэша")?;
    AtomicFile::new(path, AllowOverwrite)
        .write(|file| {
            use std::io::Write;
            file.write_all(&contents)
        })
        .context("Не удалось атомарно сохранить JSON-кэш")
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

    #[test]
    fn missing_cache_is_a_cache_miss() {
        let directory = tempfile::tempdir().unwrap();
        assert_eq!(
            read(&directory.path().join("cache.json"), date(12)).unwrap(),
            None
        );
    }

    #[test]
    fn updates_one_date_without_overwriting_other_dates() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("cache.json");
        write(&path, date(12), &[asteroid("First")]).unwrap();
        write(&path, date(13), &[asteroid("Second")]).unwrap();
        write(&path, date(12), &[asteroid("Updated")]).unwrap();

        assert_eq!(
            read(&path, date(12)).unwrap(),
            Some(vec![asteroid("Updated")])
        );
        assert_eq!(
            read(&path, date(13)).unwrap(),
            Some(vec![asteroid("Second")])
        );
        assert_eq!(read(&path, date(14)).unwrap(), None);
    }

    #[test]
    fn reports_corrupt_or_unsupported_cache() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("cache.json");
        std::fs::write(&path, "{not json").unwrap();
        assert!(read(&path, date(12)).is_err());

        std::fs::write(&path, r#"{"version":99,"dates":{"2026-10-12":[]}}"#).unwrap();
        assert!(read(&path, date(12))
            .unwrap_err()
            .to_string()
            .contains("версия"));
        assert!(write(&path, date(12), &[asteroid("Ignored")]).is_err());
    }

    #[test]
    fn reports_cache_write_failure() {
        let directory = tempfile::tempdir().unwrap();
        let blocking_file = directory.path().join("not-a-directory");
        std::fs::write(&blocking_file, "file").unwrap();
        assert!(write(
            &blocking_file.join("cache.json"),
            date(12),
            &[asteroid("First")]
        )
        .is_err());
    }
}
