use crate::helpers::*;
use crate::{EventParams, UUID_LENGTH};

/// Encode a single Event FlatBuffer.
///
/// Event table fields:
/// - field 0: event_type `u8`
/// - field 1: timestamp `u64`
/// - field 2: device_id `[ubyte]`
/// - field 3: session_id `[ubyte]`
/// - field 4: event_name `string`
/// - field 5: payload `[ubyte]`
pub fn encode_event(params: &EventParams<'_>) -> Vec<u8> {
    let has_device_id = params.device_id.is_some();
    let has_session_id = params.session_id.is_some();
    let has_event_name = params.event_name.is_some();
    let has_payload = params.payload.is_some();

    // VTable: size(u16) + table_size(u16) + 6 field slots = 16 bytes
    let vtable_size: u16 = 4 + 6 * 2;

    // Fixed table layout (after soffset):
    // +4: device_id offset (u32)
    // +8: session_id offset (u32)
    // +12: event_name offset (u32)
    // +16: payload offset (u32)
    // +20: timestamp (u64)
    // +28: event_type (u8)
    // +29-31: padding (3 bytes)
    let table_size: u16 = 4 + 28;

    let device_id_size = if has_device_id { 4 + UUID_LENGTH } else { 0 };
    let session_id_size = if has_session_id { 4 + UUID_LENGTH } else { 0 };
    let event_name_size = params.event_name.map(|s| 4 + s.len() + 1).unwrap_or(0);
    let payload_size = params.payload.map(|p| 4 + p.len()).unwrap_or(0);

    let estimated = 4 + vtable_size as usize + table_size as usize
        + device_id_size + session_id_size + event_name_size + payload_size + 16;
    let mut buf = Vec::with_capacity(estimated);

    // Root offset placeholder
    buf.extend_from_slice(&[0u8; 4]);

    // VTable
    let vtable_start = buf.len();
    write_u16(&mut buf, vtable_size);
    write_u16(&mut buf, table_size);

    // Field offsets
    write_u16(&mut buf, 28);                                                // field 0: event_type at +28
    write_u16(&mut buf, 20);                                                // field 1: timestamp at +20
    write_u16(&mut buf, if has_device_id { 4 } else { 0 });                // field 2: device_id
    write_u16(&mut buf, if has_session_id { 8 } else { 0 });               // field 3: session_id
    write_u16(&mut buf, if has_event_name { 12 } else { 0 });              // field 4: event_name
    write_u16(&mut buf, if has_payload { 16 } else { 0 });                 // field 5: payload

    // Table
    let table_start = buf.len();
    let soffset = (table_start - vtable_start) as i32;
    write_i32(&mut buf, soffset);

    // Offset placeholders
    let device_id_off_pos = buf.len();
    write_u32(&mut buf, 0);

    let session_id_off_pos = buf.len();
    write_u32(&mut buf, 0);

    let event_name_off_pos = buf.len();
    write_u32(&mut buf, 0);

    let payload_off_pos = buf.len();
    write_u32(&mut buf, 0);

    // timestamp (u64)
    write_u64(&mut buf, params.timestamp);

    // event_type (u8)
    buf.push(params.event_type.as_u8());

    // padding (3 bytes)
    buf.extend_from_slice(&[0u8; 3]);

    // Vectors and strings
    align4(&mut buf);

    // device_id
    let device_id_start = params.device_id.map(|id| write_byte_vector(&mut buf, id));
    align4(&mut buf);

    // session_id
    let session_id_start = params.session_id.map(|id| write_byte_vector(&mut buf, id));
    align4(&mut buf);

    // event_name
    let event_name_start = params.event_name.map(|name| write_string(&mut buf, name));
    align4(&mut buf);

    // payload
    let payload_start = params.payload.map(|data| write_byte_vector(&mut buf, data));

    // Fill in offsets
    buf[0..4].copy_from_slice(&(table_start as u32).to_le_bytes());

    if let Some(start) = device_id_start {
        patch_offset(&mut buf, device_id_off_pos, start);
    }
    if let Some(start) = session_id_start {
        patch_offset(&mut buf, session_id_off_pos, start);
    }
    if let Some(start) = event_name_start {
        patch_offset(&mut buf, event_name_off_pos, start);
    }
    if let Some(start) = payload_start {
        patch_offset(&mut buf, payload_off_pos, start);
    }

    buf
}

/// Encode an EventData FlatBuffer containing a vector of pre-encoded events.
///
/// EventData table:
/// - field 0: events `[Event]` (vector of tables)
///
/// Each entry in `encoded_events` is a standalone FlatBuffer with its own root offset.
/// The vector offsets point to the actual table data within each event.
pub fn encode_event_data(encoded_events: &[Vec<u8>]) -> Vec<u8> {
    // VTable: size(u16) + table_size(u16) + 1 field = 6 bytes
    let vtable_size: u16 = 4 + 2;
    let table_size: u16 = 8; // soffset(4) + events_offset(4)

    let events_total: usize = encoded_events.iter().map(|e| e.len() + 4).sum();
    let estimated = 4 + vtable_size as usize + table_size as usize + 4 + events_total + 64;
    let mut buf = Vec::with_capacity(estimated);

    // Root offset placeholder
    buf.extend_from_slice(&[0u8; 4]);

    // VTable
    let vtable_start = buf.len();
    write_u16(&mut buf, vtable_size);
    write_u16(&mut buf, table_size);
    write_u16(&mut buf, 4); // field 0: events at table+4

    // Align vtable to 4 bytes (6 bytes -> pad 2)
    buf.extend_from_slice(&[0u8; 2]);

    // Table
    let table_start = buf.len();
    let soffset = (table_start - vtable_start) as i32;
    write_i32(&mut buf, soffset);

    let events_off_pos = buf.len();
    write_u32(&mut buf, 0);

    align4(&mut buf);

    // Events vector
    let events_vec_start = buf.len();
    let count = encoded_events.len();

    // Vector length
    write_u32(&mut buf, count as u32);

    // Reserve offset slots
    let offsets_start = buf.len();
    for _ in 0..count {
        write_u32(&mut buf, 0);
    }

    align4(&mut buf);

    // Write event data, track table positions
    let mut table_positions = Vec::with_capacity(count);
    for event_bytes in encoded_events {
        align4(&mut buf);

        let event_start = buf.len();

        // Read root offset from the event (first 4 bytes LE u32)
        let root_offset = if event_bytes.len() >= 4 {
            u32::from_le_bytes([event_bytes[0], event_bytes[1], event_bytes[2], event_bytes[3]])
                as usize
        } else {
            0
        };

        table_positions.push(event_start + root_offset);
        buf.extend_from_slice(event_bytes);
    }

    // Patch event offsets
    for (i, &table_pos) in table_positions.iter().enumerate() {
        let offset_pos = offsets_start + i * 4;
        patch_offset(&mut buf, offset_pos, table_pos);
    }

    // Patch events vector offset
    patch_offset(&mut buf, events_off_pos, events_vec_start);

    // Patch root offset
    buf[0..4].copy_from_slice(&(table_start as u32).to_le_bytes());

    buf
}

/// Encode multiple events directly into a caller-owned buffer as an EventData FlatBuffer.
///
/// Zero-copy: writes the header first with reserved offset slots, then encodes
/// events directly in their final position. No intermediate allocations or copies.
/// The caller can reuse `buf` across flushes via `buf.clear()`.
///
/// Returns the range `start..buf.len()` of the EventData bytes within `buf`.
pub fn encode_event_data_into(buf: &mut Vec<u8>, events: &[EventParams<'_>]) -> std::ops::Range<usize> {
    let data_start = buf.len();
    let count = events.len();

    // Write EventData header (all sizes deterministic):
    // [4] root offset placeholder
    // [8] vtable (6 bytes + 2 pad)
    // [8] table (soffset + events_offset)
    // [4] vector length
    // [4*N] offset slot placeholders

    let root_pos = buf.len();
    buf.extend_from_slice(&[0u8; 4]);

    let vtable_start = buf.len();
    write_u16(buf, 6); // vtable_size
    write_u16(buf, 8); // table_size
    write_u16(buf, 4); // field 0: events at table+4
    buf.extend_from_slice(&[0u8; 2]); // align vtable

    let table_start = buf.len();
    write_i32(buf, (table_start - vtable_start) as i32);

    let events_off_pos = buf.len();
    write_u32(buf, 0);

    align4(buf);

    let events_vec_start = buf.len();
    write_u32(buf, count as u32);

    let offsets_start = buf.len();
    for _ in 0..count {
        write_u32(buf, 0);
    }

    align4(buf);

    // Encode events directly after header — each written once, in final position
    let mut table_positions = Vec::with_capacity(count);
    for params in events {
        align4(buf);
        let event_start = buf.len();
        encode_event_into(buf, params);
        let root_offset = u32::from_le_bytes([
            buf[event_start],
            buf[event_start + 1],
            buf[event_start + 2],
            buf[event_start + 3],
        ]) as usize;
        table_positions.push(event_start + root_offset);
    }

    // Patch vector offset slots → each event's table position
    for (i, &table_pos) in table_positions.iter().enumerate() {
        patch_offset(buf, offsets_start + i * 4, table_pos);
    }

    patch_offset(buf, events_off_pos, events_vec_start);
    buf[root_pos..root_pos + 4].copy_from_slice(&((table_start - data_start) as u32).to_le_bytes());

    data_start..buf.len()
}

/// Encode a single event directly into an existing buffer.
fn encode_event_into(buf: &mut Vec<u8>, params: &EventParams<'_>) {
    let has_device_id = params.device_id.is_some();
    let has_session_id = params.session_id.is_some();
    let has_event_name = params.event_name.is_some();
    let has_payload = params.payload.is_some();

    let vtable_size: u16 = 4 + 6 * 2;
    let table_size: u16 = 4 + 28;

    // Root offset placeholder
    let root_pos = buf.len();
    buf.extend_from_slice(&[0u8; 4]);

    // VTable
    let vtable_start = buf.len();
    write_u16(buf, vtable_size);
    write_u16(buf, table_size);

    write_u16(buf, 28);
    write_u16(buf, 20);
    write_u16(buf, if has_device_id { 4 } else { 0 });
    write_u16(buf, if has_session_id { 8 } else { 0 });
    write_u16(buf, if has_event_name { 12 } else { 0 });
    write_u16(buf, if has_payload { 16 } else { 0 });

    // Table
    let table_start = buf.len();
    let soffset = (table_start - vtable_start) as i32;
    write_i32(buf, soffset);

    let device_id_off_pos = buf.len();
    write_u32(buf, 0);
    let session_id_off_pos = buf.len();
    write_u32(buf, 0);
    let event_name_off_pos = buf.len();
    write_u32(buf, 0);
    let payload_off_pos = buf.len();
    write_u32(buf, 0);

    write_u64(buf, params.timestamp);
    buf.push(params.event_type.as_u8());
    buf.extend_from_slice(&[0u8; 3]);

    align4(buf);

    let device_id_start = params.device_id.map(|id| write_byte_vector(buf, id));
    align4(buf);
    let session_id_start = params.session_id.map(|id| write_byte_vector(buf, id));
    align4(buf);
    let event_name_start = params.event_name.map(|name| write_string(buf, name));
    align4(buf);
    let payload_start = params.payload.map(|data| write_byte_vector(buf, data));

    // Root offset relative to event start (not absolute position)
    buf[root_pos..root_pos + 4].copy_from_slice(&((table_start - root_pos) as u32).to_le_bytes());

    if let Some(start) = device_id_start {
        patch_offset(buf, device_id_off_pos, start);
    }
    if let Some(start) = session_id_start {
        patch_offset(buf, session_id_off_pos, start);
    }
    if let Some(start) = event_name_start {
        patch_offset(buf, event_name_off_pos, start);
    }
    if let Some(start) = payload_start {
        patch_offset(buf, payload_off_pos, start);
    }
}
