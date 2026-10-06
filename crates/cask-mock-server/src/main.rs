//! Mock of the Cask snapshot API for tests and examples.
//!
//! Usage: cask-mock-server [--addr 127.0.0.1:8099] [--api-key test-key]
//!                         [--fixture path.json] [--synthetic CUSTOMERS,PRODUCT_VERSIONS,FEATURES]
//!                         [--delay-ms N] [--fail-status 503]

use std::io::Write;
use std::thread::sleep;
use std::time::Duration;

use flate2::Compression;
use flate2::write::GzEncoder;
use tiny_http::{Header, Response, Server};

const DEFAULT_FIXTURE: &str = include_str!("../fixtures/snapshot.json");

fn main() {
    let mut addr = "127.0.0.1:8099".to_string();
    let mut api_key = "test-key".to_string();
    let mut fixture = None;
    let mut synthetic: Option<(usize, usize, usize)> = None;
    let mut delay = Duration::ZERO;
    let mut fail_status: Option<u16> = None;

    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let value = args.next().unwrap_or_else(|| panic!("missing value for {flag}"));
        match flag.as_str() {
            "--addr" => addr = value,
            "--api-key" => api_key = value,
            "--fixture" => fixture = Some(value),
            "--synthetic" => {
                let n: Vec<usize> = value.split(',').map(|v| v.parse().expect("bad --synthetic")).collect();
                assert!(n.len() == 3, "--synthetic takes CUSTOMERS,PRODUCT_VERSIONS,FEATURES");
                synthetic = Some((n[0], n[1], n[2]));
            }
            "--delay-ms" => delay = Duration::from_millis(value.parse().expect("bad --delay-ms")),
            "--fail-status" => fail_status = Some(value.parse().expect("bad --fail-status")),
            other => panic!("unknown flag {other}"),
        }
    }

    let body = match (synthetic, fixture) {
        (Some((c, p, f)), _) => cask_mock_server::generate(c, p, f).into_bytes(),
        (None, Some(path)) => std::fs::read(&path).unwrap_or_else(|e| panic!("reading {path}: {e}")),
        (None, None) => DEFAULT_FIXTURE.as_bytes().to_vec(),
    };
    let gzipped = {
        let mut enc = GzEncoder::new(Vec::new(), Compression::fast());
        enc.write_all(&body).unwrap();
        enc.finish().unwrap()
    };

    let server = Server::http(&addr).unwrap_or_else(|e| panic!("binding {addr}: {e}"));
    eprintln!("cask-mock-server listening on http://{addr} (api key: {api_key})");

    let header = |k: &str, v: &str| Header::from_bytes(k.as_bytes(), v.as_bytes()).unwrap();

    for req in server.incoming_requests() {
        sleep(delay);
        if req.method() != &tiny_http::Method::Get || req.url() != "/api/v1/sdk/snapshot" {
            let _ = req.respond(Response::empty(404));
            continue;
        }
        let authed = req.headers().iter().any(|h| {
            h.field.equiv("Authorization") && h.value.as_str() == format!("Bearer {api_key}")
        });
        if !authed {
            let _ = req.respond(Response::empty(401));
            continue;
        }
        if let Some(status) = fail_status {
            let _ = req.respond(Response::empty(status));
            continue;
        }
        let wants_gzip = req
            .headers()
            .iter()
            .any(|h| h.field.equiv("Accept-Encoding") && h.value.as_str().contains("gzip"));
        let resp = if wants_gzip {
            Response::from_data(gzipped.clone()).with_header(header("Content-Encoding", "gzip"))
        } else {
            Response::from_data(body.clone())
        };
        let _ = req.respond(resp.with_header(header("Content-Type", "application/json")));
    }
}
