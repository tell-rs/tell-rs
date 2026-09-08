use super::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[test]
fn test_unlabelled_gauge_reads_reader_on_each_sample() {
    let depth = Arc::new(AtomicU64::new(3));
    let d = depth.clone();
    let reader: Reader = Box::new(move |out| out.push(("", d.load(Ordering::Relaxed) as f64)));
    let g = Gauge::new("queue_depth", None, reader);
    assert_eq!(g.name(), "queue_depth");

    let mut buf = Vec::new();
    let mut out = Vec::new();
    g.sample(1, &mut buf, &mut out);
    depth.store(0, Ordering::Relaxed);
    g.sample(2, &mut buf, &mut out);

    assert_eq!(out.len(), 2);
    assert_eq!(out[0].value, 3.0);
    assert_eq!(
        out[1].value, 0.0,
        "a drained queue ships a zero, never a stale value"
    );
    assert!(out[0].labels.is_empty());
    assert_eq!(out[0].metric_type, MetricType::Gauge);
    assert_eq!(out[0].temporality, Temporality::Unspecified);
}

#[test]
fn test_labelled_gauge_ships_one_point_per_pair() {
    let reader: Reader = Box::new(|out| {
        out.push(("pending", 5.0));
        out.push(("running", 2.0));
    });
    let g = Gauge::new("jobs_queued", Some("status"), reader);
    let mut buf = Vec::new();
    let mut out = Vec::new();
    g.sample(1, &mut buf, &mut out);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].labels, vec![("status".into(), "pending".into())]);
    assert_eq!(out[0].value, 5.0);
    assert_eq!(out[1].labels, vec![("status".into(), "running".into())]);
    assert_eq!(out[1].value, 2.0);
}

#[test]
fn test_reader_that_appends_nothing_ships_nothing() {
    let reader: Reader = Box::new(|_| {});
    let g = Gauge::new("silent", None, reader);
    let mut out = Vec::new();
    g.sample(1, &mut Vec::new(), &mut out);
    assert!(out.is_empty());
}
