use std::sync::mpsc::{self, Receiver, TryRecvError};

use chrono::{Local, NaiveDate};
use eframe::egui;

use crate::{cache, date, models::Asteroid, nasa::NasaClient};

struct Loaded {
    asteroids: Vec<Asteroid>,
    source: String,
}

pub struct AsteroidApp {
    date_input: String,
    asteroids: Vec<Asteroid>,
    status: String,
    loading: bool,
    receiver: Option<Receiver<Result<Loaded, String>>>,
}

impl Default for AsteroidApp {
    fn default() -> Self {
        Self {
            date_input: Local::now().date_naive().format("%Y-%m-%d").to_string(),
            asteroids: Vec::new(),
            status: "Введите дату и нажмите «Загрузить».".to_owned(),
            loading: false,
            receiver: None,
        }
    }
}

impl AsteroidApp {
    fn start_load(&mut self) {
        self.asteroids.clear();
        let selected_date = match date::parse_date(&self.date_input) {
            Ok(date) => date,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };

        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        self.loading = true;
        self.status = "Загрузка данных NASA…".to_owned();
        std::thread::spawn(move || {
            let result = load_data(selected_date).map_err(|error| error.to_string());
            let _ = sender.send(result);
        });
    }

    fn poll_result(&mut self) {
        let result = match self.receiver.as_ref().map(Receiver::try_recv) {
            Some(Ok(result)) => Some(result),
            Some(Err(TryRecvError::Disconnected)) => {
                Some(Err("Задача загрузки завершилась без результата.".to_owned()))
            }
            Some(Err(TryRecvError::Empty)) | None => None,
        };
        if let Some(result) = result {
            self.loading = false;
            self.receiver = None;
            match result {
                Ok(loaded) => {
                    self.asteroids = loaded.asteroids;
                    self.status = loaded.source;
                }
                Err(error) => self.status = error,
            }
        }
    }
}

fn load_data(date: NaiveDate) -> anyhow::Result<Loaded> {
    let cache_path = cache::app_cache_path();
    let used_demo_key = std::env::var("NASA_API_KEY")
        .ok()
        .filter(|key| !key.trim().is_empty())
        .is_none();

    fetch_and_cache(date, cache_path, used_demo_key, |date| {
        NasaClient::new()?.fetch(date)
    })
}

fn fetch_and_cache<F>(
    date: NaiveDate,
    cache_path: anyhow::Result<std::path::PathBuf>,
    used_demo_key: bool,
    fetch: F,
) -> anyhow::Result<Loaded>
where
    F: FnOnce(NaiveDate) -> anyhow::Result<Vec<Asteroid>>,
{
    match fetch(date) {
        Ok(asteroids) => {
            let mut source = if used_demo_key {
                "Источник: NASA API (DEMO_KEY; действуют ограничения квоты).".to_owned()
            } else {
                "Источник: NASA API.".to_owned()
            };
            match cache_path {
                Ok(path) => {
                    if let Err(error) = cache::write(&path, date, &asteroids) {
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
            Ok(path) => match cache::read(&path, date) {
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

impl eframe::App for AsteroidApp {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_result();
        if self.loading {
            context.request_repaint_after(std::time::Duration::from_millis(100));
        }

        egui::CentralPanel::default().show(context, |ui| {
            ui.heading("NASA Asteroid Tracker");
            ui.horizontal(|ui| {
                ui.label("Дата (ГГГГ-ММ-ДД):");
                ui.text_edit_singleline(&mut self.date_input);
                if ui
                    .add_enabled(!self.loading, egui::Button::new("Загрузить"))
                    .clicked()
                {
                    self.start_load();
                }
            });
            if self.loading {
                ui.add(egui::ProgressBar::new(0.0).animate(true).text("Загрузка…"));
            }
            ui.label(&self.status);
            ui.separator();

            if self.asteroids.is_empty() && !self.loading {
                if self.status.starts_with("Источник:") || self.status.starts_with("Нет связи")
                {
                    ui.label("Астероиды не найдены.");
                }
                return;
            }

            egui::ScrollArea::both().show(ui, |ui| {
                egui::Grid::new("asteroids").striped(true).show(ui, |ui| {
                    for heading in [
                        "Название",
                        "Диаметр, км",
                        "Скорость, км/с",
                        "Расстояние, км",
                        "Опасен",
                        "Сближение (UTC)",
                    ] {
                        ui.strong(heading);
                    }
                    ui.end_row();
                    for asteroid in &self.asteroids {
                        ui.label(&asteroid.name);
                        ui.label(format!(
                            "{} – {}",
                            number(asteroid.diameter_min_km),
                            number(asteroid.diameter_max_km)
                        ));
                        ui.label(asteroid.velocity_km_s.as_deref().unwrap_or("—"));
                        ui.label(asteroid.miss_distance_km.as_deref().unwrap_or("—"));
                        ui.label(if asteroid.hazardous { "Да" } else { "Нет" });
                        ui.label(asteroid.approach_time_utc.as_deref().unwrap_or("—"));
                        ui.end_row();
                    }
                });
            });
        });
    }
}

fn number(value: Option<f64>) -> String {
    value
        .map(|number| format!("{number:.6}"))
        .unwrap_or_else(|| "—".to_owned())
}
