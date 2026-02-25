use crate::validation::*;

#[test]
fn valid_api_key() {
    let result = validate_and_decode_api_key("feed1e11feed1e11feed1e11feed1e11");
    assert!(result.is_ok());
    let bytes = result.unwrap();
    assert_eq!(bytes[0], 0xa1);
    assert_eq!(bytes[1], 0xb2);
    assert_eq!(bytes[15], 0x90);
}

#[test]
fn api_key_uppercase_hex() {
    let result = validate_and_decode_api_key("A1B2C3D4E5F60718293A4B5C6D7E8F90");
    assert!(result.is_ok());
}

#[test]
fn api_key_too_short() {
    let result = validate_and_decode_api_key("abc123");
    assert!(result.is_err());
}

#[test]
fn api_key_too_long() {
    let result = validate_and_decode_api_key("feed1e11feed1e11feed1e11feed1e11ff");
    assert!(result.is_err());
}

#[test]
fn api_key_non_hex() {
    let result = validate_and_decode_api_key("g1b2c3d4e5f60718293a4b5c6d7e8f90");
    assert!(result.is_err());
}

#[test]
fn api_key_empty() {
    let result = validate_and_decode_api_key("");
    assert!(result.is_err());
}

#[test]
fn valid_user_id() {
    assert!(validate_user_id("user_123").is_ok());
}

#[test]
fn empty_user_id() {
    assert!(validate_user_id("").is_err());
}

#[test]
fn valid_event_name() {
    assert!(validate_event_name("Page Viewed").is_ok());
}

#[test]
fn empty_event_name() {
    assert!(validate_event_name("").is_err());
}

#[test]
fn long_event_name() {
    let name = "x".repeat(257);
    assert!(validate_event_name(&name).is_err());
}

#[test]
fn valid_log_message() {
    assert!(validate_log_message("Server started").is_ok());
}

#[test]
fn empty_log_message() {
    assert!(validate_log_message("").is_err());
}

#[test]
fn long_log_message() {
    let msg = "x".repeat(65_537);
    assert!(validate_log_message(&msg).is_err());
}
