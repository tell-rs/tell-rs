//! Byte-level JSON object builder for event payloads.
//!
//! Splices pre-serialized fragments (super properties, caller props) into a
//! single output buffer without going through a `serde_json::Value` DOM.

use serde::Serialize;

/// Incremental JSON object writer. Fields are appended in call order.
pub(crate) struct JsonObject {
    buf: Vec<u8>,
}

impl JsonObject {
    /// Start an object with room for roughly `cap` bytes of content.
    #[inline]
    pub(crate) fn with_capacity(cap: usize) -> Self {
        let mut buf = Vec::with_capacity(cap + 2);
        buf.push(b'{');
        Self { buf }
    }

    #[inline]
    fn separator(&mut self) {
        if self.buf.len() > 1 {
            self.buf.push(b',');
        }
    }

    /// Append `key_colon` (a literal like `b"\"user_id\":"`) and a serialized value.
    #[inline]
    pub(crate) fn field(mut self, key_colon: &[u8], value: impl Serialize) -> Self {
        self.separator();
        self.buf.extend_from_slice(key_colon);
        // Writing to a Vec cannot fail; non-finite floats serialize as null.
        let _ = serde_json::to_writer(&mut self.buf, &value);
        self
    }

    /// Splice a pre-serialized fragment of `"k":v,"k2":v2` pairs (no braces).
    #[inline]
    pub(crate) fn fragment(mut self, fragment: &[u8]) -> Self {
        if fragment.is_empty() {
            return self;
        }
        self.separator();
        self.buf.extend_from_slice(fragment);
        self
    }

    /// Splice the members of a serialized JSON object. Non-objects are ignored.
    #[inline]
    pub(crate) fn object(mut self, object: Option<&[u8]>) -> Self {
        if let Some(inner) = object_inner(object) {
            self.separator();
            self.buf.extend_from_slice(inner);
        }
        self
    }

    /// Close the object and return the bytes.
    #[inline]
    pub(crate) fn finish(mut self) -> Vec<u8> {
        self.buf.push(b'}');
        self.buf
    }
}

/// The members of a serialized JSON object, without the surrounding braces.
///
/// Returns `None` for `{}`, non-objects, and `None` input.
#[inline]
pub(crate) fn object_inner(bytes: Option<&[u8]>) -> Option<&[u8]> {
    let b = bytes?;
    if b.len() > 2 && b.first() == Some(&b'{') && b.last() == Some(&b'}') {
        Some(&b[1..b.len() - 1])
    } else {
        None
    }
}
