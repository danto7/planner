//! Modal task editor.

use crate::app::{Action, PlannerApp};
use crate::ical::local_midnight;
use crate::model::{Task, When};
use chrono::{Duration, NaiveDate, NaiveTime};
use egui::{Context, RichText};

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
        let (mode, date, time, duration) = match task.when {
            None => (
                Mode::Inbox,
                chrono::Local::now().date_naive(),
                NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
                default_minutes,
            ),
            Some(When::Day(d)) => (
                Mode::Day,
                d,
                NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
                default_minutes,
            ),
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
                    .map_err(|_| format!("time {:?} is not HH:MM", self.time))?;
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
            .map_err(|_| format!("date {:?} is not YYYY-MM-DD", self.date))
    }
}

use chrono::Timelike;

pub fn show(app: &mut PlannerApp, ctx: &Context) {
    let Some(editor) = app.editor.as_mut() else {
        return;
    };
    let mut open = true;
    let mut close = false;
    let mut action: Option<Action> = None;

    egui::Window::new("Task")
        .id(egui::Id::new("task-editor"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(380.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut editor.summary)
                    .hint_text("Summary")
                    .font(egui::TextStyle::Heading)
                    .desired_width(f32::INFINITY),
            );
            ui.add_space(4.0);
            ui.add(
                egui::TextEdit::multiline(&mut editor.description)
                    .hint_text("Notes")
                    .desired_rows(4)
                    .desired_width(f32::INFINITY),
            );
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label("Plan:");
                ui.selectable_value(&mut editor.mode, Mode::Inbox, "Inbox");
                ui.selectable_value(&mut editor.mode, Mode::Day, "Day");
                ui.selectable_value(&mut editor.mode, Mode::Time, "Time slot");
            });
            if editor.mode != Mode::Inbox {
                ui.horizontal(|ui| {
                    ui.label("Date");
                    ui.add(egui::TextEdit::singleline(&mut editor.date).desired_width(100.0));
                    if editor.mode == Mode::Time {
                        ui.label("at");
                        ui.add(egui::TextEdit::singleline(&mut editor.time).desired_width(56.0));
                        ui.label("for");
                        ui.add(
                            egui::DragValue::new(&mut editor.duration_min)
                                .range(5..=720)
                                .speed(5)
                                .suffix(" min"),
                        );
                    }
                });
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Priority");
                let labels = ["none", "high", "medium", "low"];
                let values = [0u8, 1, 5, 9];
                let current = match editor.priority {
                    0 => 0,
                    1..=4 => 1,
                    5 => 2,
                    _ => 3,
                };
                for (i, label) in labels.iter().enumerate() {
                    if ui.selectable_label(current == i, *label).clicked() {
                        editor.priority = values[i];
                    }
                }
                ui.separator();
                ui.checkbox(&mut editor.completed, "Completed");
            });

            if let Some(err) = &editor.error {
                ui.add_space(4.0);
                ui.colored_label(egui::Color32::from_rgb(220, 80, 60), err);
            }
            ui.add_space(10.0);
            ui.separator();
            ui.horizontal(|ui| {
                let save = ui.add(egui::Button::new(RichText::new("Save").strong()));
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.command);
                if save.clicked() || enter {
                    if editor.summary.trim().is_empty() {
                        editor.error = Some("a task needs a summary".into());
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
                if ui.button("Cancel").clicked() {
                    close = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if editor.confirm_delete {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Really delete").color(egui::Color32::WHITE),
                                )
                                .fill(egui::Color32::from_rgb(200, 60, 50)),
                            )
                            .clicked()
                        {
                            action = Some(Action::Delete(editor.uid.clone()));
                            close = true;
                        }
                        if ui.button("Keep").clicked() {
                            editor.confirm_delete = false;
                        }
                    } else if ui.button("Delete…").clicked() {
                        editor.confirm_delete = true;
                    }
                });
            });
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                close = true;
            }
        });

    if let Some(a) = action {
        app.queue(a);
    }
    if close || !open {
        app.editor = None;
    }
}
