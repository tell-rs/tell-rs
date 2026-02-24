use std::sync::Arc;
use std::time::Duration;

use crate::error::TellError;
use crate::validation::validate_and_decode_api_key;

/// Default collector endpoint.
pub const DEFAULT_ENDPOINT: &str = "collect.tell.rs:50000";

/// Default localhost endpoint for development.
pub const DEV_ENDPOINT: &str = "localhost:50000";

/// Configuration for the Tell SDK.
#[derive(Clone)]
pub struct TellConfig {
    /// Decoded 16-byte API key.
    pub(crate) api_key_bytes: [u8; 16],
    /// Service name stamped on every event and log.
    pub(crate) service: Option<String>,
    /// Collector host:port.
    pub(crate) endpoint: String,
    /// Max events per batch before flush.
    pub(crate) batch_size: usize,
    /// Time between automatic flushes.
    pub(crate) flush_interval: Duration,
    /// Retry attempts per failed batch.
    pub(crate) max_retries: u32,
    /// Graceful shutdown deadline.
    pub(crate) close_timeout: Duration,
    /// TCP/connection timeout.
    pub(crate) network_timeout: Duration,
    /// Error callback.
    pub(crate) on_error: Option<Arc<dyn Fn(TellError) + Send + Sync>>,
}

impl std::fmt::Debug for TellConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TellConfig")
            .field("endpoint", &self.endpoint)
            .field("batch_size", &self.batch_size)
            .field("flush_interval", &self.flush_interval)
            .field("max_retries", &self.max_retries)
            .field("close_timeout", &self.close_timeout)
            .field("network_timeout", &self.network_timeout)
            .finish()
    }
}

/// Builder for constructing a `TellConfig`.
pub struct TellConfigBuilder {
    api_key: String,
    service: Option<String>,
    endpoint: Option<String>,
    batch_size: Option<usize>,
    flush_interval: Option<Duration>,
    max_retries: Option<u32>,
    close_timeout: Option<Duration>,
    network_timeout: Option<Duration>,
    on_error: Option<Arc<dyn Fn(TellError) + Send + Sync>>,
}

impl TellConfigBuilder {
    /// Start building a config with the given API key.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            service: None,
            endpoint: None,
            batch_size: None,
            flush_interval: None,
            max_retries: None,
            close_timeout: None,
            network_timeout: None,
            on_error: None,
        }
    }

    /// Set the service name stamped on every event and log. No auto-detect for server SDKs.
    pub fn service(mut self, name: impl Into<String>) -> Self {
        self.service = Some(name.into());
        self
    }

    /// Set the collector endpoint (`host:port`). Default: `collect.tell.app:50000`.
    pub fn endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    /// Max events per batch before an automatic flush. Default: `100`.
    pub fn batch_size(mut self, size: usize) -> Self {
        self.batch_size = Some(size);
        self
    }

    /// Interval between automatic flushes. Default: `10s`.
    pub fn flush_interval(mut self, interval: Duration) -> Self {
        self.flush_interval = Some(interval);
        self
    }

    /// Retry attempts per failed batch send. Default: `3`.
    pub fn max_retries(mut self, retries: u32) -> Self {
        self.max_retries = Some(retries);
        self
    }

    /// Deadline for graceful shutdown via [`close`](crate::Tell::close). Default: `5s`.
    pub fn close_timeout(mut self, timeout: Duration) -> Self {
        self.close_timeout = Some(timeout);
        self
    }

    /// TCP connection timeout. Default: `30s`.
    pub fn network_timeout(mut self, timeout: Duration) -> Self {
        self.network_timeout = Some(timeout);
        self
    }

    /// Callback invoked on non-fatal errors (validation failures, send errors).
    pub fn on_error(mut self, f: impl Fn(TellError) + Send + Sync + 'static) -> Self {
        self.on_error = Some(Arc::new(f));
        self
    }

    /// Build the config, validating the API key.
    pub fn build(self) -> Result<TellConfig, TellError> {
        let api_key_bytes = validate_and_decode_api_key(&self.api_key)?;

        if let Some(ref s) = self.service {
            if s.is_empty() {
                return Err(TellError::validation("service", "must not be empty"));
            }
        }

        Ok(TellConfig {
            api_key_bytes,
            service: self.service,
            endpoint: self.endpoint.unwrap_or_else(|| DEFAULT_ENDPOINT.to_string()),
            batch_size: self.batch_size.unwrap_or(100),
            flush_interval: self.flush_interval.unwrap_or(Duration::from_secs(10)),
            max_retries: self.max_retries.unwrap_or(3),
            close_timeout: self.close_timeout.unwrap_or(Duration::from_secs(5)),
            network_timeout: self.network_timeout.unwrap_or(Duration::from_secs(30)),
            on_error: self.on_error,
        })
    }
}

impl TellConfig {
    /// Start building a config.
    pub fn builder(api_key: impl Into<String>) -> TellConfigBuilder {
        TellConfigBuilder::new(api_key)
    }

    /// Development preset: localhost:50000, batch=10, flush=2s.
    pub fn development(api_key: impl Into<String>) -> Result<Self, TellError> {
        Self::builder(api_key)
            .endpoint(DEV_ENDPOINT)
            .batch_size(10)
            .flush_interval(Duration::from_secs(2))
            .build()
    }

    /// Production preset: default endpoint, batch=100, flush=10s.
    pub fn production(api_key: impl Into<String>) -> Result<Self, TellError> {
        Self::builder(api_key).build()
    }
}
