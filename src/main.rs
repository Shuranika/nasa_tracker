mod app;
mod cache;
mod date;
mod models;
mod nasa;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "NASA Asteroid Tracker",
        options,
        Box::new(|_creation_context| Ok(Box::new(app::AsteroidApp::default()))),
    )
}
