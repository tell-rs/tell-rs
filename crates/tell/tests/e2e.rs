//! End-to-end smoke test — sends every API method to a real collector.
//!
//! Start your Tell server, then:
//!
//!   cargo test -p tell --test e2e -- --ignored --nocapture

use serde_json::json;
use tell::{props, Props, Events, Tell, TellConfig};

const API_KEY: &str = "feed1e11feed1e11feed1e11feed1e11";
const USER: &str = "e2e_user_rust";
const ENDPOINT: &str = "localhost:50000";

#[ignore]
#[tokio::test]
async fn smoke() {
    let endpoint = ENDPOINT;

    println!();
    println!("  Tell Rust SDK — E2E smoke test");
    println!("  Endpoint: {endpoint}");
    println!();

    let client = Tell::new(
        TellConfig::builder(API_KEY)
            .endpoint(endpoint)
            .batch_size(10)
            .on_error(|e| eprintln!("  !! {e}"))
            .build()
            .expect("config"),
    )
    .expect("client");

    // ── Super properties ──────────────────────────────────────────────
    send("register super properties");
    client.register(json!({
        "sdk": "rust",
        "sdk_version": "0.1.0",
        "test": "e2e"
    }));

    // ── Track ─────────────────────────────────────────────────────────
    send("track with props!");
    client.track(USER, Events::PAGE_VIEWED, props! {
        "url" => "/home",
        "referrer" => "google",
        "screen" => "1920x1080"
    });

    send("track with Props::new()");
    client.track(USER, Events::FEATURE_USED, Props::new()
        .add("feature", "export")
        .add("format", "csv")
        .add("rows", 1500)
    );

    send("track with json!()");
    client.track(USER, Events::SEARCH_PERFORMED, Some(json!({
        "query": "analytics sdk",
        "results": 42
    })));

    send("track with no properties");
    client.track(USER, "App Opened", None::<serde_json::Value>);

    // ── Identify ──────────────────────────────────────────────────────
    send("identify");
    client.identify(USER, props! {
        "name" => "E2E Test User",
        "email" => "e2e@tell.app",
        "plan" => "pro",
        "created_at" => "2025-01-01T00:00:00Z"
    });

    // ── Group ─────────────────────────────────────────────────────────
    send("group");
    client.group(USER, "org_rust_sdk", props! {
        "name" => "Tell Engineering",
        "plan" => "enterprise",
        "seats" => 50
    });

    // ── Revenue ───────────────────────────────────────────────────────
    send("revenue with properties");
    client.revenue(USER, 49.99, "USD", "order_e2e_001", props! {
        "product" => "pro_annual",
        "coupon" => "LAUNCH50"
    });

    send("revenue without properties");
    client.revenue(USER, 9.99, "USD", "order_e2e_002", None::<serde_json::Value>);

    // ── Alias ─────────────────────────────────────────────────────────
    send("alias");
    client.alias("anon_visitor_abc", USER);

    // ── Logging — all 9 levels ────────────────────────────────────────
    send("log_emergency");
    client.log_emergency("System failure — disk full", Some("storage"), props! {
        "disk" => "/dev/sda1",
        "usage_pct" => 100
    });

    send("log_alert");
    client.log_alert("Database replication lag > 30s", Some("db"), props! {
        "lag_seconds" => 34
    });

    send("log_critical");
    client.log_critical("Payment gateway unreachable", Some("billing"), props! {
        "gateway" => "stripe",
        "timeout_ms" => 5000
    });

    send("log_error");
    client.log_error("Failed to send email", Some("notifications"), props! {
        "recipient" => "user@example.com",
        "error" => "SMTP timeout"
    });

    send("log_warning");
    client.log_warning("Rate limit approaching", Some("api"), props! {
        "current_rps" => 950,
        "limit_rps" => 1000
    });

    send("log_notice");
    client.log_notice("New deployment started", Some("deploy"), props! {
        "version" => "2.1.0",
        "region" => "us-east-1"
    });

    send("log_info");
    client.log_info("User signed in", Some("auth"), props! {
        "method" => "oauth",
        "provider" => "github"
    });

    send("log_debug");
    client.log_debug("Cache miss for key", Some("cache"), props! {
        "key" => "user:e2e:profile",
        "ttl_remaining" => 0
    });

    send("log_trace");
    client.log_trace("Entering request handler", Some("http"), props! {
        "method" => "GET",
        "path" => "/api/v1/events"
    });

    send("log with no service/data");
    client.log_info("Heartbeat", None, None::<serde_json::Value>);

    // ── Unregister ────────────────────────────────────────────────────
    send("unregister 'test' super property");
    client.unregister("test");

    send("track after unregister (should lack 'test' key)");
    client.track(USER, "Post Unregister", props! {
        "step" => "verify_unregister"
    });

    // ── Session reset ─────────────────────────────────────────────────
    send("reset_session");
    client.reset_session();

    send("track after reset (new session_id)");
    client.track(USER, "Post Reset", props! {
        "step" => "verify_new_session"
    });

    // ── Flush & close ─────────────────────────────────────────────────
    send("flush");
    match client.flush().await {
        Ok(()) => println!("  .. flush ok"),
        Err(e) => eprintln!("  !! flush: {e}"),
    }

    send("close");
    match client.close().await {
        Ok(()) => println!("  .. close ok"),
        Err(e) => eprintln!("  !! close: {e}"),
    }

    println!();
    println!("  Done — 23 calls sent. Verify on the collector.");
    println!();
}

fn send(label: &str) {
    println!("  -> {label}");
}
