use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cask_sdk_core::{Cask, Config, Error, Limit};
use tiny_http::{Header, Response, Server};

const KEY: &str = "test-key";

fn snapshot(seats: u32) -> String {
    format!(
        r#"{{
          "features": [
            {{"key": "sso", "type": "BOOLEAN"}},
            {{"key": "seats", "type": "NUMERIC"}}
          ],
          "product_versions": [
            {{"token": "pv", "entitlements": [
              {{"feature": "sso", "value": true}},
              {{"feature": "seats", "value": {seats}}}
            ]}}
          ],
          "customers": {{"c1": ["pv"]}}
        }}"#
    )
}

struct Behavior {
    status: u16,
    body: String,
    delay: Duration,
    fail_first: usize,
    gzip: bool,
}

struct TestServer {
    url: String,
    behavior: Arc<Mutex<Behavior>>,
    hits: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl TestServer {
    fn start(body: String) -> TestServer {
        let server = Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}", server.server_addr().to_ip().unwrap());
        let behavior = Arc::new(Mutex::new(Behavior {
            status: 200,
            body,
            delay: Duration::ZERO,
            fail_first: 0,
            gzip: false,
        }));
        let hits = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (b, h, s) = (behavior.clone(), hits.clone(), stop.clone());
        let handle = std::thread::spawn(move || {
            while !s.load(Ordering::SeqCst) {
                let Ok(Some(req)) = server.recv_timeout(Duration::from_millis(20)) else {
                    continue;
                };
                let n = h.fetch_add(1, Ordering::SeqCst);
                let (status, body, delay, fail_first, gzip) = {
                    let b = b.lock().unwrap();
                    (b.status, b.body.clone(), b.delay, b.fail_first, b.gzip)
                };
                std::thread::sleep(delay);
                if req.url() != "/api/v1/sdk/snapshot" {
                    let _ = req.respond(Response::empty(404));
                    continue;
                }
                let authed = req.headers().iter().any(|h| {
                    h.field.equiv("Authorization") && h.value.as_str() == format!("Bearer {KEY}")
                });
                if !authed {
                    let _ = req.respond(Response::empty(401));
                } else if n < fail_first {
                    let _ = req.respond(Response::empty(503));
                } else if status != 200 {
                    let _ = req.respond(Response::empty(status));
                } else if gzip {
                    let mut enc =
                        flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
                    enc.write_all(body.as_bytes()).unwrap();
                    let resp = Response::from_data(enc.finish().unwrap())
                        .with_header(Header::from_bytes("Content-Encoding", "gzip").unwrap());
                    let _ = req.respond(resp);
                } else {
                    let _ = req.respond(Response::from_string(body));
                }
            }
        });
        TestServer {
            url,
            behavior,
            hits,
            stop,
            handle: Some(handle),
        }
    }

    fn client(&self, key: &str, poll_interval: Duration) -> Cask {
        let mut config = Config::new(key);
        config.base_url = self.url.clone();
        config.poll_interval = poll_interval;
        Cask::new(config).unwrap()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn wait_for(timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    cond()
}

const POLL: Duration = Duration::from_millis(50);
const WAIT: Duration = Duration::from_secs(5);

#[test]
fn loads_snapshot_and_serves_checks() {
    let server = TestServer::start(snapshot(5));
    let cask = server.client(KEY, POLL);
    cask.wait_until_ready(WAIT).unwrap();
    assert!(cask.is_ready());
    assert_eq!(cask.check_bool("c1", "sso"), Ok(true));
    assert_eq!(cask.check_numeric("c1", "seats"), Ok(Some(Limit::Value(5.0))));
}

#[test]
fn handles_gzip_responses() {
    let server = TestServer::start(snapshot(5));
    server.behavior.lock().unwrap().gzip = true;
    let cask = server.client(KEY, POLL);
    cask.wait_until_ready(WAIT).unwrap();
    assert_eq!(cask.check_bool("c1", "sso"), Ok(true));
}

#[test]
fn checks_before_first_snapshot_return_not_ready() {
    let server = TestServer::start(snapshot(5));
    server.behavior.lock().unwrap().delay = Duration::from_millis(500);
    let cask = server.client(KEY, POLL);
    assert!(!cask.is_ready());
    assert_eq!(cask.check_bool("c1", "sso"), Err(Error::NotReady));
    assert_eq!(cask.check_numeric("c1", "seats"), Err(Error::NotReady));
    assert_eq!(cask.check_enum("c1", "seats"), Err(Error::NotReady));
}

#[test]
fn wait_until_ready_timeout_does_not_cancel_load() {
    let server = TestServer::start(snapshot(5));
    server.behavior.lock().unwrap().delay = Duration::from_millis(400);
    let cask = server.client(KEY, POLL);
    assert_eq!(
        cask.wait_until_ready(Duration::from_millis(50)),
        Err(Error::NotReady)
    );
    cask.wait_until_ready(WAIT).unwrap();
    assert_eq!(cask.check_bool("c1", "sso"), Ok(true));
}

#[test]
fn bad_api_key_reports_auth_error() {
    let server = TestServer::start(snapshot(5));
    let cask = server.client("wrong-key", POLL);
    assert_eq!(
        cask.wait_until_ready(Duration::from_millis(500)),
        Err(Error::Auth)
    );
    assert_eq!(cask.check_bool("c1", "sso"), Err(Error::NotReady));
}

#[test]
fn server_errors_are_retried_with_backoff() {
    let server = TestServer::start(snapshot(5));
    server.behavior.lock().unwrap().fail_first = 2;
    let cask = server.client(KEY, POLL);
    cask.wait_until_ready(WAIT).unwrap();
    assert!(server.hits.load(Ordering::SeqCst) >= 3);
    assert_eq!(cask.check_bool("c1", "sso"), Ok(true));
}

#[test]
fn persistent_server_errors_surface_as_network_error() {
    let server = TestServer::start(snapshot(5));
    server.behavior.lock().unwrap().status = 500;
    let cask = server.client(KEY, POLL);
    assert!(matches!(
        cask.wait_until_ready(Duration::from_millis(300)),
        Err(Error::Network(_))
    ));
    assert!(server.hits.load(Ordering::SeqCst) >= 2, "should keep retrying");
}

#[test]
fn unreachable_server_is_a_network_error() {
    let mut config = Config::new(KEY);
    config.base_url = "http://127.0.0.1:1".into();
    config.poll_interval = POLL;
    let cask = Cask::new(config).unwrap();
    assert!(matches!(
        cask.wait_until_ready(Duration::from_millis(500)),
        Err(Error::Network(_))
    ));
}

#[test]
fn refreshes_pick_up_new_snapshots() {
    let server = TestServer::start(snapshot(5));
    let cask = server.client(KEY, POLL);
    cask.wait_until_ready(WAIT).unwrap();
    server.behavior.lock().unwrap().body = snapshot(50);
    assert!(wait_for(WAIT, || {
        cask.check_numeric("c1", "seats") == Ok(Some(Limit::Value(50.0)))
    }));
}

#[test]
fn failed_refresh_keeps_last_good_snapshot() {
    let server = TestServer::start(snapshot(5));
    let cask = server.client(KEY, POLL);
    cask.wait_until_ready(WAIT).unwrap();

    let hits_before = server.hits.load(Ordering::SeqCst);
    server.behavior.lock().unwrap().status = 503;
    assert!(wait_for(WAIT, || {
        server.hits.load(Ordering::SeqCst) >= hits_before + 3
    }));
    assert!(cask.is_ready());
    assert_eq!(cask.check_numeric("c1", "seats"), Ok(Some(Limit::Value(5.0))));
    assert_eq!(cask.check_bool("c1", "sso"), Ok(true));
}

#[test]
fn invalid_refresh_keeps_last_good_snapshot() {
    let server = TestServer::start(snapshot(5));
    let cask = server.client(KEY, POLL);
    cask.wait_until_ready(WAIT).unwrap();

    // Value type does not match the feature type.
    server.behavior.lock().unwrap().body =
        snapshot(5).replace(r#""value": true"#, r#""value": 1"#);
    let hits_before = server.hits.load(Ordering::SeqCst);
    assert!(wait_for(WAIT, || {
        server.hits.load(Ordering::SeqCst) >= hits_before + 3
    }));
    assert_eq!(cask.check_bool("c1", "sso"), Ok(true));

    // And it recovers once the server is healthy again.
    server.behavior.lock().unwrap().body = snapshot(7);
    assert!(wait_for(WAIT, || {
        cask.check_numeric("c1", "seats") == Ok(Some(Limit::Value(7.0)))
    }));
}

#[test]
fn close_stops_polling_but_checks_keep_working() {
    let server = TestServer::start(snapshot(5));
    let cask = server.client(KEY, POLL);
    cask.wait_until_ready(WAIT).unwrap();
    cask.close();
    let hits = server.hits.load(Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(server.hits.load(Ordering::SeqCst), hits);
    assert_eq!(cask.check_bool("c1", "sso"), Ok(true));
}
