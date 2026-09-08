//! A gauge: a reader closure the worker calls on each sample tick.

use crate::types::{MetricType, QueuedMetric, Temporality};

use super::label_pair;

/// A reader appends `(label value, reading)` pairs. An unlabelled gauge
/// appends exactly one pair whose label value is ignored.
pub(super) type Reader = Box<dyn Fn(&mut Vec<(&'static str, f64)>) + Send + Sync>;

/// A point-in-time value read on every sample tick.
///
/// Registered through [`Metrics::gauge`](super::Metrics::gauge) or
/// [`Metrics::gauge_by`](super::Metrics::gauge_by). The reader runs on the
/// SDK worker, so it must be quick: an atomic load, a short lock, a small
/// scan. Anything slower delays every batch behind it.
pub struct Gauge {
    name: &'static str,
    key: Option<&'static str>,
    read: Reader,
}

impl Gauge {
    pub(super) fn new(name: &'static str, key: Option<&'static str>, read: Reader) -> Self {
        Self { name, key, read }
    }

    /// Metric name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Call the reader and append one point per pair it produced.
    pub(super) fn sample(
        &self,
        timestamp: u64,
        buf: &mut Vec<(&'static str, f64)>,
        out: &mut Vec<QueuedMetric>,
    ) {
        buf.clear();
        (self.read)(buf);
        for (value, reading) in buf.iter() {
            out.push(QueuedMetric {
                metric_type: MetricType::Gauge,
                timestamp,
                name: self.name.into(),
                value: *reading,
                labels: label_pair(self.key, value),
                temporality: Temporality::Unspecified,
                histogram: None,
            });
        }
    }
}

#[cfg(test)]
#[path = "gauge_test.rs"]
mod gauge_test;
