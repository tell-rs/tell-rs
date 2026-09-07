use crate::clock::{now_ms, resync, set_correction_for_test};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// The correction is process-global; serialize tests that touch it.
static LOCK: Mutex<()> = Mutex::new(());

fn wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[test]
fn test_now_ms_tracks_system_time() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    resync();
    let now = now_ms();
    let wall = wall_ms();
    assert!(now.abs_diff(wall) < 100, "now_ms {now} vs wall {wall}");
}

#[test]
fn test_resync_repairs_injected_drift() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_correction_for_test(3_600_000);
    let drifted = now_ms();
    let drift = resync();
    let repaired = now_ms();
    let wall = wall_ms();

    assert!(drifted > wall + 3_000_000, "injected drift not visible");
    assert!(
        repaired.abs_diff(wall) < 100,
        "resync did not repair: {repaired} vs {wall}"
    );
    assert!(
        drift.abs() < 100,
        "reported drift {drift} unexpectedly large"
    );
}

#[test]
fn test_now_ms_never_goes_backwards_between_calls() {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    resync();
    let a = now_ms();
    let b = now_ms();
    assert!(b >= a);
}
