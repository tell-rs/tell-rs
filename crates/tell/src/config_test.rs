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
    assert!(TellConfig::builder("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz").build().is_err());
}

#[test]
fn api_key_decoded_correctly() {
    let config = TellConfig::builder(VALID_KEY).build().unwrap();
    assert_eq!(config.api_key_bytes[0], 0xa1);
    assert_eq!(config.api_key_bytes[1], 0xb2);
}

#[test]
fn on_error_callback() {
    let config = TellConfig::builder(VALID_KEY)
        .on_error(|_| {})
        .build()
        .unwrap();
    assert!(config.on_error.is_some());
}
