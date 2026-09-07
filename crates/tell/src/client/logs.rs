//! Structured logging methods.

use super::Tell;
use crate::clock::now_ms;
use crate::payload::JsonObject;
use crate::props::IntoPayload;
use crate::types::{LogLevel, QueuedLog};
use crate::validation::validate_log_message;
use crate::worker::WorkerMessage;

impl Tell {
    /// Send a structured log entry.
    ///
    /// `component` is an optional label for the module or subsystem that produced
    /// the log (e.g. `"auth"`, `"cache"`, `"db"`). The app-level `service` name
    /// is taken from [`TellConfig`](crate::TellConfig) and stamped automatically.
    ///
    /// Fire-and-forget: silently drops the entry if the queue is full.
    /// Use [`try_log`](Self::try_log) when the caller needs backpressure.
    pub fn log(
        &self,
        level: LogLevel,
        message: &str,
        component: Option<&str>,
        data: impl IntoPayload,
    ) {
        self.try_log(level, message, component, data);
    }

    /// Send a structured log entry, returning `false` if the queue is full.
    ///
    /// Same as [`log`](Self::log) but lets the caller react to backpressure
    /// (e.g. stop reading a file and retry on the next poll).
    pub fn try_log(
        &self,
        level: LogLevel,
        message: &str,
        component: Option<&str>,
        data: impl IntoPayload,
    ) -> bool {
        self.try_log_with_service(level, message, component, None, data)
    }

    /// Send a log entry with a per-entry service override.
    ///
    /// Same as [`try_log`](Self::try_log) but allows overriding the global
    /// service name for this specific entry. Useful when a single collector
    /// forwards logs from multiple services.
    pub fn try_log_with_service(
        &self,
        level: LogLevel,
        message: &str,
        component: Option<&str>,
        service: Option<&str>,
        data: impl IntoPayload,
    ) -> bool {
        self.try_log_inner(
            self.read_session_id(),
            level,
            message,
            component,
            service,
            data,
        )
    }

    /// Send a log entry stamped with a caller-supplied session id.
    ///
    /// Takes the level as an argument — this is the single log-with-session
    /// method, there are no per-level variants. The provided `sid` is stamped
    /// verbatim, overriding any process-wide default. The caller owns the id's
    /// provenance; the SDK treats it as opaque.
    ///
    /// Returns `false` if the queue is full.
    pub fn log_with_session(
        &self,
        sid: &[u8; 16],
        level: LogLevel,
        message: &str,
        component: Option<&str>,
        data: impl IntoPayload,
    ) -> bool {
        self.try_log_inner(Some(*sid), level, message, component, None, data)
    }

    fn try_log_inner(
        &self,
        session_id: Option<[u8; 16]>,
        level: LogLevel,
        message: &str,
        component: Option<&str>,
        service: Option<&str>,
        data: impl IntoPayload,
    ) -> bool {
        if let Err(e) = validate_log_message(message) {
            self.report_error(e);
            return true; // validation error, not channel pressure
        }

        let service = service.filter(|s| !s.is_empty());
        let data = data.into_payload();
        let payload =
            JsonObject::with_capacity(16 + message.len() + data.as_ref().map_or(0, Vec::len))
                .field(b"\"message\":", message)
                .object(data.as_deref())
                .finish();

        self.enqueue(WorkerMessage::Log(QueuedLog {
            level,
            timestamp: now_ms(),
            session_id,
            component: component.map(str::to_owned),
            service: service.map(str::to_owned),
            payload: Some(payload),
        }))
    }

    /// Fire-and-forget variant of [`try_log_with_service`](Self::try_log_with_service).
    pub fn log_with_service(
        &self,
        level: LogLevel,
        message: &str,
        component: Option<&str>,
        service: Option<&str>,
        data: impl IntoPayload,
    ) {
        self.try_log_with_service(level, message, component, service, data);
    }

    /// Log at **Emergency** level (RFC 5424 severity 0). System is unusable.
    pub fn log_emergency(&self, message: &str, component: Option<&str>, data: impl IntoPayload) {
        self.log(LogLevel::Emergency, message, component, data);
    }

    /// Log at **Alert** level (RFC 5424 severity 1). Immediate action required.
    pub fn log_alert(&self, message: &str, component: Option<&str>, data: impl IntoPayload) {
        self.log(LogLevel::Alert, message, component, data);
    }

    /// Log at **Critical** level (RFC 5424 severity 2). Critical failure.
    pub fn log_critical(&self, message: &str, component: Option<&str>, data: impl IntoPayload) {
        self.log(LogLevel::Critical, message, component, data);
    }

    /// Log at **Error** level (RFC 5424 severity 3). Runtime error.
    pub fn log_error(&self, message: &str, component: Option<&str>, data: impl IntoPayload) {
        self.log(LogLevel::Error, message, component, data);
    }

    /// Log at **Warning** level (RFC 5424 severity 4). Potential issue.
    pub fn log_warning(&self, message: &str, component: Option<&str>, data: impl IntoPayload) {
        self.log(LogLevel::Warning, message, component, data);
    }

    /// Log at **Notice** level (RFC 5424 severity 5). Normal but significant.
    pub fn log_notice(&self, message: &str, component: Option<&str>, data: impl IntoPayload) {
        self.log(LogLevel::Notice, message, component, data);
    }

    /// Log at **Info** level (RFC 5424 severity 6). Informational.
    pub fn log_info(&self, message: &str, component: Option<&str>, data: impl IntoPayload) {
        self.log(LogLevel::Info, message, component, data);
    }

    /// Log at **Debug** level (RFC 5424 severity 7). Debug-level detail.
    pub fn log_debug(&self, message: &str, component: Option<&str>, data: impl IntoPayload) {
        self.log(LogLevel::Debug, message, component, data);
    }

    /// Log at **Trace** level (RFC 5424 severity 8). Finest-grained detail.
    pub fn log_trace(&self, message: &str, component: Option<&str>, data: impl IntoPayload) {
        self.log(LogLevel::Trace, message, component, data);
    }
}
