use std::time::Duration;

use cask_sdk_core::{Cask, Config};

fn main() {
    let mut config = Config::new("test-key");
    config.debug = true;
    if let Ok(url) = std::env::var("CASK_BASE_URL") {
        config.base_url = url;
    }
    config.poll_interval = Duration::from_secs(10);
    let cask = Cask::new(config).expect("failed to start cask");
    cask.wait_until_ready(Duration::from_secs(5))
        .expect("snapshot did not load");

    let customer = "customer_01A10971485672EFB40302EAE6021F03";
    println!("sso: {:?}", cask.check_bool(customer, "sso"));
    println!("seats: {:?}", cask.check_numeric(customer, "seats"));
    println!(
        "support_tier: {:?}",
        cask.check_enum(customer, "support_tier")
    );
    println!("unknown customer: {:?}", cask.check_bool("nope", "sso"));

    cask.close();
}
