use super::*;

#[test]
fn test_empty_registry_samples_nothing() {
    let m = Metrics::new();
    assert!(m.is_empty());
    let mut out = Vec::new();
    m.sample(1, &mut out);
    assert!(
        out.is_empty(),
        "no instruments, no traffic — not even the dropped gauge"
    );
}

#[test]
fn test_sample_ships_every_instrument_plus_dropped_gauge() {
    let m = Metrics::new();
    let uploads = m
        .counter("uploads_total")
        .by("source", &["web", "api"])
        .register();
    let bytes = m.counter("bytes_total").cumulative().register();
    let latency = m.histogram("latency_ms", &[100.0]).register();
    m.gauge("workers", || 4.0);
    m.gauge_by("jobs", "status", |out| out.push(("pending", 1.0)));
    assert_eq!(m.len(), 5);

    uploads.add("web", 2);
    bytes.inc(10);
    latency.record(50.0);
    m.record_drop();
    m.record_drop();

    let mut out = Vec::new();
    m.sample(42, &mut out);
    let names: Vec<&str> = out.iter().map(|q| q.name.as_ref()).collect();
    assert_eq!(
        names,
        vec![
            "uploads_total",
            "uploads_total",
            "bytes_total",
            "latency_ms",
            "workers",
            "jobs",
            DROPPED_GAUGE,
        ]
    );
    assert!(out.iter().all(|q| q.timestamp == 42));
    let dropped = out.last().expect("dropped gauge");
    assert_eq!(dropped.value, 2.0);
    assert_eq!(dropped.metric_type, MetricType::Gauge);
    assert_eq!(m.dropped(), 2);
}

#[test]
fn test_counter_builder_defaults_to_delta_and_cumulative_opts_in() {
    let m = Metrics::new();
    let d = m.counter("d").register();
    let c = m.counter("c").cumulative().register();
    assert_eq!(d.temporality(), Temporality::Delta);
    assert_eq!(c.temporality(), Temporality::Cumulative);
}

#[test]
fn test_handles_stay_valid_after_registration_and_share_state() {
    let m = Metrics::new();
    let a = m.counter("hits").register();
    let b = Arc::clone(&a);
    a.inc(1);
    b.inc(1);
    let mut out = Vec::new();
    m.sample(1, &mut out);
    assert_eq!(out[0].value, 2.0);
}

#[test]
fn test_label_pair_is_empty_without_a_key() {
    assert!(label_pair(None, "x").is_empty());
    assert_eq!(label_pair(Some("k"), "v"), vec![("k".into(), "v".into())]);
    assert_eq!(series_index(&["a", "b"], "b"), Some(1));
    assert_eq!(series_index(&["a", "b"], "z"), None);
}
