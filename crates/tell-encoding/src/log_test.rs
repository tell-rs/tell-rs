use crate::{encode_log_entry, encode_log_data, encode_log_data_into, LogEntryParams, LogEventType, LogLevel, UUID_LENGTH};

#[test]
fn encode_log_entry_with_all_fields() {
    let session_id = [0x03u8; UUID_LENGTH];
    let payload = br#"{"code":500}"#;

    let bytes = encode_log_entry(&LogEntryParams {
        event_type: LogEventType::Log,
        session_id: Some(&session_id),
        level: LogLevel::Error,
        timestamp: 1706000000000,
        source: Some("web-01"),
        service: Some("api"),
        payload: Some(payload),
    });

    let root_offset = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    assert!(root_offset < bytes.len());

    let table_start = root_offset;

    // event_type at table+28
    assert_eq!(bytes[table_start + 28], LogEventType::Log.as_u8());

    // level at table+29
    assert_eq!(bytes[table_start + 29], LogLevel::Error.as_u8());

    // timestamp at table+20
    let ts = u64::from_le_bytes([
        bytes[table_start + 20],
        bytes[table_start + 21],
        bytes[table_start + 22],
        bytes[table_start + 23],
        bytes[table_start + 24],
        bytes[table_start + 25],
        bytes[table_start + 26],
        bytes[table_start + 27],
    ]);
    assert_eq!(ts, 1706000000000);

    // session_id
    let found = bytes.windows(UUID_LENGTH).any(|w| w == session_id);
    assert!(found, "session_id not found");

    // source
    let found = bytes.windows(6).any(|w| w == b"web-01");
    assert!(found, "source not found");

    // service
    let found = bytes.windows(3).any(|w| w == b"api");
    assert!(found, "service not found");

    // payload
    let found = bytes.windows(payload.len()).any(|w| w == payload.as_slice());
    assert!(found, "payload not found");
}

#[test]
fn encode_log_entry_minimal() {
    let bytes = encode_log_entry(&LogEntryParams {
        event_type: LogEventType::Log,
        session_id: None,
        level: LogLevel::Info,
        timestamp: 0,
        source: None,
        service: None,
        payload: None,
    });

    let root_offset = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    assert!(root_offset < bytes.len());
    assert_eq!(bytes[root_offset + 28], LogEventType::Log.as_u8());
    assert_eq!(bytes[root_offset + 29], LogLevel::Info.as_u8());
}

#[test]
fn encode_log_data_single() {
    let session_id = [0xBB; UUID_LENGTH];
    let log = encode_log_entry(&LogEntryParams {
        event_type: LogEventType::Log,
        session_id: Some(&session_id),
        level: LogLevel::Warning,
        timestamp: 2000,
        source: None,
        service: Some("worker"),
        payload: None,
    });

    let data = encode_log_data(&[log]);

    let root = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    assert!(root < data.len());

    let found = data.windows(6).any(|w| w == b"worker");
    assert!(found, "service not found in log_data");
}

#[test]
fn encode_log_data_multiple() {
    let logs: Vec<Vec<u8>> = (0..3)
        .map(|i| {
            encode_log_entry(&LogEntryParams {
                event_type: LogEventType::Log,
                session_id: None,
                level: LogLevel::Debug,
                timestamp: 3000 + i,
                source: Some(&format!("host-{i}")),
                service: Some("svc"),
                payload: None,
            })
        })
        .collect();

    let data = encode_log_data(&logs);

    let root = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    assert!(root < data.len());

    for i in 0..3 {
        let host = format!("host-{i}");
        let found = data.windows(host.len()).any(|w| w == host.as_bytes());
        assert!(found, "Host '{}' not found in log_data", host);
    }
}

#[test]
fn encode_log_data_into_matches_encode_log_data() {
    let session_ids: Vec<[u8; UUID_LENGTH]> = (0..3).map(|i| [i as u8; UUID_LENGTH]).collect();
    let sources: Vec<String> = (0..3).map(|i| format!("host-{i}")).collect();
    let payloads: Vec<Vec<u8>> = (0..3).map(|i| format!(r#"{{"n":{i}}}"#).into_bytes()).collect();

    let params: Vec<LogEntryParams<'_>> = (0..3)
        .map(|i| LogEntryParams {
            event_type: LogEventType::Log,
            session_id: Some(&session_ids[i]),
            level: LogLevel::Error,
            timestamp: 3000 + i as u64,
            source: Some(&sources[i]),
            service: Some("api"),
            payload: Some(&payloads[i]),
        })
        .collect();

    // Encode with _into
    let mut buf = Vec::new();
    let range = encode_log_data_into(&mut buf, &params);
    let into_bytes = &buf[range];

    // Valid FlatBuffer root
    let root = u32::from_le_bytes([into_bytes[0], into_bytes[1], into_bytes[2], into_bytes[3]]) as usize;
    assert!(root < into_bytes.len());

    // All sources, payloads, and service present
    for i in 0..3 {
        let host = format!("host-{i}");
        assert!(
            into_bytes.windows(host.len()).any(|w| w == host.as_bytes()),
            "source '{}' not found in encode_log_data_into output",
            host,
        );
        let payload = format!(r#"{{"n":{i}}}"#);
        assert!(
            into_bytes.windows(payload.len()).any(|w| w == payload.as_bytes()),
            "payload {} not found in encode_log_data_into output",
            i,
        );
    }
    assert!(into_bytes.windows(3).any(|w| w == b"api"));
}

#[test]
fn encode_log_data_into_reuses_buffer() {
    let mut buf = Vec::new();

    let params = [LogEntryParams {
        event_type: LogEventType::Log,
        session_id: None,
        level: LogLevel::Info,
        timestamp: 100,
        source: Some("test"),
        service: None,
        payload: None,
    }];
    let range1 = encode_log_data_into(&mut buf, &params);
    assert!(range1.end > range1.start);

    let params2 = [LogEntryParams {
        event_type: LogEventType::Log,
        session_id: None,
        level: LogLevel::Debug,
        timestamp: 200,
        source: Some("test2"),
        service: None,
        payload: None,
    }];
    let range2 = encode_log_data_into(&mut buf, &params2);
    assert_eq!(range2.start, range1.end);

    let into_bytes = &buf[range2.clone()];
    let root = u32::from_le_bytes([into_bytes[0], into_bytes[1], into_bytes[2], into_bytes[3]]) as usize;
    assert!(root < into_bytes.len());
}

#[test]
fn log_level_values() {
    assert_eq!(LogLevel::Emergency.as_u8(), 0);
    assert_eq!(LogLevel::Alert.as_u8(), 1);
    assert_eq!(LogLevel::Critical.as_u8(), 2);
    assert_eq!(LogLevel::Error.as_u8(), 3);
    assert_eq!(LogLevel::Warning.as_u8(), 4);
    assert_eq!(LogLevel::Notice.as_u8(), 5);
    assert_eq!(LogLevel::Info.as_u8(), 6);
    assert_eq!(LogLevel::Debug.as_u8(), 7);
    assert_eq!(LogLevel::Trace.as_u8(), 8);
}

#[test]
fn log_event_type_values() {
    assert_eq!(LogEventType::Unknown.as_u8(), 0);
    assert_eq!(LogEventType::Log.as_u8(), 1);
    assert_eq!(LogEventType::Enrich.as_u8(), 2);
}
