//! Minimal CalDAV client: discovery, calendar-query reports and PUT/DELETE.

use crate::model::{Calendar, Event, Task};
use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use reqwest::blocking::{Client as Http, Response};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE, ETAG, LOCATION};
use reqwest::{Method, StatusCode};
use std::collections::HashSet;
use std::time::Duration;
use url::Url;

const NS_DAV: &str = "DAV:";
const NS_CALDAV: &str = "urn:ietf:params:xml:ns:caldav";
const NS_APPLE: &str = "http://apple.com/ns/ical/";

pub struct Client {
    http: Http,
    base: Url,
    username: String,
    password: String,
}

/// One `<D:response>` of a multistatus body, with the properties we care about.
#[derive(Debug, Default, Clone)]
pub struct DavResponse {
    pub href: String,
    pub etag: Option<String>,
    pub calendar_data: Option<String>,
    pub displayname: Option<String>,
    pub is_calendar: bool,
    pub components: Vec<String>,
    pub color: Option<String>,
    pub principal: Option<String>,
    pub calendar_home: Option<String>,
}

/// A raw calendar object returned by a report.
#[derive(Debug, Clone)]
pub struct Resource {
    pub href: String,
    pub etag: Option<String>,
    pub data: String,
}

impl Client {
    pub fn new(server_url: &str, username: &str, password: &str) -> Result<Self> {
        let mut raw = server_url.trim().to_string();
        if raw.is_empty() {
            bail!("no server URL configured");
        }
        if !raw.contains("://") {
            raw = format!("https://{raw}");
        }
        let base = Url::parse(&raw).with_context(|| format!("invalid server URL {raw:?}"))?;
        let http = Http::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(60))
            .user_agent(concat!("planner/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            http,
            base,
            username: username.to_string(),
            password: password.to_string(),
        })
    }

    fn request(
        &self,
        method: &str,
        url: &str,
        depth: Option<&str>,
        body: Option<&str>,
        extra: HeaderMap,
    ) -> Result<Response> {
        let mut url = self.resolve(url)?;
        for _ in 0..6 {
            let http_method = Method::from_bytes(method.as_bytes())?;
            let mut req = self.http.request(http_method, url.clone());
            if !self.username.is_empty() {
                req = req.basic_auth(&self.username, Some(&self.password));
            }
            if let Some(d) = depth {
                req = req.header("Depth", d);
            }
            if let Some(b) = body {
                req = req
                    .header(CONTENT_TYPE, "application/xml; charset=utf-8")
                    .body(b.to_string());
            }
            req = req.headers(extra.clone());
            let resp = req.send().with_context(|| format!("{method} {url}"))?;
            if resp.status().is_redirection() {
                if let Some(loc) = resp.headers().get(LOCATION).and_then(|v| v.to_str().ok()) {
                    url = url
                        .join(loc)
                        .with_context(|| format!("bad redirect {loc:?}"))?;
                    continue;
                }
            }
            return Ok(resp);
        }
        bail!("too many redirects for {url}")
    }

    fn resolve(&self, href: &str) -> Result<Url> {
        if href.contains("://") {
            return Url::parse(href).with_context(|| format!("bad URL {href:?}"));
        }
        self.base
            .join(href)
            .with_context(|| format!("cannot resolve {href:?}"))
    }

    fn absolute(&self, href: &str) -> String {
        self.resolve(href)
            .map(|u| u.to_string())
            .unwrap_or_else(|_| href.to_string())
    }

    fn propfind(&self, url: &str, depth: &str, props: &str) -> Result<Vec<DavResponse>> {
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?><D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav" xmlns:A="http://apple.com/ns/ical/"><D:prop>{props}</D:prop></D:propfind>"#
        );
        let resp = self.request("PROPFIND", url, Some(depth), Some(&body), HeaderMap::new())?;
        let status = resp.status();
        let text = resp.text()?;
        check_status(status, "PROPFIND", url, &text)?;
        let mut items = parse_multistatus(&text)?;
        for item in &mut items {
            item.href = self.absolute(&item.href);
            if let Some(p) = &item.principal {
                item.principal = Some(self.absolute(p));
            }
            if let Some(h) = &item.calendar_home {
                item.calendar_home = Some(self.absolute(h));
            }
        }
        Ok(items)
    }

    /// Discover every calendar collection reachable from the configured URL.
    pub fn discover(&self) -> Result<Vec<Calendar>> {
        let start = self.base.to_string();
        let home = match self.find_calendar_home(&start) {
            Ok(Some(home)) => home,
            Ok(None) => start.clone(),
            Err(e) => {
                log::warn!("calendar home discovery failed, trying {start} directly: {e:#}");
                start.clone()
            }
        };
        log::info!("calendar home: {home}");
        let mut calendars = self.list_calendars(&home)?;
        if calendars.is_empty() && home != start {
            calendars = self.list_calendars(&start)?;
        }
        if calendars.is_empty() {
            bail!("no calendars found under {home}");
        }
        calendars.sort_by_key(|c| c.name.to_lowercase());
        Ok(calendars)
    }

    fn find_calendar_home(&self, start: &str) -> Result<Option<String>> {
        let props = "<D:current-user-principal/><D:resourcetype/><C:calendar-home-set/>";
        let mut candidates = vec![start.to_string()];
        if let Ok(wk) = self.resolve("/.well-known/caldav") {
            candidates.push(wk.to_string());
        }
        let mut principal = None;
        for url in &candidates {
            match self.propfind(url, "0", props) {
                Ok(items) => {
                    if let Some(home) = items.iter().find_map(|i| i.calendar_home.clone()) {
                        return Ok(Some(home));
                    }
                    if let Some(p) = items.iter().find_map(|i| i.principal.clone()) {
                        principal = Some(p);
                        break;
                    }
                }
                Err(e) => log::debug!("PROPFIND {url} failed: {e:#}"),
            }
        }
        let Some(principal) = principal else {
            return Ok(None);
        };
        let items = self.propfind(&principal, "0", "<C:calendar-home-set/>")?;
        Ok(items.into_iter().find_map(|i| i.calendar_home))
    }

    fn list_calendars(&self, home: &str) -> Result<Vec<Calendar>> {
        let props = "<D:resourcetype/><D:displayname/><C:supported-calendar-component-set/><A:calendar-color/>";
        let items = self.propfind(home, "1", props)?;
        Ok(items
            .into_iter()
            .filter(|i| i.is_calendar)
            .map(|i| {
                let comps = &i.components;
                // Servers that omit the component set are assumed to support both.
                let (todo, event) = if comps.is_empty() {
                    (true, true)
                } else {
                    (
                        comps.iter().any(|c| c == "VTODO"),
                        comps.iter().any(|c| c == "VEVENT"),
                    )
                };
                let url = with_trailing_slash(&i.href);
                let name = i
                    .displayname
                    .filter(|n| !n.trim().is_empty())
                    .unwrap_or_else(|| {
                        url.trim_end_matches('/')
                            .rsplit('/')
                            .next()
                            .unwrap_or("calendar")
                            .to_string()
                    });
                Calendar {
                    url,
                    name,
                    supports_todo: todo,
                    supports_event: event,
                    color: i.color,
                }
            })
            .collect())
    }

    fn report(&self, url: &str, body: &str) -> Result<Vec<Resource>> {
        let resp = self.request("REPORT", url, Some("1"), Some(body), HeaderMap::new())?;
        let status = resp.status();
        let text = resp.text()?;
        check_status(status, "REPORT", url, &text)?;
        Ok(parse_multistatus(&text)?
            .into_iter()
            .filter_map(|i| {
                i.calendar_data.map(|data| Resource {
                    href: self.absolute(&i.href),
                    etag: i.etag,
                    data,
                })
            })
            .collect())
    }

    pub fn fetch_tasks(&self, calendar: &Calendar) -> Result<Vec<Task>> {
        let body = r#"<?xml version="1.0" encoding="utf-8"?><C:calendar-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"><D:prop><D:getetag/><C:calendar-data/></D:prop><C:filter><C:comp-filter name="VCALENDAR"><C:comp-filter name="VTODO"/></C:comp-filter></C:filter></C:calendar-query>"#;
        let mut tasks = Vec::new();
        for res in self.report(&calendar.url, body)? {
            match Task::from_ics(&res.href, res.etag.clone(), &calendar.url, &res.data) {
                Ok(t) => tasks.push(t),
                Err(e) => log::warn!("skipping {}: {e:#}", res.href),
            }
        }
        Ok(tasks)
    }

    pub fn fetch_events(
        &self,
        calendar: &Calendar,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Event>> {
        let s = start.format("%Y%m%dT%H%M%SZ");
        let e = end.format("%Y%m%dT%H%M%SZ");
        let expanded = format!(
            r#"<?xml version="1.0" encoding="utf-8"?><C:calendar-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"><D:prop><D:getetag/><C:calendar-data><C:expand start="{s}" end="{e}"/></C:calendar-data></D:prop><C:filter><C:comp-filter name="VCALENDAR"><C:comp-filter name="VEVENT"><C:time-range start="{s}" end="{e}"/></C:comp-filter></C:comp-filter></C:filter></C:calendar-query>"#
        );
        let plain = format!(
            r#"<?xml version="1.0" encoding="utf-8"?><C:calendar-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"><D:prop><D:getetag/><C:calendar-data/></D:prop><C:filter><C:comp-filter name="VCALENDAR"><C:comp-filter name="VEVENT"><C:time-range start="{s}" end="{e}"/></C:comp-filter></C:comp-filter></C:filter></C:calendar-query>"#
        );
        // Expanded instances lose the DATE value type (they come back as UTC
        // date-times), so the unexpanded masters are fetched too and used to
        // tell which events are all-day.
        let (resources, all_day_uids) = match self.report(&calendar.url, &expanded) {
            Ok(r) => {
                let masters = self.report(&calendar.url, &plain).unwrap_or_default();
                (r, all_day_uids(&masters))
            }
            Err(e) => {
                log::warn!(
                    "expanded report on {} failed ({e:#}); retrying without expand",
                    calendar.url
                );
                (self.report(&calendar.url, &plain)?, HashSet::new())
            }
        };
        let mut events = Vec::new();
        for res in resources {
            match Event::from_ics(&res.href, &calendar.url, &res.data) {
                Ok(mut evs) => {
                    for ev in &mut evs {
                        if all_day_uids.contains(&ev.uid) {
                            ev.force_all_day();
                        }
                    }
                    events.append(&mut evs);
                }
                Err(e) => log::warn!("skipping {}: {e:#}", res.href),
            }
        }
        Ok(events)
    }

    /// Create or update a task. Returns the href and new etag.
    pub fn put_task(&self, task: &mut Task) -> Result<(String, Option<String>)> {
        let href = if task.href.is_empty() {
            format!("{}{}.ics", with_trailing_slash(&task.calendar), task.uid)
        } else {
            task.href.clone()
        };
        let ics = task.render_ics();
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("text/calendar; charset=utf-8"),
        );
        match &task.etag {
            Some(etag) if !etag.is_empty() => {
                headers.insert("If-Match", HeaderValue::from_str(etag)?);
            }
            _ => {
                headers.insert("If-None-Match", HeaderValue::from_static("*"));
            }
        }
        let resp = self.request("PUT", &href, None, Some(&ics), headers)?;
        let status = resp.status();
        let new_etag = resp
            .headers()
            .get(ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let text = resp.text().unwrap_or_default();
        check_status(status, "PUT", &href, &text)?;
        let etag = match new_etag {
            Some(e) => Some(e),
            None => self
                .propfind(&href, "0", "<D:getetag/>")
                .ok()
                .and_then(|items| items.into_iter().find_map(|i| i.etag)),
        };
        Ok((href, etag))
    }

    pub fn delete(&self, href: &str, etag: Option<&str>) -> Result<()> {
        let mut headers = HeaderMap::new();
        if let Some(etag) = etag.filter(|e| !e.is_empty()) {
            headers.insert("If-Match", HeaderValue::from_str(etag)?);
        }
        let resp = self.request("DELETE", href, None, None, headers)?;
        let status = resp.status();
        if status == StatusCode::NOT_FOUND {
            return Ok(());
        }
        let text = resp.text().unwrap_or_default();
        check_status(status, "DELETE", href, &text)
    }
}

fn check_status(status: StatusCode, method: &str, url: &str, body: &str) -> Result<()> {
    if status.is_success() || status == StatusCode::MULTI_STATUS {
        return Ok(());
    }
    let hint = match status {
        StatusCode::UNAUTHORIZED => " (check username and password)",
        StatusCode::PRECONDITION_FAILED => " (the item changed on the server; refresh and retry)",
        StatusCode::FORBIDDEN => " (no permission)",
        _ => "",
    };
    let snippet: String = body.chars().take(200).collect();
    Err(anyhow!("{method} {url}: HTTP {status}{hint} {snippet}"))
}

/// UIDs of VEVENTs whose master DTSTART is a DATE (all-day) value.
fn all_day_uids(masters: &[Resource]) -> HashSet<String> {
    let mut uids = HashSet::new();
    for res in masters {
        let Ok(comps) = crate::ical::parse(&res.data) else {
            continue;
        };
        for cal in comps.iter().filter(|c| c.name == "VCALENDAR") {
            for ev in cal.children_named("VEVENT") {
                let is_date = ev
                    .get("DTSTART")
                    .and_then(crate::ical::parse_time)
                    .map(|t| matches!(t, crate::ical::TimeValue::Date(_)))
                    .unwrap_or(false);
                if is_date {
                    if let Some(uid) = ev.get_text("UID") {
                        uids.insert(uid);
                    }
                }
            }
        }
    }
    uids
}

pub fn with_trailing_slash(s: &str) -> String {
    if s.ends_with('/') {
        s.to_string()
    } else {
        format!("{s}/")
    }
}

pub fn parse_multistatus(xml: &str) -> Result<Vec<DavResponse>> {
    let doc = roxmltree::Document::parse(xml).context("invalid multistatus XML")?;
    let mut out = Vec::new();
    for resp in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name() == (NS_DAV, "response").into())
    {
        let mut item = DavResponse::default();
        for child in resp.children().filter(|n| n.is_element()) {
            if child.tag_name() == (NS_DAV, "href").into() {
                if item.href.is_empty() {
                    item.href = child.text().unwrap_or_default().trim().to_string();
                }
            } else if child.tag_name() == (NS_DAV, "propstat").into() {
                let ok = child
                    .children()
                    .find(|n| n.tag_name() == (NS_DAV, "status").into())
                    .and_then(|n| n.text())
                    .map(|s| s.contains(" 200 ") || s.contains(" 207 "))
                    .unwrap_or(true);
                if !ok {
                    continue;
                }
                let Some(prop) = child
                    .children()
                    .find(|n| n.tag_name() == (NS_DAV, "prop").into())
                else {
                    continue;
                };
                for p in prop.children().filter(|n| n.is_element()) {
                    read_prop(&p, &mut item);
                }
            }
        }
        if !item.href.is_empty() {
            out.push(item);
        }
    }
    Ok(out)
}

fn read_prop(p: &roxmltree::Node, item: &mut DavResponse) {
    let ns = p.tag_name().namespace().unwrap_or("");
    let name = p.tag_name().name();
    let text = || p.text().map(|t| t.to_string());
    let first_href = || {
        p.descendants()
            .find(|n| n.tag_name() == (NS_DAV, "href").into())
            .and_then(|n| n.text())
            .map(|t| t.trim().to_string())
    };
    match (ns, name) {
        (NS_DAV, "getetag") => item.etag = text().map(|t| t.trim().to_string()),
        (NS_DAV, "displayname") => item.displayname = text(),
        (NS_DAV, "current-user-principal") => item.principal = first_href(),
        (NS_DAV, "resourcetype") => {
            item.is_calendar = p
                .children()
                .any(|n| n.tag_name() == (NS_CALDAV, "calendar").into());
        }
        (NS_CALDAV, "calendar-home-set") => item.calendar_home = first_href(),
        (NS_CALDAV, "calendar-data") => item.calendar_data = text(),
        (NS_CALDAV, "supported-calendar-component-set") => {
            item.components = p
                .children()
                .filter(|n| n.tag_name() == (NS_CALDAV, "comp").into())
                .filter_map(|n| n.attribute("name").map(|s| s.to_ascii_uppercase()))
                .collect();
        }
        (NS_APPLE, "calendar-color") => item.color = text().map(|t| t.trim().to_string()),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_calendar_listing() {
        let xml = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:" xmlns:cal="urn:ietf:params:xml:ns:caldav" xmlns:x1="http://apple.com/ns/ical/">
  <d:response>
    <d:href>/remote.php/dav/calendars/me/</d:href>
    <d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat>
  </d:response>
  <d:response>
    <d:href>/remote.php/dav/calendars/me/tasks/</d:href>
    <d:propstat>
      <d:prop>
        <d:resourcetype><d:collection/><cal:calendar/></d:resourcetype>
        <d:displayname>Tasks</d:displayname>
        <cal:supported-calendar-component-set><cal:comp name="VTODO"/></cal:supported-calendar-component-set>
        <x1:calendar-color>#FF0000FF</x1:calendar-color>
      </d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
    <d:propstat><d:prop><d:getetag/></d:prop><d:status>HTTP/1.1 404 Not Found</d:status></d:propstat>
  </d:response>
</d:multistatus>"#;
        let items = parse_multistatus(xml).unwrap();
        assert_eq!(items.len(), 2);
        assert!(!items[0].is_calendar);
        let cal = &items[1];
        assert!(cal.is_calendar);
        assert_eq!(cal.displayname.as_deref(), Some("Tasks"));
        assert_eq!(cal.components, vec!["VTODO"]);
        assert_eq!(cal.color.as_deref(), Some("#FF0000FF"));
        assert!(cal.etag.is_none());
    }

    #[test]
    fn parses_report() {
        let xml = r#"<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav"><d:response><d:href>/cal/a.ics</d:href><d:propstat><d:prop><d:getetag>"abc"</d:getetag><c:calendar-data>BEGIN:VCALENDAR
END:VCALENDAR
</c:calendar-data></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>"#;
        let items = parse_multistatus(xml).unwrap();
        assert_eq!(items[0].etag.as_deref(), Some("\"abc\""));
        assert!(items[0]
            .calendar_data
            .as_deref()
            .unwrap()
            .starts_with("BEGIN:VCALENDAR"));
    }

    /// End-to-end check against a real server. Run with:
    /// `PLANNER_TEST_SERVER=http://127.0.0.1:5232/ PLANNER_TEST_USER=demo PLANNER_TEST_PASSWORD=x cargo test -- --ignored`
    #[test]
    #[ignore]
    fn live_round_trip() {
        let server = std::env::var("PLANNER_TEST_SERVER").expect("PLANNER_TEST_SERVER");
        let user = std::env::var("PLANNER_TEST_USER").unwrap_or_default();
        let pass = std::env::var("PLANNER_TEST_PASSWORD").unwrap_or_default();
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = Client::new(&server, &user, &pass).unwrap();
        let calendars = client.discover().unwrap();
        let cal = calendars
            .iter()
            .find(|c| c.supports_todo)
            .expect("a task calendar");

        let mut task = Task::new(&cal.url, "live round trip");
        let (href, etag) = client.put_task(&mut task).unwrap();
        assert!(href.ends_with(".ics"));
        task.href = href.clone();
        task.etag = etag.clone();

        let fetched = client.fetch_tasks(cal).unwrap();
        let mine = fetched
            .iter()
            .find(|t| t.uid == task.uid)
            .expect("task on server");
        assert_eq!(mine.summary, "live round trip");
        assert!(mine.when.is_none());

        task.summary = "live round trip (edited)".into();
        task.when = Some(crate::model::When::Day(
            chrono::NaiveDate::from_ymd_opt(2026, 10, 7).unwrap(),
        ));
        let (_, etag2) = client.put_task(&mut task).unwrap();
        task.etag = etag2;
        let fetched = client.fetch_tasks(cal).unwrap();
        let mine = fetched.iter().find(|t| t.uid == task.uid).unwrap();
        assert_eq!(mine.summary, "live round trip (edited)");
        assert!(matches!(mine.when, Some(crate::model::When::Day(_))));

        // A stale etag must be rejected so we never clobber another client's edit.
        let mut stale = mine.clone();
        stale.etag = Some("\"stale\"".into());
        stale.summary = "should not win".into();
        assert!(client.put_task(&mut stale).is_err());

        client.delete(&task.href, task.etag.as_deref()).unwrap();
        let fetched = client.fetch_tasks(cal).unwrap();
        assert!(fetched.iter().all(|t| t.uid != task.uid));
    }

    #[test]
    fn principal_discovery() {
        let xml = r#"<d:multistatus xmlns:d="DAV:"><d:response><d:href>/</d:href><d:propstat><d:prop><d:current-user-principal><d:href>/principals/users/me/</d:href></d:current-user-principal></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>"#;
        let items = parse_multistatus(xml).unwrap();
        assert_eq!(items[0].principal.as_deref(), Some("/principals/users/me/"));
    }
}
