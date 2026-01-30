use crate::error::TellError;

/// Validate a 32-character hex API key string and decode to 16 bytes.
pub fn validate_and_decode_api_key(api_key: &str) -> Result<[u8; 16], TellError> {
    if api_key.len() != 32 {
        return Err(TellError::configuration(format!(
            "apiKey must be 32 hex characters, got {}",
            api_key.len()
        )));
    }

    let mut bytes = [0u8; 16];
    for (i, chunk) in api_key.as_bytes().chunks(2).enumerate() {
        let hi = hex_val(chunk[0]).ok_or_else(|| {
            TellError::configuration(format!(
                "apiKey contains non-hex character '{}'",
                chunk[0] as char
            ))
        })?;
        let lo = hex_val(chunk[1]).ok_or_else(|| {
            TellError::configuration(format!(
                "apiKey contains non-hex character '{}'",
                chunk[1] as char
            ))
        })?;
        bytes[i] = (hi << 4) | lo;
    }

    Ok(bytes)
}

fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Validate a user ID (non-empty string).
pub fn validate_user_id(user_id: &str) -> Result<(), TellError> {
    if user_id.is_empty() {
        return Err(TellError::validation("userId", "is required"));
    }
    Ok(())
}

/// Validate an event name (non-empty, max 256 chars).
pub fn validate_event_name(name: &str) -> Result<(), TellError> {
    if name.is_empty() {
        return Err(TellError::validation("eventName", "is required"));
    }
    if name.len() > 256 {
        return Err(TellError::validation(
            "eventName",
            format!("must be at most 256 characters, got {}", name.len()),
        ));
    }
    Ok(())
}

/// Validate a log message (non-empty, max 64KB).
pub fn validate_log_message(message: &str) -> Result<(), TellError> {
    if message.is_empty() {
        return Err(TellError::validation("message", "is required"));
    }
    if message.len() > 65_536 {
        return Err(TellError::validation(
            "message",
            format!("must be at most 65536 characters, got {}", message.len()),
        ));
    }
    Ok(())
}
