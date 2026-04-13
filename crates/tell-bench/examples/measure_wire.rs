use tell_encoding::{
    BatchParams, EventParams, EventType, SchemaType, encode_batch, encode_event, encode_event_data,
};

fn main() {
    let payload_json =
        br#"{"user_id":"throughput_user","url":"/home","referrer":"google","screen":"1920x1080"}"#;
    let no_payload: Option<&[u8]> = None;
    let device_id = [0u8; 16];
    let session_id = [1u8; 16];
    let api_key = [2u8; 16];

    // Single event with payload
    let event_with = encode_event(&EventParams {
        event_type: EventType::Track,
        timestamp: 1706000000000,
        service: None,
        device_id: Some(&device_id),
        session_id: Some(&session_id),
        event_name: Some("Page Viewed"),
        payload: Some(payload_json),
    });
    println!("Single event (with payload): {} bytes", event_with.len());

    // Single event without payload
    let event_without = encode_event(&EventParams {
        event_type: EventType::Track,
        timestamp: 1706000000000,
        service: None,
        device_id: Some(&device_id),
        session_id: Some(&session_id),
        event_name: Some("Page Viewed"),
        payload: no_payload,
    });
    println!("Single event (no payload):   {} bytes", event_without.len());

    // EventData wrapping 1 event
    let event_data_1 = encode_event_data(std::slice::from_ref(&event_with));
    println!("EventData (1 event):         {} bytes", event_data_1.len());

    // Batch wrapping 1 event
    let batch_1 = encode_batch(&BatchParams {
        api_key: &api_key,
        schema_type: SchemaType::Event,
        version: 1,
        batch_id: 1,
        data: &event_data_1,
    });
    println!(
        "Batch (1 event, with payload): {} bytes (+ 4 byte TCP frame)",
        batch_1.len()
    );

    // Batch wrapping 10 events
    let events_10: Vec<Vec<u8>> = (0..10).map(|_| event_with.clone()).collect();
    let event_data_10 = encode_event_data(&events_10);
    let batch_10 = encode_batch(&BatchParams {
        api_key: &api_key,
        schema_type: SchemaType::Event,
        version: 1,
        batch_id: 1,
        data: &event_data_10,
    });
    println!(
        "Batch (10 events):  {:>6} bytes ({} per event)",
        batch_10.len(),
        batch_10.len() / 10
    );

    // Batch wrapping 100 events
    let events_100: Vec<Vec<u8>> = (0..100).map(|_| event_with.clone()).collect();
    let event_data_100 = encode_event_data(&events_100);
    let batch_100 = encode_batch(&BatchParams {
        api_key: &api_key,
        schema_type: SchemaType::Event,
        version: 1,
        batch_id: 1,
        data: &event_data_100,
    });
    println!(
        "Batch (100 events): {:>6} bytes ({} per event)",
        batch_100.len(),
        batch_100.len() / 100
    );

    // Batch wrapping 500 events
    let events_500: Vec<Vec<u8>> = (0..500).map(|_| event_with.clone()).collect();
    let event_data_500 = encode_event_data(&events_500);
    let batch_500 = encode_batch(&BatchParams {
        api_key: &api_key,
        schema_type: SchemaType::Event,
        version: 1,
        batch_id: 1,
        data: &event_data_500,
    });
    println!(
        "Batch (500 events): {:>6} bytes ({} per event)",
        batch_500.len(),
        batch_500.len() / 500
    );

    println!();
    println!("JSON payload alone: {} bytes", payload_json.len());
    println!("Payload: {}", std::str::from_utf8(payload_json).unwrap());
}
