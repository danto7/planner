//! Preferences dialog: account, calendars and week-view groups as boxed lists.

use super::dialogs::dialog;
use super::theme::{self, palette};
use crate::app::{Action, PlannerApp};
use crate::config::Config;
use crate::model::Calendar;
use egui::{Context, Layout, Margin, RichText};
use std::collections::HashMap;

#[derive(Default)]
pub struct SettingsState {
    pub open: bool,
    pub draft: Config,
    pub task_enabled: HashMap<String, bool>,
    pub event_enabled: HashMap<String, bool>,
}

impl SettingsState {
    pub fn open_with(&mut self, config: &Config, calendars: &[Calendar]) {
        self.open = true;
        self.draft = config.clone();
        self.task_enabled.clear();
        self.event_enabled.clear();
        for c in calendars {
            self.task_enabled
                .insert(c.url.clone(), config.uses_task_calendar(&c.url));
            self.event_enabled
                .insert(c.url.clone(), config.uses_event_calendar(&c.url));
        }
    }

    fn finish(&self, calendars: &[Calendar]) -> Config {
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

fn entry_row(
    ui: &mut egui::Ui,
    first: bool,
    label: &str,
    value: &mut String,
    hint: &str,
    password: bool,
) {
    theme::list_row(ui, first, |ui| {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(
                    egui::TextEdit::singleline(value)
                        .hint_text(hint)
                        .password(password)
                        .desired_width(240.0)
                        .margin(Margin::symmetric(8, 5)),
                );
            });
        });
    });
}

fn switch_row(
    ui: &mut egui::Ui,
    first: bool,
    label: &str,
    subtitle: Option<&str>,
    value: &mut bool,
) -> bool {
    let p = palette(ui);
    theme::list_row(ui, first, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(label);
                if let Some(s) = subtitle {
                    ui.label(RichText::new(s).color(p.dim_fg).size(12.0));
                }
            });
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                theme::switch(ui, value).changed()
            })
            .inner
        })
        .inner
    })
    .inner
}

fn spin_row(
    ui: &mut egui::Ui,
    first: bool,
    label: &str,
    value: &mut u32,
    range: std::ops::RangeInclusive<u32>,
    suffix: &str,
) {
    theme::list_row(ui, first, |ui| {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(egui::DragValue::new(value).range(range).suffix(suffix));
            });
        });
    });
}

pub fn show(app: &mut PlannerApp, ctx: &Context) {
    if !app.settings.open {
        return;
    }
    let mut result: Option<Config> = None;
    let mut close = false;
    let calendars = app.calendars.clone();
    let first_run = !app.config.is_configured();
    let settings = &mut app.settings;

    let (_, backdrop) = dialog(
        ctx,
        "preferences",
        "Preferences",
        520.0,
        |start, end, ui| {
            if !first_run && theme::flat_button(start, "Cancel").clicked() {
                close = true;
            }
            let apply = theme::suggested_button(end, "Apply").clicked();
            let p = palette(ui);
            egui::ScrollArea::vertical().max_height(560.0).auto_shrink([false, true]).show(ui, |ui| {
                ui.set_width(ui.available_width());
                if first_run {
                    ui.label("Connect a CalDAV account to load your tasks and calendars.");
                    ui.add_space(12.0);
                }
                theme::group_title(ui, "Account");
                ui.add_space(6.0);
                theme::card_frame(ui).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    entry_row(ui, true, "Server", &mut settings.draft.server_url, "https://cloud.example.com/remote.php/dav", false);
                    entry_row(ui, false, "Username", &mut settings.draft.username, "", false);
                    entry_row(ui, false, "Password", &mut settings.draft.password, "", true);
                });
                ui.add_space(4.0);
                ui.add(egui::Label::new(RichText::new(
                    "Works with Nextcloud, Radicale, Baïkal, iCloud, Fastmail and other CalDAV servers. Use an app password where your provider offers one.",
                ).color(p.dim_fg).size(12.0)).wrap());
                ui.add_space(18.0);

                if !calendars.is_empty() {
                    theme::group_title(ui, "Calendars");
                    ui.add_space(6.0);
                    theme::card_frame(ui).show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for (i, c) in calendars.iter().enumerate() {
                            theme::list_row(ui, i == 0, |ui| {
                                ui.horizontal(|ui| {
                                    if let Some([r, g, b]) = c.rgb() {
                                        let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                                        ui.painter().circle_filled(rect.center(), 6.0, egui::Color32::from_rgb(r, g, b));
                                    }
                                    ui.label(&c.name);
                                    ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                                        if c.supports_event {
                                            let v = settings.event_enabled.entry(c.url.clone()).or_insert(true);
                                            theme::switch(ui, v);
                                            ui.label(RichText::new("Events").color(p.dim_fg).size(12.0));
                                            ui.add_space(8.0);
                                        }
                                        if c.supports_todo {
                                            let v = settings.task_enabled.entry(c.url.clone()).or_insert(true);
                                            theme::switch(ui, v);
                                            ui.label(RichText::new("Tasks").color(p.dim_fg).size(12.0));
                                        }
                                    });
                                });
                            });
                        }
                        let task_cals: Vec<&Calendar> = calendars.iter().filter(|c| c.supports_todo).collect();
                        if task_cals.len() > 1 {
                            theme::list_row(ui, false, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label("Add new tasks to");
                                    ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                                        let name = task_cals
                                            .iter()
                                            .find(|c| c.url == settings.draft.default_task_calendar)
                                            .map(|c| c.name.clone())
                                            .unwrap_or_else(|| "First calendar".into());
                                        egui::ComboBox::from_id_salt("default_task_calendar")
                                            .selected_text(name)
                                            .show_ui(ui, |ui| {
                                                for c in &task_cals {
                                                    ui.selectable_value(&mut settings.draft.default_task_calendar, c.url.clone(), &c.name);
                                                }
                                            });
                                    });
                                });
                            });
                        }
                    });
                    ui.add_space(18.0);
                }

                theme::group_title(ui, "Week View");
                ui.add_space(6.0);
                theme::card_frame(ui).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    spin_row(ui, true, "Day starts at", &mut settings.draft.day_start_hour, 0..=23, ":00");
                    spin_row(ui, false, "Day ends at", &mut settings.draft.day_end_hour, 1..=24, ":00");
                    let mut minutes = settings.draft.default_task_minutes.clamp(5, 480) as u32;
                    spin_row(ui, false, "Default task length", &mut minutes, 5..=480, " min");
                    settings.draft.default_task_minutes = minutes as i64;
                    switch_row(ui, false, "Show completed tasks", None, &mut settings.draft.show_completed);
                });
                ui.add_space(8.0);
                ui.label(RichText::new(format!("Stored in {}", Config::path().display())).color(p.dim_fg).size(12.0));
            });
            if apply {
                result = Some(settings.finish(&calendars));
                close = true;
            }
        },
    );

    if let Some(cfg) = result {
        app.queue(Action::SaveSettings(cfg));
    }
    if close || (backdrop && !first_run) {
        app.settings.open = false;
    }
}
