//! End-to-end tests of the CalDAV client against a real Stalwart Mail Server.
//!
//! The server binary is located via `STALWART_BIN`, `target/stalwart/stalwart`
//! (see `scripts/fetch-stalwart.sh`) or `PATH`. Without it the test is
//! skipped unless `PLANNER_REQUIRE_STALWART` is set.

mod common;

use chrono::{Datelike, Duration, NaiveDate, TimeZone, Utc};
use common::{Credentials, Stalwart};
use planner::caldav::Client;
use planner::ical::local_midnight;
use planner::model::{Calendar, Task, When};

/// One server for the whole file: provisioning takes a few seconds, the
/// scenarios themselves are fast. Each scenario gets its own user.
#[test]
fn caldav_against_stalwart() {
    let Some(server) = common::spawn_or_skip() else {
        return;
    };

    let user = server.create_user("alice");
    let (client, calendar) = connect(&server, &user);

    discovery_finds_default_calendar(&calendar);
    task_lifecycle(&client, &calendar);
    scheduling_round_trip(&client, &calendar);
    foreign_properties_survive(&client, &calendar);
    events_are_fetched_for_the_week(&server, &user, &client, &calendar);
}

fn connect(server: &Stalwart, user: &Credentials) -> (Client, Calendar) {
    // The bare base URL is what a person would type: discovery has to find
    // the principal, the calendar home and the calendars from there.
    let client = Client::new(&server.base_url, &user.username, &user.password).expect("client");
    let calendars = client
        .discover()
        .unwrap_or_else(|e| panic!("discover: {e:#}\n{}", server.log_tail()));
    let calendar = calendars
        .iter()
        .find(|c| c.supports_todo)
        .cloned()
        .unwrap_or_else(|| panic!("no task calendar in {calendars:?}"));
    (client, calendar)
}

fn discovery_finds_default_calendar(calendar: &Calendar) {
    assert!(
        calendar.url.starts_with("http://127.0.0.1:"),
        "absolute URL: {}",
        calendar.url
    );
    assert!(calendar.url.ends_with('/'));
    assert!(calendar.supports_todo && calendar.supports_event);
    assert!(!calendar.name.is_empty());
}

fn task_lifecycle(client: &Client, calendar: &Calendar) {
    assert!(
        client.fetch_tasks(calendar).unwrap().is_empty(),
        "fresh calendar has no tasks"
    );

    let mut task = Task::new(&calendar.url, "Write the report");
    task.description = "Numbers from finance first.\nThen the summary.".into();
    task.priority = 1;
    let (href, etag) = client.put_task(&mut task).expect("create");
    assert!(href.starts_with(&calendar.url));
    assert!(etag.is_some(), "server returns an etag");
    task.href = href;
    task.etag = etag;

    let fetched = client.fetch_tasks(calendar).unwrap();
    assert_eq!(fetched.len(), 1);
    let mine = &fetched[0];
    assert_eq!(mine.uid, task.uid);
    assert_eq!(mine.summary, "Write the report");
    assert_eq!(
        mine.description,
        "Numbers from finance first.\nThen the summary."
    );
    assert_eq!(mine.priority, 1);
    assert!(mine.is_inbox());
    assert!(!mine.completed);

    task.summary = "Write the quarterly report".into();
    task.completed = true;
    let (_, etag2) = client
        .put_task(&mut task)
        .expect("update with matching etag");
    assert_ne!(etag2, None);
    task.etag = etag2;
    let mine = client.fetch_tasks(calendar).unwrap().remove(0);
    assert_eq!(mine.summary, "Write the quarterly report");
    assert!(mine.completed);

    let mut stale = mine.clone();
    stale.etag = Some("\"not-the-current-etag\"".into());
    stale.summary = "must not be written".into();
    let err = client
        .put_task(&mut stale)
        .expect_err("stale etag is rejected");
    assert!(
        format!("{err:#}").contains("412"),
        "precondition failure: {err:#}"
    );
    assert_eq!(
        client.fetch_tasks(calendar).unwrap()[0].summary,
        "Write the quarterly report"
    );

    client
        .delete(&task.href, task.etag.as_deref())
        .expect("delete");
    assert!(client.fetch_tasks(calendar).unwrap().is_empty());
}

fn scheduling_round_trip(client: &Client, calendar: &Calendar) {
    let day = NaiveDate::from_ymd_opt(2026, 10, 7).unwrap();
    let mut by_day = Task::new(&calendar.url, "Pay the bill");
    by_day.when = Some(When::Day(day));
    let (href1, etag1) = client.put_task(&mut by_day).unwrap();

    let start = local_midnight(day) + Duration::hours(9) + Duration::minutes(30);
    let mut by_time = Task::new(&calendar.url, "Review pull requests");
    by_time.when = Some(When::At {
        start,
        duration: Duration::minutes(90),
    });
    let (href2, etag2) = client.put_task(&mut by_time).unwrap();

    let tasks = client.fetch_tasks(calendar).unwrap();
    let find = |uid: &str| tasks.iter().find(|t| t.uid == uid).expect("task on server");
    assert_eq!(find(&by_day.uid).when, Some(When::Day(day)));
    assert_eq!(
        find(&by_time.uid).when,
        Some(When::At {
            start,
            duration: Duration::minutes(90)
        })
    );

    // Unscheduling removes the dates again.
    let mut back = find(&by_time.uid).clone();
    back.when = None;
    let (_, etag3) = client.put_task(&mut back).unwrap();
    let tasks = client.fetch_tasks(calendar).unwrap();
    assert!(tasks
        .iter()
        .find(|t| t.uid == by_time.uid)
        .unwrap()
        .is_inbox());

    client.delete(&href1, etag1.as_deref()).unwrap();
    client.delete(&href2, etag3.or(etag2).as_deref()).unwrap();
}

fn foreign_properties_survive(client: &Client, calendar: &Calendar) {
    // A task created by another client with properties we do not model.
    let mut task = Task::new(&calendar.url, "Imported task");
    {
        let todo = task.ical.child_mut("VTODO").unwrap();
        todo.properties
            .push(planner::ical::Property::new("X-OTHER-CLIENT", "keep me"));
        todo.properties
            .push(planner::ical::Property::new("CATEGORIES", "home,urgent"));
    }
    let (href, etag) = client.put_task(&mut task).unwrap();

    let mut fetched = client
        .fetch_tasks(calendar)
        .unwrap()
        .into_iter()
        .find(|t| t.uid == task.uid)
        .unwrap();
    fetched.summary = "Imported task (renamed)".into();
    let (_, etag2) = client.put_task(&mut fetched).unwrap();

    let again = client
        .fetch_tasks(calendar)
        .unwrap()
        .into_iter()
        .find(|t| t.uid == task.uid)
        .unwrap();
    let todo = again.ical.child("VTODO").unwrap();
    assert_eq!(
        todo.get("X-OTHER-CLIENT").map(|p| p.value.as_str()),
        Some("keep me")
    );
    assert_eq!(
        todo.get("CATEGORIES").map(|p| p.value.as_str()),
        Some("home,urgent")
    );
    assert_eq!(again.summary, "Imported task (renamed)");

    client.delete(&href, etag2.or(etag).as_deref()).unwrap();
}

fn events_are_fetched_for_the_week(
    server: &Stalwart,
    user: &Credentials,
    client: &Client,
    calendar: &Calendar,
) {
    // Monday 5 October 2026 to Sunday 11 October 2026.
    let monday = NaiveDate::from_ymd_opt(2026, 10, 5).unwrap();
    assert_eq!(monday.weekday(), chrono::Weekday::Mon);

    server.put_ics(
        user,
        &calendar.url,
        "standup",
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//test//EN\r\nBEGIN:VEVENT\r\nUID:standup\r\nSUMMARY:Daily standup\r\nDTSTART:20260928T073000Z\r\nDTEND:20260928T075000Z\r\nRRULE:FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
    );
    server.put_ics(
        user,
        &calendar.url,
        "conference",
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//test//EN\r\nBEGIN:VEVENT\r\nUID:conference\r\nSUMMARY:Conference\r\nDTSTART;VALUE=DATE:20261008\r\nDTEND;VALUE=DATE:20261010\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
    );
    server.put_ics(
        user,
        &calendar.url,
        "old",
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//test//EN\r\nBEGIN:VEVENT\r\nUID:old\r\nSUMMARY:Last month\r\nDTSTART:20260901T100000Z\r\nDTEND:20260901T110000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
    );

    let start = Utc.with_ymd_and_hms(2026, 10, 4, 0, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2026, 10, 13, 0, 0, 0).unwrap();
    let events = client
        .fetch_events(calendar, start, end)
        .expect("fetch events");

    assert!(
        events.iter().all(|e| e.uid != "old"),
        "time-range filter excludes last month"
    );

    let conference: Vec<_> = events.iter().filter(|e| e.uid == "conference").collect();
    assert_eq!(conference.len(), 1, "{events:?}");
    assert!(
        conference[0].all_day,
        "all-day event stays all-day after expansion: {conference:?}"
    );
    assert!(conference[0].covers_day(NaiveDate::from_ymd_opt(2026, 10, 8).unwrap()));
    assert!(conference[0].covers_day(NaiveDate::from_ymd_opt(2026, 10, 9).unwrap()));
    assert!(!conference[0].covers_day(NaiveDate::from_ymd_opt(2026, 10, 10).unwrap()));

    let standups: Vec<_> = events.iter().filter(|e| e.uid == "standup").collect();
    assert!(!standups.is_empty(), "recurring event is returned");
    let in_week: Vec<_> = standups
        .iter()
        .filter(|e| (0..7).any(|d| e.covers_day(monday + Duration::days(d))))
        .collect();
    if standups.len() > 1 {
        // The server expanded the recurrence: one instance per weekday.
        assert_eq!(in_week.len(), 5, "five weekday instances: {standups:?}");
        for e in &in_week {
            assert_eq!(e.end - e.start, Duration::minutes(20));
        }
    } else {
        eprintln!("note: server did not expand recurrences; only the master instance was returned");
    }
}
