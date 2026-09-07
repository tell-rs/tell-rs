//! Metric methods: gauge, counter, histogram.
//!
//! Metric timestamps are nanoseconds since the Unix epoch (events and logs
//! use milliseconds).

use std::borrow::Cow;

use super::Tell;
use crate::clock::now_ms;
use crate::error::TellError;
use crate::types::{HistogramParams, MetricLabel, MetricType, QueuedMetric, Temporality};
use crate::worker::WorkerMessage;

impl Tell {
    /// Send a gauge metric (point-in-time value).
    ///
    /// Labels are string key-value pairs for metric dimensions
    /// (e.g. `&[("core", "0"), ("host", "web-01")]`).
    ///
    /// Zero heap allocation for the labels when name and labels are string literals.
    /// Never blocks, never panics.
    pub fn gauge(&self, name: &'static str, value: f64, labels: &[(&'static str, &'static str)]) {
        self.send_metric(
            MetricType::Gauge,
            name,
            value,
            static_labels(labels),
            Temporality::Unspecified,
            None,
        );
    }

    /// Send a counter metric (cumulative or delta count).
    ///
    /// Uses delta temporality by default (change since last report).
    pub fn counter(&self, name: &'static str, value: f64, labels: &[(&'static str, &'static str)]) {
        self.send_metric(
            MetricType::Counter,
            name,
            value,
            static_labels(labels),
            Temporality::Delta,
            None,
        );
    }

    /// Send a counter metric with explicit temporality.
    pub fn counter_with_temporality(
        &self,
        name: &'static str,
        value: f64,
        labels: &[(&'static str, &'static str)],
        temporality: Temporality,
    ) {
        self.send_metric(
            MetricType::Counter,
            name,
            value,
            static_labels(labels),
            temporality,
            None,
        );
    }

    /// Send a histogram metric (distribution with explicit buckets).
    ///
    /// `buckets` is a list of `(upper_bound, cumulative_count)` sorted by upper_bound.
    /// Use `f64::INFINITY` for the final catch-all bucket.
    pub fn histogram(
        &self,
        name: &'static str,
        histogram: HistogramParams,
        labels: &[(&'static str, &'static str)],
    ) {
        self.send_metric(
            MetricType::Histogram,
            name,
            0.0,
            static_labels(labels),
            Temporality::Cumulative,
            Some(histogram),
        );
    }

    // --- Dynamic label variants (for runtime-generated label values) ---

    /// Send a gauge with dynamic (non-static) label values. Allocates per call.
    pub fn gauge_dyn(&self, name: &'static str, value: f64, labels: &[(&'static str, &str)]) {
        self.send_metric(
            MetricType::Gauge,
            name,
            value,
            dyn_labels(labels),
            Temporality::Unspecified,
            None,
        );
    }

    /// Send a counter with dynamic label values. Allocates per call.
    pub fn counter_dyn(&self, name: &'static str, value: f64, labels: &[(&'static str, &str)]) {
        self.send_metric(
            MetricType::Counter,
            name,
            value,
            dyn_labels(labels),
            Temporality::Delta,
            None,
        );
    }

    /// Send a counter with dynamic label values and explicit temporality.
    pub fn counter_dyn_with_temporality(
        &self,
        name: &'static str,
        value: f64,
        labels: &[(&'static str, &str)],
        temporality: Temporality,
    ) {
        self.send_metric(
            MetricType::Counter,
            name,
            value,
            dyn_labels(labels),
            temporality,
            None,
        );
    }

    fn send_metric(
        &self,
        metric_type: MetricType,
        name: &'static str,
        value: f64,
        labels: Vec<MetricLabel>,
        temporality: Temporality,
        histogram: Option<HistogramParams>,
    ) {
        if name.is_empty() {
            self.report_error(TellError::validation("name", "metric name is required"));
            return;
        }

        self.enqueue(WorkerMessage::Metric(QueuedMetric {
            metric_type,
            timestamp: now_ms() * 1_000_000,
            name: Cow::Borrowed(name),
            value,
            labels,
            temporality,
            histogram,
        }));
    }
}

/// Borrow static labels: no per-label allocation.
fn static_labels(labels: &[(&'static str, &'static str)]) -> Vec<MetricLabel> {
    labels
        .iter()
        .map(|&(k, v)| (Cow::Borrowed(k), Cow::Borrowed(v)))
        .collect()
}

/// Static keys, owned values.
fn dyn_labels(labels: &[(&'static str, &str)]) -> Vec<MetricLabel> {
    labels
        .iter()
        .map(|&(k, v)| (Cow::Borrowed(k), Cow::Owned(v.to_owned())))
        .collect()
}
