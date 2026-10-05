//! Application state, message handling and the top-level layout.

use crate::config::Config;
use crate::model::{Calendar, Event, Task, When};
use crate::sync::{self, Command, Message, SyncHandle};
use crate::ui::editor::TaskEditor;
use crate::ui::settings::SettingsState;
use crate::ui::{editor, inbox, settings, week};
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate};
use egui::{Color32, RichText};
use std::collections::HashSet;
use std::time::Instant;

const AUTO_REFRESH_SECS: u64 = 300;

/// Something the UI asked for during a frame; applied once the frame is drawn.
pub enum Action {
    Create {
        summary: String,
    },
    Open(String),
    Schedule {
        uid: String,
        when: Option<When>,
    },
    ScheduleAt {
        uid: String,
        start: DateTime<Local>,
    },
    SetCompleted {
        uid: String,
        completed: bool,
    },
    ApplyEditor {
        editor: TaskEditor,
        when: Option<When>,
    },
    Delete(String),
    SaveSettings(Config),
}

pub struct PlannerApp {
    pub config: Config,
    pub settings: SettingsState,
    pub calendars: Vec<Calendar>,
    pub tasks: Vec<Task>,
    pub events: Vec<Event>,
    pub week_start: NaiveDate,
    pub new_task: String,
    pub new_task_calendar: String,
    pub editor: Option<TaskEditor>,
    pub busy: bool,
    pub error: Option<String>,
    pub last_sync: Option<DateTime<Local>>,
    sync: SyncHandle,
    actions: Vec<Action>,
    /// Tasks with local edits that the server has not confirmed yet.
    pending: HashSet<String>,
    last_refresh: Instant,
    connected: bool,
}

impl PlannerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (config, error) = match Config::load() {
            Ok(Some(c)) => (c, None),
            Ok(None) => (Config::default(), None),
            Err(e) => (Config::default(), Some(format!("{e:#}"))),
        };
        let sync = sync::spawn(cc.egui_ctx.clone());
        let mut app = Self {
            week_start: monday_of(Local::now().date_naive()),
            new_task_calendar: config.default_task_calendar.clone(),
            config,
            settings: SettingsState::default(),
            calendars: Vec::new(),
            tasks: Vec::new(),
            events: Vec::new(),
            new_task: String::new(),
            editor: None,
            busy: false,
            error,
            last_sync: None,
            sync,
            actions: Vec::new(),
            pending: HashSet::new(),
            last_refresh: Instant::now(),
            connected: false,
        };
        if app.config.is_configured() {
            app.connect();
        } else {
            app.settings.open_with(&app.config, &[]);
        }
        app
    }

    pub fn queue(&mut self, action: Action) {
        self.actions.push(action);
    }

    fn connect(&mut self) {
        self.sync.send(Command::Configure(self.config.clone()));
        self.sync.send(Command::Refresh {
            week_start: self.week_start,
        });
        self.last_refresh = Instant::now();
    }

    fn refresh(&mut self) {
        self.sync.send(Command::Refresh {
            week_start: self.week_start,
        });
        self.last_refresh = Instant::now();
    }

    fn task_mut(&mut self, uid: &str) -> Option<&mut Task> {
        self.tasks.iter_mut().find(|t| t.uid == uid)
    }

    fn save(&mut self, uid: &str) {
        if let Some(task) = self.tasks.iter().find(|t| t.uid == uid) {
            let task = task.clone();
            self.pending.insert(uid.to_string());
            self.sync.send(Command::Save(task));
        }
    }

    fn handle_messages(&mut self) {
        while let Some(msg) = self.sync.try_recv() {
            match msg {
                Message::Calendars(cals) => {
                    self.connected = true;
                    self.error = None;
                    self.calendars = cals;
                    let valid = |url: &str| {
                        self.calendars.iter().any(|c| {
                            c.url == url
                                && c.supports_todo
                                && self.config.uses_task_calendar(&c.url)
                        })
                    };
                    if !valid(&self.new_task_calendar) {
                        self.new_task_calendar = if valid(&self.config.default_task_calendar) {
                            self.config.default_task_calendar.clone()
                        } else {
                            self.calendars
                                .iter()
                                .find(|c| c.supports_todo && self.config.uses_task_calendar(&c.url))
                                .map(|c| c.url.clone())
                                .unwrap_or_default()
                        };
                    }
                }
                Message::Tasks(fresh) => {
                    // Keep local versions of tasks whose save is still in flight.
                    let mut merged: Vec<Task> = self
                        .tasks
                        .iter()
                        .filter(|t| self.pending.contains(&t.uid))
                        .cloned()
                        .collect();
                    for t in fresh {
                        if !self.pending.contains(&t.uid) {
                            merged.push(t);
                        }
                    }
                    self.tasks = merged;
                    self.last_sync = Some(Local::now());
                }
                Message::Events(events) => self.events = events,
                Message::Saved { uid, href, etag } => {
                    if let Some(t) = self.task_mut(&uid) {
                        t.href = href;
                        t.etag = etag;
                    }
                    self.pending.remove(&uid);
                }
                Message::Deleted { uid } => {
                    self.tasks.retain(|t| t.uid != uid);
                    self.pending.remove(&uid);
                }
                Message::Error(e) => {
                    self.error = Some(e);
                    self.pending.clear();
                }
                Message::Busy(b) => self.busy = b,
            }
        }
    }

    fn apply_actions(&mut self) {
        let actions = std::mem::take(&mut self.actions);
        for action in actions {
            match action {
                Action::Create { summary } => {
                    let summary = summary.trim().to_string();
                    if summary.is_empty() || self.new_task_calendar.is_empty() {
                        continue;
                    }
                    let task = Task::new(&self.new_task_calendar, &summary);
                    let uid = task.uid.clone();
                    self.tasks.push(task);
                    self.save(&uid);
                }
                Action::Open(uid) => {
                    if let Some(t) = self.tasks.iter().find(|t| t.uid == uid) {
                        self.editor =
                            Some(TaskEditor::from_task(t, self.config.default_task_minutes));
                    }
                }
                Action::Schedule { uid, when } => {
                    if let Some(t) = self.task_mut(&uid) {
                        if t.when != when {
                            t.when = when;
                            self.save(&uid);
                        }
                    }
                }
                Action::ScheduleAt { uid, start } => {
                    let default = Duration::minutes(self.config.default_task_minutes.max(5));
                    if let Some(t) = self.task_mut(&uid) {
                        let duration = match t.when {
                            Some(When::At { duration, .. }) => duration,
                            _ => default,
                        };
                        t.when = Some(When::At { start, duration });
                        self.save(&uid);
                    }
                }
                Action::SetCompleted { uid, completed } => {
                    if let Some(t) = self.task_mut(&uid) {
                        t.completed = completed;
                        self.save(&uid);
                    }
                }
                Action::ApplyEditor { editor, when } => {
                    if let Some(t) = self.task_mut(&editor.uid) {
                        t.summary = editor.summary.trim().to_string();
                        t.description = editor.description.clone();
                        t.priority = editor.priority;
                        t.completed = editor.completed;
                        t.when = when;
                        self.save(&editor.uid);
                    }
                }
                Action::Delete(uid) => {
                    if let Some(pos) = self.tasks.iter().position(|t| t.uid == uid) {
                        let task = self.tasks.remove(pos);
                        self.sync.send(Command::Delete(task));
                    }
                }
                Action::SaveSettings(cfg) => {
                    let reconnect = cfg.server_url != self.config.server_url
                        || cfg.username != self.config.username
                        || cfg.password != self.config.password
                        || cfg.task_calendars != self.config.task_calendars
                        || cfg.event_calendars != self.config.event_calendars
                        || !self.connected;
                    if let Err(e) = cfg.save() {
                        self.error = Some(format!("could not save settings: {e:#}"));
                    }
                    self.config = cfg;
                    self.new_task_calendar = self.config.default_task_calendar.clone();
                    if reconnect && self.config.is_configured() {
                        self.connect();
                    }
                }
            }
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            if ui.button("◀").on_hover_text("Previous week").clicked() {
                self.week_start -= Duration::days(7);
                self.refresh();
            }
            if ui.button("Today").clicked() {
                self.week_start = monday_of(Local::now().date_naive());
                self.refresh();
            }
            if ui.button("▶").on_hover_text("Next week").clicked() {
                self.week_start += Duration::days(7);
                self.refresh();
            }
            ui.add_space(8.0);
            ui.heading(week::week_label(self.week_start));

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(4.0);
                if ui.button("⚙ Settings").clicked() {
                    self.settings.open_with(&self.config, &self.calendars);
                }
                let sync_btn =
                    ui.add_enabled(!self.busy && self.connected, egui::Button::new("⟳ Sync"));
                if sync_btn.clicked() {
                    self.refresh();
                }
                if self.busy {
                    ui.spinner();
                    ui.weak("syncing…");
                } else if let Some(t) = self.last_sync {
                    ui.weak(format!("synced {}", t.format("%H:%M")));
                } else if !self.connected {
                    ui.weak("not connected");
                }
                ui.add_space(8.0);
                let mut show = self.config.show_completed;
                if ui.checkbox(&mut show, "Show completed").changed() {
                    self.config.show_completed = show;
                    let _ = self.config.save();
                }
            });
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if let Some(err) = self.error.clone() {
                if ui.small_button("✕").clicked() {
                    self.error = None;
                }
                let first_line = err.lines().next().unwrap_or("").to_string();
                ui.colored_label(Color32::from_rgb(220, 80, 60), RichText::new(first_line))
                    .on_hover_text(err);
                if ui.small_button("Retry").clicked() {
                    self.error = None;
                    if self.connected {
                        self.refresh();
                    } else {
                        self.connect();
                    }
                }
            } else {
                let planned = self
                    .tasks
                    .iter()
                    .filter(|t| {
                        !t.completed
                            && t.when
                                .map(|w| in_week(w.date(), self.week_start))
                                .unwrap_or(false)
                    })
                    .count();
                let inbox = self
                    .tasks
                    .iter()
                    .filter(|t| !t.completed && t.is_inbox())
                    .count();
                ui.weak(format!(
                    "{planned} task{} planned this week · {inbox} unscheduled · {} event{}",
                    plural(planned),
                    self.events.len(),
                    plural(self.events.len())
                ));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.weak("Drag tasks between the inbox and the grid · click a task to edit");
            });
        });
    }
}

impl eframe::App for PlannerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_messages();

        if self.connected
            && !self.busy
            && self.last_refresh.elapsed().as_secs() >= AUTO_REFRESH_SECS
        {
            self.refresh();
        }
        ctx.request_repaint_after(std::time::Duration::from_secs(60));

        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(4.0);
            self.top_bar(ui);
            ui.add_space(4.0);
        });
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.add_space(2.0);
            self.status_bar(ui);
            ui.add_space(2.0);
        });
        egui::Panel::left("inbox")
            .default_size(300.0)
            .size_range(220.0..=480.0)
            .resizable(true)
            .show(ui, |ui| inbox::show(self, ui));
        egui::CentralPanel::default().show(ui, |ui| week::show(self, ui));

        editor::show(self, &ctx);
        settings::show(self, &ctx);

        self.apply_actions();

        #[cfg(feature = "screenshot")]
        self.debug_screenshot(&ctx);
    }
}

#[cfg(feature = "screenshot")]
impl PlannerApp {
    /// Test aid: capture the window after a delay and exit.
    fn debug_screenshot(&mut self, ctx: &egui::Context) {
        use std::sync::OnceLock;
        static START: OnceLock<Instant> = OnceLock::new();
        let Ok(path) = std::env::var("PLANNER_SCREENSHOT") else {
            return;
        };
        let after_ms: u64 = std::env::var("PLANNER_SCREENSHOT_AFTER_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3000);
        let start = START.get_or_init(Instant::now);
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
        if start.elapsed().as_millis() as u64 >= after_ms {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        let shot = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = shot {
            let [w, h] = image.size;
            let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
            image::save_buffer(&path, &bytes, w as u32, h as u32, image::ColorType::Rgba8)
                .expect("write screenshot");
            log::info!("screenshot written to {path}");
            std::process::exit(0);
        }
    }
}

pub fn monday_of(date: NaiveDate) -> NaiveDate {
    date - Duration::days(date.weekday().num_days_from_monday() as i64)
}

fn in_week(date: NaiveDate, week_start: NaiveDate) -> bool {
    date >= week_start && date < week_start + Duration::days(7)
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}
