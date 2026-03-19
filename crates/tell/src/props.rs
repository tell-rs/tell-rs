use serde::Serialize;

/// Pre-serialized JSON properties buffer.
///
/// Writes JSON bytes directly into a `Vec<u8>`, skipping the intermediate
/// `serde_json::Value` DOM that `json!()` allocates. Each value is still
/// serialized via `serde_json::to_writer` (safe escaping).
///
/// # Example
///
/// ```
/// use tell::Props;
///
/// let props = Props::new()
///     .add("url", "/home")
///     .add("status", 200)
///     .add("active", true);
/// ```
pub struct Props {
    buf: Vec<u8>,
    count: usize,
}

impl Default for Props {
    fn default() -> Self {
        Self::new()
    }
}

impl Props {
    /// Create a new empty properties buffer.
    #[inline]
    pub fn new() -> Self {
        Self {
            buf: Vec::with_capacity(128),
            count: 0,
        }
    }

    /// Add a key-value pair. The value is serialized via serde (safe escaping).
    #[inline]
    pub fn add(mut self, key: &str, value: impl Serialize) -> Self {
        if self.count == 0 {
            self.buf.push(b'{');
        } else {
            self.buf.push(b',');
        }
        self.buf.push(b'"');
        self.buf.extend_from_slice(key.as_bytes());
        self.buf.extend_from_slice(b"\":");
        let _ = serde_json::to_writer(&mut self.buf, &value);
        self.count += 1;
        self
    }

    /// Finish building and return the JSON bytes.
    #[inline]
    pub(crate) fn finish(mut self) -> Vec<u8> {
        if self.count == 0 {
            self.buf.extend_from_slice(b"{}");
        } else {
            self.buf.push(b'}');
        }
        self.buf
    }
}

/// Construct [`Props`] with a concise syntax.
///
/// # Example
///
/// ```
/// use tell::props;
///
/// let p = props! {
///     "url" => "/home",
///     "referrer" => "google",
///     "status" => 200
/// };
/// ```
#[macro_export]
macro_rules! props {
    ($($key:expr => $value:expr),+ $(,)?) => {
        $crate::Props::new()
            $(.add($key, $value))+
    };
}

// ---------------------------------------------------------------------------
// IntoPayload — unifies Props, Option<impl Serialize>, and None
// ---------------------------------------------------------------------------

/// Trait for types that can become a JSON payload.
///
/// Implemented for [`Props`] (zero-serde fast path) and
/// `Option<T: Serialize>` (existing serde path).
pub trait IntoPayload {
    #[doc(hidden)]
    fn into_payload(self) -> Option<Vec<u8>>;
}

impl IntoPayload for Props {
    #[inline]
    fn into_payload(self) -> Option<Vec<u8>> {
        Some(self.finish())
    }
}

impl<T: Serialize> IntoPayload for Option<T> {
    #[inline]
    fn into_payload(self) -> Option<Vec<u8>> {
        self.and_then(|v| serde_json::to_vec(&v).ok())
    }
}

impl IntoPayload for serde_json::Value {
    #[inline]
    fn into_payload(self) -> Option<Vec<u8>> {
        serde_json::to_vec(&self).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn props_empty() {
        let p = Props::new();
        assert_eq!(p.finish(), b"{}");
    }

    #[test]
    fn props_single_string() {
        let p = Props::new().add("url", "/home");
        assert_eq!(p.finish(), br#"{"url":"/home"}"#);
    }

    #[test]
    fn props_multiple_types() {
        let p = Props::new()
            .add("url", "/home")
            .add("count", 42u32)
            .add("active", true)
            .add("rate", 2.78f64);
        let json: serde_json::Value = serde_json::from_slice(&p.finish()).unwrap();
        assert_eq!(json["url"], "/home");
        assert_eq!(json["count"], 42);
        assert_eq!(json["active"], true);
        assert_eq!(json["rate"], 2.78);
    }

    #[test]
    fn props_escapes_strings() {
        let dangerous = "O'Brien\"};DROP TABLE";
        let p = Props::new().add("name", dangerous);
        let bytes = p.finish();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["name"], dangerous);
    }

    #[test]
    fn props_macro() {
        let p = props! {
            "url" => "/home",
            "count" => 42,
        };
        let bytes = p.finish();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["url"], "/home");
        assert_eq!(json["count"], 42);
    }

    #[test]
    fn props_macro_dynamic_values() {
        let url = String::from("/search");
        let count: u64 = 99;
        let p = props! {
            "url" => &url,
            "count" => count,
        };
        let bytes = p.finish();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["url"], "/search");
        assert_eq!(json["count"], 99);
    }

    #[test]
    fn into_payload_props() {
        let p = Props::new().add("url", "/home");
        let bytes = p.into_payload().unwrap();
        assert_eq!(bytes, br#"{"url":"/home"}"#);
    }

    #[test]
    fn into_payload_option_some() {
        let opt = Some(serde_json::json!({"url": "/home"}));
        let bytes = opt.into_payload().unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["url"], "/home");
    }

    #[test]
    fn into_payload_option_none() {
        let opt: Option<serde_json::Value> = None;
        assert!(opt.into_payload().is_none());
    }

    #[test]
    fn props_default() {
        let p = Props::default();
        assert_eq!(p.finish(), b"{}");
    }
}
