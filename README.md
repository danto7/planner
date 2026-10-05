# Week Planner

A desktop week planner for people who keep their to-dos in CalDAV. It shows
your calendar events next to your tasks so you can drag the week into shape,
and every change is written straight back to the server.

![Week Planner showing the inbox and a week grid](docs/screenshot.png)

- **Week grid** – seven days with an all-day row and hourly slots. Calendar
  events (VEVENT) are drawn read-only so you can plan around them.
- **Inbox** – every task (VTODO) without a date, plus anything overdue. Type a
  task and press Enter to capture it while you look at the week.
- **Drag and drop** – drag a task from the inbox onto a day (all-day) or onto a
  time slot (snaps to 15 minutes). Drag it back to the inbox to unschedule.
- **Edit** – click any task to change its summary, notes, date, time, length,
  priority or completion. Tick the checkbox to complete it.
- **Sync** – tasks and events come from any CalDAV server (Nextcloud, Radicale,
  Baïkal, iCloud, Fastmail, …) and tasks are written back with etag checks, so
  an edit made elsewhere is never overwritten silently. The week refreshes
  every five minutes and whenever you navigate.

Built in Rust with [egui/eframe](https://github.com/emilk/egui).

## Build and run

```sh
cargo run --release
```

On Linux you need the usual GUI libraries (X11 or Wayland, OpenGL,
`libxkbcommon`). No cmake or system TLS library is required.

On first start the settings dialog asks for your server URL, username and
password. Any of these work as the URL: the bare host, the `/.well-known/caldav`
path, your principal URL or the calendar home; the app discovers the rest. Use
an app-specific password where your provider offers one.

## Configuration

Settings are stored in `planner/config.toml` under your OS config directory
(`~/.config` on Linux, `~/Library/Application Support` on macOS,
`%APPDATA%` on Windows). Set `PLANNER_CONFIG=/path/to/config.toml` to use a
different file.

```toml
server_url = "https://cloud.example.com/remote.php/dav"
username = "me"
password = "app-password"
default_task_calendar = ""   # calendar URL for new tasks; empty = first task calendar
task_calendars = []          # calendar URLs to load tasks from; empty = all
event_calendars = []         # calendar URLs to load events from; empty = all
day_start_hour = 7
day_end_hour = 21
default_task_minutes = 60
show_completed = false
```

The Settings dialog edits all of this, including which calendars are used
once they have been discovered.

## How tasks are stored

- A task dropped on a day gets `DUE;VALUE=DATE`.
- A task dropped on a time slot gets `DTSTART` and `DUE` (slot start and end),
  so clients that show either property agree.
- Unscheduling removes both. Completing sets `STATUS:COMPLETED`,
  `PERCENT-COMPLETE:100` and `COMPLETED`.
- All other properties of the VTODO are preserved byte for byte.

## Development

```sh
cargo test                          # unit tests (iCalendar, CalDAV XML, model)
cargo clippy
```

There is also an end-to-end test that talks to a real server. With a local
[Radicale](https://radicale.org) on port 5232:

```sh
PLANNER_TEST_SERVER=http://127.0.0.1:5232/ PLANNER_TEST_USER=demo \
PLANNER_TEST_PASSWORD=x cargo test -- --ignored
```

For UI work in a headless environment, the `screenshot` feature saves a PNG
after a delay and exits:

```sh
cargo build --features screenshot
PLANNER_SCREENSHOT=out.png PLANNER_SCREENSHOT_AFTER_MS=3000 \
  xvfb-run -a target/debug/planner
```

### Layout

| Path | Purpose |
| --- | --- |
| `src/ical.rs` | Round-trip-safe iCalendar parser and writer |
| `src/model.rs` | `Task`, `Event`, `Calendar` and their mapping to VTODO/VEVENT |
| `src/caldav.rs` | HTTP client: discovery, `calendar-query` reports, PUT/DELETE |
| `src/sync.rs` | Background thread so the UI never blocks on the network |
| `src/app.rs` | Application state, actions and top-level layout |
| `src/ui/` | Inbox, week grid, task editor, settings dialog |
