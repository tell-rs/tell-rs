use crate::{
    EventParams, EventType, UUID_LENGTH, encode_event, encode_event_data, encode_event_data_into,
};

#[test]
fn encode_event_with_all_fields() {
    let device_id = [0x01u8; UUID_LENGTH];
    let session_id = [0x02u8; UUID_LENGTH];
    let payload = br#"{"page":"/home"}"#;

    let bytes = encode_event(&EventParams {
        event_type: EventType::Track,
        timestamp: 1706000000000,
        service: Some("website"),
        device_id: Some(&device_id),
        session_id: Some(&session_id),
        event_name: Some("Page Viewed"),
        payload: Some(payload),
    });

    // Root offset
    let root_offset = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    assert!(root_offset < bytes.len());

    let table_start = root_offset;

    // event_type at table+28
    assert_eq!(bytes[table_start + 28], EventType::Track.as_u8());

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

    // device_id bytes should appear
    let found = bytes.windows(UUID_LENGTH).any(|w| w == device_id);
    assert!(found, "device_id not found");

    // session_id bytes should appear
    let found = bytes.windows(UUID_LENGTH).any(|w| w == session_id);
    assert!(found, "session_id not found");

    // service should appear
    let svc = b"website";
    let found = bytes.windows(svc.len()).any(|w| w == svc.as_slice());
    assert!(found, "service not found");

    // event_name should appear
    let name = b"Page Viewed";
    let found = bytes.windows(name.len()).any(|w| w == name.as_slice());
    assert!(found, "event_name not found");

    // payload should appear
    let found = bytes
        .windows(payload.len())
        .any(|w| w == payload.as_slice());
    assert!(found, "payload not found");
}

#[test]
fn encode_event_minimal() {
    let bytes = encode_event(&EventParams {
        event_type: EventType::Identify,
        timestamp: 0,
        service: None,
        device_id: None,
        session_id: None,
        event_name: None,
        payload: None,
    });

    let root_offset = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    assert!(root_offset < bytes.len());
    assert_eq!(bytes[root_offset + 28], EventType::Identify.as_u8());
}

#[test]
fn encode_event_with_service() {
    let bytes = encode_event(&EventParams {
        event_type: EventType::Track,
        timestamp: 1000,
        service: Some("my-backend"),
        device_id: None,
        session_id: None,
        event_name: Some("Click"),
        payload: None,
    });

    let root_offset = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    assert!(root_offset < bytes.len());

    // Service string should appear in the buffer
    let svc = b"my-backend";
    let found = bytes.windows(svc.len()).any(|w| w == svc.as_slice());
    assert!(found, "service string not found in encoded event");

    // VTable field 2 (service) should point to table+32
    let vtable_start = root_offset
        - i32::from_le_bytes([
            bytes[root_offset],
            bytes[root_offset + 1],
            bytes[root_offset + 2],
            bytes[root_offset + 3],
        ]) as usize;
    let field2 = u16::from_le_bytes([bytes[vtable_start + 8], bytes[vtable_start + 9]]);
    assert_eq!(
        field2, 32,
        "vtable field 2 (service) should point to offset 32"
    );

    // service offset at table+32 should be non-zero (relative offset to string)
    let service_off = u32::from_le_bytes([
        bytes[root_offset + 32],
        bytes[root_offset + 33],
        bytes[root_offset + 34],
        bytes[root_offset + 35],
    ]);
    assert_ne!(
        service_off, 0,
        "service offset should be non-zero when service is present"
    );
}

#[test]
fn encode_event_without_service() {
    let bytes = encode_event(&EventParams {
        event_type: EventType::Track,
        timestamp: 1000,
        service: None,
        device_id: None,
        session_id: None,
        event_name: None,
        payload: None,
    });

    let root_offset = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;

    // VTable field 2 (service) should be 0 (absent)
    let vtable_start = root_offset
        - i32::from_le_bytes([
            bytes[root_offset],
            bytes[root_offset + 1],
            bytes[root_offset + 2],
            bytes[root_offset + 3],
        ]) as usize;
    let field2 = u16::from_le_bytes([bytes[vtable_start + 8], bytes[vtable_start + 9]]);
    assert_eq!(
        field2, 0,
        "vtable field 2 (service) should be 0 when absent"
    );
}

#[test]
fn encode_event_data_single() {
    let device_id = [0xAA; UUID_LENGTH];
    let event = encode_event(&EventParams {
        event_type: EventType::Track,
        timestamp: 1000,
        service: None,
        device_id: Some(&device_id),
        session_id: None,
        event_name: Some("Click"),
        payload: None,
    });

    let data = encode_event_data(&[event]);

    // Should be valid FlatBuffer
    let root = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    assert!(root < data.len());

    // device_id should still be findable
    let found = data.windows(UUID_LENGTH).any(|w| w == [0xAA; UUID_LENGTH]);
    assert!(found, "device_id not found in event_data");
}

#[test]
fn encode_event_data_multiple() {
    let events: Vec<Vec<u8>> = (0..5)
        .map(|i| {
            let device_id = [i as u8; UUID_LENGTH];
            encode_event(&EventParams {
                event_type: EventType::Track,
                timestamp: 1000 + i,
                service: None,
                device_id: Some(&device_id),
                session_id: None,
                event_name: Some(&format!("Event{i}")),
                payload: None,
            })
        })
        .collect();

    let data = encode_event_data(&events);

    let root = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    assert!(root < data.len());

    // All event names should be present
    for i in 0..5 {
        let name = format!("Event{i}");
        let found = data.windows(name.len()).any(|w| w == name.as_bytes());
        assert!(found, "Event name '{}' not found in event_data", name);
    }
}

#[test]
fn encode_event_data_into_matches_encode_event_data() {
    let device_ids: Vec<[u8; UUID_LENGTH]> = (0..5).map(|i| [i as u8; UUID_LENGTH]).collect();
    let session_ids: Vec<[u8; UUID_LENGTH]> =
        (0..5).map(|i| [(i + 10) as u8; UUID_LENGTH]).collect();
    let names: Vec<String> = (0..5).map(|i| format!("Event{i}")).collect();
    let payloads: Vec<Vec<u8>> = (0..5)
        .map(|i| format!(r#"{{"idx":{i}}}"#).into_bytes())
        .collect();

    let params: Vec<EventParams<'_>> = (0..5)
        .map(|i| EventParams {
            event_type: EventType::Track,
            timestamp: 1000 + i as u64,
            service: None,
            device_id: Some(&device_ids[i]),
            session_id: Some(&session_ids[i]),
            event_name: Some(&names[i]),
            payload: Some(&payloads[i]),
        })
        .collect();

    // Encode with _into
    let mut buf = Vec::new();
    let range = encode_event_data_into(&mut buf, &params);
    let into_bytes = &buf[range];

    // Verify: valid FlatBuffer root
    let root =
        u32::from_le_bytes([into_bytes[0], into_bytes[1], into_bytes[2], into_bytes[3]]) as usize;
    assert!(root < into_bytes.len());

    // Verify: all event names, device_ids, payloads are present
    for (i, did) in device_ids.iter().enumerate() {
        let name = format!("Event{i}");
        assert!(
            into_bytes.windows(name.len()).any(|w| w == name.as_bytes()),
            "Event name '{}' not found in encode_event_data_into output",
            name,
        );
        assert!(
            into_bytes.windows(UUID_LENGTH).any(|w| w == *did),
            "device_id {} not found in encode_event_data_into output",
            i,
        );
        let payload = format!(r#"{{"idx":{i}}}"#);
        assert!(
            into_bytes
                .windows(payload.len())
                .any(|w| w == payload.as_bytes()),
            "payload {} not found in encode_event_data_into output",
            i,
        );
    }
}

#[test]
fn encode_event_data_into_reuses_buffer() {
    let mut buf = Vec::new();

    // First encode
    let params = [EventParams {
        event_type: EventType::Track,
        timestamp: 100,
        service: None,
        device_id: None,
        session_id: None,
        event_name: Some("First"),
        payload: None,
    }];
    let range1 = encode_event_data_into(&mut buf, &params);
    let len1 = range1.end - range1.start;
    assert!(len1 > 0);

    // Second encode appends
    let params2 = [EventParams {
        event_type: EventType::Identify,
        timestamp: 200,
        service: None,
        device_id: None,
        session_id: None,
        event_name: Some("Second"),
        payload: None,
    }];
    let range2 = encode_event_data_into(&mut buf, &params2);
    assert_eq!(range2.start, range1.end);
    assert!(buf.len() > len1);

    // Second encoding is independently valid
    let into_bytes = &buf[range2.clone()];
    let root =
        u32::from_le_bytes([into_bytes[0], into_bytes[1], into_bytes[2], into_bytes[3]]) as usize;
    assert!(root < into_bytes.len());
}

#[test]
fn encode_event_types() {
    for (et, val) in [
        (EventType::Unknown, 0),
        (EventType::Track, 1),
        (EventType::Identify, 2),
        (EventType::Group, 3),
        (EventType::Alias, 4),
        (EventType::Context, 6),
    ] {
        assert_eq!(et.as_u8(), val);
    }
}

// --- R7: session_id None vs Some in encoded FlatBuffers ---

/// Extract the vtable slot value for a given field index (0-based) from a
/// standalone Event FlatBuffer. Returns the u16 stored at vtable[4 + field*2].
fn event_vtable_slot(bytes: &[u8], field: usize) -> u16 {
    let root_offset = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    let soffset = i32::from_le_bytes([
        bytes[root_offset],
        bytes[root_offset + 1],
        bytes[root_offset + 2],
        bytes[root_offset + 3],
    ]);
    let vtable_start = (root_offset as i64 - soffset as i64) as usize;
    let slot_offset = vtable_start + 4 + field * 2;
    u16::from_le_bytes([bytes[slot_offset], bytes[slot_offset + 1]])
}

#[test]
fn test_encode_event_session_id_none_absent() {
    let bytes = encode_event(&EventParams {
        event_type: EventType::Track,
        timestamp: 1000,
        service: None,
        device_id: None,
        session_id: None,
        event_name: None,
        payload: None,
    });

    // Field 4 = session_id. When absent, the vtable slot must be 0.
    let slot = event_vtable_slot(&bytes, 4);
    assert_eq!(
        slot, 0,
        "session_id vtable slot must be 0 when session_id is None"
    );

    // No 16-byte window of 0x55 should appear (sanity — absence of sentinel).
    let sentinel = [0x55u8; UUID_LENGTH];
    assert!(
        !bytes.windows(UUID_LENGTH).any(|w| w == sentinel),
        "sentinel bytes must not appear in frame when session_id is None"
    );
}

#[test]
fn test_encode_event_session_id_some_present() {
    let sentinel = [0x55u8; UUID_LENGTH];
    let bytes = encode_event(&EventParams {
        event_type: EventType::Track,
        timestamp: 1000,
        service: None,
        device_id: None,
        session_id: Some(&sentinel),
        event_name: None,
        payload: None,
    });

    // Field 4 = session_id. When present, the vtable slot must be non-zero.
    let slot = event_vtable_slot(&bytes, 4);
    assert_ne!(
        slot, 0,
        "session_id vtable slot must be non-zero when session_id is Some"
    );

    // The sentinel bytes must appear verbatim in the encoded buffer.
    assert!(
        bytes.windows(UUID_LENGTH).any(|w| w == sentinel),
        "sentinel session_id bytes must appear in the encoded event"
    );
}
