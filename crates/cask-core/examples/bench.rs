//! cargo run --release -p cask-sdk --example bench
use std::hint::black_box;
use std::time::Instant;

use cask_mock_server::{customer_id, generate};
use cask_sdk::Snapshot;

const SIZES: [(usize, usize, usize); 2] = [(1_000, 20, 10), (100_000, 500, 50)];
const TOTAL_CHECKS: usize = 2_000_000;
const SAMPLED_CHECKS: usize = 200_000;

fn main() {
    for (customers, pvs, features) in SIZES {
        println!("\n== {customers} customers, {pvs} product versions, {features} features ==");
        let json = generate(customers, pvs, features);
        let t = Instant::now();
        let snapshot = Snapshot::from_json(json.as_bytes()).unwrap();
        println!(
            "snapshot: {:.1} MB JSON, parse+index {:.1} ms",
            json.len() as f64 / 1e6,
            t.elapsed().as_secs_f64() * 1e3
        );

        let ids: Vec<String> = (0..customers).map(customer_id).collect();
        let names: Vec<String> = (0..features).map(|f| format!("feature_{f}")).collect();
        let run = |i: usize| {
            let c = &ids[i.wrapping_mul(2654435761) % ids.len()];
            let f = i % features;
            let name = &names[f];
            match f % 3 {
                0 => {
                    black_box(snapshot.check_bool(c, name).unwrap());
                }
                1 => {
                    black_box(snapshot.check_numeric(c, name).unwrap());
                }
                _ => {
                    black_box(snapshot.check_enum(c, name).unwrap());
                }
            }
        };

        let t = Instant::now();
        for i in 0..TOTAL_CHECKS {
            run(i);
        }
        let mean_ns = t.elapsed().as_nanos() as f64 / TOTAL_CHECKS as f64;

        let mut samples: Vec<u128> = (0..SAMPLED_CHECKS)
            .map(|i| {
                let t = Instant::now();
                run(i);
                t.elapsed().as_nanos()
            })
            .collect();
        samples.sort_unstable();
        let pct = |p: f64| samples[((samples.len() - 1) as f64 * p) as usize];
        println!(
            "check: mean {mean_ns:.0} ns | p50 {} ns | p99 {} ns | p99.9 {} ns | max {} ns",
            pct(0.50),
            pct(0.99),
            pct(0.999),
            samples.last().unwrap()
        );
    }
}
