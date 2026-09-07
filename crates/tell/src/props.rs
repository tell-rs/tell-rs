//! Property payloads: the `Props` builder, the `props!` macro, and `IntoPayload`.

use serde::Serialize;

/// Pre-serialized JSON properties buffer.
///
/// Writes JSON bytes directly into a `Vec<u8>`, skipping the intermediate
/// `serde_json::Value` DOM that `json!()` allocates. Each value is still
/// serialized via `serde_json::to_writer` (safe escaping). Keys are copied
/// verbatim when they contain no quote, backslash, or control byte, and
/// escaped through serde otherwise.
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

/// Whether a key can be emitted between quotes without escaping.
///
/// Rejects `"`, `\`, and control bytes below 0x20. Used at compile time by
/// [`props!`] for literal keys and at runtime by [`Props::add`].
#[doc(hidden)]
#[must_use]
pub const fn key_is_plain(key: &str) -> bool {
    let bytes = key.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c < 0x20 || c == b'"' || c == b'\\' {
            return false;
        }
        i += 1;
    }
    true
}

/// Runtime twin of [`key_is_plain`]: eight bytes per step, no branches per byte.
///
/// Uses the classic SWAR "has byte below n" / "has byte equal to b" tests,
/// which are exact for existence. Keys are short, so this is about one
/// nanosecond for a typical key.
#[inline]
pub(crate) fn key_is_plain_fast(key: &str) -> bool {
    const LO: u64 = 0x0101_0101_0101_0101;
    const HI: u64 = 0x8080_8080_8080_8080;
    const QUOTE: u64 = LO * b'"' as u64;
    const BACKSLASH: u64 = LO * b'\\' as u64;
    const SPACE: u64 = LO * 0x20;

    let bytes = key.as_bytes();
    let mut chunks = bytes.chunks_exact(8);
    let mut bad: u64 = 0;
    for chunk in &mut chunks {
        let mut arr = [0u8; 8];
        arr.copy_from_slice(chunk);
        let w = u64::from_le_bytes(arr);
        let below_space = w.wrapping_sub(SPACE) & !w & HI;
        let q = w ^ QUOTE;
        let has_quote = q.wrapping_sub(LO) & !q & HI;
        let b = w ^ BACKSLASH;
        let has_backslash = b.wrapping_sub(LO) & !b & HI;
        bad |= below_space | has_quote | has_backslash;
    }
    let tail_ok = chunks
        .remainder()
        .iter()
        .fold(true, |ok, &c| ok & (c >= 0x20) & (c != b'"') & (c != b'\\'));
    bad == 0 && tail_ok
}

impl Props {
    /// Create a new empty properties buffer.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self {
            buf: Vec::with_capacity(128),
            count: 0,
        }
    }

    /// Add a key-value pair. Both key and value are emitted as valid JSON.
    ///
    /// Keys are scanned once (about one nanosecond for a typical key). Plain
    /// keys are copied raw; keys containing a quote, backslash, or control
    /// byte are escaped through serde.
    #[inline]
    #[must_use]
    pub fn add(mut self, key: &str, value: impl Serialize) -> Self {
        self.separator();
        if key_is_plain_fast(key) {
            self.buf.push(b'"');
            self.buf.extend_from_slice(key.as_bytes());
            self.buf.extend_from_slice(b"\":");
        } else {
            // Writing to a Vec cannot fail and &str always serializes.
            let _ = serde_json::to_writer(&mut self.buf, key);
            self.buf.push(b':');
        }
        // Writing to a Vec cannot fail; non-finite floats serialize as null.
        let _ = serde_json::to_writer(&mut self.buf, &value);
        self.count += 1;
        self
    }

    /// Add a key already proven plain at compile time by [`props!`]. Skips the scan.
    #[doc(hidden)]
    #[inline]
    #[must_use]
    pub fn add_plain(mut self, key: &str, value: impl Serialize) -> Self {
        debug_assert!(key_is_plain(key));
        self.separator();
        self.buf.push(b'"');
        self.buf.extend_from_slice(key.as_bytes());
        self.buf.extend_from_slice(b"\":");
        let _ = serde_json::to_writer(&mut self.buf, &value);
        self.count += 1;
        self
    }

    #[inline]
    fn separator(&mut self) {
        if self.count == 0 {
            self.buf.push(b'{');
        } else {
            self.buf.push(b',');
        }
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
/// String-literal keys are validated at compile time and copied without a
/// runtime scan. Any other key expression is scanned at runtime and escaped
/// if needed.
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
    () => { $crate::Props::new() };
    ($($tt:tt)+) => { $crate::__props_inner!([$crate::Props::new()] $($tt)+) };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __props_inner {
    ([$b:expr]) => { $b };
    ([$b:expr] $key:literal => $value:expr $(, $($rest:tt)*)?) => {
        $crate::__props_inner!([{
            const _: () = assert!(
                $crate::key_is_plain($key),
                "props! literal key contains a quote, backslash, or control character"
            );
            $b.add_plain($key, $value)
        }] $($($rest)*)?)
    };
    ([$b:expr] $key:expr => $value:expr $(, $($rest:tt)*)?) => {
        $crate::__props_inner!([$b.add($key, $value)] $($($rest)*)?)
    };
}

// ---------------------------------------------------------------------------
// IntoPayload — unifies Props, Option<impl Serialize>, Value, and ()
// ---------------------------------------------------------------------------

/// Trait for types that can become a JSON payload.
///
/// Implemented for [`Props`] (zero-serde fast path), `Option<T: Serialize>`,
/// `serde_json::Value`, and `()` for "no properties".
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

/// `()` means "no properties". Cheaper to write than `None::<serde_json::Value>`.
impl IntoPayload for () {
    #[inline]
    fn into_payload(self) -> Option<Vec<u8>> {
        None
    }
}
