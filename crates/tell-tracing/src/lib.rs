//! A [`tracing_subscriber::Layer`] that forwards `tracing` events to the Tell SDK.
//!
//! Install the layer once at startup and every `tracing` event — including those
//! from libraries — becomes a Tell structured log. Event fields become log
//! properties. By default the event target becomes the log component, which
//! replaces the log's `source` (the host/instance from
//! [`TellConfig`](tell::TellConfig)) — see [`TellLayer::with_component_from_target`].
//!
//! ```no_run
//! use tell::{Tell, TellConfig};
//! use tell_tracing::TellLayer;
//! use tracing_subscriber::layer::SubscriberExt;
//! use tracing_subscriber::util::SubscriberInitExt;
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let client = Tell::new(TellConfig::development("feed1e11feed1e11feed1e11feed1e11")?)?;
//! tracing_subscriber::registry()
//!     .with(TellLayer::new(client.clone()))
//!     .init();
//!
//! tracing::info!(user = "u1", count = 3, "hello world");
//!
//! client.close().await?;
//! # Ok(())
//! # }
//! ```
//!
//! # When to use it
//!
//! The direct [`Tell`] API is faster and remains the recommended path for hot
//! loops: it writes properties straight into a pre-sized JSON buffer, while this
//! layer pays for `tracing`'s field visiting and formats non-primitive values
//! through `Debug`. Use the layer to adopt Tell in an existing `tracing`-based
//! codebase, and call [`Tell::log`] directly where throughput matters.
//!
//! # Scope
//!
//! Spans are ignored. Only events are forwarded, and span fields are not merged
//! into the event's properties — this is the minimum viable layer.

use tell::{LogLevel, Props, Tell};
use tracing_core::field::{Field, Visit};
use tracing_core::{Event, Level, Subscriber};
use tracing_subscriber::layer::{Context, Layer};

#[cfg(test)]
mod lib_test;

/// Map a [`tracing`] level to the matching Tell log level.
///
/// `tracing` has five levels, Tell has the nine RFC 5424 severities; the four
/// Tell levels with no `tracing` counterpart (`Emergency`, `Alert`, `Critical`,
/// `Notice`) are only reachable through the direct [`Tell`] API.
#[must_use]
#[inline]
pub fn level_to_tell(level: &Level) -> LogLevel {
    match *level {
        Level::ERROR => LogLevel::Error,
        Level::WARN => LogLevel::Warning,
        Level::INFO => LogLevel::Info,
        Level::DEBUG => LogLevel::Debug,
        _ => LogLevel::Trace,
    }
}

/// A `tracing-subscriber` layer that sends every event to a [`Tell`] client.
///
/// Cheap to construct and clone — it holds a [`Tell`] handle, which is an `Arc`
/// under the hood. The client is fire-and-forget, so the layer never blocks the
/// thread emitting the event.
#[derive(Clone)]
pub struct TellLayer {
    client: Tell,
    component_from_target: bool,
}

impl TellLayer {
    /// Create a layer that forwards events to `client`.
    ///
    /// The event target (usually the emitting module path) is used as the log
    /// component, which replaces the log's host/instance `source`. Call
    /// [`with_component_from_target`](Self::with_component_from_target) with
    /// `false` to keep `source` as the host.
    #[must_use]
    pub fn new(client: Tell) -> Self {
        Self {
            client,
            component_from_target: true,
        }
    }

    /// Set whether the event target is sent as the log component.
    ///
    /// Defaults to `true` for compatibility. In Tell a log's `source` is the
    /// host or instance that sent it; a component replaces it, so with the
    /// default every module path becomes its own "source". `false` (recommended)
    /// sends no component: `source` stays the configured host and the service
    /// name labels the log. Record a module you need as an event field
    /// (`tracing::info!(module = "auth", "…")`), which becomes a log property.
    #[must_use]
    pub fn with_component_from_target(mut self, enabled: bool) -> Self {
        self.component_from_target = enabled;
        self
    }
}

impl<S: Subscriber> Layer<S> for TellLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();

        let mut visitor = PropsVisitor::new();
        event.record(&mut visitor);
        let PropsVisitor { message, props } = visitor;

        let message = message.as_deref().unwrap_or_else(|| metadata.name());
        let component = self.component_from_target.then(|| metadata.target());

        self.client.log(
            level_to_tell(metadata.level()),
            message,
            component,
            props.unwrap_or_default(),
        );
    }
}

/// Add a key-value pair to the visitor's [`Props`].
///
/// `Props::add` consumes and returns `self`, so the value is taken out of the
/// `Option` and put back. A macro rather than a helper method to avoid taking a
/// `serde` dependency just to name the `Serialize` bound.
macro_rules! push {
    ($self:expr, $field:expr, $value:expr) => {
        if let Some(props) = $self.props.take() {
            $self.props = Some(props.add($field.name(), $value));
        }
    };
}

/// Collects event fields into a [`Props`], pulling out the `message` field.
///
/// One `Props` per event, written directly as JSON bytes — no intermediate
/// `serde_json::Value`.
struct PropsVisitor {
    message: Option<String>,
    props: Option<Props>,
}

impl PropsVisitor {
    fn new() -> Self {
        Self {
            message: None,
            props: Some(Props::new()),
        }
    }
}

/// The field `tracing` uses for the event's format string.
const MESSAGE_FIELD: &str = "message";

impl Visit for PropsVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == MESSAGE_FIELD {
            self.message = Some(value.to_owned());
            return;
        }
        push!(self, field, value);
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        push!(self, field, value);
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        push!(self, field, value);
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        push!(self, field, value);
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        push!(self, field, value);
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let rendered = format!("{value:?}");
        if field.name() == MESSAGE_FIELD {
            self.message = Some(rendered);
            return;
        }
        push!(self, field, rendered);
    }
}
