//! Event methods: track, identify, group, revenue, alias.

use std::borrow::Cow;

use super::Tell;
use crate::clock::now_ms;
use crate::constants::Events;
use crate::error::TellError;
use crate::payload::JsonObject;
use crate::props::IntoPayload;
use crate::types::{EventType, QueuedEvent};
use crate::validation::{validate_event_name, validate_user_id};
use crate::worker::WorkerMessage;

impl Tell {
    /// Track a user action.
    ///
    /// Accepts [`Props`](crate::Props), `props!{..}`, `Some(json!({..}))`,
    /// any `Option<impl Serialize>`, or `()` for no properties.
    ///
    /// Never blocks, never panics. Invalid input is reported via `on_error`.
    /// Silently drops the event if the queue is full; use
    /// [`try_track`](Self::try_track) to observe backpressure.
    pub fn track(&self, user_id: &str, event_name: &str, properties: impl IntoPayload) {
        self.try_track(user_id, event_name, properties);
    }

    /// Track a user action, returning `false` if the queue is full.
    ///
    /// Validation failures are reported via `on_error` and return `true`,
    /// since they are not backpressure.
    pub fn try_track(&self, user_id: &str, event_name: &str, properties: impl IntoPayload) -> bool {
        self.track_inner(
            self.read_session_id(),
            user_id,
            Cow::Owned(event_name.to_owned()),
            properties,
        )
    }

    /// Track a user action with a `'static` event name, avoiding one allocation.
    ///
    /// Use with string literals or the [`Events`] constants.
    pub fn track_static(
        &self,
        user_id: &str,
        event_name: &'static str,
        properties: impl IntoPayload,
    ) {
        self.track_inner(
            self.read_session_id(),
            user_id,
            Cow::Borrowed(event_name),
            properties,
        );
    }

    /// Track a user action stamped with a caller-supplied session id.
    ///
    /// The provided `sid` is stamped verbatim, overriding any process-wide
    /// default set by [`TellConfigBuilder::enable_session`](crate::TellConfigBuilder::enable_session).
    /// The caller owns the id's provenance; the SDK treats it as opaque.
    pub fn track_with_session(
        &self,
        sid: &[u8; 16],
        user_id: &str,
        event_name: &str,
        properties: impl IntoPayload,
    ) {
        self.track_inner(
            Some(*sid),
            user_id,
            Cow::Owned(event_name.to_owned()),
            properties,
        );
    }

    fn track_inner(
        &self,
        session_id: Option<[u8; 16]>,
        user_id: &str,
        event_name: Cow<'static, str>,
        properties: impl IntoPayload,
    ) -> bool {
        if let Err(e) = validate_user_id(user_id) {
            self.report_error(e);
            return true;
        }
        if let Err(e) = validate_event_name(&event_name) {
            self.report_error(e);
            return true;
        }

        let props = properties.into_payload();
        let payload = self.event_payload(user_id, props.as_deref());
        self.enqueue(WorkerMessage::Event(self.queued(
            EventType::Track,
            session_id,
            Some(event_name),
            payload,
        )))
    }

    /// Identify a user with optional traits.
    ///
    /// Identity-control messages never carry a session id — they describe
    /// who the actor is, not activity inside a session.
    pub fn identify(&self, user_id: &str, traits: impl IntoPayload) {
        if let Err(e) = validate_user_id(user_id) {
            self.report_error(e);
            return;
        }

        let traits = traits.into_payload();
        let payload = JsonObject::with_capacity(16 + user_id.len() + len_of(&traits))
            .field(b"\"user_id\":", user_id)
            .object(traits.as_deref())
            .finish();

        self.enqueue(WorkerMessage::Event(self.queued(
            EventType::Identify,
            None,
            None,
            payload,
        )));
    }

    /// Associate a user with a group.
    ///
    /// Identity-control messages never carry a session id — group membership
    /// is metadata about the actor, not activity inside a session.
    pub fn group(&self, user_id: &str, group_id: &str, properties: impl IntoPayload) {
        if let Err(e) = validate_user_id(user_id) {
            self.report_error(e);
            return;
        }
        if group_id.is_empty() {
            self.report_error(TellError::validation("groupId", "is required"));
            return;
        }

        let props = properties.into_payload();
        let fragment = self.super_fragment();
        let cap = 32 + group_id.len() + user_id.len() + len_of(&fragment) + len_of(&props);
        let payload = JsonObject::with_capacity(cap)
            .field(b"\"group_id\":", group_id)
            .field(b"\"user_id\":", user_id)
            .fragment(fragment.as_deref().unwrap_or(&[]))
            .object(props.as_deref())
            .finish();

        self.enqueue(WorkerMessage::Event(self.queued(
            EventType::Group,
            None,
            None,
            payload,
        )));
    }

    /// Track a revenue event.
    pub fn revenue(
        &self,
        user_id: &str,
        amount: f64,
        currency: &str,
        order_id: &str,
        properties: impl IntoPayload,
    ) {
        self.revenue_inner(
            self.read_session_id(),
            user_id,
            amount,
            currency,
            order_id,
            properties,
        );
    }

    /// Track a revenue event stamped with a caller-supplied session id.
    ///
    /// The provided `sid` is stamped verbatim, overriding any process-wide
    /// default set by [`TellConfigBuilder::enable_session`](crate::TellConfigBuilder::enable_session).
    /// The caller owns the id's provenance; the SDK treats it as opaque.
    pub fn revenue_with_session(
        &self,
        sid: &[u8; 16],
        user_id: &str,
        amount: f64,
        currency: &str,
        order_id: &str,
        properties: impl IntoPayload,
    ) {
        self.revenue_inner(Some(*sid), user_id, amount, currency, order_id, properties);
    }

    fn revenue_inner(
        &self,
        session_id: Option<[u8; 16]>,
        user_id: &str,
        amount: f64,
        currency: &str,
        order_id: &str,
        properties: impl IntoPayload,
    ) {
        if let Err(e) = validate_revenue(user_id, amount, currency, order_id) {
            self.report_error(e);
            return;
        }

        let props = properties.into_payload();
        let fragment = self.super_fragment();
        let cap = 64
            + user_id.len()
            + currency.len()
            + order_id.len()
            + len_of(&fragment)
            + len_of(&props);
        let payload = JsonObject::with_capacity(cap)
            .field(b"\"user_id\":", user_id)
            .field(b"\"amount\":", amount)
            .field(b"\"currency\":", currency)
            .field(b"\"order_id\":", order_id)
            .fragment(fragment.as_deref().unwrap_or(&[]))
            .object(props.as_deref())
            .finish();

        self.enqueue(WorkerMessage::Event(self.queued(
            EventType::Track,
            session_id,
            Some(Cow::Borrowed(Events::ORDER_COMPLETED)),
            payload,
        )));
    }

    /// Link two user identities.
    ///
    /// Identity-control messages never carry a session id — identity linkage
    /// is metadata about the actor, not activity inside a session.
    pub fn alias(&self, previous_id: &str, user_id: &str) {
        if previous_id.is_empty() {
            self.report_error(TellError::validation("previousId", "is required"));
            return;
        }
        if let Err(e) = validate_user_id(user_id) {
            self.report_error(e);
            return;
        }

        let payload = JsonObject::with_capacity(32 + previous_id.len() + user_id.len())
            .field(b"\"previous_id\":", previous_id)
            .field(b"\"user_id\":", user_id)
            .finish();

        self.enqueue(WorkerMessage::Event(self.queued(
            EventType::Alias,
            None,
            None,
            payload,
        )));
    }

    /// `{"user_id":..,<super props>,<props>}` — caller props override super props.
    fn event_payload(&self, user_id: &str, props: Option<&[u8]>) -> Vec<u8> {
        let fragment = self.super_fragment();
        let cap = 16 + user_id.len() + len_of(&fragment) + props.map_or(0, <[u8]>::len);
        JsonObject::with_capacity(cap)
            .field(b"\"user_id\":", user_id)
            .fragment(fragment.as_deref().unwrap_or(&[]))
            .object(props)
            .finish()
    }

    fn queued(
        &self,
        event_type: EventType,
        session_id: Option<[u8; 16]>,
        event_name: Option<Cow<'static, str>>,
        payload: Vec<u8>,
    ) -> QueuedEvent {
        QueuedEvent {
            event_type,
            timestamp: now_ms(),
            device_id: self.device_id(),
            session_id,
            event_name,
            payload: Some(payload),
        }
    }
}

fn validate_revenue(
    user_id: &str,
    amount: f64,
    currency: &str,
    order_id: &str,
) -> Result<(), TellError> {
    validate_user_id(user_id)?;
    if amount <= 0.0 {
        return Err(TellError::validation("amount", "must be positive"));
    }
    if currency.is_empty() {
        return Err(TellError::validation("currency", "is required"));
    }
    if order_id.is_empty() {
        return Err(TellError::validation("orderId", "is required"));
    }
    Ok(())
}

#[inline]
fn len_of<B: AsRef<[u8]>>(bytes: &Option<B>) -> usize {
    bytes.as_ref().map_or(0, |b| b.as_ref().len())
}
