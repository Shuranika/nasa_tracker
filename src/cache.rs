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
