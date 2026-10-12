# NASA Near Earth Objects Tracker (NASA NEO Tracker)

![Rust](https://img.shields.io/badge/RUST-%23000000.svg?style=for-the-badge&logo=rust&logoColor=white)
![Git](https://img.shields.io/badge/GIT-%23F05032.svg?style=for-the-badge&logo=git&logoColor=white)
![VS Code](https://img.shields.io/badge/VS%20CODE-%23007ACC.svg?style=for-the-badge&logo=visual-studio-code&logoColor=white)
![NASA API](https://img.shields.io/badge/NASA%20API-%230B3D91.svg?style=for-the-badge&logo=nasa&logoColor=white)

Десктопное приложение для мониторинга околоземных астероидов, разработанное с использованием методологии **Spec-Driven Development** (через GitHub Spec Kit) и собранное с помощью **Google Antigravity**.

---

## 🚀 Описание проекта
Приложение осуществляет мониторинг астероидов, пролетающих близко к Земле в выбранную дату, используя данные **NASA NeoWs API** и эфемериды **JPL Horizons**. 

### Ключевые возможности:
* **Интерактивный интерфейс:** Выбор даты, отображение детальной информации по астероидам (диаметр, скорость, расстояние до Земли, статус потенциальной опасности).
* **Отказоустойчивость и кэширование:** При отсутствии сети приложение не падает, автоматически переключаясь на последний успешно закэсированный день с указанием даты.
* **Валидация ввода:** Проверка формата дат (защита от несуществующих дней, например, 31 февраля).
* **Надежные тесты:** Покрытие unit-тестами логики сетевых запросов, разбора данных и валидации.

---

## 🛠 Стек технологий
* **Язык:** Rust (2021 edition)
* **Интерфейс:** `eframe` / `egui`
* **Сеть и данные:** `reqwest`, `serde`, `serde_json`, `chrono`
* **Контроль версий:** Git (ветвление, Pull Requests, осмысленные коммиты)

---

## ⚙️ Запуск и тестирование

### Запуск приложения
```bash
cargo run
