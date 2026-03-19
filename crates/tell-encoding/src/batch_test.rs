use crate::{API_KEY_LENGTH, BatchParams, SchemaType, encode_batch};

#[test]
fn encode_batch_produces_valid_flatbuffer() {
    let api_key = [0xA1u8; API_KEY_LENGTH];
    let data = b"test payload data";

    let bytes = encode_batch(&BatchParams {
        api_key: &api_key,
        schema_type: SchemaType::Event,
        version: 100,
        batch_id: 42,
        data,
    });

    // Root offset is first 4 bytes
    let root_offset = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    assert!(root_offset < bytes.len());

    // Read soffset from table
    let table_start = root_offset;
    let soffset = i32::from_le_bytes([
        bytes[table_start],
        bytes[table_start + 1],
        bytes[table_start + 2],
        bytes[table_start + 3],
    ]);
    let vtable_start = table_start - soffset as usize;

    // Read vtable size
    let vtable_size = u16::from_le_bytes([bytes[vtable_start], bytes[vtable_start + 1]]) as usize;
    assert_eq!(vtable_size, 16); // 4 + 6*2

    // Read schema_type from table (at table_start + 24)
    assert_eq!(bytes[table_start + 24], SchemaType::Event.as_u8());

    // Read version from table (at table_start + 25)
    assert_eq!(bytes[table_start + 25], 100);

    // Read batch_id from table (at table_start + 16)
    let batch_id = u64::from_le_bytes([
        bytes[table_start + 16],
        bytes[table_start + 17],
        bytes[table_start + 18],
        bytes[table_start + 19],
        bytes[table_start + 20],
        bytes[table_start + 21],
        bytes[table_start + 22],
        bytes[table_start + 23],
    ]);
    assert_eq!(batch_id, 42);
}

#[test]
fn encode_batch_contains_api_key() {
    let api_key: [u8; 16] = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
        0x10,
    ];
    let data = b"payload";

    let bytes = encode_batch(&BatchParams {
        api_key: &api_key,
        schema_type: SchemaType::Log,
        version: 100,
        batch_id: 0,
        data,
    });

    // The api_key bytes should appear in the output
    let found = bytes.windows(API_KEY_LENGTH).any(|w| w == api_key);
    assert!(found, "api_key not found in encoded batch");
}

#[test]
fn encode_batch_contains_data_payload() {
    let api_key = [0xFFu8; API_KEY_LENGTH];
    let data = b"hello world payload";

    let bytes = encode_batch(&BatchParams {
        api_key: &api_key,
        schema_type: SchemaType::Event,
        version: 100,
        batch_id: 1,
        data,
    });

    let found = bytes.windows(data.len()).any(|w| w == data.as_slice());
    assert!(found, "data payload not found in encoded batch");
}

#[test]
fn encode_batch_zero_batch_id() {
    let api_key = [0x00u8; API_KEY_LENGTH];
    let data = b"x";

    let bytes = encode_batch(&BatchParams {
        api_key: &api_key,
        schema_type: SchemaType::Event,
        version: 100,
        batch_id: 0,
        data,
    });

    // Should still produce valid output
    assert!(bytes.len() > 4);

    let root_offset = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    assert!(root_offset < bytes.len());
}
