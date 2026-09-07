use crate::payload::{JsonObject, object_inner};

fn parse(bytes: &[u8]) -> serde_json::Value {
    serde_json::from_slice(bytes).expect("valid JSON")
}

#[test]
fn test_empty_object() {
    assert_eq!(JsonObject::with_capacity(0).finish(), b"{}");
}

#[test]
fn test_single_field() {
    let b = JsonObject::with_capacity(16)
        .field(b"\"user_id\":", "u1")
        .finish();
    assert_eq!(b, br#"{"user_id":"u1"}"#);
}

#[test]
fn test_field_escapes_value() {
    let b = JsonObject::with_capacity(16)
        .field(b"\"user_id\":", "a\"b")
        .finish();
    assert_eq!(parse(&b)["user_id"], "a\"b");
}

#[test]
fn test_fragment_and_object_order() {
    let b = JsonObject::with_capacity(32)
        .field(b"\"user_id\":", "u1")
        .fragment(br#""app":"web","plan":"free""#)
        .object(Some(br#"{"plan":"pro"}"#))
        .finish();
    assert_eq!(
        b,
        br#"{"user_id":"u1","app":"web","plan":"free","plan":"pro"}"#
    );
    // Last key wins under serde_json, so caller props override super props.
    assert_eq!(parse(&b)["plan"], "pro");
}

#[test]
fn test_empty_fragment_and_empty_object_add_nothing() {
    let b = JsonObject::with_capacity(16)
        .field(b"\"k\":", 1)
        .fragment(b"")
        .object(Some(b"{}"))
        .object(None)
        .finish();
    assert_eq!(b, br#"{"k":1}"#);
}

#[test]
fn test_non_object_payload_ignored() {
    let b = JsonObject::with_capacity(16)
        .field(b"\"k\":", 1)
        .object(Some(b"[1,2]"))
        .object(Some(b"\"str\""))
        .finish();
    assert_eq!(b, br#"{"k":1}"#);
}

#[test]
fn test_fragment_first_is_valid() {
    let b = JsonObject::with_capacity(16)
        .fragment(br#""a":1"#)
        .field(b"\"b\":", 2)
        .finish();
    assert_eq!(b, br#"{"a":1,"b":2}"#);
}

#[test]
fn test_object_inner() {
    assert_eq!(object_inner(Some(br#"{"a":1}"#)), Some(&br#""a":1"#[..]));
    assert_eq!(object_inner(Some(b"{}")), None);
    assert_eq!(object_inner(Some(b"[]")), None);
    assert_eq!(object_inner(None), None);
}
