//! Sidebar: task capture entry, overdue tasks and the unscheduled inbox,
//! laid out as GNOME boxed lists.

use super::theme::{self, palette, Icon};
use super::widgets::{drag_item, drop_zone, DragTask};
use crate::app::{Action, PlannerApp};
use crate::model::Task;
use chrono::Local;
use egui::{Frame, Id, Key, Layout, Margin, RichText, Ui};

pub fn show(app: &mut PlannerApp, ui: &mut Ui) {
    let p = palette(ui);
    Frame::new()
        .inner_margin(Margin {
            left: 12,
            right: 12,
            top: 12,
            bottom: 6,
        })
        .show(ui, |ui| {
            list_picker(app, ui);
            ui.add_space(8.0);
            capture_box(app, ui);
        });

    let today = Local::now().date_naive();
    let (_, dropped) = drop_zone(ui, Frame::NONE, |ui| {
        ui.set_min_height(ui.available_height());
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                Frame::new()
                    .inner_margin(Margin::symmetric(12, 6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let mut overdue: Vec<Task> = app
                            .tasks
                            .iter()
                            .filter(|t| {
                                !t.completed && t.when.map(|w| w.date() < today).unwrap_or(false)
                            })
                            .cloned()
                            .collect();
                        overdue.sort_by_key(|t| t.when.map(|w| w.date()));
                        if !overdue.is_empty() {
                            ui.label(
                                RichText::new("Overdue")
                                    .font(theme::title_font(13.0))
                                    .color(p.error),
                            );
                            ui.add_space(6.0);
                            boxed_task_list(app, ui, &overdue, true);
                            ui.add_space(18.0);
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
                                .then_with(|| {
                                    a.summary.to_lowercase().cmp(&b.summary.to_lowercase())
                                })
                        });
                        let open = inbox.iter().filter(|t| !t.completed).count();
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Unscheduled").font(theme::title_font(13.0)));
                            ui.label(RichText::new(open.to_string()).color(p.dim_fg).size(12.0));
                        });
                        ui.add_space(6.0);
                        if inbox.is_empty() {
                            theme::status_page(
                                ui,
                                Icon::Check,
                                "Inbox Is Empty",
                                "Add a task above, or drag a planned task here to unschedule it.",
                            );
                        } else {
                            boxed_task_list(app, ui, &inbox, false);
                        }
                        ui.add_space(24.0);
                    });
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

/// Sidebar title: the name of the task list being shown, as a dropdown when
/// the account has several lists.
fn list_picker(app: &mut PlannerApp, ui: &mut Ui) {
    let lists = app.task_lists();
    let current = app.task_list();
    let title = current
        .as_ref()
        .map(|c| c.name.clone())
        .unwrap_or_else(|| "Inbox".to_string());
    if lists.len() <= 1 {
        ui.label(RichText::new(title).font(theme::title_font(17.0)));
        return;
    }
    let mut selected = current.map(|c| c.url).unwrap_or_default();
    let response = egui::ComboBox::from_id_salt("task-list")
        .selected_text(RichText::new(title).font(theme::title_font(17.0)))
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for c in &lists {
                ui.selectable_value(&mut selected, c.url.clone(), &c.name);
            }
        });
    response
        .response
        .on_hover_text("Choose which task list to plan");
    if selected != app.config.task_list && lists.iter().any(|c| c.url == selected) {
        app.queue(Action::SelectTaskList(selected));
    }
}

fn capture_box(app: &mut PlannerApp, ui: &mut Ui) {
    let can_add = app.task_list().is_some();

    let mut submitted = false;
    ui.horizontal(|ui| {
        let entry_width = ui.available_width() - 34.0 - ui.spacing().item_spacing.x;
        let response = ui.add_enabled(
            can_add,
            egui::TextEdit::singleline(&mut app.new_task)
                .hint_text(if can_add {
                    "New task"
                } else {
                    "Connect an account to add tasks"
                })
                .desired_width(entry_width)
                .margin(Margin::symmetric(10, 8)),
        );
        if app.focus_new_task {
            response.request_focus();
            app.focus_new_task = false;
        }
        submitted = response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
        let add_enabled = can_add && !app.new_task.trim().is_empty();
        if theme::icon_button_enabled(ui, Icon::Plus, "Add task", add_enabled).clicked() {
            submitted = true;
        }
        if submitted && add_enabled {
            let summary = std::mem::take(&mut app.new_task);
            app.queue(Action::Create { summary });
            response.request_focus();
        }
    });
}

fn boxed_task_list(app: &mut PlannerApp, ui: &mut Ui, tasks: &[Task], show_date: bool) {
    theme::card_frame(ui).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.set_width(ui.available_width());
        for (i, task) in tasks.iter().enumerate() {
            theme::list_row(ui, i == 0, |ui| task_row(app, ui, task, show_date));
        }
    });
}

fn task_row(app: &mut PlannerApp, ui: &mut Ui, task: &Task, show_date: bool) {
    let p = palette(ui);
    let uid = task.uid.clone();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        let mut done = task.completed;
        if theme::check_button(ui, &mut done).changed() {
            app.queue(Action::SetCompleted {
                uid: uid.clone(),
                completed: done,
            });
        }
        let width = ui.available_width();
        let response = drag_item(
            ui,
            Id::new(("inbox-task", &uid)),
            DragTask { uid: uid.clone() },
            |ui| {
                ui.set_width(width.max(0.0));
                ui.horizontal(|ui| {
                    if task.priority >= 1 && task.priority <= 4 {
                        let (r, _) =
                            ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                        ui.painter().circle_filled(r.center(), 4.0, p.error);
                    }
                    let mut text = RichText::new(&task.summary);
                    if task.completed {
                        text = text.strikethrough().color(p.dim_fg);
                    }
                    ui.add(egui::Label::new(text).truncate().selectable(false));
                    if show_date {
                        if let Some(w) = task.when {
                            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(
                                    RichText::new(w.date().format("%-d %b").to_string())
                                        .color(p.dim_fg)
                                        .size(12.0),
                                );
                            });
                        }
                    }
                });
            },
        );
        if response.clicked() {
            app.queue(Action::Open(uid.clone()));
        }
        if !task.description.trim().is_empty() {
            response.on_hover_text(task.description.trim());
        }
    });
}
