use crate::props::{IntoPayload, Props, key_is_plain, key_is_plain_fast};

fn parse(bytes: &[u8]) -> serde_json::Value {
    serde_json::from_slice(bytes).expect("valid JSON")
}

#[test]
fn test_props_empty() {
    assert_eq!(Props::new().finish(), b"{}");
}

#[test]
fn test_props_single_string() {
    let p = Props::new().add("url", "/home");
    assert_eq!(p.finish(), br#"{"url":"/home"}"#);
}

#[test]
fn test_props_multiple_types() {
    let p = Props::new()
        .add("url", "/home")
        .add("count", 42u32)
        .add("active", true)
        .add("rate", 2.78f64);
    let json = parse(&p.finish());
    assert_eq!(json["url"], "/home");
    assert_eq!(json["count"], 42);
    assert_eq!(json["active"], true);
    assert_eq!(json["rate"], 2.78);
}

#[test]
fn test_props_escapes_string_values() {
    let dangerous = "O'Brien\"};DROP TABLE";
    let p = Props::new().add("name", dangerous);
    assert_eq!(parse(&p.finish())["name"], dangerous);
}

#[test]
fn test_props_escapes_dynamic_keys() {
    let key = String::from("a\"b\\c\n");
    let p = Props::new().add(&key, 1);
    let bytes = p.finish();
    let json = parse(&bytes);
    assert_eq!(json[&key], 1);
}

#[test]
fn test_props_plain_key_copied_verbatim() {
    let p = Props::new().add("plain_key-1.two", 1);
    assert_eq!(p.finish(), br#"{"plain_key-1.two":1}"#);
}

#[test]
fn test_key_is_plain_fast_matches_const() {
    for key in [
        "url",
        "with space and ünïcode",
        "quote\"",
        "back\\slash",
        "tab\t",
        "",
        "\u{1f}x",
    ] {
        assert_eq!(key_is_plain_fast(key), key_is_plain(key), "{key:?}");
    }
}

#[test]
fn test_key_is_plain() {
    assert!(key_is_plain("url"));
    assert!(key_is_plain("with space and ünïcode"));
    assert!(!key_is_plain("quote\""));
    assert!(!key_is_plain("back\\slash"));
    assert!(!key_is_plain("tab\t"));
    assert!(key_is_plain(""));
}

#[test]
fn test_props_macro_literal_keys() {
    let p = crate::props! {
        "url" => "/home",
        "count" => 42,
    };
    let json = parse(&p.finish());
    assert_eq!(json["url"], "/home");
    assert_eq!(json["count"], 42);
}

#[test]
fn test_props_macro_dynamic_keys_and_values() {
    let url = String::from("/search");
    let key = String::from("dyn\"key");
    let count: u64 = 99;
    let p = crate::props! {
        "url" => &url,
        key.as_str() => count,
        "static" => 1,
    };
    let json = parse(&p.finish());
    assert_eq!(json["url"], "/search");
    assert_eq!(json["dyn\"key"], 99);
    assert_eq!(json["static"], 1);
}

#[test]
fn test_props_macro_empty() {
    let p = crate::props! {};
    assert_eq!(p.finish(), b"{}");
}

#[test]
fn test_into_payload_props() {
    let bytes = Props::new().add("url", "/home").into_payload().unwrap();
    assert_eq!(bytes, br#"{"url":"/home"}"#);
}

#[test]
fn test_into_payload_option_some() {
    let bytes = Some(serde_json::json!({"url": "/home"}))
        .into_payload()
        .unwrap();
    assert_eq!(parse(&bytes)["url"], "/home");
}

#[test]
fn test_into_payload_option_none() {
    assert!(None::<serde_json::Value>.into_payload().is_none());
}

#[test]
fn test_into_payload_unit() {
    assert!(().into_payload().is_none());
}

#[test]
fn test_props_default() {
    assert_eq!(Props::default().finish(), b"{}");
}
