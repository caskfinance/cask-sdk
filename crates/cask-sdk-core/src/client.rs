use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use arc_swap::ArcSwapOption;

use crate::error::Error;
use crate::snapshot::{Limit, Snapshot};

pub const DEFAULT_BASE_URL: &str = "http://127.0.0.1:8080";
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(30);

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_BACKOFF: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct Config {
    pub api_key: String,
    pub base_url: String,
    pub poll_interval: Duration,
    /// Prints debug messages to stderr.
    pub debug: bool,
}

impl Config {
    pub fn new(api_key: impl Into<String>) -> Config {
        Config {
            api_key: api_key.into(),
            base_url: DEFAULT_BASE_URL.to_string(),
            poll_interval: DEFAULT_POLL_INTERVAL,
            debug: false,
        }
    }
}

macro_rules! debug_log {
    ($config:expr, $($arg:tt)*) => {
        if $config.debug {
            eprintln!("[cask] {}", format_args!($($arg)*));
        }
    };
}

#[derive(Default)]
struct Status {
    stopped: bool,
    ready: bool,
    last_error: Option<Error>,
}

struct Shared {
    snapshot: ArcSwapOption<Snapshot>,
    status: Mutex<Status>,
    cond: Condvar,
}

pub struct Cask {
    shared: Arc<Shared>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

impl Cask {
    /// Starts loading the snapshot in the background and returns immediately.
    pub fn new(config: Config) -> Result<Cask, Error> {
        let http = reqwest::blocking::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| Error::Network(e.to_string()))?;
        let shared = Arc::new(Shared {
            snapshot: ArcSwapOption::empty(),
            status: Mutex::new(Status::default()),
            cond: Condvar::new(),
        });
        let worker = shared.clone();
        let handle = std::thread::Builder::new()
            .name("cask-poller".into())
            .spawn(move || poll_loop(worker, http, config))
            .map_err(|e| Error::Network(e.to_string()))?;
        Ok(Cask {
            shared,
            handle: Mutex::new(Some(handle)),
        })
    }

    /// Blocks until the first snapshot is loaded or `timeout` elapses.
    /// The load keeps going in the background after a timeout.
    pub fn wait_until_ready(&self, timeout: Duration) -> Result<(), Error> {
        let deadline = Instant::now() + timeout;
        let mut status = self.shared.status.lock().unwrap();
        while !status.ready {
            let now = Instant::now();
            if now >= deadline {
                return Err(status.last_error.clone().unwrap_or(Error::NotReady));
            }
            status = self
                .shared
                .cond
                .wait_timeout(status, deadline - now)
                .unwrap()
                .0;
        }
        Ok(())
    }

    pub fn is_ready(&self) -> bool {
        self.shared.snapshot.load().is_some()
    }

    pub fn check_bool(&self, customer: &str, feature: &str) -> Result<bool, Error> {
        self.shared
            .snapshot
            .load()
            .as_ref()
            .ok_or(Error::NotReady)?
            .check_bool(customer, feature)
    }

    pub fn check_numeric(&self, customer: &str, feature: &str) -> Result<Option<Limit>, Error> {
        self.shared
            .snapshot
            .load()
            .as_ref()
            .ok_or(Error::NotReady)?
            .check_numeric(customer, feature)
    }

    pub fn check_enum(&self, customer: &str, feature: &str) -> Result<Option<String>, Error> {
        self.shared
            .snapshot
            .load()
            .as_ref()
            .ok_or(Error::NotReady)?
            .check_enum(customer, feature)
    }

    /// Stops background polling. Checks keep working against the last snapshot.
    pub fn close(&self) {
        self.shared.status.lock().unwrap().stopped = true;
        self.shared.cond.notify_all();
        if let Some(h) = self.handle.lock().unwrap().take() {
            let _ = h.join();
        }
    }
}

impl Drop for Cask {
    fn drop(&mut self) {
        self.close();
    }
}

fn poll_loop(shared: Arc<Shared>, http: reqwest::blocking::Client, config: Config) {
    let url = format!("{}/api/v1/sdk/snapshot", config.base_url.trim_end_matches('/'));
    let mut backoff = Duration::from_secs(1);
    debug_log!(config, "polling {url} every {:?}", config.poll_interval);
    loop {
        debug_log!(config, "fetching snapshot");
        let started = Instant::now();
        let wait = match fetch(&http, &url, &config.api_key) {
            Ok(snapshot) => {
                debug_log!(config, "snapshot loaded in {:?}", started.elapsed());
                shared.snapshot.store(Some(Arc::new(snapshot)));
                let mut status = shared.status.lock().unwrap();
                status.ready = true;
                status.last_error = None;
                shared.cond.notify_all();
                backoff = Duration::from_secs(1);
                config.poll_interval
            }
            Err(e) => {
                let wait = backoff.min(config.poll_interval);
                debug_log!(config, "snapshot fetch failed: {e}; retrying in {wait:?}");
                shared.status.lock().unwrap().last_error = Some(e);
                backoff = (backoff * 2).min(MAX_BACKOFF);
                wait
            }
        };
        let status = shared.status.lock().unwrap();
        let (status, _) = shared
            .cond
            .wait_timeout_while(status, wait, |s| !s.stopped)
            .unwrap();
        if status.stopped {
            debug_log!(config, "poller stopped");
            return;
        }
    }
}

fn fetch(http: &reqwest::blocking::Client, url: &str, api_key: &str) -> Result<Snapshot, Error> {
    let resp = http
        .get(url)
        .bearer_auth(api_key)
        .send()
        .map_err(|e| Error::Network(e.to_string()))?;
    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(Error::Auth);
    }
    if !status.is_success() {
        return Err(Error::Network(format!("unexpected status {status}")));
    }
    let body = resp.bytes().map_err(|e| Error::Network(e.to_string()))?;
    Snapshot::from_json(&body)
}
