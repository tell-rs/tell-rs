use serde_json::json;
use tell::{Tell, TellConfig};

#[tokio::main]
async fn main() {
    let client = Tell::new(
        TellConfig::development("a1b2c3d4e5f60718293a4b5c6d7e8f90")
            .expect("invalid config"),
    )
    .expect("failed to create client");

    // Structured logging at different levels
    client.log_info("Server started", Some("api"), Some(json!({
        "port": 8080,
        "workers": 4
    })));

    client.log_warning("High memory usage", Some("api"), Some(json!({
        "used_mb": 3800,
        "total_mb": 4096
    })));

    client.log_error("Database connection failed", Some("api"), Some(json!({
        "host": "db.internal",
        "error": "connection refused",
        "retry_count": 3
    })));

    client.log_debug("Cache miss", Some("cache"), Some(json!({
        "key": "user:123:profile",
        "ttl_remaining": 0
    })));

    client.log_critical("Disk space critical", Some("infra"), Some(json!({
        "mount": "/data",
        "used_percent": 98.5
    })));

    // Generic log with explicit level
    client.log(
        tell::LogLevel::Notice,
        "Deployment completed",
        Some("deploy"),
        Some(json!({
            "version": "3.1.0",
            "commit": "abc123f"
        })),
    );

    // Flush and close
    client.close().await.ok();

    println!("Logs sent successfully.");
}
