//! Background worker that talks to the CalDAV server so the UI never blocks.

use crate::caldav::Client;
use crate::config::Config;
use crate::model::{Calendar, Event, Task};
use anyhow::Result;
use chrono::{Duration, NaiveDate, Utc};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;

pub enum Command {
    /// (Re)connect with this configuration and discover calendars.
    Configure(Config),
    /// Reload the tasks of one list and the events around the given week.
    Refresh {
        week_start: NaiveDate,
        /// URL of the task list; empty = the first calendar supporting VTODO.
        task_list: String,
    },
    Save(Task),
    Delete(Task),
}

pub enum Message {
    Calendars(Vec<Calendar>),
    Tasks(Vec<Task>),
    Events(Vec<Event>),
    Saved {
        uid: String,
        href: String,
        etag: Option<String>,
    },
    Deleted {
        uid: String,
    },
    Error(String),
    Busy(bool),
}

pub struct SyncHandle {
    tx: Sender<Command>,
    rx: Receiver<Message>,
}

impl SyncHandle {
    pub fn send(&self, cmd: Command) {
        let _ = self.tx.send(cmd);
    }

    pub fn try_recv(&self) -> Option<Message> {
        match self.rx.try_recv() {
            Ok(m) => Some(m),
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => None,
        }
    }
}

pub fn spawn(ctx: egui::Context) -> SyncHandle {
    let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();
    let (msg_tx, msg_rx) = mpsc::channel::<Message>();
    thread::Builder::new()
        .name("caldav-sync".into())
        .spawn(move || Worker::new(cmd_rx, msg_tx, ctx).run())
        .expect("spawn sync thread");
    SyncHandle {
        tx: cmd_tx,
        rx: msg_rx,
    }
}

struct Worker {
    rx: Receiver<Command>,
    tx: Sender<Message>,
    ctx: egui::Context,
    config: Option<Config>,
    client: Option<Client>,
    calendars: Vec<Calendar>,
}

impl Worker {
    fn new(rx: Receiver<Command>, tx: Sender<Message>, ctx: egui::Context) -> Self {
        Self {
            rx,
            tx,
            ctx,
            config: None,
            client: None,
            calendars: Vec::new(),
        }
    }

    fn send(&self, msg: Message) {
        let _ = self.tx.send(msg);
        self.ctx.request_repaint();
    }

    fn run(mut self) {
        while let Ok(cmd) = self.rx.recv() {
            self.send(Message::Busy(true));
            let result = match cmd {
                Command::Configure(cfg) => self.configure(cfg),
                Command::Refresh {
                    week_start,
                    task_list,
                } => self.refresh(week_start, &task_list),
                Command::Save(task) => self.save(task),
                Command::Delete(task) => self.delete(task),
            };
            if let Err(e) = result {
                log::error!("{e:#}");
                self.send(Message::Error(format!("{e:#}")));
            }
            self.send(Message::Busy(false));
        }
    }

    fn configure(&mut self, cfg: Config) -> Result<()> {
        let client = Client::new(&cfg.server_url, &cfg.username, &cfg.password)?;
        let calendars = client.discover()?;
        log::info!("found {} calendars", calendars.len());
        self.calendars = calendars.clone();
        self.client = Some(client);
        self.config = Some(cfg);
        self.send(Message::Calendars(calendars));
        Ok(())
    }

    fn client(&self) -> Result<(&Client, &Config)> {
        match (&self.client, &self.config) {
            (Some(c), Some(cfg)) => Ok((c, cfg)),
            _ => anyhow::bail!("not connected: open Settings and enter your CalDAV server"),
        }
    }

    fn refresh(&mut self, week_start: NaiveDate, task_list: &str) -> Result<()> {
        let (client, cfg) = self.client()?;
        let mut tasks = Vec::new();
        let mut errors = Vec::new();
        let list = self
            .calendars
            .iter()
            .find(|c| c.supports_todo && c.url == task_list)
            .or_else(|| self.calendars.iter().find(|c| c.supports_todo));
        if let Some(cal) = list {
            match client.fetch_tasks(cal) {
                Ok(mut t) => tasks.append(&mut t),
                Err(e) => errors.push(format!("{}: {e:#}", cal.name)),
            }
        }
        self.send(Message::Tasks(tasks));

        let start = crate::ical::local_midnight(week_start - Duration::days(1)).with_timezone(&Utc);
        let end = crate::ical::local_midnight(week_start + Duration::days(8)).with_timezone(&Utc);
        let mut events = Vec::new();
        for cal in self
            .calendars
            .iter()
            .filter(|c| c.supports_event && cfg.uses_event_calendar(&c.url))
        {
            match client.fetch_events(cal, start, end) {
                Ok(mut e) => events.append(&mut e),
                Err(e) => errors.push(format!("{}: {e:#}", cal.name)),
            }
        }
        self.send(Message::Events(events));
        if !errors.is_empty() {
            anyhow::bail!("{}", errors.join("\n"));
        }
        Ok(())
    }

    fn save(&mut self, mut task: Task) -> Result<()> {
        let (client, _) = self.client()?;
        let (href, etag) = client.put_task(&mut task)?;
        self.send(Message::Saved {
            uid: task.uid.clone(),
            href,
            etag,
        });
        Ok(())
    }

    fn delete(&mut self, task: Task) -> Result<()> {
        let (client, _) = self.client()?;
        if !task.href.is_empty() {
            client.delete(&task.href, task.etag.as_deref())?;
        }
        self.send(Message::Deleted { uid: task.uid });
        Ok(())
    }
}
