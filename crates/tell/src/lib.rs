//! Tell analytics SDK for Rust.
//!
//! Events and structured logging over TCP + FlatBuffers.
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

mod client;
mod config;
mod error;
mod types;
mod constants;
mod props;
mod validation;
mod worker;
mod transport;

#[cfg(test)]
mod config_test;
#[cfg(test)]
mod transport_test;
#[cfg(test)]
mod validation_test;

pub use client::Tell;
pub use config::{TellConfig, TellConfigBuilder};
pub use constants::Events;
pub use error::{TellError, Result};
pub use props::{Props, IntoPayload};
pub use types::{EventType, LogEventType, LogLevel, SchemaType};

pub use tell_encoding;
