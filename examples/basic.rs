use serde_json::json;
use tell::{Tell, TellConfig, Events};

#[tokio::main]
async fn main() {
    // Initialize with production defaults
    let client = Tell::new(
        TellConfig::builder("a1b2c3d4e5f60718293a4b5c6d7e8f90")
            .on_error(|e| eprintln!("[Tell] {e}"))
            .build()
            .expect("invalid config"),
    )
    .expect("failed to create client");

    // Track events
    client.track("user_123", Events::PAGE_VIEWED, Some(json!({
        "url": "/home",
        "referrer": "google"
    })));

    client.track("user_123", Events::FEATURE_USED, Some(json!({
        "feature": "export",
        "format": "csv"
    })));

    // Identify user
    client.identify("user_123", Some(json!({
        "name": "Jane Doe",
        "email": "jane@example.com",
        "plan": "enterprise"
    })));

    // Group
    client.group("user_123", "org_456", Some(json!({
        "name": "Acme Corp",
        "plan": "enterprise"
    })));

    // Revenue
    client.revenue("user_123", 49.99, "USD", "order_789", Some(json!({
        "product": "premium_plan"
    })));

    // Alias
    client.alias("anon_abc", "user_123");

    // Super properties (attached to every event)
    client.register(json!({
        "app_version": "3.0.0",
        "service": "api"
    }));

    client.track("user_123", "Dashboard Loaded", None::<serde_json::Value>);
    // → payload includes app_version and service automatically

    client.unregister("service");

    // Flush and close
    client.flush().await.ok();
    client.close().await.ok();

    println!("Events sent successfully.");
}
