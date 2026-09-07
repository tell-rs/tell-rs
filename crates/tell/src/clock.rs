//! Millisecond wall-clock timestamps with a fast monotonic hot path.
//!
//! [`now_ms`] reads quanta's raw counter (a few nanoseconds) and adds it to a
//! wall-clock anchor captured at first use. Monotonic clocks do not advance
//! during system suspend and never see NTP steps, so the background worker
//! calls [`resync`] once a second to fold the current drift into a single
//! atomic correction. The hot path stays one counter read, one relaxed load,
//! and an add.

use std::sync::LazyLock;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static CLOCK: LazyLock<quanta::Clock> = LazyLock::new(quanta::Clock::new);

/// `(system_ms, raw_ticks)` captured together at first use.
///
/// The clock is forced first: quanta calibrates on creation, and that must
/// not sit between the two readings.
static ANCHOR: LazyLock<(u64, u64)> = LazyLock::new(|| {
    let clock = LazyLock::force(&CLOCK);
    (system_ms(), clock.raw())
});

/// Signed correction applied to the monotonic estimate, refreshed by [`resync`].
static CORRECTION_MS: AtomicI64 = AtomicI64::new(0);

/// Wall-clock milliseconds since the Unix epoch via `SystemTime`.
fn system_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Monotonic estimate: anchor plus elapsed counter ticks, uncorrected.
fn monotonic_ms() -> u64 {
    let (anchor_system, anchor_raw) = *ANCHOR;
    anchor_system + CLOCK.delta_as_nanos(anchor_raw, CLOCK.raw()) / 1_000_000
}

/// Current wall-clock time in milliseconds since the Unix epoch.
#[inline]
pub(crate) fn now_ms() -> u64 {
    let estimate = monotonic_ms() as i64 + CORRECTION_MS.load(Ordering::Relaxed);
    estimate.max(0) as u64
}

/// Re-anchor against `SystemTime`. Returns the drift that was corrected, in ms.
///
/// Called periodically by the worker. Cheap: one `SystemTime::now` call.
pub(crate) fn resync() -> i64 {
    let drift = system_ms() as i64 - monotonic_ms() as i64;
    CORRECTION_MS.store(drift, Ordering::Relaxed);
    drift
}

/// Inject a bogus correction so tests can prove `resync` repairs it.
#[cfg(test)]
pub(crate) fn set_correction_for_test(ms: i64) {
    CORRECTION_MS.store(ms, Ordering::Relaxed);
}
