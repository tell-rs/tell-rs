use super::*;

const BOUNDS: [f64; 4] = [100.0, 500.0, 1000.0, f64::INFINITY];

#[test]
fn test_bounds_are_sorted_deduplicated_and_capped_with_infinity() {
    let h = Histogram::new("x", None, &[], &[500.0, 100.0, 500.0, f64::NAN]);
    assert_eq!(h.bounds(), &[100.0, 500.0, f64::INFINITY]);
    let h = Histogram::new("x", None, &[], &BOUNDS);
    assert_eq!(h.bounds(), &BOUNDS);
    let h = Histogram::new("x", None, &[], &[]);
    assert_eq!(
        h.bounds(),
        &[f64::INFINITY],
        "no bounds still yields a catch-all"
    );
}

#[test]
fn test_snapshot_matches_observations() {
    let h = Histogram::new("analysis_ms", Some("format"), &["elf", "pe"], &BOUNDS);
    for v in [50.0, 300.0, 700.0, 4000.0] {
        h.observe("elf", v);
    }
    h.observe("pe", 10.0);
    h.observe("unknown-format", 1.0);
    assert_eq!(h.count("elf"), 4);
    assert_eq!(h.count("pe"), 1);
    assert_eq!(h.count("unknown-format"), 0);

    let mut out = Vec::new();
    h.sample(9, &mut out);
    assert_eq!(out.len(), 2, "only series with observations ship");
    let elf = out
        .iter()
        .find(|m| m.labels[0].1 == "elf")
        .and_then(|m| m.histogram.as_ref())
        .expect("elf histogram");
    assert_eq!(elf.count, 4);
    assert_eq!(elf.sum, 5050.0);
    assert_eq!(elf.min, 50.0);
    assert_eq!(elf.max, 4000.0);
    let counts: Vec<u64> = elf.buckets.iter().map(|(_, c)| *c).collect();
    assert_eq!(counts, vec![1, 2, 3, 4], "buckets are cumulative");
    assert_eq!(out[0].metric_type, MetricType::Histogram);
    assert_eq!(out[0].temporality, Temporality::Cumulative);
}

#[test]
fn test_samples_are_cumulative_across_ticks() {
    let h = Histogram::new("x", None, &[], &BOUNDS);
    let mut out = Vec::new();
    h.record(10.0);
    h.sample(1, &mut out);
    h.record(20.0);
    h.sample(2, &mut out);
    let second = out[1].histogram.as_ref().expect("histogram");
    assert_eq!(
        (second.count, second.sum, second.min, second.max),
        (2, 30.0, 10.0, 20.0)
    );
    assert!(out[1].labels.is_empty());
}

#[test]
fn test_empty_histogram_ships_nothing() {
    let h = Histogram::new("x", None, &[], &BOUNDS);
    let mut out = Vec::new();
    h.sample(1, &mut out);
    assert!(out.is_empty());
}

#[test]
fn test_concurrent_observations_are_not_lost() {
    let h = std::sync::Arc::new(Histogram::new("x", None, &[], &BOUNDS));
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let h = h.clone();
            std::thread::spawn(move || {
                for _ in 0..1000 {
                    h.record(1.5);
                }
            })
        })
        .collect();
    for t in threads {
        t.join().expect("thread");
    }
    let mut out = Vec::new();
    h.sample(1, &mut out);
    let p = out[0].histogram.as_ref().expect("histogram");
    assert_eq!(p.count, 8000);
    assert_eq!(p.sum, 12_000.0, "the float sum survives contention");
}
