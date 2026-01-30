//! Tell SDK — events, identify, revenue, logging.
//!
//!   cargo run -p tell --example events

use tell::{props, Tell, TellConfig};

#[tokio::main]
async fn main() {
    let client = Tell::new(
        TellConfig::development("a1b2c3d4e5f60718293a4b5c6d7e8f90").unwrap(),
    )
    .unwrap();

    // Track events
    client.track("user_123", "Page Viewed", props! {
        "url" => "/home",
        "referrer" => "google"
    });

    // Identify users
    client.identify("user_123", props! {
        "name" => "Jane",
        "plan" => "pro"
    });

    // Revenue
    client.revenue("user_123", 49.99, "USD", "order_456", props! {
        "product" => "annual_plan"
    });

    // Structured logging
    client.log_error("DB connection failed", Some("api"), props! {
        "host" => "db.internal",
        "retries" => 3
    });

    client.log_info("User signed in", Some("auth"), props! {
        "method" => "oauth"
    });

    client.close().await.ok();
}
