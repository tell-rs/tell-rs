//! A counter handle: call sites add, the worker ships one point per label
//! value on each sample tick.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::types::{MetricType, QueuedMetric, Temporality};

use super::{label_pair, series_index};

/// A monotonic counter split by one closed set of label values.
///
/// Obtained from [`Metrics::counter`](super::Metrics::counter). `add` is a
/// single relaxed atomic increment; it never allocates, locks, or touches
/// the queue.
pub struct Counter {
    name: &'static str,
    key: Option<&'static str>,
    values: Box<[&'static str]>,
    totals: Box<[AtomicU64]>,
    /// Totals as of the previous sample; only the worker writes here.
    shipped: Box<[AtomicU64]>,
    temporality: Temporality,
}

impl Counter {
    pub(super) fn new(
        name: &'static str,
        key: Option<&'static str>,
        values: &[&'static str],
        temporality: Temporality,
    ) -> Self {
        let n = values.len().max(1);
        Self {
            name,
            key,
            values: if values.is_empty() {
                Box::new([""])
            } else {
                values.into()
            },
            totals: (0..n).map(|_| AtomicU64::new(0)).collect(),
            shipped: (0..n).map(|_| AtomicU64::new(0)).collect(),
            temporality,
        }
    }

    /// Metric name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// How the worker ships this counter: the change since the previous
    /// sample (`Delta`) or the running total (`Cumulative`).
    #[must_use]
    pub const fn temporality(&self) -> Temporality {
        self.temporality
    }

    /// Add `n` to the series for `value`. A value outside the registered
    /// set is dropped, so label cardinality can never grow at runtime.
    pub fn add(&self, value: &str, n: u64) {
        if let Some(total) = series_index(&self.values, value).and_then(|i| self.totals.get(i)) {
            total.fetch_add(n, Ordering::Relaxed);
        }
    }

    /// Add `n` to an unlabelled counter (the first series).
    pub fn inc(&self, n: u64) {
        if let Some(total) = self.totals.first() {
            total.fetch_add(n, Ordering::Relaxed);
        }
    }

    /// Running total for `value` since the client was created.
    #[must_use]
    pub fn total(&self, value: &str) -> u64 {
        series_index(&self.values, value)
            .and_then(|i| self.totals.get(i))
            .map_or(0, |t| t.load(Ordering::Relaxed))
    }

    /// Running total of an unlabelled counter.
    #[must_use]
    pub fn value(&self) -> u64 {
        self.totals.first().map_or(0, |t| t.load(Ordering::Relaxed))
    }

    /// Append one point per label value. Delta counters ship the change
    /// since the previous call; cumulative counters ship the total.
    pub(super) fn sample(&self, timestamp: u64, out: &mut Vec<QueuedMetric>) {
        let rows = self.values.iter().zip(&self.totals).zip(&self.shipped);
        for ((value, total), shipped) in rows {
            let now = total.load(Ordering::Relaxed);
            let point = match self.temporality {
                Temporality::Cumulative => now,
                _ => now.saturating_sub(shipped.swap(now, Ordering::Relaxed)),
            };
            out.push(QueuedMetric {
                metric_type: MetricType::Counter,
                timestamp,
                name: self.name.into(),
                value: point as f64,
                labels: label_pair(self.key, value),
                temporality: self.temporality,
                histogram: None,
            });
        }
    }
}

#[cfg(test)]
#[path = "counter_test.rs"]
mod counter_test;
