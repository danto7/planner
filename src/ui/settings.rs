//! Connection and display settings.

use crate::app::{Action, PlannerApp};
use crate::config::Config;
use egui::{Context, RichText};
use std::collections::HashMap;

#[derive(Default)]
pub struct SettingsState {
    pub open: bool,
    pub draft: Config,
    pub task_enabled: HashMap<String, bool>,
    pub event_enabled: HashMap<String, bool>,
    pub test_message: Option<String>,
}

impl SettingsState {
    pub fn open_with(&mut self, config: &Config, calendars: &[crate::model::Calendar]) {
        self.open = true;
        self.draft = config.clone();
        self.test_message = None;
        self.task_enabled.clear();
        self.event_enabled.clear();
        for c in calendars {
            self.task_enabled
                .insert(c.url.clone(), config.uses_task_calendar(&c.url));
            self.event_enabled
                .insert(c.url.clone(), config.uses_event_calendar(&c.url));
        }
    }

    fn finish(&self, calendars: &[crate::model::Calendar]) -> Config {
        let mut cfg = self.draft.clone();
        cfg.server_url = cfg.server_url.trim().to_string();
        cfg.username = cfg.username.trim().to_string();
        let todo_cals: Vec<&str> = calendars
            .iter()
            .filter(|c| c.supports_todo)
            .map(|c| c.url.as_str())
            .collect();
        let event_cals: Vec<&str> = calendars
            .iter()
            .filter(|c| c.supports_event)
            .map(|c| c.url.as_str())
            .collect();
        let enabled = |map: &HashMap<String, bool>, all: &[&str]| -> Vec<String> {
            let on: Vec<String> = all
                .iter()
                .filter(|u| map.get(**u).copied().unwrap_or(true))
                .map(|u| u.to_string())
                .collect();
            if on.len() == all.len() {
                Vec::new()
            } else {
                on
            }
        };
        if !calendars.is_empty() {
            cfg.task_calendars = enabled(&self.task_enabled, &todo_cals);
            cfg.event_calendars = enabled(&self.event_enabled, &event_cals);
        }
        cfg.day_end_hour = cfg.day_end_hour.clamp(cfg.day_start_hour + 1, 24);
        cfg
    }
}

pub fn show(app: &mut PlannerApp, ctx: &Context) {
    if !app.settings.open {
        return;
    }
    let mut open = true;
    let mut result: Option<Config> = None;
    let mut close = false;
    let calendars = app.calendars.clone();
    let config_path = Config::path();
    let first_run = !app.config.is_configured();
    let settings = &mut app.settings;

    egui::Window::new("Settings")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(460.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            if first_run {
                ui.label(RichText::new("Welcome! Connect your CalDAV account to load tasks and calendars.").strong());
                ui.add_space(6.0);
            }
            ui.heading("CalDAV server");
            egui::Grid::new("conn").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                ui.label("Server URL");
                ui.add(
                    egui::TextEdit::singleline(&mut settings.draft.server_url)
                        .hint_text("https://cloud.example.com/remote.php/dav")
                        .desired_width(320.0),
                );
                ui.end_row();
                ui.label("Username");
                ui.add(egui::TextEdit::singleline(&mut settings.draft.username).desired_width(320.0));
                ui.end_row();
                ui.label("Password");
                ui.add(
                    egui::TextEdit::singleline(&mut settings.draft.password)
                        .password(true)
                        .desired_width(320.0),
                );
                ui.end_row();
            });
            ui.weak("Nextcloud, Radicale, Baïkal, iCloud, Fastmail and other CalDAV servers work. Use an app password where your provider offers one.");

            if !calendars.is_empty() {
                ui.add_space(10.0);
                ui.heading("Calendars");
                egui::Grid::new("cals").num_columns(4).spacing([12.0, 4.0]).striped(true).show(ui, |ui| {
                    ui.strong("Name");
                    ui.strong("Tasks");
                    ui.strong("Events");
                    ui.strong("New tasks go here");
                    ui.end_row();
                    for c in &calendars {
                        ui.horizontal(|ui| {
                            if let Some([r, g, b]) = c.rgb() {
                                let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                                ui.painter().circle_filled(rect.center(), 5.0, egui::Color32::from_rgb(r, g, b));
                            }
                            ui.label(&c.name);
                        });
                        if c.supports_todo {
                            let v = settings.task_enabled.entry(c.url.clone()).or_insert(true);
                            ui.checkbox(v, "");
                        } else {
                            ui.weak("–");
                        }
                        if c.supports_event {
                            let v = settings.event_enabled.entry(c.url.clone()).or_insert(true);
                            ui.checkbox(v, "");
                        } else {
                            ui.weak("–");
                        }
                        if c.supports_todo {
                            ui.radio_value(&mut settings.draft.default_task_calendar, c.url.clone(), "");
                        } else {
                            ui.label("");
                        }
                        ui.end_row();
                    }
                });
            }

            ui.add_space(10.0);
            ui.heading("Week view");
            ui.horizontal(|ui| {
                ui.label("Show hours from");
                ui.add(egui::DragValue::new(&mut settings.draft.day_start_hour).range(0..=23));
                ui.label("to");
                ui.add(egui::DragValue::new(&mut settings.draft.day_end_hour).range(1..=24));
                ui.separator();
                ui.label("Default task length");
                ui.add(egui::DragValue::new(&mut settings.draft.default_task_minutes).range(5..=480).suffix(" min"));
            });
            ui.checkbox(&mut settings.draft.show_completed, "Show completed tasks");

            ui.add_space(8.0);
            ui.weak(format!("Saved to {}", config_path.display()));
            ui.add_space(8.0);
            ui.separator();
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(RichText::new("Save & connect").strong())).clicked() {
                    result = Some(settings.finish(&calendars));
                    close = true;
                }
                if !first_run && ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });

    if let Some(cfg) = result {
        app.queue(Action::SaveSettings(cfg));
    }
    if close || !open {
        app.settings.open = false;
    }
}
