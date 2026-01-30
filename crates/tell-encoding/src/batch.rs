use crate::helpers::*;
use crate::{BatchParams, API_KEY_LENGTH, DEFAULT_VERSION};

/// Encode a FlatBuffer Batch message.
///
/// Layout:
/// ```text
/// [4 bytes: root offset] -> points to table
/// [vtable]
///   - vtable_size (u16)
///   - table_size (u16)
///   - field offsets (u16 each, 0 = not present)
/// [table]
///   - soffset to vtable (i32)
///   - inline scalars and vector offsets
/// [vectors]
///   - length (u32) + data bytes
/// ```
///
/// Batch fields:
/// - field 0: api_key `[ubyte]` (required)
/// - field 1: schema_type `u8`
/// - field 2: version `u8`
/// - field 3: batch_id `u64`
/// - field 4: data `[ubyte]` (required)
/// - field 5: source_ip `[ubyte]` (not used by SDKs)
pub fn encode_batch(params: &BatchParams<'_>) -> Vec<u8> {
    let mut buf = Vec::new();
    encode_batch_into(&mut buf, params);
    buf
}

/// Encode a Batch into a caller-owned buffer (avoids allocation when buffer is reused).
pub fn encode_batch_into(buf: &mut Vec<u8>, params: &BatchParams<'_>) {
    let has_batch_id = params.batch_id != 0;
    let version = if params.version == 0 { DEFAULT_VERSION } else { params.version };

    // VTable: size(u16) + table_size(u16) + 6 field slots (u16 each) = 16 bytes
    let vtable_size: u16 = 4 + 6 * 2;

    // Fixed table layout (32 bytes total):
    // soffset (4) + api_key_off (4) + data_off (4) + source_ip_off (4) + batch_id (8) + schema_type (1) + version (1) + pad (2) = 28 after soffset
    let table_size: u16 = 4 + 28;

    let api_key_vec_size = 4 + API_KEY_LENGTH;
    let data_vec_size = 4 + params.data.len();

    let estimated = 4 + vtable_size as usize + table_size as usize + api_key_vec_size + data_vec_size + 16;
    buf.reserve(estimated);

    let base = buf.len();

    // Root offset placeholder
    buf.extend_from_slice(&[0u8; 4]);

    // VTable
    let vtable_start = buf.len();
    write_u16(buf, vtable_size);
    write_u16(buf, table_size);

    // Field offsets
    write_u16(buf, 4);                                              // field 0: api_key at table+4
    write_u16(buf, 24);                                             // field 1: schema_type at table+24
    write_u16(buf, 25);                                             // field 2: version at table+25
    write_u16(buf, if has_batch_id { 16 } else { 0 });             // field 3: batch_id at table+16
    write_u16(buf, 8);                                              // field 4: data at table+8
    write_u16(buf, 0);                                              // field 5: source_ip (not used)

    // Table
    let table_start = buf.len();
    let soffset = (table_start - vtable_start) as i32;
    write_i32(buf, soffset);

    // api_key offset placeholder
    let api_key_off_pos = buf.len();
    write_u32(buf, 0);

    // data offset placeholder
    let data_off_pos = buf.len();
    write_u32(buf, 0);

    // source_ip offset placeholder (unused)
    write_u32(buf, 0);

    // batch_id (u64)
    write_u64(buf, params.batch_id);

    // schema_type (u8)
    buf.push(params.schema_type.as_u8());

    // version (u8)
    buf.push(version);

    // padding (2 bytes)
    buf.extend_from_slice(&[0u8; 2]);

    // Vectors
    align4(buf);

    // api_key vector
    let api_key_vec_start = write_byte_vector(buf, params.api_key);

    align4(buf);

    // data vector
    let data_vec_start = write_byte_vector(buf, params.data);

    // Fill in offsets
    buf[base..base + 4].copy_from_slice(&(table_start as u32).to_le_bytes());
    patch_offset(buf, api_key_off_pos, api_key_vec_start);
    patch_offset(buf, data_off_pos, data_vec_start);
}
