#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod caldav;
mod config;
mod ical;
mod model;
mod sync;
mod ui;

fn main() -> eframe::Result {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,planner=debug"),
    )
    .init();
    // reqwest is built without a bundled TLS provider so that no cmake is needed.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Week Planner")
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Week Planner",
        options,
        Box::new(|cc| Ok(Box::new(app::PlannerApp::new(cc)))),
    )
}
