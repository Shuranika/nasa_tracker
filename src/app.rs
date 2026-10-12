use std::sync::mpsc::{self, Receiver, TryRecvError};

use chrono::{Local, NaiveDate};
use eframe::egui;

use crate::{cache, date, models::Asteroid, nasa::NasaClient};

#[derive(Debug)]
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

        context.set_visuals(egui::Visuals::dark());
        let mut style = (*context.style()).clone();
        style.spacing.item_spacing = egui::vec2(12.0, 12.0);
        style.visuals.panel_fill = color(12, 18, 32);
        style.visuals.window_fill = color(17, 26, 44);
        style.visuals.widgets.noninteractive.bg_fill = color(24, 36, 56);
        style.visuals.widgets.inactive.bg_fill = color(31, 46, 68);
        style.visuals.widgets.hovered.bg_fill = color(44, 66, 91);
        style.visuals.widgets.active.bg_fill = color(41, 190, 190);
        style.visuals.selection.bg_fill = color(41, 190, 190);
        context.set_style(style);

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(color(12, 18, 32))
                    .inner_margin(24.0),
            )
            .show(context, |ui| {
                ui.set_max_width(1040.0);
                ui.vertical_centered(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("NASA  /  NEOWS")
                                .size(12.0)
                                .strong()
                                .color(color(73, 220, 212)),
                        );
                        ui.label(
                            egui::RichText::new("LIVE ASTEROID TRACKER")
                                .size(11.0)
                                .color(color(136, 157, 181)),
                        );
                    });
                    ui.add_space(8.0);
                    ui.heading(
                        egui::RichText::new("Близкие встречи")
                            .size(32.0)
                            .strong()
                            .color(color(236, 244, 255)),
                    );
                    ui.label(
                        egui::RichText::new("Астероиды, пролетающие рядом с Землёй")
                            .size(14.0)
                            .color(color(152, 171, 194)),
                    );
                });

                ui.add_space(24.0);
                egui::Frame::new()
                    .fill(color(21, 32, 51))
                    .corner_radius(14)
                    .inner_margin(18.0)
                    .stroke(egui::Stroke::new(1.0_f32, color(38, 55, 79)))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("ВЫБЕРИТЕ ДАТУ СБЛИЖЕНИЯ")
                                .size(11.0)
                                .strong()
                                .color(color(152, 171, 194)),
                        );
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.date_input)
                                    .hint_text("ГГГГ-ММ-ДД")
                                    .desired_width(210.0)
                                    .margin(egui::vec2(12.0, 11.0)),
                            );
                            let button = egui::Button::new(
                                egui::RichText::new(if self.loading {
                                    "  Загружаем…  "
                                } else {
                                    "  Показать астероиды  "
                                })
                                .strong()
                                .color(color(8, 23, 37)),
                            )
                            .fill(color(73, 220, 212))
                            .corner_radius(9);
                            if ui.add_enabled(!self.loading, button).clicked() {
                                self.start_load();
                            }
                        });
                        if self.loading {
                            ui.add_space(10.0);
                            ui.add(
                                egui::ProgressBar::new(0.0)
                                    .animate(true)
                                    .fill(color(73, 220, 212))
                                    .text("Запрашиваем NASA NeoWs…"),
                            );
                        }
                    });

                ui.add_space(12.0);
                status_panel(ui, &self.status, self.loading);

                if self.loading || self.status.starts_with("Введите дату") {
                    empty_state(
                        ui,
                        "Выберите дату",
                        "Получите список астероидов и параметры их сближения.",
                    );
                    return;
                }
                if self.asteroids.is_empty() {
                    let message = if self.status.starts_with("Источник:")
                        || self.status.starts_with("Нет связи")
                    {
                        "Для этой даты астероиды не найдены."
                    } else {
                        "Проверьте дату или подключение и попробуйте ещё раз."
                    };
                    empty_state(ui, "Нет данных", message);
                    return;
                }

                ui.add_space(22.0);
                ui.horizontal(|ui| {
                    ui.heading(
                        egui::RichText::new("Объекты сближения")
                            .size(20.0)
                            .strong()
                            .color(color(236, 244, 255)),
                    );
                    ui.label(
                        egui::RichText::new(format!("{} объектов", self.asteroids.len()))
                            .size(12.0)
                            .color(color(152, 171, 194)),
                    );
                });
                ui.add_space(8.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for asteroid in &self.asteroids {
                            asteroid_card(ui, asteroid);
                            ui.add_space(10.0);
                        }
                    });
            });
    }
}

fn color(red: u8, green: u8, blue: u8) -> egui::Color32 {
    egui::Color32::from_rgb(red, green, blue)
}

fn status_panel(ui: &mut egui::Ui, status: &str, loading: bool) {
    let (accent, label) = if loading {
        (color(73, 220, 212), "ЗАГРУЗКА")
    } else if status.starts_with("Источник:") {
        (color(73, 220, 212), "ДАННЫЕ ОБНОВЛЕНЫ")
    } else if status.starts_with("Нет связи") {
        (color(255, 190, 92), "РЕЖИМ КЭША")
    } else if status.starts_with("Введите дату") {
        (color(136, 157, 181), "ГОТОВО К ЗАПРОСУ")
    } else {
        (color(255, 130, 130), "ТРЕБУЕТ ВНИМАНИЯ")
    };

    egui::Frame::new()
        .fill(color(18, 28, 45))
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(14, 10))
        .stroke(egui::Stroke::new(1.0_f32, color(38, 55, 79)))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("●").color(accent));
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(label).size(10.0).strong().color(accent));
                    ui.label(
                        egui::RichText::new(status)
                            .size(12.0)
                            .color(color(184, 199, 217)),
                    );
                });
            });
        });
}

fn empty_state(ui: &mut egui::Ui, title: &str, description: &str) {
    ui.add_space(36.0);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new("✦")
                .size(38.0)
                .color(color(73, 220, 212)),
        );
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(title)
                .size(20.0)
                .strong()
                .color(color(236, 244, 255)),
        );
        ui.label(
            egui::RichText::new(description)
                .size(13.0)
                .color(color(152, 171, 194)),
        );
    });
}

fn asteroid_card(ui: &mut egui::Ui, asteroid: &Asteroid) {
    egui::Frame::new()
        .fill(color(21, 32, 51))
        .corner_radius(13)
        .inner_margin(16.0)
        .stroke(egui::Stroke::new(1.0_f32, color(38, 55, 79)))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("◉")
                        .size(18.0)
                        .color(color(73, 220, 212)),
                );
                ui.label(
                    egui::RichText::new(&asteroid.name)
                        .size(17.0)
                        .strong()
                        .color(color(236, 244, 255)),
                );
                let (badge, badge_color) = if asteroid.hazardous {
                    ("ПОТЕНЦИАЛЬНО ОПАСЕН", color(255, 130, 130))
                } else {
                    ("БЕЗОПАСНО", color(105, 214, 161))
                };
                ui.label(
                    egui::RichText::new(badge)
                        .size(10.0)
                        .strong()
                        .color(badge_color),
                );
            });
            ui.add_space(12.0);
            ui.columns(3, |columns| {
                metric(
                    &mut columns[0],
                    "ДИАМЕТР, КМ",
                    &format!(
                        "{} — {}",
                        number(asteroid.diameter_min_km),
                        number(asteroid.diameter_max_km)
                    ),
                );
                metric(
                    &mut columns[1],
                    "СКОРОСТЬ, КМ/С",
                    asteroid.velocity_km_s.as_deref().unwrap_or("—"),
                );
                metric(
                    &mut columns[2],
                    "МИН. РАССТОЯНИЕ, КМ",
                    asteroid.miss_distance_km.as_deref().unwrap_or("—"),
                );
            });
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new(format!(
                    "Сближение (UTC): {}",
                    asteroid.approach_time_utc.as_deref().unwrap_or("—")
                ))
                .size(11.0)
                .color(color(152, 171, 194)),
            );
        });
}

fn metric(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.vertical(|ui| {
        ui.label(
            egui::RichText::new(label)
                .size(10.0)
                .strong()
                .color(color(136, 157, 181)),
        );
        ui.label(
            egui::RichText::new(value)
                .size(14.0)
                .color(color(220, 232, 246)),
        );
    });
}

fn number(value: Option<f64>) -> String {
    value
        .map(|number| format!("{number:.6}"))
        .unwrap_or_else(|| "—".to_owned())
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
    fn uses_cache_only_for_the_requested_date_after_fetch_failure() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("cache.json");
        cache::write(&path, date(12), &[asteroid("Cached")]).unwrap();

        let loaded = fetch_and_cache(date(12), Ok(path.clone()), false, |_| {
            anyhow::bail!("network unavailable")
        })
        .unwrap();
        assert_eq!(loaded.asteroids, vec![asteroid("Cached")]);
        assert!(loaded.source.contains("локальный кэш"));

        let error = fetch_and_cache(date(13), Ok(path), false, |_| {
            anyhow::bail!("network unavailable")
        })
        .unwrap_err()
        .to_string();
        assert!(error.contains("кэше нет"));
    }

    #[test]
    fn empty_network_result_is_success_and_cache_write_failure_is_reported() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("cache.json");
        let loaded =
            fetch_and_cache(date(12), Ok(path.clone()), false, |_| Ok(Vec::new())).unwrap();
        assert!(loaded.asteroids.is_empty());
        assert_eq!(cache::read(&path, date(12)).unwrap(), Some(Vec::new()));

        let blocker = directory.path().join("file");
        std::fs::write(&blocker, "not a directory").unwrap();
        let loaded = fetch_and_cache(date(12), Ok(blocker.join("cache.json")), false, |_| {
            Ok(vec![asteroid("Online")])
        })
        .unwrap();
        assert_eq!(loaded.asteroids, vec![asteroid("Online")]);
        assert!(loaded.source.contains("Не удалось сохранить кэш"));
    }

    #[test]
    fn fetch_success_is_not_blocked_by_unavailable_cache_directory() {
        let directory = tempfile::tempdir().unwrap();
        let blocker = directory.path().join("file");
        std::fs::write(&blocker, "not a directory").unwrap();

        let loaded = fetch_and_cache(
            date(12),
            Err(anyhow::anyhow!("data directory unavailable")),
            false,
            |_| Ok(vec![asteroid("Online")]),
        )
        .unwrap();
        assert_eq!(loaded.asteroids, vec![asteroid("Online")]);
        assert!(loaded.source.contains("Не удалось определить папку кэша"));
    }
}
