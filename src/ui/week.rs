//! Central panel: the seven-day grid with an all-day row and hourly slots.

use super::theme::{self, palette};
use super::widgets::{
    calendar_color, drag_item, drop_zone, find_calendar, tint, DragTask, DEFAULT_EVENT_COLOR,
    DEFAULT_TASK_COLOR,
};
use crate::app::{Action, PlannerApp};
use crate::ical::local_midnight;
use crate::model::{Event, Task, When};
use chrono::{DateTime, Duration, Local, NaiveDate};
use egui::{
    Align2, FontId, Frame, Id, Margin, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui,
    UiBuilder, Vec2,
};

pub const HOUR_HEIGHT: f32 = 56.0;
const GUTTER: f32 = 48.0;
const GAP: f32 = 4.0;
const SNAP_MINUTES: i64 = 15;
const MIN_BLOCK_HEIGHT: f32 = 20.0;

pub fn show(app: &mut PlannerApp, ui: &mut Ui) {
    let days: Vec<NaiveDate> = (0..7).map(|i| app.week_start + Duration::days(i)).collect();
    let today = Local::now().date_naive();
    let col_w = ((ui.available_width() - GUTTER - 7.0 * GAP) / 7.0).max(70.0);

    header(ui, &days, today, col_w);
    ui.add_space(2.0);
    all_day_row(app, ui, &days, today, col_w);
    ui.add_space(2.0);
    ui.separator();

    egui::ScrollArea::vertical()
        .id_salt("week-grid")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            timed_grid(app, ui, &days, today, col_w);
        });
}

fn header(ui: &mut Ui, days: &[NaiveDate], today: NaiveDate, col_w: f32) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = GAP;
        ui.add_space(GUTTER - GAP);
        for &day in days {
            let is_today = day == today;
            ui.allocate_ui_with_layout(
                Vec2::new(col_w, 50.0),
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    ui.set_width(col_w);
                    let pal = palette(ui);
                    ui.spacing_mut().item_spacing.y = 2.0;
                    let mut name = RichText::new(day.format("%a").to_string().to_uppercase())
                        .size(11.0)
                        .color(pal.dim_fg);
                    if is_today {
                        name = name.color(pal.accent);
                    }
                    ui.label(name);
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::hover());
                    let color = if is_today {
                        ui.painter()
                            .circle_filled(rect.center(), 14.0, pal.accent_bg);
                        pal.accent_fg
                    } else if day < today {
                        pal.dim_fg
                    } else {
                        pal.window_fg
                    };
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        day.format("%-d").to_string(),
                        theme::title_font(15.0),
                        color,
                    );
                },
            );
        }
    });
}

fn all_day_row(
    app: &mut PlannerApp,
    ui: &mut Ui,
    days: &[NaiveDate],
    today: NaiveDate,
    col_w: f32,
) {
    let dark = ui.visuals().dark_mode;
    let show_completed = app.config.show_completed;

    // Gather per-day items first so every column gets the same height.
    let mut day_tasks: Vec<Vec<Task>> = Vec::with_capacity(7);
    let mut day_events: Vec<Vec<Event>> = Vec::with_capacity(7);
    for &day in days {
        let mut tasks: Vec<Task> = app
            .tasks
            .iter()
            .filter(|t| matches!(t.when, Some(When::Day(d)) if d == day))
            .filter(|t| show_completed || !t.completed)
            .cloned()
            .collect();
        tasks.sort_by(|a, b| {
            a.completed
                .cmp(&b.completed)
                .then_with(|| a.summary.cmp(&b.summary))
        });
        let events: Vec<Event> = app
            .events
            .iter()
            .filter(|e| e.all_day && e.covers_day(day))
            .cloned()
            .collect();
        day_tasks.push(tasks);
        day_events.push(events);
    }
    let rows = day_tasks
        .iter()
        .zip(&day_events)
        .map(|(t, e)| t.len() + e.len())
        .max()
        .unwrap_or(0)
        .max(1);
    let row_h = rows as f32 * 26.0 + 6.0;

    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = GAP;
        ui.allocate_ui_with_layout(
            Vec2::new(GUTTER - GAP, row_h),
            egui::Layout::top_down(egui::Align::Max),
            |ui| {
                ui.add_space(4.0);
                let pal = palette(ui);
                ui.label(RichText::new("All day").size(11.0).color(pal.dim_fg));
            },
        );
        for (i, &day) in days.iter().enumerate() {
            let pal = palette(ui);
            let fill = if day == today {
                tint(pal.accent_bg, dark).gamma_multiply(0.4)
            } else {
                pal.view_bg
            };
            let frame = Frame::new()
                .fill(fill)
                .stroke(Stroke::new(1.0, pal.border))
                .inner_margin(Margin::same(2))
                .corner_radius(6);
            let dropped = ui
                .allocate_ui_with_layout(
                    Vec2::new(col_w, row_h),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        let (_, dropped) = drop_zone(ui, frame, |ui| {
                            ui.set_min_size(Vec2::new(
                                (col_w - 4.0).max(0.0),
                                (row_h - 4.0).max(0.0),
                            ));
                            ui.set_max_width((col_w - 4.0).max(0.0));
                            ui.spacing_mut().item_spacing.y = 2.0;
                            for ev in &day_events[i] {
                                event_chip(app, ui, ev, dark);
                            }
                            for task in &day_tasks[i] {
                                day_task_chip(app, ui, task, dark);
                            }
                        });
                        dropped
                    },
                )
                .inner;
            if let Some(payload) = dropped {
                app.queue(Action::Schedule {
                    uid: payload.uid.clone(),
                    when: Some(When::Day(day)),
                });
            }
        }
    });
}

fn event_chip(app: &PlannerApp, ui: &mut Ui, ev: &Event, dark: bool) {
    let accent = calendar_color(
        find_calendar(&app.calendars, &ev.calendar),
        DEFAULT_EVENT_COLOR,
    );
    let resp = Frame::new()
        .fill(tint(accent, dark))
        .inner_margin(Margin::symmetric(6, 3))
        .corner_radius(3)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(
                egui::Label::new(RichText::new(&ev.summary).small())
                    .truncate()
                    .selectable(false),
            );
        })
        .response;
    event_tooltip(resp.interact(Sense::hover()), ev, app);
}

fn day_task_chip(app: &mut PlannerApp, ui: &mut Ui, task: &Task, dark: bool) {
    let accent = calendar_color(
        find_calendar(&app.calendars, &task.calendar),
        DEFAULT_TASK_COLOR,
    );
    let uid = task.uid.clone();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
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
            Id::new(("day-task", &uid)),
            DragTask { uid: uid.clone() },
            |ui| {
                Frame::new()
                    .fill(tint(accent, dark))
                    .stroke(Stroke::new(1.0, accent.gamma_multiply(0.6)))
                    .inner_margin(Margin::symmetric(6, 3))
                    .corner_radius(3)
                    .show(ui, |ui| {
                        ui.set_width((width - 12.0).max(0.0));
                        let mut text = RichText::new(&task.summary).small();
                        if task.completed {
                            text = text.strikethrough().weak();
                        }
                        ui.add(egui::Label::new(text).truncate().selectable(false));
                    });
            },
        );
        if response.clicked() {
            app.queue(Action::Open(uid));
        }
    });
}

struct Placed {
    start: DateTime<Local>,
    end: DateTime<Local>,
    kind: Kind,
    lane: usize,
}

enum Kind {
    Task(usize),
    Event(usize),
}

fn timed_grid(app: &mut PlannerApp, ui: &mut Ui, days: &[NaiveDate], today: NaiveDate, col_w: f32) {
    let start_h = app.config.day_start_hour.min(23);
    let end_h = app.config.day_end_hour.clamp(start_h + 1, 24);
    let hours = end_h - start_h;
    let total_h = hours as f32 * HOUR_HEIGHT;

    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = GAP;
        let (gutter, _) = ui.allocate_exact_size(Vec2::new(GUTTER - GAP, total_h), Sense::hover());
        let painter = ui.painter();
        let color = palette(ui).dim_fg;
        for h in 0..=hours {
            let y = gutter.top() + h as f32 * HOUR_HEIGHT;
            painter.text(
                Pos2::new(gutter.right() - 2.0, y),
                Align2::RIGHT_CENTER,
                format!("{:02}:00", start_h + h),
                FontId::proportional(11.0),
                color,
            );
        }
        for &day in days {
            day_column(app, ui, day, today, col_w, total_h, start_h, end_h);
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn day_column(
    app: &mut PlannerApp,
    ui: &mut Ui,
    day: NaiveDate,
    today: NaiveDate,
    col_w: f32,
    total_h: f32,
    start_h: u32,
    end_h: u32,
) {
    let dark = ui.visuals().dark_mode;
    let midnight = local_midnight(day);
    let show_completed = app.config.show_completed;

    let (inner, dropped) = drop_zone(ui, Frame::NONE, |ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(col_w, total_h), Sense::hover());
        let painter = ui.painter_at(rect);

        // Background and hour lines.
        let pal = palette(ui);
        let bg = if day == today {
            tint(pal.accent_bg, dark).gamma_multiply(0.3)
        } else {
            pal.view_bg
        };
        painter.rect_filled(rect, 0.0, bg);
        let line = pal.border;
        let faint = line.gamma_multiply(0.45);
        for h in 0..=(end_h - start_h) {
            let y = rect.top() + h as f32 * HOUR_HEIGHT;
            painter.hline(rect.x_range(), y, Stroke::new(1.0, line));
            if h < end_h - start_h {
                painter.hline(
                    rect.x_range(),
                    y + HOUR_HEIGHT / 2.0,
                    Stroke::new(1.0, faint),
                );
            }
        }

        let y_of = |t: DateTime<Local>| -> f32 {
            let minutes = (t - midnight).num_minutes() as f32;
            rect.top() + (minutes / 60.0 - start_h as f32) * HOUR_HEIGHT
        };

        // Collect items for this day and assign lanes.
        let mut placed: Vec<Placed> = Vec::new();
        for (i, ev) in app.events.iter().enumerate() {
            if !ev.all_day && ev.covers_day(day) {
                placed.push(Placed {
                    start: ev.start,
                    end: ev.end,
                    kind: Kind::Event(i),
                    lane: 0,
                });
            }
        }
        for (i, task) in app.tasks.iter().enumerate() {
            if let Some(When::At { start, duration }) = task.when {
                if start.date_naive() == day && (show_completed || !task.completed) {
                    placed.push(Placed {
                        start,
                        end: start + duration,
                        kind: Kind::Task(i),
                        lane: 0,
                    });
                }
            }
        }
        placed.sort_by_key(|p| (p.start, std::cmp::Reverse(p.end)));
        let mut lane_ends: Vec<DateTime<Local>> = Vec::new();
        for p in &mut placed {
            let visible_end = p.end.max(p.start + Duration::minutes(20));
            match lane_ends.iter().position(|end| *end <= p.start) {
                Some(l) => {
                    p.lane = l;
                    lane_ends[l] = visible_end;
                }
                None => {
                    p.lane = lane_ends.len();
                    lane_ends.push(visible_end);
                }
            }
        }
        let lanes = lane_ends.len().max(1) as f32;
        let lane_w = (rect.width() - 4.0) / lanes;

        let block_rect = |p: &Placed| -> Rect {
            let y0 = y_of(p.start).clamp(rect.top(), rect.bottom() - MIN_BLOCK_HEIGHT);
            let y1 = y_of(p.end).clamp(y0 + MIN_BLOCK_HEIGHT, rect.bottom());
            let x0 = rect.left() + 2.0 + p.lane as f32 * lane_w;
            Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x0 + lane_w - 2.0, y1))
        };

        // Events first (behind tasks).
        for p in placed.iter().filter(|p| matches!(p.kind, Kind::Event(_))) {
            let Kind::Event(i) = p.kind else { continue };
            let ev = &app.events[i];
            let r = block_rect(p);
            let accent = calendar_color(
                find_calendar(&app.calendars, &ev.calendar),
                DEFAULT_EVENT_COLOR,
            );
            let painter = ui.painter_at(r);
            painter.rect_filled(r, 4.0, tint(accent, dark));
            painter.rect_filled(
                Rect::from_min_size(r.min, Vec2::new(3.0, r.height())),
                0.0,
                accent,
            );
            let text_color = pal.window_fg;
            let mut y = r.top() + 2.0;
            if r.height() >= 34.0 {
                painter.text(
                    Pos2::new(r.left() + 7.0, y),
                    Align2::LEFT_TOP,
                    format!("{} – {}", ev.start.format("%H:%M"), ev.end.format("%H:%M")),
                    FontId::proportional(10.0),
                    pal.dim_fg,
                );
                y += 13.0;
            }
            let galley = painter.layout(
                ev.summary.clone(),
                FontId::proportional(12.0),
                text_color,
                (r.width() - 10.0).max(10.0),
            );
            painter.galley(Pos2::new(r.left() + 7.0, y), galley, text_color);
            let resp = ui.interact(
                r,
                Id::new(("event", &ev.href, &ev.uid, ev.start)),
                Sense::hover(),
            );
            event_tooltip(resp, ev, app);
        }

        // Tasks: draggable blocks.
        let mut opened: Option<String> = None;
        for p in placed.iter().filter(|p| matches!(p.kind, Kind::Task(_))) {
            let Kind::Task(i) = p.kind else { continue };
            let task = &app.tasks[i];
            let r = block_rect(p);
            let accent = calendar_color(
                find_calendar(&app.calendars, &task.calendar),
                DEFAULT_TASK_COLOR,
            );
            let uid = task.uid.clone();
            let mut child = ui.new_child(
                UiBuilder::new()
                    .max_rect(r)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            child.set_clip_rect(r.expand(1.0).intersect(ui.clip_rect()));
            let response = drag_item(
                &mut child,
                Id::new(("grid-task", &uid)),
                DragTask { uid: uid.clone() },
                |ui| {
                    let fill = if task.completed {
                        tint(accent, dark).gamma_multiply(0.5)
                    } else {
                        tint(accent, dark)
                    };
                    Frame::new()
                        .fill(fill)
                        .stroke(Stroke::new(1.0, accent))
                        .corner_radius(4)
                        .inner_margin(Margin::symmetric(5, 2))
                        .show(ui, |ui| {
                            let inner =
                                Vec2::new((r.width() - 10.0).max(0.0), (r.height() - 4.0).max(0.0));
                            ui.set_min_size(inner);
                            ui.set_max_size(inner);
                            ui.spacing_mut().item_spacing.y = 0.0;
                            if r.height() >= 34.0 {
                                ui.label(
                                    RichText::new(format!(
                                        "{} · {}m",
                                        p.start.format("%H:%M"),
                                        (p.end - p.start).num_minutes()
                                    ))
                                    .size(10.0)
                                    .weak(),
                                );
                            }
                            let mut text = RichText::new(&task.summary).size(12.0);
                            if task.completed {
                                text = text.strikethrough().weak();
                            }
                            let label = if r.height() >= 48.0 {
                                egui::Label::new(text).wrap()
                            } else {
                                egui::Label::new(text).truncate()
                            };
                            ui.add(label.selectable(false));
                        });
                },
            );
            if response.clicked() {
                opened = Some(uid.clone());
            }
            if !task.description.trim().is_empty() {
                response.on_hover_text(task.description.trim());
            }
        }
        if let Some(uid) = opened {
            app.queue(Action::Open(uid));
        }

        // Snap preview while dragging over this column.
        if egui::DragAndDrop::has_payload_of_type::<DragTask>(ui.ctx()) {
            if let Some(pos) = ui.input(|i| i.pointer.latest_pos()) {
                if rect.contains(pos) {
                    let minutes = snapped_minutes(pos.y, rect.top(), start_h);
                    let y = rect.top() + (minutes as f32 / 60.0 - start_h as f32) * HOUR_HEIGHT;
                    let accent = pal.accent_bg;
                    painter.hline(rect.x_range(), y, Stroke::new(2.0, accent));
                    painter.text(
                        Pos2::new(rect.left() + 4.0, y - 2.0),
                        Align2::LEFT_BOTTOM,
                        format!("{:02}:{:02}", minutes / 60, minutes % 60),
                        FontId::proportional(11.0),
                        accent,
                    );
                }
            }
        }

        // Current time.
        if day == today {
            let now = Local::now();
            let y = y_of(now);
            if y >= rect.top() && y <= rect.bottom() {
                let red = pal.destructive_bg;
                painter.hline(rect.x_range(), y, Stroke::new(2.0, red));
                painter.circle_filled(Pos2::new(rect.left() + 1.0, y), 4.0, red);
            }
            // Keep the time indicator moving.
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs(30));
        }
        painter.rect_stroke(rect, 0.0, Stroke::new(1.0, line), StrokeKind::Inside);
        rect
    });

    if let Some(payload) = dropped {
        let rect = inner.inner;
        if let Some(pos) = ui.input(|i| i.pointer.latest_pos()) {
            let minutes = snapped_minutes(pos.y, rect.top(), start_h);
            let start = midnight + Duration::minutes(minutes);
            app.queue(Action::ScheduleAt {
                uid: payload.uid.clone(),
                start,
            });
        }
    }
}

fn snapped_minutes(y: f32, top: f32, start_h: u32) -> i64 {
    let raw = ((y - top) / HOUR_HEIGHT * 60.0) as i64 + start_h as i64 * 60;
    let snapped = (raw as f64 / SNAP_MINUTES as f64).round() as i64 * SNAP_MINUTES;
    snapped.clamp(0, 24 * 60 - SNAP_MINUTES)
}

fn event_tooltip(resp: egui::Response, ev: &Event, app: &PlannerApp) {
    resp.on_hover_ui(|ui| {
        ui.strong(&ev.summary);
        if ev.all_day {
            let days = (ev.end - ev.start).num_days();
            if days > 1 {
                ui.label(format!(
                    "{} – {}",
                    ev.start.format("%a %-d %b"),
                    (ev.end - Duration::days(1)).format("%a %-d %b")
                ));
            } else {
                ui.label(format!("{} · all day", ev.start.format("%A %-d %B")));
            }
        } else {
            ui.label(format!(
                "{} · {} – {}",
                ev.start.format("%a %-d %b"),
                ev.start.format("%H:%M"),
                ev.end.format("%H:%M")
            ));
        }
        if !ev.location.trim().is_empty() {
            ui.weak(ev.location.trim());
        }
        if let Some(cal) = find_calendar(&app.calendars, &ev.calendar) {
            ui.weak(&cal.name);
        }
    });
}

/// Header-bar title and subtitle, e.g. `5 – 11 October 2026` and `Week 41`.
pub fn week_title(week_start: NaiveDate) -> (String, String) {
    let end = week_start + Duration::days(6);
    let range = if week_start.format("%b%Y").to_string() == end.format("%b%Y").to_string() {
        format!("{} – {}", week_start.format("%-d"), end.format("%-d %B %Y"))
    } else if week_start.format("%Y").to_string() == end.format("%Y").to_string() {
        format!(
            "{} – {}",
            week_start.format("%-d %b"),
            end.format("%-d %b %Y")
        )
    } else {
        format!(
            "{} – {}",
            week_start.format("%-d %b %Y"),
            end.format("%-d %b %Y")
        )
    };
    (range, format!("Week {}", week_start.format("%V")))
}
