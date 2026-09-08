//! Tell analytics SDK for Rust.
//!
//! Events, structured logging, and metrics over TCP + FlatBuffers.
//! Synchronous API, async background worker — never blocks on I/O.
//!
//! ```no_run
//! use tell::{Tell, TellConfig, props};
//!
//! #[tokio::main]
//! async fn main() {
//!     let client = Tell::new(
//!         TellConfig::development("feed1e11feed1e11feed1e11feed1e11").unwrap()
//!     ).unwrap();
//!
//!     client.track("user_123", "Page Viewed", props! { "url" => "/home" });
//!     client.log_info("Request handled", Some("http"), props! { "status" => 200 });
//!
//!     client.close().await.ok();
//! }
//! ```
//!
//! Without a Tokio runtime the worker runs on a dedicated thread; use
//! [`Tell::close_blocking`] instead of `close().await` in that case.

pub(crate) mod buffer;
mod client;
mod clock;
mod config;
mod constants;
mod error;
pub mod metrics;
mod payload;
mod props;
mod transport;
mod types;
mod validation;
mod worker;

#[cfg(test)]
mod buffer_test;
#[cfg(test)]
mod client_test;
#[cfg(test)]
mod clock_test;
#[cfg(test)]
mod config_test;
#[cfg(test)]
mod error_test;
#[cfg(test)]
mod payload_test;
#[cfg(test)]
mod props_test;
#[cfg(test)]
mod session_test;
#[cfg(test)]
mod transport_test;
#[cfg(test)]
mod validation_test;
#[cfg(test)]
mod worker_test;

pub use client::Tell;
pub use config::{
    DEFAULT_ENDPOINT, DEFAULT_METRICS_INTERVAL, DEFAULT_QUEUE_CAPACITY, DEV_ENDPOINT, TellConfig,
    TellConfigBuilder,
};
pub use constants::Events;
pub use error::{Result, TellError};
pub use metrics::{Counter, Gauge, Histogram, Metrics};
#[doc(hidden)]
pub use props::key_is_plain;
pub use props::{IntoPayload, Props};
pub use types::{
    EventType, HistogramParams, LogEventType, LogLevel, MetricType, SchemaType, Temporality,
};

pub use tell_encoding;
