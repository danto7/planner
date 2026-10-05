//! Application state, message handling and the top-level layout.

use crate::config::Config;
use crate::model::{Calendar, Event, Task, When};
use crate::sync::{self, Command, Message, SyncHandle};
use crate::ui::editor::TaskEditor;
use crate::ui::settings::SettingsState;
use crate::ui::theme::{self, palette_of, Icon};
use crate::ui::{dialogs, editor, inbox, settings, week};
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate};
use egui::{Align2, Frame, Key, Layout, Margin, Modifiers, RichText, Stroke, Vec2};
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
    SelectTaskList(String),
}

/// An in-app notification shown at the bottom of the window.
pub struct Toast {
    pub text: String,
    pub detail: String,
    pub retry: bool,
}

pub struct PlannerApp {
    pub config: Config,
    pub settings: SettingsState,
    pub calendars: Vec<Calendar>,
    pub tasks: Vec<Task>,
    pub events: Vec<Event>,
    pub week_start: NaiveDate,
    pub new_task: String,
    pub focus_new_task: bool,
    pub editor: Option<TaskEditor>,
    pub busy: bool,
    pub toast: Option<Toast>,
    pub last_sync: Option<DateTime<Local>>,
    about_open: bool,
    shortcuts_open: bool,
    sync: SyncHandle,
    actions: Vec<Action>,
    /// Tasks with local edits that the server has not confirmed yet.
    pending: HashSet<String>,
    last_refresh: Instant,
    connected: bool,
}

impl PlannerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        let (config, load_error) = match Config::load() {
            Ok(Some(c)) => (c, None),
            Ok(None) => (Config::default(), None),
            Err(e) => (Config::default(), Some(format!("{e:#}"))),
        };
        let sync = sync::spawn(cc.egui_ctx.clone());
        let mut app = Self {
            week_start: monday_of(Local::now().date_naive()),
            config,
            settings: SettingsState::default(),
            calendars: Vec::new(),
            tasks: Vec::new(),
            events: Vec::new(),
            new_task: String::new(),
            focus_new_task: false,
            editor: None,
            busy: false,
            toast: load_error.map(|e| Toast {
                text: "Could not read the configuration file".into(),
                detail: e,
                retry: false,
            }),
            last_sync: None,
            about_open: false,
            shortcuts_open: false,
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
            task_list: self.config.task_list.clone(),
        });
        self.last_refresh = Instant::now();
    }

    fn refresh(&mut self) {
        self.sync.send(Command::Refresh {
            week_start: self.week_start,
            task_list: self.config.task_list.clone(),
        });
        self.last_refresh = Instant::now();
    }

    fn go_to_week(&mut self, start: NaiveDate) {
        if start != self.week_start {
            self.week_start = start;
            self.refresh();
        }
    }

    /// Every discovered calendar that can hold tasks.
    pub fn task_lists(&self) -> Vec<Calendar> {
        self.calendars
            .iter()
            .filter(|c| c.supports_todo)
            .cloned()
            .collect()
    }

    /// The list currently shown: the configured one, else the first available.
    pub fn task_list(&self) -> Option<Calendar> {
        let lists = self.task_lists();
        lists
            .iter()
            .find(|c| c.url == self.config.task_list)
            .or_else(|| lists.first())
            .cloned()
    }

    fn select_task_list(&mut self, url: String) {
        if self.config.task_list == url && self.tasks.iter().all(|t| t.calendar == url) {
            return;
        }
        self.config.task_list = url.clone();
        let _ = self.config.save();
        self.tasks.retain(|t| t.calendar == url);
        self.editor = None;
        self.refresh();
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
                    self.toast = None;
                    self.calendars = cals;
                    // The configured list may be stale; fall back to the first task list.
                    let configured = self.config.task_list.clone();
                    let valid = self.task_lists().iter().any(|c| c.url == configured);
                    if !valid {
                        let first = self.task_lists().first().map(|c| c.url.clone());
                        if let Some(url) = first {
                            self.config.task_list = url;
                            let _ = self.config.save();
                            if !configured.is_empty() {
                                self.refresh();
                            }
                        }
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
                    let text = if self.connected {
                        "Sync failed"
                    } else {
                        "Could not connect"
                    };
                    self.toast = Some(Toast {
                        text: text.into(),
                        detail: e,
                        retry: true,
                    });
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
                    let Some(list) = self.task_list() else {
                        continue;
                    };
                    if summary.is_empty() {
                        continue;
                    }
                    let task = Task::new(&list.url, &summary);
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
                        || cfg.event_calendars != self.config.event_calendars
                        || !self.connected;
                    let list_changed = cfg.task_list != self.config.task_list;
                    if let Err(e) = cfg.save() {
                        self.toast = Some(Toast {
                            text: "Could not save preferences".into(),
                            detail: format!("{e:#}"),
                            retry: false,
                        });
                    }
                    self.config = cfg;
                    if reconnect && self.config.is_configured() {
                        self.connect();
                    } else if list_changed {
                        let url = self.config.task_list.clone();
                        self.select_task_list(url);
                    }
                }
                Action::SelectTaskList(url) => self.select_task_list(url),
            }
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let mut go: Option<NaiveDate> = None;
        ctx.input_mut(|i| {
            if i.consume_key(Modifiers::ALT, Key::ArrowLeft) {
                go = Some(self.week_start - Duration::days(7));
            }
            if i.consume_key(Modifiers::ALT, Key::ArrowRight) {
                go = Some(self.week_start + Duration::days(7));
            }
            if i.consume_key(Modifiers::COMMAND, Key::T) {
                go = Some(monday_of(Local::now().date_naive()));
            }
            if i.consume_key(Modifiers::COMMAND, Key::R) && self.connected {
                self.last_refresh =
                    Instant::now() - std::time::Duration::from_secs(AUTO_REFRESH_SECS);
            }
            if i.consume_key(Modifiers::COMMAND, Key::N) {
                self.focus_new_task = true;
            }
            if i.consume_key(Modifiers::COMMAND, Key::Comma) {
                self.settings.open_with(&self.config, &self.calendars);
            }
            if i.consume_key(Modifiers::COMMAND, Key::Questionmark)
                || i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Slash)
            {
                self.shortcuts_open = true;
            }
        });
        if let Some(start) = go {
            self.go_to_week(start);
        }
    }

    fn header_bar(&mut self, ui: &mut egui::Ui) {
        let p = palette_of(ui.ctx());
        let full = ui.max_rect();
        let (title, subtitle) = week::week_title(self.week_start);
        ui.painter().text(
            full.center() + Vec2::new(0.0, -7.0),
            Align2::CENTER_CENTER,
            title,
            theme::title_font(15.0),
            p.window_fg,
        );
        ui.painter().text(
            full.center() + Vec2::new(0.0, 9.0),
            Align2::CENTER_CENTER,
            subtitle,
            egui::FontId::proportional(11.0),
            p.dim_fg,
        );

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if theme::icon_button(ui, Icon::ChevronLeft, "Previous week (Alt+←)").clicked() {
                self.go_to_week(self.week_start - Duration::days(7));
            }
            if theme::icon_button(ui, Icon::ChevronRight, "Next week (Alt+→)").clicked() {
                self.go_to_week(self.week_start + Duration::days(7));
            }
            ui.add_space(4.0);
            if ui
                .button("Today")
                .on_hover_text("Go to this week (Ctrl+T)")
                .clicked()
            {
                self.go_to_week(monday_of(Local::now().date_naive()));
            }

            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let menu = theme::icon_button(ui, Icon::Menu, "Main menu");
                egui::Popup::menu(&menu).width(260.0).show(|ui| {
                    let mut show = self.config.show_completed;
                    if ui.checkbox(&mut show, "Show Completed Tasks").changed() {
                        self.config.show_completed = show;
                        let _ = self.config.save();
                    }
                    ui.separator();
                    if ui
                        .add_enabled(
                            self.connected,
                            egui::Button::new("Refresh").shortcut_text("Ctrl+R"),
                        )
                        .clicked()
                    {
                        self.refresh();
                        ui.close();
                    }
                    ui.separator();
                    if ui
                        .add(egui::Button::new("Preferences").shortcut_text("Ctrl+,"))
                        .clicked()
                    {
                        self.settings.open_with(&self.config, &self.calendars);
                        ui.close();
                    }
                    if ui
                        .add(egui::Button::new("Keyboard Shortcuts").shortcut_text("Ctrl+?"))
                        .clicked()
                    {
                        self.shortcuts_open = true;
                        ui.close();
                    }
                    if ui.button("About Week Planner").clicked() {
                        self.about_open = true;
                        ui.close();
                    }
                });

                if self.busy {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(34.0), egui::Sense::hover());
                    egui::Spinner::new().size(16.0).paint_at(
                        ui,
                        egui::Rect::from_center_size(rect.center(), Vec2::splat(16.0)),
                    );
                } else {
                    let tooltip = match self.last_sync {
                        Some(t) => format!("Refresh (Ctrl+R) · last synced {}", t.format("%H:%M")),
                        None => "Refresh (Ctrl+R)".to_string(),
                    };
                    if theme::icon_button_enabled(ui, Icon::Refresh, &tooltip, self.connected)
                        .clicked()
                    {
                        self.refresh();
                    }
                }
            });
        });
    }

    fn show_toast(&mut self, ctx: &egui::Context) {
        let Some(toast) = &self.toast else { return };
        let dark = ctx.theme() == egui::Theme::Dark;
        let fill = if dark {
            egui::Color32::from_rgb(0x4a, 0x4a, 0x4a)
        } else {
            egui::Color32::from_rgb(0x35, 0x35, 0x35)
        };
        let mut dismiss = false;
        let mut retry = false;
        egui::Area::new(egui::Id::new("toast"))
            .anchor(Align2::CENTER_BOTTOM, [0.0, -12.0])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(fill)
                    .corner_radius(24.0)
                    .shadow(ctx.style_of(ctx.theme()).visuals.popup_shadow)
                    .inner_margin(Margin {
                        left: 16,
                        right: 8,
                        top: 6,
                        bottom: 6,
                    })
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 10.0;
                            let label = ui.add(
                                egui::Label::new(
                                    RichText::new(&toast.text).color(egui::Color32::WHITE),
                                )
                                .selectable(false),
                            );
                            if !toast.detail.is_empty() {
                                label.on_hover_text(&toast.detail);
                            }
                            if toast.retry {
                                let button = egui::Button::new(
                                    RichText::new("Retry").color(egui::Color32::WHITE),
                                )
                                .fill(egui::Color32::from_white_alpha(30))
                                .corner_radius(14.0);
                                if ui.add(button).clicked() {
                                    retry = true;
                                }
                            }
                            let (rect, resp) =
                                ui.allocate_exact_size(Vec2::splat(30.0), egui::Sense::click());
                            if resp.hovered() {
                                ui.painter().circle_filled(
                                    rect.center(),
                                    15.0,
                                    egui::Color32::from_white_alpha(25),
                                );
                            }
                            theme::paint_icon(
                                ui,
                                Icon::Close,
                                egui::Rect::from_center_size(rect.center(), Vec2::splat(14.0)),
                                egui::Color32::WHITE,
                            );
                            if resp.on_hover_text("Dismiss").clicked() {
                                dismiss = true;
                            }
                        });
                    });
            });
        if retry {
            self.toast = None;
            if self.connected {
                self.refresh();
            } else {
                self.connect();
            }
        } else if dismiss {
            self.toast = None;
        }
    }
}

impl eframe::App for PlannerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_messages();
        self.handle_shortcuts(&ctx);

        if self.connected
            && !self.busy
            && self.last_refresh.elapsed().as_secs() >= AUTO_REFRESH_SECS
        {
            self.refresh();
        }
        ctx.request_repaint_after(std::time::Duration::from_secs(60));

        let p = palette_of(&ctx);
        egui::Panel::top("header")
            .frame(
                Frame::new()
                    .fill(p.headerbar_bg)
                    .inner_margin(Margin::symmetric(6, 6)),
            )
            .show_separator_line(false)
            .show(ui, |ui| {
                ui.set_height(35.0);
                self.header_bar(ui);
                let r = ui.max_rect();
                ui.painter()
                    .hline(r.x_range(), r.bottom() + 6.0, Stroke::new(1.0, p.border));
            });
        egui::Panel::left("sidebar")
            .frame(Frame::new().fill(p.sidebar_bg))
            .default_size(300.0)
            .size_range(240.0..=480.0)
            .resizable(true)
            .show_separator_line(false)
            .show(ui, |ui| {
                let r = ui.max_rect();
                ui.painter()
                    .vline(r.right(), r.y_range(), Stroke::new(1.0, p.border));
                inbox::show(self, ui);
            });
        egui::CentralPanel::default()
            .frame(Frame::new().fill(p.window_bg).inner_margin(Margin {
                left: 6,
                right: 12,
                top: 8,
                bottom: 0,
            }))
            .show(ui, |ui| week::show(self, ui));

        editor::show(self, &ctx);
        settings::show(self, &ctx);
        dialogs::about(&ctx, &mut self.about_open);
        dialogs::shortcuts(&ctx, &mut self.shortcuts_open);
        self.show_toast(&ctx);

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
            // Optional: open a dialog before the capture.
            match std::env::var("PLANNER_SCREENSHOT_DIALOG").as_deref() {
                Ok("editor") if self.editor.is_none() => {
                    if let Some(t) = self.tasks.first() {
                        self.editor =
                            Some(TaskEditor::from_task(t, self.config.default_task_minutes));
                    }
                }
                Ok("preferences") if !self.settings.open => {
                    self.settings.open_with(&self.config, &self.calendars)
                }
                Ok("about") => self.about_open = true,
                Ok("shortcuts") => self.shortcuts_open = true,
                _ => {}
            }
            if start.elapsed().as_millis() as u64 >= after_ms + 600 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
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
