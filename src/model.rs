//! Domain model: calendars, tasks (VTODO) and events (VEVENT).

use crate::ical::{
    self, local_midnight, parse_time, time_property, Component, Property, TimeValue,
};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Duration, Local, NaiveDate, Utc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Calendar {
    pub url: String,
    pub name: String,
    pub supports_todo: bool,
    pub supports_event: bool,
    /// CSS-style colour such as `#3a87adff`, if the server reports one.
    pub color: Option<String>,
}

impl Calendar {
    pub fn rgb(&self) -> Option<[u8; 3]> {
        let hex = self.color.as_ref()?.trim().trim_start_matches('#');
        if hex.len() < 6 {
            return None;
        }
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some([r, g, b])
    }
}

/// Where a task is planned in the week.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    /// Planned for a day without a specific time.
    Day(NaiveDate),
    /// Planned for a time slot.
    At {
        start: DateTime<Local>,
        duration: Duration,
    },
}

impl When {
    pub fn date(&self) -> NaiveDate {
        match self {
            When::Day(d) => *d,
            When::At { start, .. } => start.date_naive(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Task {
    /// Full URL of the resource on the server. Empty until first saved.
    pub href: String,
    pub etag: Option<String>,
    /// Full URL of the calendar collection this task belongs to.
    pub calendar: String,
    pub uid: String,
    pub summary: String,
    pub description: String,
    pub completed: bool,
    pub when: Option<When>,
    /// 0 = undefined, 1 = highest … 9 = lowest.
    pub priority: u8,
    /// The complete VCALENDAR as received from (or destined for) the server.
    pub ical: Component,
}

impl Task {
    pub fn new(calendar: &str, summary: &str) -> Self {
        let uid = uuid::Uuid::new_v4().to_string();
        let mut cal = Component::new("VCALENDAR");
        cal.properties.push(Property::new("VERSION", "2.0"));
        cal.properties
            .push(Property::new("PRODID", "-//planner//Week Planner//EN"));
        let mut todo = Component::new("VTODO");
        todo.properties.push(Property::new("UID", uid.clone()));
        todo.properties
            .push(Property::new("CREATED", ical::format_utc(Utc::now())));
        cal.children.push(todo);
        let mut task = Task {
            href: String::new(),
            etag: None,
            calendar: calendar.to_string(),
            uid,
            summary: summary.to_string(),
            description: String::new(),
            completed: false,
            when: None,
            priority: 0,
            ical: cal,
        };
        task.apply_to_ical();
        task
    }

    pub fn from_ics(href: &str, etag: Option<String>, calendar: &str, text: &str) -> Result<Self> {
        let comps = ical::parse(text)?;
        let cal = comps
            .into_iter()
            .find(|c| c.name == "VCALENDAR")
            .ok_or_else(|| anyhow!("{href}: no VCALENDAR"))?;
        let todo = cal
            .child("VTODO")
            .ok_or_else(|| anyhow!("{href}: no VTODO"))?;
        let uid = todo
            .get_text("UID")
            .unwrap_or_else(|| href.rsplit('/').next().unwrap_or(href).to_string());
        let summary = todo.get_text("SUMMARY").unwrap_or_default();
        let description = todo.get_text("DESCRIPTION").unwrap_or_default();
        let completed = todo
            .get("STATUS")
            .map(|s| s.value.eq_ignore_ascii_case("COMPLETED"))
            .unwrap_or(false)
            || todo.get("COMPLETED").is_some();
        let priority = todo
            .get("PRIORITY")
            .and_then(|p| p.value.trim().parse().ok())
            .unwrap_or(0);
        let when = read_when(todo);
        Ok(Task {
            href: href.to_string(),
            etag,
            calendar: calendar.to_string(),
            uid,
            summary,
            description,
            completed,
            when,
            priority,
            ical: cal,
        })
    }

    pub fn is_inbox(&self) -> bool {
        self.when.is_none()
    }

    /// Write the editable fields back into the VTODO and return the ICS text.
    pub fn render_ics(&mut self) -> String {
        self.apply_to_ical();
        self.ical.serialize()
    }

    fn apply_to_ical(&mut self) {
        let todo = match self.ical.child_mut("VTODO") {
            Some(t) => t,
            None => {
                self.ical.children.push(Component::new("VTODO"));
                self.ical.child_mut("VTODO").unwrap()
            }
        };
        todo.set(Property::new("UID", self.uid.clone()));
        todo.set_text("SUMMARY", self.summary.trim());
        if self.description.trim().is_empty() {
            todo.remove("DESCRIPTION");
        } else {
            todo.set_text("DESCRIPTION", &self.description);
        }
        if self.priority > 0 {
            todo.set(Property::new("PRIORITY", self.priority.to_string()));
        } else {
            todo.remove("PRIORITY");
        }

        match self.when {
            None => {
                todo.remove("DTSTART");
                todo.remove("DUE");
            }
            Some(When::Day(d)) => {
                todo.remove("DTSTART");
                todo.set(time_property("DUE", TimeValue::Date(d)));
            }
            Some(When::At { start, duration }) => {
                todo.set(time_property("DTSTART", TimeValue::DateTime(start)));
                todo.set(time_property("DUE", TimeValue::DateTime(start + duration)));
            }
        }
        // DURATION and DUE are mutually exclusive.
        todo.remove("DURATION");

        if self.completed {
            todo.set(Property::new("STATUS", "COMPLETED"));
            todo.set(Property::new("PERCENT-COMPLETE", "100"));
            if todo.get("COMPLETED").is_none() {
                todo.set(Property::new("COMPLETED", ical::format_utc(Utc::now())));
            }
        } else {
            todo.set(Property::new("STATUS", "NEEDS-ACTION"));
            todo.remove("COMPLETED");
            todo.remove("PERCENT-COMPLETE");
        }

        let now = ical::format_utc(Utc::now());
        todo.set(Property::new("DTSTAMP", now.clone()));
        todo.set(Property::new("LAST-MODIFIED", now));
        let seq: u64 = todo
            .get("SEQUENCE")
            .and_then(|s| s.value.trim().parse().ok())
            .unwrap_or(0);
        todo.set(Property::new("SEQUENCE", (seq + 1).to_string()));
    }
}

fn read_when(todo: &Component) -> Option<When> {
    let dtstart = todo.get("DTSTART").and_then(parse_time);
    let due = todo.get("DUE").and_then(parse_time);
    let duration = todo
        .get("DURATION")
        .and_then(|p| ical::parse_duration(&p.value));
    match (dtstart, due) {
        (Some(TimeValue::DateTime(start)), Some(TimeValue::DateTime(due))) => {
            let duration = (due - start).max(Duration::minutes(15));
            Some(When::At { start, duration })
        }
        (Some(TimeValue::DateTime(start)), _) => Some(When::At {
            start,
            duration: duration
                .filter(|d| *d > Duration::zero())
                .unwrap_or_else(|| Duration::hours(1)),
        }),
        (_, Some(TimeValue::DateTime(due))) => Some(When::At {
            start: due,
            duration: Duration::hours(1),
        }),
        (_, Some(TimeValue::Date(d))) => Some(When::Day(d)),
        (Some(TimeValue::Date(d)), None) => Some(When::Day(d)),
        (None, None) => None,
    }
}

#[derive(Debug, Clone)]
pub struct Event {
    pub href: String,
    pub calendar: String,
    pub uid: String,
    pub summary: String,
    pub location: String,
    pub all_day: bool,
    pub start: DateTime<Local>,
    /// Exclusive end.
    pub end: DateTime<Local>,
}

impl Event {
    /// Parse every VEVENT in an ICS stream (a server that expands recurrences
    /// returns several instances in one resource).
    pub fn from_ics(href: &str, calendar: &str, text: &str) -> Result<Vec<Event>> {
        let comps = ical::parse(text)?;
        let cal = comps
            .into_iter()
            .find(|c| c.name == "VCALENDAR")
            .ok_or_else(|| anyhow!("{href}: no VCALENDAR"))?;
        let mut events = Vec::new();
        for ev in cal.children_named("VEVENT") {
            if ev
                .get("STATUS")
                .map(|s| s.value.eq_ignore_ascii_case("CANCELLED"))
                .unwrap_or(false)
            {
                continue;
            }
            let Some(start) = ev.get("DTSTART").and_then(parse_time) else {
                continue;
            };
            let end_prop = ev.get("DTEND").and_then(parse_time);
            let duration = ev
                .get("DURATION")
                .and_then(|p| ical::parse_duration(&p.value));
            let (all_day, start_dt, end_dt) = match start {
                TimeValue::Date(d) => {
                    let end_date = match end_prop {
                        Some(e) => e.date(),
                        None => match duration {
                            Some(dur) => d + Duration::days(dur.num_days().max(1)),
                            None => d + Duration::days(1),
                        },
                    };
                    let end_date = end_date.max(d + Duration::days(1));
                    (true, local_midnight(d), local_midnight(end_date))
                }
                TimeValue::DateTime(s) => {
                    let e = match end_prop {
                        Some(e) => e.start(),
                        None => s + duration.unwrap_or_else(Duration::zero),
                    };
                    (false, s, e.max(s))
                }
            };
            events.push(Event {
                href: href.to_string(),
                calendar: calendar.to_string(),
                uid: ev.get_text("UID").unwrap_or_default(),
                summary: ev.get_text("SUMMARY").unwrap_or_default(),
                location: ev.get_text("LOCATION").unwrap_or_default(),
                all_day,
                start: start_dt,
                end: end_dt,
            });
        }
        Ok(events)
    }

    /// Reinterpret an expanded instance of an all-day event. Servers that
    /// expand recurrences (RFC 4791 §9.6.5) return every instance as a UTC
    /// DATE-TIME, so an all-day event comes back as midnight UTC; the
    /// original dates are recovered from the UTC calendar day.
    pub fn force_all_day(&mut self) {
        if self.all_day {
            return;
        }
        let start = self.start.with_timezone(&Utc).date_naive();
        let mut end = self.end.with_timezone(&Utc).date_naive();
        if end <= start {
            end = start + Duration::days(1);
        }
        self.all_day = true;
        self.start = local_midnight(start);
        self.end = local_midnight(end);
    }

    pub fn covers_day(&self, day: NaiveDate) -> bool {
        let day_start = local_midnight(day);
        let day_end = local_midnight(day + Duration::days(1));
        self.start < day_end && self.end > day_start
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_task_round_trips_through_ics() {
        let mut t = Task::new("https://example.com/cal/", "Write report");
        let day = NaiveDate::from_ymd_opt(2026, 10, 7).unwrap();
        t.when = Some(When::Day(day));
        let ics = t.render_ics();
        let parsed =
            Task::from_ics("https://example.com/cal/x.ics", None, &t.calendar, &ics).unwrap();
        assert_eq!(parsed.summary, "Write report");
        assert_eq!(parsed.when, Some(When::Day(day)));
        assert!(!parsed.completed);

        let start = local_midnight(day) + Duration::hours(9);
        t.when = Some(When::At {
            start,
            duration: Duration::minutes(90),
        });
        t.completed = true;
        let ics = t.render_ics();
        let parsed = Task::from_ics("x", None, &t.calendar, &ics).unwrap();
        assert_eq!(
            parsed.when,
            Some(When::At {
                start,
                duration: Duration::minutes(90)
            })
        );
        assert!(parsed.completed);
        assert!(ics.contains("STATUS:COMPLETED"));
        assert!(ics.contains("SEQUENCE:3"));
    }

    #[test]
    fn unknown_properties_survive() {
        let ics = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VTODO\r\nUID:u1\r\nSUMMARY:Old\r\nX-FOO-BAR;X-P=1:keep me\r\nDUE;VALUE=DATE:20261001\r\nEND:VTODO\r\nEND:VCALENDAR\r\n";
        let mut t = Task::from_ics("x", Some("\"e1\"".into()), "c", ics).unwrap();
        t.summary = "New".into();
        t.when = None;
        let out = t.render_ics();
        assert!(out.contains("X-FOO-BAR;X-P=1:keep me"));
        assert!(out.contains("SUMMARY:New"));
        assert!(!out.contains("DUE"));
    }

    #[test]
    fn expanded_all_day_instance_is_restored() {
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:c\r\nSUMMARY:Conference\r\nDTSTART:20261008T000000Z\r\nDTEND:20261010T000000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let mut ev = Event::from_ics("x", "c", ics).unwrap().remove(0);
        assert!(!ev.all_day);
        ev.force_all_day();
        assert!(ev.all_day);
        assert!(ev.covers_day(NaiveDate::from_ymd_opt(2026, 10, 8).unwrap()));
        assert!(ev.covers_day(NaiveDate::from_ymd_opt(2026, 10, 9).unwrap()));
        assert!(!ev.covers_day(NaiveDate::from_ymd_opt(2026, 10, 10).unwrap()));
    }

    #[test]
    fn parses_events() {
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:e1\r\nSUMMARY:Standup\r\nDTSTART:20261006T080000Z\r\nDTEND:20261006T083000Z\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:e2\r\nSUMMARY:Holiday\r\nDTSTART;VALUE=DATE:20261008\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let events = Event::from_ics("x", "c", ics).unwrap();
        assert_eq!(events.len(), 2);
        assert!(!events[0].all_day);
        assert_eq!(events[0].end - events[0].start, Duration::minutes(30));
        assert!(events[1].all_day);
        assert!(events[1].covers_day(NaiveDate::from_ymd_opt(2026, 10, 8).unwrap()));
        assert!(!events[1].covers_day(NaiveDate::from_ymd_opt(2026, 10, 9).unwrap()));
    }
}
