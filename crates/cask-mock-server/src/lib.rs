//! Deterministic synthetic snapshots for benchmarks.
//!
//! Naming is predictable so benchmark clients can derive IDs without reading the JSON:
//! customers are `customer_{i:032X}`, features are `feature_{i}` (type = i % 3: BOOLEAN, NUMERIC, ENUM).

pub fn customer_id(i: usize) -> String {
    format!("customer_{i:032X}")
}

pub fn generate(customers: usize, product_versions: usize, features: usize) -> String {
    let mut rng = Lcg(42);
    let mut out = String::new();

    out.push_str(r#"{"features":["#);
    for f in 0..features {
        if f > 0 {
            out.push(',');
        }
        let ty = ["BOOLEAN", "NUMERIC", "ENUM"][f % 3];
        out.push_str(&format!(r#"{{"key":"feature_{f}","type":"{ty}"}}"#));
    }

    out.push_str(r#"],"product_versions":["#);
    for p in 0..product_versions {
        if p > 0 {
            out.push(',');
        }
        out.push_str(&format!(r#"{{"token":"product_version_{p:032X}","entitlements":["#));
        let mut first = true;
        for f in 0..features {
            if rng.next() % 2 == 0 {
                continue;
            }
            if !first {
                out.push(',');
            }
            first = false;
            let value = match f % 3 {
                0 => (rng.next() % 2 == 0).to_string(),
                1 => match rng.next() % 10 {
                    0 => r#""unlimited""#.to_string(),
                    n => (n * 5).to_string(),
                },
                _ => format!(r#""tier_{}""#, rng.next() % 4),
            };
            out.push_str(&format!(r#"{{"feature":"feature_{f}","value":{value}}}"#));
        }
        out.push_str("]}");
    }

    out.push_str(r#"],"customers":{"#);
    for c in 0..customers {
        if c > 0 {
            out.push(',');
        }
        let n = 1 + rng.next() as usize % 3;
        let tokens: Vec<String> = (0..n)
            .map(|_| {
                let p = rng.next() as usize % product_versions;
                format!(r#""product_version_{p:032X}""#)
            })
            .collect();
        out.push_str(&format!(r#""{}":[{}]"#, customer_id(c), tokens.join(",")));
    }
    out.push_str("}}");
    out
}

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}
