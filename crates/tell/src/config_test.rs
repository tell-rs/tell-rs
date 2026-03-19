use std::time::Duration;

use crate::config::*;

const VALID_KEY: &str = "feed1e11feed1e11feed1e11feed1e11";

#[test]
fn builder_defaults() {
    let config = TellConfig::builder(VALID_KEY).build().unwrap();
    assert_eq!(config.endpoint, DEFAULT_ENDPOINT);
    assert_eq!(config.batch_size, 100);
    assert_eq!(config.flush_interval, Duration::from_secs(10));
    assert_eq!(config.max_retries, 3);
    assert_eq!(config.close_timeout, Duration::from_secs(5));
}

#[test]
fn builder_custom_values() {
    let config = TellConfig::builder(VALID_KEY)
        .endpoint("localhost:9999")
        .batch_size(50)
        .flush_interval(Duration::from_secs(5))
        .max_retries(5)
        .close_timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    assert_eq!(config.endpoint, "localhost:9999");
    assert_eq!(config.batch_size, 50);
    assert_eq!(config.flush_interval, Duration::from_secs(5));
    assert_eq!(config.max_retries, 5);
    assert_eq!(config.close_timeout, Duration::from_secs(10));
}

#[test]
fn development_preset() {
    let config = TellConfig::development(VALID_KEY).unwrap();
    assert_eq!(config.endpoint, DEV_ENDPOINT);
    assert_eq!(config.batch_size, 10);
    assert_eq!(config.flush_interval, Duration::from_secs(2));
}

#[test]
fn production_preset() {
    let config = TellConfig::production(VALID_KEY).unwrap();
    assert_eq!(config.endpoint, DEFAULT_ENDPOINT);
    assert_eq!(config.batch_size, 100);
}

#[test]
fn invalid_api_key_rejected() {
    assert!(TellConfig::builder("bad").build().is_err());
    assert!(TellConfig::builder("").build().is_err());
    assert!(
        TellConfig::builder("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz")
            .build()
            .is_err()
    );
}

#[test]
fn api_key_decoded_correctly() {
    let config = TellConfig::builder(VALID_KEY).build().unwrap();
    assert_eq!(config.api_key_bytes[0], 0xfe);
    assert_eq!(config.api_key_bytes[1], 0xed);
}

#[test]
fn on_error_callback() {
    let config = TellConfig::builder(VALID_KEY)
        .on_error(|_| {})
        .build()
        .unwrap();
    assert!(config.on_error.is_some());
}

#[test]
fn empty_service_rejected() {
    let result = TellConfig::builder(VALID_KEY).service("").build();
    assert!(result.is_err());
}

#[test]
fn debug_format() {
    let config = TellConfig::builder(VALID_KEY).build().unwrap();
    let debug = format!("{:?}", config);
    assert!(debug.contains("TellConfig"));
    assert!(debug.contains("endpoint"));
    assert!(debug.contains("batch_size"));
}

#[test]
fn builder_optional_setters() {
    let config = TellConfig::builder(VALID_KEY)
        .source("web-01")
        .network_timeout(Duration::from_secs(15))
        .buffer_path("/tmp/tell-wal")
        .buffer_max_bytes(1024 * 1024)
        .build()
        .unwrap();

    assert_eq!(config.source.as_deref(), Some("web-01"));
    assert_eq!(config.network_timeout, Duration::from_secs(15));
    assert_eq!(
        config.buffer_path.as_deref(),
        Some(std::path::Path::new("/tmp/tell-wal"))
    );
    assert_eq!(config.buffer_max_bytes, 1024 * 1024);
}
