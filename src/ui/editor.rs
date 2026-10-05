//! Task dialog: a modal with Cancel / Save in its header bar.

use super::dialogs::dialog;
use super::theme::{self, palette};
use crate::app::{Action, PlannerApp};
use crate::ical::local_midnight;
use crate::model::{Task, When};
use chrono::{Duration, NaiveDate, NaiveTime, Timelike};
use egui::{Context, Margin, RichText};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Inbox,
    Day,
    Time,
}

#[derive(Debug, Clone)]
pub struct TaskEditor {
    pub uid: String,
    pub summary: String,
    pub description: String,
    pub mode: Mode,
    pub date: String,
    pub time: String,
    pub duration_min: i64,
    pub priority: u8,
    pub completed: bool,
    pub error: Option<String>,
    pub confirm_delete: bool,
}

impl TaskEditor {
    pub fn from_task(task: &Task, default_minutes: i64) -> Self {
        let nine = NaiveTime::from_hms_opt(9, 0, 0).unwrap();
        let (mode, date, time, duration) = match task.when {
            None => (
                Mode::Inbox,
                chrono::Local::now().date_naive(),
                nine,
                default_minutes,
            ),
            Some(When::Day(d)) => (Mode::Day, d, nine, default_minutes),
            Some(When::At { start, duration }) => (
                Mode::Time,
                start.date_naive(),
                start.time(),
                duration.num_minutes(),
            ),
        };
        Self {
            uid: task.uid.clone(),
            summary: task.summary.clone(),
            description: task.description.clone(),
            mode,
            date: date.format("%Y-%m-%d").to_string(),
            time: time.format("%H:%M").to_string(),
            duration_min: duration.max(5),
            priority: task.priority,
            completed: task.completed,
            error: None,
            confirm_delete: false,
        }
    }

    pub fn when(&self) -> Result<Option<When>, String> {
        match self.mode {
            Mode::Inbox => Ok(None),
            Mode::Day => Ok(Some(When::Day(self.parse_date()?))),
            Mode::Time => {
                let date = self.parse_date()?;
                let time = NaiveTime::parse_from_str(self.time.trim(), "%H:%M")
                    .or_else(|_| NaiveTime::parse_from_str(self.time.trim(), "%H:%M:%S"))
                    .map_err(|_| format!("Time “{}” is not in HH:MM format", self.time))?;
                let start = local_midnight(date)
                    + Duration::seconds(time.num_seconds_from_midnight() as i64);
                Ok(Some(When::At {
                    start,
                    duration: Duration::minutes(self.duration_min.max(5)),
                }))
            }
        }
    }

    fn parse_date(&self) -> Result<NaiveDate, String> {
        NaiveDate::parse_from_str(self.date.trim(), "%Y-%m-%d")
            .map_err(|_| format!("Date “{}” is not in YYYY-MM-DD format", self.date))
    }
}

pub fn show(app: &mut PlannerApp, ctx: &Context) {
    let Some(editor) = app.editor.as_mut() else {
        return;
    };
    let mut close = false;
    let mut action: Option<Action> = None;
    let mut save = ctx.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.command);

    let (_, backdrop) = dialog(ctx, "task-editor", "Edit Task", 440.0, |start, end, ui| {
        if theme::flat_button(start, "Cancel").clicked() {
            close = true;
        }
        if theme::suggested_button(end, "Save").clicked() {
            save = true;
        }
        let p = palette(ui);
        ui.add(
            egui::TextEdit::singleline(&mut editor.summary)
                .hint_text("Task name")
                .font(egui::FontId::proportional(16.0))
                .margin(Margin::symmetric(10, 8))
                .desired_width(f32::INFINITY),
        );
        ui.add_space(6.0);
        ui.add(
            egui::TextEdit::multiline(&mut editor.description)
                .hint_text("Notes")
                .desired_rows(3)
                .margin(Margin::symmetric(10, 8))
                .desired_width(f32::INFINITY),
        );
        ui.add_space(12.0);

        theme::group_title(ui, "Plan");
        ui.add_space(6.0);
        theme::card_frame(ui).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            theme::list_row(ui, true, |ui| {
                ui.horizontal(|ui| {
                    ui.label("When");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        toggle_group(
                            ui,
                            &mut editor.mode,
                            &[
                                (Mode::Time, "Time slot"),
                                (Mode::Day, "Day"),
                                (Mode::Inbox, "Inbox"),
                            ],
                        );
                    });
                });
            });
            if editor.mode != Mode::Inbox {
                theme::list_row(ui, false, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Date");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut editor.date)
                                    .desired_width(110.0)
                                    .margin(Margin::symmetric(8, 5)),
                            );
                        });
                    });
                });
            }
            if editor.mode == Mode::Time {
                theme::list_row(ui, false, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Time");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut editor.time)
                                    .desired_width(64.0)
                                    .margin(Margin::symmetric(8, 5)),
                            );
                        });
                    });
                });
                theme::list_row(ui, false, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Duration");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(
                                egui::DragValue::new(&mut editor.duration_min)
                                    .range(5..=720)
                                    .speed(5)
                                    .suffix(" min"),
                            );
                        });
                    });
                });
            }
            theme::list_row(ui, false, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Priority");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let mut level = match editor.priority {
                            0 => 0u8,
                            1..=4 => 1,
                            5 => 2,
                            _ => 3,
                        };
                        if toggle_group(
                            ui,
                            &mut level,
                            &[(3, "Low"), (2, "Medium"), (1, "High"), (0, "None")],
                        ) {
                            editor.priority = [0, 1, 5, 9][level as usize];
                        }
                    });
                });
            });
            theme::list_row(ui, false, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Completed");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        theme::switch(ui, &mut editor.completed);
                    });
                });
            });
        });

        if let Some(err) = &editor.error {
            ui.add_space(8.0);
            ui.colored_label(p.error, err);
        }
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            if editor.confirm_delete {
                if theme::destructive_button(ui, "Delete").clicked() {
                    action = Some(Action::Delete(editor.uid.clone()));
                    close = true;
                }
                if theme::flat_button(ui, "Keep").clicked() {
                    editor.confirm_delete = false;
                }
                ui.label(
                    RichText::new("This cannot be undone.")
                        .color(p.dim_fg)
                        .size(12.0),
                );
            } else if ui
                .add(
                    egui::Button::new(RichText::new("Delete Task…").color(p.destructive))
                        .frame(false),
                )
                .clicked()
            {
                editor.confirm_delete = true;
            }
        });
    });

    if save {
        if editor.summary.trim().is_empty() {
            editor.error = Some("Enter a name for the task".into());
        } else {
            match editor.when() {
                Ok(when) => {
                    action = Some(Action::ApplyEditor {
                        editor: editor.clone(),
                        when,
                    });
                    close = true;
                }
                Err(e) => editor.error = Some(e),
            }
        }
    }
    if let Some(a) = action {
        app.queue(a);
    }
    if close || backdrop {
        app.editor = None;
    }
}

/// A linked group of toggle buttons. Items are given right-to-left when the
/// layout is right-to-left, so pass them in the order they should appear from the end.
fn toggle_group<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    value: &mut T,
    items: &[(T, &str)],
) -> bool {
    let p = palette(ui);
    let mut changed = false;
    egui::Frame::new()
        .fill(p.button_bg)
        .corner_radius(8.0)
        .inner_margin(Margin::same(2))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (item, label) in items {
                let selected = *value == *item;
                let fill = if selected {
                    p.card_bg
                } else {
                    egui::Color32::TRANSPARENT
                };
                let button = egui::Button::new(RichText::new(*label).size(13.0))
                    .fill(fill)
                    .corner_radius(6.0)
                    .min_size(egui::vec2(0.0, 26.0));
                if ui.add(button).clicked() && !selected {
                    *value = *item;
                    changed = true;
                }
            }
        });
    changed
}
