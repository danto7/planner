//! Test support: spawn a throwaway Stalwart Mail Server for CalDAV tests.
//!
//! Stalwart 0.16 keeps its configuration in a registry inside the data
//! store, so a fresh instance is provisioned in three short runs:
//!
//! 1. **Bootstrap mode** (no `config.json`): the server opens one HTTP port and
//!    accepts a `Bootstrap/set` JMAP call that creates the data store, the
//!    default domain and an administrator.
//! 2. **Recovery mode**: with the registry now persistent but no listeners
//!    defined yet, we add a single plain-HTTP listener on our test port so the
//!    normal start does not fall back to the stock ports (25, 443, 8080, …).
//! 3. **Normal mode**: the server runs with that listener and the admin
//!    creates user accounts through JMAP.

#![allow(dead_code)]

use serde_json::{json, Value};
use std::io::{Read, Seek};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub const DOMAIN: &str = "example.org";
const BOOTSTRAP_ADMIN: (&str, &str) = ("admin", "bootstrap-pw-Nk3fQ9");

pub struct Credentials {
    pub username: String,
    pub password: String,
}

pub struct Stalwart {
    pub port: u16,
    pub base_url: String,
    pub admin: Credentials,
    domain_id: String,
    binary: PathBuf,
    dir: tempfile::TempDir,
    child: Option<Child>,
    log: PathBuf,
    http: reqwest::blocking::Client,
}

/// Locate the server binary: `STALWART_BIN`, then `target/stalwart/stalwart`,
/// then `stalwart` on `PATH`.
pub fn find_binary() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("STALWART_BIN") {
        return Some(PathBuf::from(p));
    }
    let local = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("stalwart")
        .join("stalwart");
    if local.is_file() {
        return Some(local);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join("stalwart"))
        .find(|p| p.is_file())
}

/// Returns `None` (and prints why) when no server binary is available and the
/// test is allowed to be skipped. Set `PLANNER_REQUIRE_STALWART=1` to fail instead.
pub fn spawn_or_skip() -> Option<Stalwart> {
    match find_binary() {
        Some(bin) => Some(Stalwart::spawn(bin)),
        None if std::env::var_os("PLANNER_REQUIRE_STALWART").is_some() => {
            panic!("stalwart binary not found; run scripts/fetch-stalwart.sh or set STALWART_BIN")
        }
        None => {
            eprintln!(
                "skipping: stalwart binary not found (run scripts/fetch-stalwart.sh or set STALWART_BIN)"
            );
            None
        }
    }
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .unwrap()
        .port()
}

impl Stalwart {
    pub fn spawn(binary: PathBuf) -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let dir = tempfile::Builder::new()
            .prefix("planner-stalwart-")
            .tempdir()
            .expect("tempdir");
        let port = free_port();
        let mut server = Stalwart {
            port,
            base_url: format!("http://127.0.0.1:{port}"),
            admin: Credentials {
                username: String::new(),
                password: String::new(),
            },
            domain_id: String::new(),
            binary,
            log: dir.path().join("stalwart.log"),
            dir,
            child: None,
            http: reqwest::blocking::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap(),
        };
        server.bootstrap();
        server.add_listener();
        server.start_normal();
        server
    }

    fn data_dir(&self) -> PathBuf {
        self.dir.path().join("data")
    }

    fn config_path(&self) -> PathBuf {
        self.dir.path().join("config.json")
    }

    fn start(&mut self, env: &[(&str, String)], ready_auth: (&str, &str)) {
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log)
            .expect("open log");
        let log_err = log.try_clone().unwrap();
        let mut cmd = Command::new(&self.binary);
        cmd.arg("--config")
            .arg(self.config_path())
            .env("STALWART_HOSTNAME", format!("mail.{DOMAIN}"))
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(log_err));
        for (k, v) in env {
            cmd.env(k, v);
        }
        let child = cmd.spawn().expect("spawn stalwart");
        self.child = Some(child);
        self.wait_ready(ready_auth);
    }

    fn wait_ready(&mut self, auth: (&str, &str)) {
        let deadline = Instant::now() + Duration::from_secs(90);
        let url = format!("{}/jmap/session", self.base_url);
        loop {
            if let Some(child) = self.child.as_mut() {
                if let Ok(Some(status)) = child.try_wait() {
                    panic!("stalwart exited early with {status}\n{}", self.log_tail());
                }
            }
            let ok = self
                .http
                .get(&url)
                .basic_auth(auth.0, Some(auth.1))
                .timeout(Duration::from_secs(2))
                .send()
                .map(|r| r.status().is_success())
                .unwrap_or(false);
            if ok {
                return;
            }
            if Instant::now() > deadline {
                panic!(
                    "stalwart did not become ready on port {}\n{}",
                    self.port,
                    self.log_tail()
                );
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    pub fn log_tail(&self) -> String {
        let mut text = String::new();
        if let Ok(mut f) = std::fs::File::open(&self.log) {
            let len = f.metadata().map(|m| m.len()).unwrap_or(0);
            let _ = f.seek(std::io::SeekFrom::Start(len.saturating_sub(6000)));
            let _ = f.read_to_string(&mut text);
        }
        format!("--- stalwart log tail ---\n{text}")
    }

    /// Call one JMAP method and return its response arguments.
    pub fn jmap(&self, auth: (&str, &str), method: &str, args: Value) -> Value {
        let body = json!({
            "using": ["urn:ietf:params:jmap:core"],
            "methodCalls": [[method, args, "0"]],
        });
        let resp = self
            .http
            .post(format!("{}/jmap", self.base_url))
            .basic_auth(auth.0, Some(auth.1))
            .header("Content-Type", "application/json")
            .body(body.to_string())
            .send()
            .expect("jmap request");
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        assert!(status.is_success(), "{method}: HTTP {status}: {text}");
        let value: Value = serde_json::from_str(&text).expect("jmap json");
        let responses = value["methodResponses"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let first = responses.first().cloned().unwrap_or(Value::Null);
        assert_eq!(
            first[0].as_str(),
            Some(method),
            "unexpected response: {text}"
        );
        first[1].clone()
    }

    fn bootstrap(&mut self) {
        let env = [
            ("STALWART_RECOVERY_MODE_PORT", self.port.to_string()),
            (
                "STALWART_RECOVERY_ADMIN",
                format!("{}:{}", BOOTSTRAP_ADMIN.0, BOOTSTRAP_ADMIN.1),
            ),
        ];
        self.start(&env, BOOTSTRAP_ADMIN);
        let args = json!({
            "accountId": "a",
            "update": { "singleton": {
                "serverHostname": format!("mail.{DOMAIN}"),
                "defaultDomain": DOMAIN,
                "requestTlsCertificate": false,
                "generateDkimKeys": false,
                "dataStore": { "@type": "RocksDb", "path": self.data_dir().to_string_lossy() },
                "tracer": { "@type": "Stdout", "enable": true, "level": "info", "ansi": false },
            }},
        });
        let resp = self.jmap(BOOTSTRAP_ADMIN, "x:Bootstrap/set", args);
        let created = &resp["updated"]["singleton"];
        let username = created["username"].as_str();
        let secret = created["secret"].as_str();
        let (Some(username), Some(secret)) = (username, secret) else {
            panic!("bootstrap failed: {resp}\n{}", self.log_tail());
        };
        self.admin = Credentials {
            username: username.to_string(),
            password: secret.to_string(),
        };
        self.stop();
        assert!(self.config_path().is_file(), "config.json was not written");
    }

    fn add_listener(&mut self) {
        let env = [
            ("STALWART_RECOVERY_MODE", "true".to_string()),
            ("STALWART_RECOVERY_MODE_PORT", self.port.to_string()),
            (
                "STALWART_RECOVERY_ADMIN",
                format!("{}:{}", BOOTSTRAP_ADMIN.0, BOOTSTRAP_ADMIN.1),
            ),
        ];
        self.start(&env, BOOTSTRAP_ADMIN);
        let args = json!({
            "accountId": "a",
            "create": { "l1": {
                "name": "http",
                "bind": { format!("127.0.0.1:{}", self.port): true },
                "protocol": "http",
                "useTls": false,
            }},
        });
        let resp = self.jmap(BOOTSTRAP_ADMIN, "x:NetworkListener/set", args);
        assert!(
            resp["created"]["l1"].is_object(),
            "listener not created: {resp}"
        );
        self.stop();
    }

    fn start_normal(&mut self) {
        let admin = (self.admin.username.clone(), self.admin.password.clone());
        self.start(&[], (&admin.0, &admin.1));
        let resp = self.jmap(
            (&admin.0, &admin.1),
            "x:Domain/query",
            json!({ "accountId": "a" }),
        );
        let id = resp["ids"][0]
            .as_str()
            .unwrap_or_else(|| panic!("no domain: {resp}"));
        self.domain_id = id.to_string();
    }

    fn admin_auth(&self) -> (&str, &str) {
        (&self.admin.username, &self.admin.password)
    }

    /// Create a user account and return its login credentials.
    pub fn create_user(&self, name: &str) -> Credentials {
        let password = format!("{name}-horse-battery-staple-42");
        let args = json!({
            "accountId": "a",
            "create": { "u": {
                "@type": "User",
                "name": name,
                "domainId": self.domain_id,
                "credentials": { "0": { "@type": "Password", "secret": password } },
                "roles": { "@type": "User" },
            }},
        });
        let resp = self.jmap(self.admin_auth(), "x:Account/set", args);
        assert!(resp["created"]["u"].is_object(), "user not created: {resp}");
        Credentials {
            username: format!("{name}@{DOMAIN}"),
            password,
        }
    }

    /// Store a raw iCalendar object in a calendar collection as the given user.
    pub fn put_ics(&self, user: &Credentials, calendar_url: &str, name: &str, ics: &str) {
        let resp = self
            .http
            .put(format!("{}{}.ics", calendar_url, name))
            .basic_auth(&user.username, Some(&user.password))
            .header("Content-Type", "text/calendar; charset=utf-8")
            .header("If-None-Match", "*")
            .body(ics.to_string())
            .send()
            .expect("put");
        assert!(
            resp.status().is_success(),
            "PUT {name}: HTTP {}",
            resp.status()
        );
    }
}

impl Drop for Stalwart {
    fn drop(&mut self) {
        self.stop();
        if std::thread::panicking() {
            eprintln!("{}", self.log_tail());
        }
    }
}
