use crate::LogEntryParams;
use crate::UUID_LENGTH;
use crate::helpers::*;

/// Encode a single LogEntry FlatBuffer.
///
/// LogEntry table fields:
/// - field 0: event_type `u8`
/// - field 1: session_id `[ubyte]`
/// - field 2: level `u8`
/// - field 3: timestamp `u64`
/// - field 4: source `string`
/// - field 5: service `string`
/// - field 6: payload `[ubyte]`
pub fn encode_log_entry(params: &LogEntryParams<'_>) -> Vec<u8> {
    let has_session_id = params.session_id.is_some();
    let has_source = params.source.is_some();
    let has_service = params.service.is_some();
    let has_payload = params.payload.is_some();

    // VTable: size(u16) + table_size(u16) + 7 field slots = 18 bytes
    let vtable_size: u16 = 4 + 7 * 2;

    // Fixed table layout (after soffset):
    // +4: session_id offset (u32)
    // +8: source offset (u32)
    // +12: service offset (u32)
    // +16: payload offset (u32)
    // +20: timestamp (u64)
    // +28: event_type (u8)
    // +29: level (u8)
    // +30-31: padding
    let table_size: u16 = 4 + 28;

    let session_id_size = if has_session_id { 4 + UUID_LENGTH } else { 0 };
    let source_size = params.source.map(|s| 4 + s.len() + 1).unwrap_or(0);
    let service_size = params.service.map(|s| 4 + s.len() + 1).unwrap_or(0);
    let payload_size = params.payload.map(|p| 4 + p.len()).unwrap_or(0);

    let estimated = 4
        + vtable_size as usize
        + table_size as usize
        + session_id_size
        + source_size
        + service_size
        + payload_size
        + 16;
    let mut buf = Vec::with_capacity(estimated);

    // Root offset placeholder
    buf.extend_from_slice(&[0u8; 4]);

    // VTable
    let vtable_start = buf.len();
    write_u16(&mut buf, vtable_size);
    write_u16(&mut buf, table_size);

    // Field offsets
    write_u16(&mut buf, 28); // field 0: event_type at +28
    write_u16(&mut buf, if has_session_id { 4 } else { 0 }); // field 1: session_id
    write_u16(&mut buf, 29); // field 2: level at +29
    write_u16(&mut buf, 20); // field 3: timestamp at +20
    write_u16(&mut buf, if has_source { 8 } else { 0 }); // field 4: source
    write_u16(&mut buf, if has_service { 12 } else { 0 }); // field 5: service
    write_u16(&mut buf, if has_payload { 16 } else { 0 }); // field 6: payload

    // Align vtable to 4 bytes (18 bytes -> pad 2)
    buf.extend_from_slice(&[0u8; 2]);

    // Table
    let table_start = buf.len();
    let soffset = (table_start - vtable_start) as i32;
    write_i32(&mut buf, soffset);

    // Offset placeholders
    let session_id_off_pos = buf.len();
    write_u32(&mut buf, 0);

    let source_off_pos = buf.len();
    write_u32(&mut buf, 0);

    let service_off_pos = buf.len();
    write_u32(&mut buf, 0);

    let payload_off_pos = buf.len();
    write_u32(&mut buf, 0);

    // timestamp (u64)
    write_u64(&mut buf, params.timestamp);

    // event_type (u8)
    buf.push(params.event_type.as_u8());

    // level (u8)
    buf.push(params.level.as_u8());

    // padding (2 bytes)
    buf.extend_from_slice(&[0u8; 2]);

    // Vectors and strings
    align4(&mut buf);

    // session_id
    let session_id_start = params.session_id.map(|id| write_byte_vector(&mut buf, id));
    align4(&mut buf);

    // source
    let source_start = params.source.map(|s| write_string(&mut buf, s));
    align4(&mut buf);

    // service
    let service_start = params.service.map(|s| write_string(&mut buf, s));
    align4(&mut buf);

    // payload
    let payload_start = params.payload.map(|data| write_byte_vector(&mut buf, data));

    // Fill in offsets
    buf[0..4].copy_from_slice(&(table_start as u32).to_le_bytes());

    if let Some(start) = session_id_start {
        patch_offset(&mut buf, session_id_off_pos, start);
    }
    if let Some(start) = source_start {
        patch_offset(&mut buf, source_off_pos, start);
    }
    if let Some(start) = service_start {
        patch_offset(&mut buf, service_off_pos, start);
    }
    if let Some(start) = payload_start {
        patch_offset(&mut buf, payload_off_pos, start);
    }

    buf
}

/// Encode a LogData FlatBuffer containing a vector of pre-encoded log entries.
///
/// LogData table:
/// - field 0: logs `[LogEntry]` (vector of tables)
pub fn encode_log_data(encoded_logs: &[Vec<u8>]) -> Vec<u8> {
    // VTable: size(u16) + table_size(u16) + 1 field = 6 bytes
    let vtable_size: u16 = 4 + 2;
    let table_size: u16 = 8; // soffset(4) + logs_offset(4)

    let logs_total: usize = encoded_logs.iter().map(|l| l.len() + 4).sum();
    let estimated = 4 + vtable_size as usize + table_size as usize + 4 + logs_total + 64;
    let mut buf = Vec::with_capacity(estimated);

    // Root offset placeholder
    buf.extend_from_slice(&[0u8; 4]);

    // VTable
    let vtable_start = buf.len();
    write_u16(&mut buf, vtable_size);
    write_u16(&mut buf, table_size);
    write_u16(&mut buf, 4); // field 0: logs at table+4

    // Align vtable (6 -> pad 2)
    buf.extend_from_slice(&[0u8; 2]);

    // Table
    let table_start = buf.len();
    let soffset = (table_start - vtable_start) as i32;
    write_i32(&mut buf, soffset);

    let logs_off_pos = buf.len();
    write_u32(&mut buf, 0);

    align4(&mut buf);

    // Logs vector
    let logs_vec_start = buf.len();
    let count = encoded_logs.len();

    write_u32(&mut buf, count as u32);

    let offsets_start = buf.len();
    for _ in 0..count {
        write_u32(&mut buf, 0);
    }

    align4(&mut buf);

    let mut table_positions = Vec::with_capacity(count);
    for log_bytes in encoded_logs {
        align4(&mut buf);

        let log_start = buf.len();
        let root_offset = if log_bytes.len() >= 4 {
            u32::from_le_bytes([log_bytes[0], log_bytes[1], log_bytes[2], log_bytes[3]]) as usize
        } else {
            0
        };

        table_positions.push(log_start + root_offset);
        buf.extend_from_slice(log_bytes);
    }

    for (i, &table_pos) in table_positions.iter().enumerate() {
        let offset_pos = offsets_start + i * 4;
        patch_offset(&mut buf, offset_pos, table_pos);
    }

    patch_offset(&mut buf, logs_off_pos, logs_vec_start);
    buf[0..4].copy_from_slice(&(table_start as u32).to_le_bytes());

    buf
}

/// Encode multiple log entries directly into a caller-owned buffer as a LogData FlatBuffer.
///
/// Zero-copy: writes the header first with reserved offset slots, then encodes
/// entries directly in their final position. No intermediate allocations or copies.
/// The caller can reuse `buf` across flushes via `buf.clear()`.
pub fn encode_log_data_into(
    buf: &mut Vec<u8>,
    logs: &[LogEntryParams<'_>],
) -> std::ops::Range<usize> {
    let data_start = buf.len();
    let count = logs.len();

    // Header: root(4) + vtable(6+2pad) + table(8) + vec_len(4) + slots(4*N)
    let root_pos = buf.len();
    buf.extend_from_slice(&[0u8; 4]);

    let vtable_start = buf.len();
    write_u16(buf, 6); // vtable_size
    write_u16(buf, 8); // table_size
    write_u16(buf, 4); // field 0: logs at table+4
    buf.extend_from_slice(&[0u8; 2]); // align vtable

    let table_start = buf.len();
    write_i32(buf, (table_start - vtable_start) as i32);

    let logs_off_pos = buf.len();
    write_u32(buf, 0);

    align4(buf);

    let logs_vec_start = buf.len();
    write_u32(buf, count as u32);

    let offsets_start = buf.len();
    for _ in 0..count {
        write_u32(buf, 0);
    }

    align4(buf);

    // Encode entries directly after header — each written once, in final position
    let mut table_positions = Vec::with_capacity(count);
    for params in logs {
        align4(buf);
        let entry_start = buf.len();
        encode_log_entry_into(buf, params);
        let root_offset = u32::from_le_bytes([
            buf[entry_start],
            buf[entry_start + 1],
            buf[entry_start + 2],
            buf[entry_start + 3],
        ]) as usize;
        table_positions.push(entry_start + root_offset);
    }

    // Patch vector offset slots → each entry's table position
    for (i, &table_pos) in table_positions.iter().enumerate() {
        patch_offset(buf, offsets_start + i * 4, table_pos);
    }

    patch_offset(buf, logs_off_pos, logs_vec_start);
    buf[root_pos..root_pos + 4].copy_from_slice(&((table_start - data_start) as u32).to_le_bytes());

    data_start..buf.len()
}

/// Encode a single log entry directly into an existing buffer.
fn encode_log_entry_into(buf: &mut Vec<u8>, params: &LogEntryParams<'_>) {
    let has_session_id = params.session_id.is_some();
    let has_source = params.source.is_some();
    let has_service = params.service.is_some();
    let has_payload = params.payload.is_some();

    let vtable_size: u16 = 4 + 7 * 2;
    let table_size: u16 = 4 + 28;

    let root_pos = buf.len();
    buf.extend_from_slice(&[0u8; 4]);

    let vtable_start = buf.len();
    write_u16(buf, vtable_size);
    write_u16(buf, table_size);

    write_u16(buf, 28);
    write_u16(buf, if has_session_id { 4 } else { 0 });
    write_u16(buf, 29);
    write_u16(buf, 20);
    write_u16(buf, if has_source { 8 } else { 0 });
    write_u16(buf, if has_service { 12 } else { 0 });
    write_u16(buf, if has_payload { 16 } else { 0 });

    buf.extend_from_slice(&[0u8; 2]); // align vtable

    let table_start = buf.len();
    let soffset = (table_start - vtable_start) as i32;
    write_i32(buf, soffset);

    let session_id_off_pos = buf.len();
    write_u32(buf, 0);
    let source_off_pos = buf.len();
    write_u32(buf, 0);
    let service_off_pos = buf.len();
    write_u32(buf, 0);
    let payload_off_pos = buf.len();
    write_u32(buf, 0);

    write_u64(buf, params.timestamp);
    buf.push(params.event_type.as_u8());
    buf.push(params.level.as_u8());
    buf.extend_from_slice(&[0u8; 2]);

    align4(buf);

    let session_id_start = params.session_id.map(|id| write_byte_vector(buf, id));
    align4(buf);
    let source_start = params.source.map(|s| write_string(buf, s));
    align4(buf);
    let service_start = params.service.map(|s| write_string(buf, s));
    align4(buf);
    let payload_start = params.payload.map(|data| write_byte_vector(buf, data));

    // Root offset relative to entry start (not absolute position)
    buf[root_pos..root_pos + 4].copy_from_slice(&((table_start - root_pos) as u32).to_le_bytes());

    if let Some(start) = session_id_start {
        patch_offset(buf, session_id_off_pos, start);
    }
    if let Some(start) = source_start {
        patch_offset(buf, source_off_pos, start);
    }
    if let Some(start) = service_start {
        patch_offset(buf, service_off_pos, start);
    }
    if let Some(start) = payload_start {
        patch_offset(buf, payload_off_pos, start);
    }
}
