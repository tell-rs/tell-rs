//! Registered instruments, sampled by the background worker.
//!
//! [`Tell::gauge`](crate::Tell::gauge), [`Tell::counter`](crate::Tell::counter)
//! and [`Tell::histogram`](crate::Tell::histogram) put one message on the
//! queue per call. That is the right shape for a value you already hold —
//! a reading from `/proc`, a number computed once a minute. It is the wrong
//! shape for anything counted per request: a counter call per upload puts
//! one message per upload into the queue shared with events and logs, and
//! a busy hour turns into thousands of one-byte increments on the wire.
//!
//! The registry holds the math instead. A call site bumps an atomic on a
//! handle; every `metrics_interval` (default 15 s) the worker samples each
//! instrument and ships one point per series — a thousand increments in a
//! tick cost one message. Sampled points are appended straight to the
//! worker's own batch, so they never compete with events and logs for the
//! queue and are never dropped by it.
//!
//! ```no_run
//! use tell::{Tell, TellConfig};
//!
//! # fn main() -> tell::Result<()> {
//! let client = Tell::new(TellConfig::production("feed1e11feed1e11feed1e11feed1e11")?)?;
//! let m = client.metrics();
//!
//! // Delta counter split by a closed label set: ships the change per tick.
//! let uploads = m.counter("uploads_total").by("source", &["web", "api"]).register();
//! uploads.add("web", 1);
//!
//! // Histogram: count, sum, min, max and cumulative buckets per tick.
//! let latency = m.histogram("request_ms", &[50.0, 200.0, 1000.0]).register();
//! latency.record(120.0);
//!
//! // Gauge: a reader the worker calls on every tick.
//! let pool = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(8));
//! let p = pool.clone();
//! m.gauge("workers_live", move || p.load(std::sync::atomic::Ordering::Relaxed) as f64);
//! # Ok(()) }
//! ```
//!
//! # Delta or cumulative, never both
//!
//! A counter ships either the change since the previous sample
//! ([`Temporality::Delta`], the default, sum the points to get a total) or
//! the running total since the client started ([`Temporality::Cumulative`],
//! read the last point). Pick per counter with
//! [`CounterBuilder::cumulative`]; a series must not mix the two, because a
//! `sum` over mixed rows counts every cumulative point again.
//!
//! # Labels are a closed set
//!
//! Label values are fixed at registration. A value outside the set is
//! dropped, so a bug or an attacker cannot grow cardinality at runtime, and
//! no allocation happens on the hot path.
//!
//! # The `tell.sdk.dropped` gauge
//!
//! Once any instrument is registered, every sample also ships
//! `tell.sdk.dropped`: the running count of messages the client dropped
//! because its queue was full, the same number [`Tell::dropped`](crate::Tell::dropped) returns.
//! Because sampled points bypass the queue, the report survives the very
//! overflow it describes.

mod counter;
mod gauge;
mod histogram;

use std::borrow::Cow;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::Mutex;

pub use counter::Counter;
pub use gauge::Gauge;
pub use histogram::Histogram;

use crate::types::{MetricLabel, MetricType, QueuedMetric, Temporality};

/// Name of the built-in dropped-messages gauge.
pub const DROPPED_GAUGE: &str = "tell.sdk.dropped";

/// The instrument registry behind [`Tell::metrics`](crate::Tell::metrics).
///
/// Registration takes a short lock; the handles it returns are lock-free.
pub struct Metrics {
    counters: Mutex<Vec<Arc<Counter>>>,
    histograms: Mutex<Vec<Arc<Histogram>>>,
    gauges: Mutex<Vec<Gauge>>,
    dropped: AtomicU64,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            counters: Mutex::new(Vec::new()),
            histograms: Mutex::new(Vec::new()),
            gauges: Mutex::new(Vec::new()),
            dropped: AtomicU64::new(0),
        }
    }

    /// Start registering a counter. Delta temporality unless
    /// [`CounterBuilder::cumulative`] is called.
    pub fn counter(&self, name: &'static str) -> CounterBuilder<'_> {
        CounterBuilder {
            registry: self,
            name,
            key: None,
            values: Vec::new(),
            temporality: Temporality::Delta,
        }
    }

    /// Start registering a histogram over `bounds` (any order; an infinite
    /// catch-all bucket is added when missing).
    pub fn histogram(&self, name: &'static str, bounds: &[f64]) -> HistogramBuilder<'_> {
        HistogramBuilder {
            registry: self,
            name,
            key: None,
            values: Vec::new(),
            bounds: bounds.to_vec(),
        }
    }

    /// Register an unlabelled gauge read on every sample tick.
    pub fn gauge(&self, name: &'static str, read: impl Fn() -> f64 + Send + Sync + 'static) {
        let reader: gauge::Reader = Box::new(move |out| out.push(("", read())));
        self.gauges.lock().push(Gauge::new(name, None, reader));
    }

    /// Register a gauge split by `key`. The reader appends one
    /// `(label value, reading)` pair per series it wants shipped.
    pub fn gauge_by(
        &self,
        name: &'static str,
        key: &'static str,
        read: impl Fn(&mut Vec<(&'static str, f64)>) + Send + Sync + 'static,
    ) {
        self.gauges
            .lock()
            .push(Gauge::new(name, Some(key), Box::new(read)));
    }

    /// Registered instruments, all kinds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.counters.lock().len() + self.histograms.lock().len() + self.gauges.lock().len()
    }

    /// Whether nothing is registered (the worker then skips sampling).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Messages the client dropped because its queue was full.
    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Count one dropped message; returns the new total.
    pub(crate) fn record_drop(&self) -> u64 {
        self.dropped.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Sample every instrument into `out` at `timestamp` (Unix
    /// nanoseconds). Does nothing when the registry is empty.
    pub(crate) fn sample(&self, timestamp: u64, out: &mut Vec<QueuedMetric>) {
        if self.is_empty() {
            return;
        }
        for c in self.counters.lock().iter() {
            c.sample(timestamp, out);
        }
        for h in self.histograms.lock().iter() {
            h.sample(timestamp, out);
        }
        let mut buf = Vec::with_capacity(4);
        for g in self.gauges.lock().iter() {
            g.sample(timestamp, &mut buf, out);
        }
        out.push(QueuedMetric {
            metric_type: MetricType::Gauge,
            timestamp,
            name: Cow::Borrowed(DROPPED_GAUGE),
            value: self.dropped() as f64,
            labels: Vec::new(),
            temporality: Temporality::Unspecified,
            histogram: None,
        });
    }
}

/// Builds one [`Counter`]; finish with [`register`](Self::register).
#[must_use = "call .register() to obtain the counter handle"]
pub struct CounterBuilder<'a> {
    registry: &'a Metrics,
    name: &'static str,
    key: Option<&'static str>,
    values: Vec<&'static str>,
    temporality: Temporality,
}

impl CounterBuilder<'_> {
    /// Split the counter by `key` over the closed set `values`.
    pub fn by(mut self, key: &'static str, values: &[&'static str]) -> Self {
        self.key = Some(key);
        self.values = values.to_vec();
        self
    }

    /// Ship the running total on every sample instead of the change since
    /// the previous one.
    pub fn cumulative(mut self) -> Self {
        self.temporality = Temporality::Cumulative;
        self
    }

    /// Register and return the handle.
    pub fn register(self) -> Arc<Counter> {
        let counter = Arc::new(Counter::new(
            self.name,
            self.key,
            &self.values,
            self.temporality,
        ));
        self.registry.counters.lock().push(Arc::clone(&counter));
        counter
    }
}

/// Builds one [`Histogram`]; finish with [`register`](Self::register).
#[must_use = "call .register() to obtain the histogram handle"]
pub struct HistogramBuilder<'a> {
    registry: &'a Metrics,
    name: &'static str,
    key: Option<&'static str>,
    values: Vec<&'static str>,
    bounds: Vec<f64>,
}

impl HistogramBuilder<'_> {
    /// Split the histogram by `key` over the closed set `values`.
    pub fn by(mut self, key: &'static str, values: &[&'static str]) -> Self {
        self.key = Some(key);
        self.values = values.to_vec();
        self
    }

    /// Register and return the handle.
    pub fn register(self) -> Arc<Histogram> {
        let histogram = Arc::new(Histogram::new(
            self.name,
            self.key,
            &self.values,
            &self.bounds,
        ));
        self.registry.histograms.lock().push(Arc::clone(&histogram));
        histogram
    }
}

/// Position of `value` in a closed label set.
fn series_index(values: &[&'static str], value: &str) -> Option<usize> {
    values.iter().position(|v| *v == value)
}

/// The label vector for one series: empty for unlabelled instruments.
fn label_pair(key: Option<&'static str>, value: &'static str) -> Vec<MetricLabel> {
    match key {
        Some(k) => vec![(Cow::Borrowed(k), Cow::Borrowed(value))],
        None => Vec::new(),
    }
}

#[cfg(test)]
#[path = "registry_test.rs"]
mod registry_test;
