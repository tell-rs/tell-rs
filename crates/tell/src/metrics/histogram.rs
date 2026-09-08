//! A histogram handle: call sites observe, the worker ships count, sum,
//! min, max and cumulative buckets per label value on each sample tick.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::types::{HistogramParams, MetricType, QueuedMetric, Temporality};

use super::{label_pair, series_index};

/// One label value's distribution. Buckets are cumulative: bucket `b`
/// counts every observation `<= bounds[b]`. Floats are kept as bits in
/// `AtomicU64` and updated with compare-and-swap.
struct Series {
    count: AtomicU64,
    sum: AtomicU64,
    min: AtomicU64,
    max: AtomicU64,
    buckets: Box<[AtomicU64]>,
}

impl Series {
    fn new(buckets: usize) -> Self {
        Self {
            count: AtomicU64::new(0),
            sum: AtomicU64::new(0f64.to_bits()),
            min: AtomicU64::new(f64::INFINITY.to_bits()),
            max: AtomicU64::new(f64::NEG_INFINITY.to_bits()),
            buckets: (0..buckets).map(|_| AtomicU64::new(0)).collect(),
        }
    }

    fn observe(&self, value: f64, bounds: &[f64]) {
        self.count.fetch_add(1, Ordering::Relaxed);
        update_f64(&self.sum, |s| s + value);
        update_f64(&self.min, |m| m.min(value));
        update_f64(&self.max, |m| m.max(value));
        for (bucket, bound) in self.buckets.iter().zip(bounds) {
            if value <= *bound {
                bucket.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// `None` until the first observation.
    fn snapshot(&self, bounds: &[f64]) -> Option<HistogramParams> {
        let count = self.count.load(Ordering::Relaxed);
        if count == 0 {
            return None;
        }
        Some(HistogramParams {
            count,
            sum: f64::from_bits(self.sum.load(Ordering::Relaxed)),
            min: f64::from_bits(self.min.load(Ordering::Relaxed)),
            max: f64::from_bits(self.max.load(Ordering::Relaxed)),
            buckets: bounds
                .iter()
                .zip(&self.buckets)
                .map(|(b, c)| (*b, c.load(Ordering::Relaxed)))
                .collect(),
        })
    }
}

/// Apply `f` to an `f64` stored as bits, retrying on contention.
fn update_f64(cell: &AtomicU64, f: impl Fn(f64) -> f64) {
    let mut current = cell.load(Ordering::Relaxed);
    loop {
        let next = f(f64::from_bits(current)).to_bits();
        match cell.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(actual) => current = actual,
        }
    }
}

/// A cumulative distribution over fixed bucket bounds, split by one closed
/// set of label values.
///
/// Obtained from [`Metrics::histogram`](super::Metrics::histogram).
/// `observe` is a handful of relaxed atomic updates; it never allocates,
/// locks, or touches the queue. Every sample ships the totals since the
/// client was created, so the collector always holds the whole
/// distribution.
pub struct Histogram {
    name: &'static str,
    key: Option<&'static str>,
    values: Box<[&'static str]>,
    bounds: Box<[f64]>,
    series: Box<[Series]>,
}

impl Histogram {
    pub(super) fn new(
        name: &'static str,
        key: Option<&'static str>,
        values: &[&'static str],
        bounds: &[f64],
    ) -> Self {
        let mut bounds: Vec<f64> = bounds.iter().copied().filter(|b| !b.is_nan()).collect();
        bounds.sort_by(f64::total_cmp);
        bounds.dedup();
        if bounds.last().is_none_or(|b| b.is_finite()) {
            bounds.push(f64::INFINITY);
        }
        let values: Box<[&'static str]> = if values.is_empty() {
            Box::new([""])
        } else {
            values.into()
        };
        Self {
            name,
            key,
            series: values.iter().map(|_| Series::new(bounds.len())).collect(),
            values,
            bounds: bounds.into(),
        }
    }

    /// Metric name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Bucket upper bounds, ascending, ending in `f64::INFINITY`.
    #[must_use]
    pub fn bounds(&self) -> &[f64] {
        &self.bounds
    }

    /// Record `observation` for `value`. A value outside the registered set
    /// is dropped.
    pub fn observe(&self, value: &str, observation: f64) {
        if let Some(series) = series_index(&self.values, value).and_then(|i| self.series.get(i)) {
            series.observe(observation, &self.bounds);
        }
    }

    /// Record `observation` on an unlabelled histogram (the first series).
    pub fn record(&self, observation: f64) {
        if let Some(series) = self.series.first() {
            series.observe(observation, &self.bounds);
        }
    }

    /// Observations recorded for `value` so far.
    #[must_use]
    pub fn count(&self, value: &str) -> u64 {
        series_index(&self.values, value)
            .and_then(|i| self.series.get(i))
            .map_or(0, |s| s.count.load(Ordering::Relaxed))
    }

    /// Append one point per label value that has at least one observation.
    pub(super) fn sample(&self, timestamp: u64, out: &mut Vec<QueuedMetric>) {
        for (value, series) in self.values.iter().zip(&self.series) {
            let Some(params) = series.snapshot(&self.bounds) else {
                continue;
            };
            out.push(QueuedMetric {
                metric_type: MetricType::Histogram,
                timestamp,
                name: self.name.into(),
                value: 0.0,
                labels: label_pair(self.key, value),
                temporality: Temporality::Cumulative,
                histogram: Some(params),
            });
        }
    }
}

#[cfg(test)]
#[path = "histogram_test.rs"]
mod histogram_test;
