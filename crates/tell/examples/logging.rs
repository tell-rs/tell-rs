//! Tell SDK — structured logging at every severity level.
//!
//!   cargo run -p tell --example logging

use tell::{props, Tell, TellConfig};

#[tokio::main]
async fn main() {
    let client = Tell::new(
        TellConfig::builder("feed1e11feed1e11feed1e11feed1e11")
            .endpoint("localhost:50000")
            .service("my-app")
            .batch_size(10)
            .build()
            .unwrap(),
    )
    .unwrap();

    // Structured logging at different levels
    client.log_info("Server started", Some("api"), props! {
        "port" => 8080,
        "workers" => 4
    });

    client.log_warning("High memory usage", Some("api"), props! {
        "used_mb" => 3800,
        "total_mb" => 4096
    });

    client.log_error("Database connection failed", Some("api"), props! {
        "host" => "db.internal",
        "error" => "connection refused",
        "retry_count" => 3
    });

    client.log_debug("Cache miss", Some("cache"), props! {
        "key" => "user:123:profile",
        "ttl_remaining" => 0
    });

    client.log_critical("Disk space critical", Some("infra"), props! {
        "mount" => "/data",
        "used_percent" => 98.5
    });

    // Generic log with explicit level
    client.log(
        tell::LogLevel::Notice,
        "Deployment completed",
        Some("deploy"),
        props! {
            "version" => "3.1.0",
            "commit" => "abc123f"
        },
    );

    client.close().await.ok();

    println!("Logs sent.");
}
