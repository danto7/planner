//! Left panel: task capture box, overdue tasks and the unscheduled inbox.

use super::widgets::{
    calendar_color, drag_item, drop_zone, find_calendar, tint, DragTask, DEFAULT_TASK_COLOR,
};
use crate::app::{Action, PlannerApp};
use crate::model::Task;
use chrono::Local;
use egui::{Frame, Id, Key, Margin, RichText, Ui};

pub fn show(app: &mut PlannerApp, ui: &mut Ui) {
    ui.add_space(6.0);
    ui.heading("Inbox");
    ui.add_space(4.0);

    capture_box(app, ui);
    ui.add_space(6.0);
    ui.separator();

    let today = Local::now().date_naive();
    let dark = ui.visuals().dark_mode;
    let (_, dropped) = drop_zone(ui, Frame::NONE, |ui| {
        ui.set_min_height(ui.available_height());
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut overdue: Vec<Task> = app
                    .tasks
                    .iter()
                    .filter(|t| !t.completed && t.when.map(|w| w.date() < today).unwrap_or(false))
                    .cloned()
                    .collect();
                overdue.sort_by_key(|t| t.when.map(|w| w.date()));
                if !overdue.is_empty() {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(format!("Overdue ({})", overdue.len()))
                            .strong()
                            .color(egui::Color32::from_rgb(220, 80, 60)),
                    );
                    for task in &overdue {
                        let hint = task.when.map(|w| w.date().format("%a %-d %b").to_string());
                        task_row(app, ui, task, hint, dark);
                    }
                    ui.add_space(8.0);
                    ui.separator();
                }

                let mut inbox: Vec<Task> = app
                    .tasks
                    .iter()
                    .filter(|t| t.is_inbox() && (app.config.show_completed || !t.completed))
                    .cloned()
                    .collect();
                inbox.sort_by(|a, b| {
                    a.completed
                        .cmp(&b.completed)
                        .then(prio_key(a.priority).cmp(&prio_key(b.priority)))
                        .then_with(|| a.summary.to_lowercase().cmp(&b.summary.to_lowercase()))
                });
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!(
                        "Unscheduled ({})",
                        inbox.iter().filter(|t| !t.completed).count()
                    ))
                    .strong(),
                );
                if inbox.is_empty() {
                    ui.add_space(12.0);
                    ui.weak("Nothing waiting. Add a task above or drag one here to unschedule it.");
                }
                for task in &inbox {
                    task_row(app, ui, task, None, dark);
                }
                ui.add_space(20.0);
            });
    });
    if let Some(payload) = dropped {
        app.queue(Action::Schedule {
            uid: payload.uid.clone(),
            when: None,
        });
    }
}

fn prio_key(p: u8) -> u8 {
    if p == 0 {
        10
    } else {
        p
    }
}

fn capture_box(app: &mut PlannerApp, ui: &mut Ui) {
    let calendars: Vec<_> = app
        .calendars
        .iter()
        .filter(|c| c.supports_todo && app.config.uses_task_calendar(&c.url))
        .cloned()
        .collect();
    let can_add = !calendars.is_empty();

    let response = ui.add_enabled(
        can_add,
        egui::TextEdit::singleline(&mut app.new_task)
            .hint_text(if can_add {
                "New task… (Enter to add)"
            } else {
                "Connect a CalDAV server to add tasks"
            })
            .desired_width(f32::INFINITY),
    );
    let submitted = response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));

    ui.horizontal(|ui| {
        if calendars.len() > 1 {
            let selected_name = calendars
                .iter()
                .find(|c| c.url == app.new_task_calendar)
                .map(|c| c.name.clone())
                .unwrap_or_else(|| "Calendar".into());
            egui::ComboBox::from_id_salt("new_task_calendar")
                .selected_text(selected_name)
                .width(ui.available_width() - 70.0)
                .show_ui(ui, |ui| {
                    for c in &calendars {
                        ui.selectable_value(&mut app.new_task_calendar, c.url.clone(), &c.name);
                    }
                });
        } else if let Some(c) = calendars.first() {
            ui.weak(format!("in {}", c.name));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let clicked = ui
                .add_enabled(
                    can_add && !app.new_task.trim().is_empty(),
                    egui::Button::new("Add"),
                )
                .clicked();
            if (clicked || submitted) && !app.new_task.trim().is_empty() {
                let summary = std::mem::take(&mut app.new_task);
                app.queue(Action::Create { summary });
                response.request_focus();
            }
        });
    });
}

fn task_row(app: &mut PlannerApp, ui: &mut Ui, task: &Task, hint: Option<String>, dark: bool) {
    let accent = calendar_color(
        find_calendar(&app.calendars, &task.calendar),
        DEFAULT_TASK_COLOR,
    );
    let uid = task.uid.clone();
    ui.horizontal(|ui| {
        let mut done = task.completed;
        if ui.checkbox(&mut done, "").changed() {
            app.queue(Action::SetCompleted {
                uid: uid.clone(),
                completed: done,
            });
        }
        let id = Id::new(("inbox-task", &uid));
        let width = ui.available_width();
        let response = drag_item(ui, id, DragTask { uid: uid.clone() }, |ui| {
            Frame::new()
                .fill(tint(accent, dark))
                .inner_margin(Margin::symmetric(8, 5))
                .corner_radius(4)
                .show(ui, |ui| {
                    ui.set_width((width - 16.0).max(0.0));
                    ui.horizontal(|ui| {
                        let mut text = RichText::new(&task.summary);
                        if task.completed {
                            text = text.strikethrough().weak();
                        }
                        if task.priority >= 1 && task.priority <= 4 {
                            ui.label(
                                RichText::new("!")
                                    .strong()
                                    .color(egui::Color32::from_rgb(220, 80, 60)),
                            );
                        }
                        ui.add(egui::Label::new(text).truncate().selectable(false));
                        if let Some(h) = &hint {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.weak(h);
                                },
                            );
                        }
                    });
                });
        });
        if response.clicked() {
            app.queue(Action::Open(uid.clone()));
        }
        if !task.description.trim().is_empty() {
            response.on_hover_text(task.description.trim());
        }
    });
}
