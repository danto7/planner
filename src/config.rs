//! Persistent configuration, stored as TOML in the OS config directory.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    /// CalDAV server URL. May be the bare host, the well-known URL, the
    /// principal URL or the calendar home — discovery handles all of them.
    pub server_url: String,
    pub username: String,
    pub password: String,
    /// URL of the task list (calendar collection) shown in the planner.
    /// Empty = the first calendar that supports VTODO.
    #[serde(alias = "default_task_calendar")]
    pub task_list: String,
    /// Calendars to load events from. Empty = every calendar that supports VEVENT.
    pub event_calendars: Vec<String>,
    /// First hour shown in the week grid.
    pub day_start_hour: u32,
    /// Last hour shown in the week grid (exclusive).
    pub day_end_hour: u32,
    /// Default duration in minutes for a task dropped into a time slot.
    pub default_task_minutes: i64,
    pub show_completed: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            username: String::new(),
            password: String::new(),
            task_list: String::new(),
            event_calendars: Vec::new(),
            day_start_hour: 7,
            day_end_hour: 21,
            default_task_minutes: 60,
            show_completed: false,
        }
    }
}

impl Config {
    pub fn path() -> PathBuf {
        if let Ok(p) = std::env::var("PLANNER_CONFIG") {
            return PathBuf::from(p);
        }
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("planner")
            .join("config.toml")
    }

    pub fn load() -> Result<Option<Config>> {
        let path = Self::path();
        if !path.exists() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let cfg: Config =
            toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        Ok(Some(cfg))
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let text = toml::to_string_pretty(self)?;
        std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    pub fn is_configured(&self) -> bool {
        !self.server_url.trim().is_empty()
    }

    pub fn uses_event_calendar(&self, url: &str) -> bool {
        self.event_calendars.is_empty() || self.event_calendars.iter().any(|c| c == url)
    }
}
