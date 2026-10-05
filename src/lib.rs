//! Week Planner: a desktop week planner for CalDAV tasks and calendars.
//!
//! The crate is split into a library and a thin binary so that integration
//! tests can exercise the CalDAV client and model against a real server.

pub mod app;
pub mod caldav;
pub mod config;
pub mod ical;
pub mod model;
pub mod sync;
pub mod ui;

/// Start the desktop application.
pub fn run() -> eframe::Result {
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
