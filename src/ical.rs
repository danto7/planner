//! A small, round-trip-safe iCalendar (RFC 5545) reader and writer.
//!
//! The parser keeps every property it does not understand, so a VTODO that was
//! created by another client can be modified and written back without losing
//! data.

use anyhow::{anyhow, Result};
use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Property {
    pub name: String,
    pub params: Vec<(String, String)>,
    pub value: String,
}

impl Property {
    pub fn new(name: &str, value: impl Into<String>) -> Self {
        Self {
            name: name.to_ascii_uppercase(),
            params: Vec::new(),
            value: value.into(),
        }
    }

    pub fn with_param(mut self, name: &str, value: &str) -> Self {
        self.params
            .push((name.to_ascii_uppercase(), value.to_string()));
        self
    }

    pub fn param(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// The property value with iCalendar TEXT escaping removed.
    pub fn text(&self) -> String {
        unescape_text(&self.value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Component {
    pub name: String,
    pub properties: Vec<Property>,
    pub children: Vec<Component>,
}

impl Component {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_ascii_uppercase(),
            properties: Vec::new(),
            children: Vec::new(),
        }
    }

    pub fn get(&self, name: &str) -> Option<&Property> {
        self.properties
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
    }

    pub fn get_text(&self, name: &str) -> Option<String> {
        self.get(name).map(Property::text)
    }

    /// Replace the first property with this name, or append it.
    pub fn set(&mut self, prop: Property) {
        if let Some(existing) = self
            .properties
            .iter_mut()
            .find(|p| p.name.eq_ignore_ascii_case(&prop.name))
        {
            *existing = prop;
        } else {
            self.properties.push(prop);
        }
    }

    pub fn set_text(&mut self, name: &str, value: &str) {
        self.set(Property::new(name, escape_text(value)));
    }

    pub fn remove(&mut self, name: &str) {
        self.properties
            .retain(|p| !p.name.eq_ignore_ascii_case(name));
    }

    pub fn child(&self, name: &str) -> Option<&Component> {
        self.children
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
    }

    pub fn child_mut(&mut self, name: &str) -> Option<&mut Component> {
        self.children
            .iter_mut()
            .find(|c| c.name.eq_ignore_ascii_case(name))
    }

    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Component> {
        self.children
            .iter()
            .filter(move |c| c.name.eq_ignore_ascii_case(name))
    }

    pub fn serialize(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }

    fn write(&self, out: &mut String) {
        fold_line(&format!("BEGIN:{}", self.name), out);
        for p in &self.properties {
            let mut line = p.name.clone();
            for (k, v) in &p.params {
                line.push(';');
                line.push_str(k);
                line.push('=');
                if v.contains([':', ';', ',']) && !v.starts_with('"') {
                    line.push('"');
                    line.push_str(v);
                    line.push('"');
                } else {
                    line.push_str(v);
                }
            }
            line.push(':');
            line.push_str(&p.value);
            fold_line(&line, out);
        }
        for c in &self.children {
            c.write(out);
        }
        fold_line(&format!("END:{}", self.name), out);
    }
}

/// Fold a content line at 75 octets (RFC 5545 §3.1) and append it with CRLF.
fn fold_line(line: &str, out: &mut String) {
    const LIMIT: usize = 75;
    let mut first = true;
    let mut current = String::new();
    for ch in line.chars() {
        let budget = if first { LIMIT } else { LIMIT - 1 };
        if current.len() + ch.len_utf8() > budget {
            if !first {
                out.push(' ');
            }
            out.push_str(&current);
            out.push_str("\r\n");
            current.clear();
            first = false;
        }
        current.push(ch);
    }
    if !first {
        out.push(' ');
    }
    out.push_str(&current);
    out.push_str("\r\n");
}

/// Parse an iCalendar stream into its top-level components (normally one VCALENDAR).
pub fn parse(text: &str) -> Result<Vec<Component>> {
    let lines = unfold(text);
    let mut stack: Vec<Component> = Vec::new();
    let mut result = Vec::new();

    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let prop = parse_content_line(&line)?;
        if prop.name == "BEGIN" {
            stack.push(Component::new(&prop.value));
        } else if prop.name == "END" {
            let comp = stack
                .pop()
                .ok_or_else(|| anyhow!("END:{} without matching BEGIN", prop.value))?;
            if !comp.name.eq_ignore_ascii_case(&prop.value) {
                return Err(anyhow!(
                    "mismatched END:{} for BEGIN:{}",
                    prop.value,
                    comp.name
                ));
            }
            match stack.last_mut() {
                Some(parent) => parent.children.push(comp),
                None => result.push(comp),
            }
        } else if let Some(current) = stack.last_mut() {
            current.properties.push(prop);
        }
        // Properties outside any component are ignored.
    }
    if !stack.is_empty() {
        return Err(anyhow!(
            "unterminated component {}",
            stack.last().unwrap().name
        ));
    }
    Ok(result)
}

fn unfold(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        if (raw.starts_with(' ') || raw.starts_with('\t')) && !lines.is_empty() {
            lines.last_mut().unwrap().push_str(&raw[1..]);
        } else {
            lines.push(raw.to_string());
        }
    }
    lines
}

fn parse_content_line(line: &str) -> Result<Property> {
    // NAME(;PARAM=VALUE)*:VALUE — parameter values may be quoted and contain ':'.
    let bytes = line.as_bytes();
    let mut i = 0;
    let mut in_quotes = false;
    let mut name_end = None;
    let mut value_start = None;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => in_quotes = !in_quotes,
            b';' if !in_quotes && name_end.is_none() => name_end = Some(i),
            b':' if !in_quotes => {
                if name_end.is_none() {
                    name_end = Some(i);
                }
                value_start = Some(i + 1);
                break;
            }
            _ => {}
        }
        i += 1;
    }
    let (name_end, value_start) = match (name_end, value_start) {
        (Some(n), Some(v)) => (n, v),
        _ => return Err(anyhow!("malformed content line: {line:?}")),
    };
    let name = line[..name_end].trim().to_ascii_uppercase();
    let value = line[value_start..].to_string();
    let mut params = Vec::new();
    if name_end < value_start - 1 {
        let param_str = &line[name_end + 1..value_start - 1];
        for part in split_unquoted(param_str, ';') {
            if let Some((k, v)) = part.split_once('=') {
                let v = v.trim_matches('"').to_string();
                params.push((k.trim().to_ascii_uppercase(), v));
            }
        }
    }
    Ok(Property {
        name,
        params,
        value,
    })
}

fn split_unquoted(s: &str, sep: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in s.chars() {
        if ch == '"' {
            in_quotes = !in_quotes;
            current.push(ch);
        } else if ch == sep && !in_quotes {
            parts.push(std::mem::take(&mut current));
        } else {
            current.push(ch);
        }
    }
    parts.push(current);
    parts
}

pub fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            c => out.push(c),
        }
    }
    out
}

pub fn unescape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') | Some('N') => out.push('\n'),
                Some(c) => out.push(c),
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// A DATE or DATE-TIME property value, converted to the local time zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeValue {
    Date(NaiveDate),
    DateTime(DateTime<Local>),
}

impl TimeValue {
    /// The local date this value falls on.
    pub fn date(&self) -> NaiveDate {
        match self {
            TimeValue::Date(d) => *d,
            TimeValue::DateTime(dt) => dt.date_naive(),
        }
    }

    /// Start of the value as a local date-time (midnight for dates).
    pub fn start(&self) -> DateTime<Local> {
        match self {
            TimeValue::Date(d) => local_midnight(*d),
            TimeValue::DateTime(dt) => *dt,
        }
    }
}

pub fn local_midnight(d: NaiveDate) -> DateTime<Local> {
    let naive = d.and_hms_opt(0, 0, 0).unwrap();
    Local
        .from_local_datetime(&naive)
        .single()
        .or_else(|| Local.from_local_datetime(&naive).earliest())
        .unwrap_or_else(|| Local.from_utc_datetime(&naive))
}

pub fn parse_time(prop: &Property) -> Option<TimeValue> {
    let value = prop.value.trim();
    let is_date = prop.param("VALUE").map(|v| v.eq_ignore_ascii_case("DATE")) == Some(true)
        || (value.len() == 8 && !value.contains('T'));
    if is_date {
        return NaiveDate::parse_from_str(value, "%Y%m%d")
            .ok()
            .map(TimeValue::Date);
    }
    let (body, utc) = match value.strip_suffix('Z') {
        Some(b) => (b, true),
        None => (value, false),
    };
    let naive = NaiveDateTime::parse_from_str(body, "%Y%m%dT%H%M%S").ok()?;
    if utc {
        return Some(TimeValue::DateTime(
            Utc.from_utc_datetime(&naive).with_timezone(&Local),
        ));
    }
    if let Some(tzid) = prop.param("TZID") {
        if let Ok(tz) = tzid.parse::<chrono_tz::Tz>() {
            if let Some(dt) = tz.from_local_datetime(&naive).earliest() {
                return Some(TimeValue::DateTime(dt.with_timezone(&Local)));
            }
        }
        // Unknown time zone identifier: fall through and treat as floating.
    }
    Local
        .from_local_datetime(&naive)
        .earliest()
        .map(TimeValue::DateTime)
}

/// Build a DATE or DATE-TIME (UTC) property.
pub fn time_property(name: &str, value: TimeValue) -> Property {
    match value {
        TimeValue::Date(d) => {
            Property::new(name, d.format("%Y%m%d").to_string()).with_param("VALUE", "DATE")
        }
        TimeValue::DateTime(dt) => Property::new(name, format_utc(dt.with_timezone(&Utc))),
    }
}

pub fn format_utc(dt: DateTime<Utc>) -> String {
    dt.format("%Y%m%dT%H%M%SZ").to_string()
}

/// Parse an RFC 5545 DURATION (e.g. `PT1H30M`, `P1D`, `-PT15M`).
pub fn parse_duration(value: &str) -> Option<chrono::Duration> {
    let mut s = value.trim();
    let mut sign = 1;
    if let Some(rest) = s.strip_prefix('-') {
        sign = -1;
        s = rest;
    } else if let Some(rest) = s.strip_prefix('+') {
        s = rest;
    }
    let s = s.strip_prefix('P')?;
    let mut total = chrono::Duration::zero();
    let mut num = String::new();
    let mut in_time = false;
    for ch in s.chars() {
        match ch {
            'T' => in_time = true,
            d if d.is_ascii_digit() => num.push(d),
            unit => {
                let n: i64 = num.parse().ok()?;
                num.clear();
                total += match (unit, in_time) {
                    ('W', _) => chrono::Duration::weeks(n),
                    ('D', _) => chrono::Duration::days(n),
                    ('H', true) => chrono::Duration::hours(n),
                    ('M', true) => chrono::Duration::minutes(n),
                    ('S', true) => chrono::Duration::seconds(n),
                    _ => return None,
                };
            }
        }
    }
    Some(total * sign)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//EN\r\nBEGIN:VTODO\r\nUID:abc-123\r\nSUMMARY:Buy milk\\, eggs\\; and bread\r\nDESCRIPTION:Line one\\nLine two that is rather long and will certainly need\r\n  to be folded when written back out again\r\nDUE;VALUE=DATE:20261007\r\nDTSTART;TZID=Europe/Berlin:20261007T090000\r\nATTENDEE;CN=\"Doe, John\";ROLE=REQ-PARTICIPANT:mailto:john@example.com\r\nSTATUS:NEEDS-ACTION\r\nEND:VTODO\r\nEND:VCALENDAR\r\n";

    #[test]
    fn parses_and_round_trips() {
        let comps = parse(SAMPLE).unwrap();
        assert_eq!(comps.len(), 1);
        let cal = &comps[0];
        assert_eq!(cal.name, "VCALENDAR");
        let todo = cal.child("VTODO").unwrap();
        assert_eq!(
            todo.get_text("SUMMARY").unwrap(),
            "Buy milk, eggs; and bread"
        );
        assert!(todo.get_text("DESCRIPTION").unwrap().starts_with(
            "Line one\nLine two that is rather long and will certainly need to be folded"
        ));
        let attendee = todo.get("ATTENDEE").unwrap();
        assert_eq!(attendee.param("CN"), Some("Doe, John"));
        assert_eq!(attendee.value, "mailto:john@example.com");

        let written = cal.serialize();
        let again = parse(&written).unwrap();
        assert_eq!(&again[0], cal);
        for line in written.split("\r\n") {
            assert!(line.len() <= 75, "line too long: {line}");
        }
    }

    #[test]
    fn parses_times() {
        let due = Property::new("DUE", "20261007").with_param("VALUE", "DATE");
        assert_eq!(
            parse_time(&due),
            Some(TimeValue::Date(
                NaiveDate::from_ymd_opt(2026, 10, 7).unwrap()
            ))
        );
        let utc = Property::new("DTSTART", "20261007T120000Z");
        let TimeValue::DateTime(dt) = parse_time(&utc).unwrap() else {
            panic!()
        };
        assert_eq!(dt.with_timezone(&Utc).format("%H%M").to_string(), "1200");
        let zoned = Property::new("DTSTART", "20261007T140000").with_param("TZID", "Europe/Berlin");
        let TimeValue::DateTime(dt) = parse_time(&zoned).unwrap() else {
            panic!()
        };
        assert_eq!(dt.with_timezone(&Utc).format("%H%M").to_string(), "1200");
    }

    #[test]
    fn parses_durations() {
        assert_eq!(
            parse_duration("PT1H30M"),
            Some(chrono::Duration::minutes(90))
        );
        assert_eq!(parse_duration("P1D"), Some(chrono::Duration::days(1)));
        assert_eq!(
            parse_duration("-PT15M"),
            Some(chrono::Duration::minutes(-15))
        );
    }
}
