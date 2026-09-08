use super::*;

fn labelled(temporality: Temporality) -> Counter {
    Counter::new(
        "uploads_total",
        Some("source"),
        &["web", "api"],
        temporality,
    )
}

#[test]
fn test_add_and_total_by_label_value() {
    let c = labelled(Temporality::Delta);
    c.add("web", 3);
    c.add("api", 1);
    c.add("web", 2);
    assert_eq!(c.total("web"), 5);
    assert_eq!(c.total("api"), 1);
    assert_eq!(c.name(), "uploads_total");
    assert_eq!(c.temporality(), Temporality::Delta);
}

#[test]
fn test_unknown_label_value_is_dropped() {
    let c = labelled(Temporality::Delta);
    c.add("sha256:deadbeef", 1);
    assert_eq!(c.total("sha256:deadbeef"), 0);
    assert_eq!(c.total("web") + c.total("api"), 0);
}

#[test]
fn test_unlabelled_counter_uses_inc_and_value() {
    let c = Counter::new("requests_total", None, &[], Temporality::Delta);
    c.inc(4);
    c.inc(1);
    assert_eq!(c.value(), 5);
    let mut out = Vec::new();
    c.sample(7, &mut out);
    assert_eq!(out.len(), 1);
    assert!(
        out[0].labels.is_empty(),
        "no label pair on an unlabelled counter"
    );
    assert_eq!(out[0].value, 5.0);
    assert_eq!(out[0].timestamp, 7);
    assert_eq!(out[0].metric_type, MetricType::Counter);
}

#[test]
fn test_delta_sample_ships_change_since_previous_sample() {
    let c = labelled(Temporality::Delta);
    let mut out = Vec::new();
    for _ in 0..1000 {
        c.add("web", 1);
    }
    c.sample(1, &mut out);
    assert_eq!(out.len(), 2, "one point per label value, not per add");
    assert_eq!(out[0].value, 1000.0);
    assert_eq!(out[1].value, 0.0);
    assert_eq!(out[0].temporality, Temporality::Delta);
    assert_eq!(out[0].labels, vec![("source".into(), "web".into())]);

    out.clear();
    c.add("web", 2);
    c.add("api", 1);
    c.sample(2, &mut out);
    assert_eq!(out[0].value, 2.0, "second sample ships only the new adds");
    assert_eq!(out[1].value, 1.0);
    assert_eq!(c.total("web"), 1002, "the total is untouched by sampling");
}

#[test]
fn test_cumulative_sample_ships_running_total() {
    let c = labelled(Temporality::Cumulative);
    let mut out = Vec::new();
    c.add("web", 3);
    c.sample(1, &mut out);
    c.add("web", 4);
    c.sample(2, &mut out);
    let web: Vec<f64> = out
        .iter()
        .filter(|m| m.labels[0].1 == "web")
        .map(|m| m.value)
        .collect();
    assert_eq!(web, vec![3.0, 7.0]);
    assert!(out.iter().all(|m| m.temporality == Temporality::Cumulative));
}
