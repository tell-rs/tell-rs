use crate::MetricEntryParams;
use crate::helpers::*;

/// Encode a single MetricEntry FlatBuffer.
///
/// MetricEntry table fields (by ID):
/// - field 0:  metric_type `u8`
/// - field 1:  timestamp `u64`
/// - field 2:  name `string` (required)
/// - field 3:  value `double`
/// - field 4:  source `string`
/// - field 5:  service `string`
/// - field 6:  labels `[Label]` (vector of tables)
/// - field 7:  temporality `u8`
/// - field 8:  histogram (not yet supported)
/// - field 9:  session_id `[ubyte]`
/// - field 10: int_labels (not yet supported)
///
/// Table layout (after soffset):
/// +4:  name offset (u32)
/// +8:  source offset (u32)
/// +12: service offset (u32)
/// +16: labels offset (u32)
/// +20: int_labels offset (u32) — always 0
/// +24: histogram offset (u32) — always 0
/// +28: session_id offset (u32)
/// +32: timestamp (u64)
/// +40: value (f64)
/// +48: metric_type (u8)
/// +49: temporality (u8)
/// +50-51: padding
pub fn encode_metric_entry(params: &MetricEntryParams<'_>) -> Vec<u8> {
    let mut buf = Vec::with_capacity(256);
    encode_metric_entry_into(&mut buf, params);
    buf
}

/// Encode a MetricData FlatBuffer containing a vector of pre-encoded metric entries.
///
/// MetricData table:
/// - field 0: metrics `[MetricEntry]` (vector of tables)
pub fn encode_metric_data(encoded_metrics: &[Vec<u8>]) -> Vec<u8> {
    let vtable_size: u16 = 4 + 2;
    let table_size: u16 = 8;

    let metrics_total: usize = encoded_metrics.iter().map(|m| m.len() + 4).sum();
    let estimated = 4 + vtable_size as usize + table_size as usize + 4 + metrics_total + 64;
    let mut buf = Vec::with_capacity(estimated);

    // Root offset placeholder
    buf.extend_from_slice(&[0u8; 4]);

    // VTable
    let vtable_start = buf.len();
    write_u16(&mut buf, vtable_size);
    write_u16(&mut buf, table_size);
    write_u16(&mut buf, 4); // field 0: metrics at table+4

    // Align vtable (6 -> pad 2)
    buf.extend_from_slice(&[0u8; 2]);

    // Table
    let table_start = buf.len();
    let soffset = (table_start - vtable_start) as i32;
    write_i32(&mut buf, soffset);

    let metrics_off_pos = buf.len();
    write_u32(&mut buf, 0);

    align4(&mut buf);

    // Metrics vector
    let metrics_vec_start = buf.len();
    let count = encoded_metrics.len();

    write_u32(&mut buf, count as u32);

    let offsets_start = buf.len();
    for _ in 0..count {
        write_u32(&mut buf, 0);
    }

    align4(&mut buf);

    let mut table_positions = Vec::with_capacity(count);
    for metric_bytes in encoded_metrics {
        align4(&mut buf);

        let metric_start = buf.len();
        let root_offset = if metric_bytes.len() >= 4 {
            u32::from_le_bytes([
                metric_bytes[0],
                metric_bytes[1],
                metric_bytes[2],
                metric_bytes[3],
            ]) as usize
        } else {
            0
        };

        table_positions.push(metric_start + root_offset);
        buf.extend_from_slice(metric_bytes);
    }

    for (i, &table_pos) in table_positions.iter().enumerate() {
        let offset_pos = offsets_start + i * 4;
        patch_offset(&mut buf, offset_pos, table_pos);
    }

    patch_offset(&mut buf, metrics_off_pos, metrics_vec_start);
    buf[0..4].copy_from_slice(&(table_start as u32).to_le_bytes());

    buf
}

/// Encode multiple metric entries directly into a caller-owned buffer as a MetricData FlatBuffer.
///
/// Zero-copy: writes the header first with reserved offset slots, then encodes
/// entries directly in their final position. No intermediate allocations or copies.
/// The caller can reuse `buf` across flushes via `buf.clear()`.
///
/// Returns the range `start..buf.len()` of the MetricData bytes within `buf`.
pub fn encode_metric_data_into(
    buf: &mut Vec<u8>,
    metrics: &[MetricEntryParams<'_>],
) -> std::ops::Range<usize> {
    let data_start = buf.len();
    let count = metrics.len();

    // Header: root(4) + vtable(6+2pad) + table(8) + vec_len(4) + slots(4*N)
    let root_pos = buf.len();
    buf.extend_from_slice(&[0u8; 4]);

    let vtable_start = buf.len();
    write_u16(buf, 6); // vtable_size
    write_u16(buf, 8); // table_size
    write_u16(buf, 4); // field 0: metrics at table+4
    buf.extend_from_slice(&[0u8; 2]); // align vtable

    let table_start = buf.len();
    write_i32(buf, (table_start - vtable_start) as i32);

    let metrics_off_pos = buf.len();
    write_u32(buf, 0);

    align4(buf);

    let metrics_vec_start = buf.len();
    write_u32(buf, count as u32);

    let offsets_start = buf.len();
    for _ in 0..count {
        write_u32(buf, 0);
    }

    align4(buf);

    // Encode entries directly after header
    let mut table_positions = Vec::with_capacity(count);
    for params in metrics {
        align4(buf);
        let entry_start = buf.len();
        encode_metric_entry_into(buf, params);
        let root_offset = u32::from_le_bytes([
            buf[entry_start],
            buf[entry_start + 1],
            buf[entry_start + 2],
            buf[entry_start + 3],
        ]) as usize;
        table_positions.push(entry_start + root_offset);
    }

    // Patch vector offset slots
    for (i, &table_pos) in table_positions.iter().enumerate() {
        patch_offset(buf, offsets_start + i * 4, table_pos);
    }

    patch_offset(buf, metrics_off_pos, metrics_vec_start);
    buf[root_pos..root_pos + 4].copy_from_slice(&((table_start - data_start) as u32).to_le_bytes());

    data_start..buf.len()
}

/// Encode a single metric entry directly into an existing buffer.
fn encode_metric_entry_into(buf: &mut Vec<u8>, params: &MetricEntryParams<'_>) {
    let has_source = params.source.is_some();
    let has_service = params.service.is_some();
    let has_labels = !params.labels.is_empty();
    let has_histogram = params.histogram.is_some();
    let has_session_id = params.session_id.is_some();

    // VTable: 4 + 11 field slots * 2 = 26 bytes, padded to 28
    let vtable_size: u16 = 4 + 11 * 2;
    // Table: soffset(4) + 7 offsets(28) + timestamp(8) + value(8) + metric_type(1) + temporality(1) + pad(2) = 52
    let table_size: u16 = 52;

    // Root offset placeholder
    let root_pos = buf.len();
    buf.extend_from_slice(&[0u8; 4]);

    // VTable
    let vtable_start = buf.len();
    write_u16(buf, vtable_size);
    write_u16(buf, table_size);

    // Field slots (by field ID 0..10)
    write_u16(buf, 48); // field 0: metric_type
    write_u16(buf, 32); // field 1: timestamp
    write_u16(buf, 4); // field 2: name (always present, required)
    write_u16(buf, 40); // field 3: value
    write_u16(buf, if has_source { 8 } else { 0 }); // field 4: source
    write_u16(buf, if has_service { 12 } else { 0 }); // field 5: service
    write_u16(buf, if has_labels { 16 } else { 0 }); // field 6: labels
    write_u16(buf, 49); // field 7: temporality
    write_u16(buf, if has_histogram { 24 } else { 0 }); // field 8: histogram
    write_u16(buf, if has_session_id { 28 } else { 0 }); // field 9: session_id
    write_u16(buf, 0); // field 10: int_labels (not supported)

    // Align vtable (26 -> pad 2)
    buf.extend_from_slice(&[0u8; 2]);

    // Table
    let table_start = buf.len();
    let soffset = (table_start - vtable_start) as i32;
    write_i32(buf, soffset);

    // Offset placeholders
    let name_off_pos = buf.len();
    write_u32(buf, 0); // +4: name

    let source_off_pos = buf.len();
    write_u32(buf, 0); // +8: source

    let service_off_pos = buf.len();
    write_u32(buf, 0); // +12: service

    let labels_off_pos = buf.len();
    write_u32(buf, 0); // +16: labels

    write_u32(buf, 0); // +20: int_labels (always 0)

    let histogram_off_pos = buf.len();
    write_u32(buf, 0); // +24: histogram

    let session_id_off_pos = buf.len();
    write_u32(buf, 0); // +28: session_id

    // Inline scalars
    write_u64(buf, params.timestamp); // +32: timestamp
    write_f64(buf, params.value); // +40: value
    buf.push(params.metric_type.as_u8()); // +48: metric_type
    buf.push(params.temporality.as_u8()); // +49: temporality
    buf.extend_from_slice(&[0u8; 2]); // +50: padding

    // Variable-length data
    align4(buf);

    // name (required)
    let name_start = write_string(buf, params.name);
    align4(buf);

    // source
    let source_start = params.source.map(|s| write_string(buf, s));
    align4(buf);

    // service
    let service_start = params.service.map(|s| write_string(buf, s));
    align4(buf);

    // session_id
    let session_id_start = params.session_id.map(|id| write_byte_vector(buf, id));
    align4(buf);

    // labels (vector of Label tables)
    let labels_start = if has_labels {
        Some(encode_labels(buf, params.labels))
    } else {
        None
    };

    // histogram (sub-table)
    let histogram_start = params.histogram.map(|h| encode_histogram(buf, h));

    // Patch offsets
    buf[root_pos..root_pos + 4].copy_from_slice(&((table_start - root_pos) as u32).to_le_bytes());
    patch_offset(buf, name_off_pos, name_start);

    if let Some(start) = source_start {
        patch_offset(buf, source_off_pos, start);
    }
    if let Some(start) = service_start {
        patch_offset(buf, service_off_pos, start);
    }
    if let Some(start) = session_id_start {
        patch_offset(buf, session_id_off_pos, start);
    }
    if let Some(start) = labels_start {
        patch_offset(buf, labels_off_pos, start);
    }
    if let Some(start) = histogram_start {
        patch_offset(buf, histogram_off_pos, start);
    }
}

/// Encode a vector of Label tables into the buffer.
///
/// Label table fields:
/// - field 0: key `string` (required)
/// - field 1: value `string` (required)
///
/// Returns the start position of the vector (for offset patching).
fn encode_labels(buf: &mut Vec<u8>, labels: &[crate::LabelParam<'_>]) -> usize {
    let count = labels.len();

    // Write vector header: [count][offset_slot_0][offset_slot_1]...
    let vec_start = buf.len();
    write_u32(buf, count as u32);

    let offsets_start = buf.len();
    for _ in 0..count {
        write_u32(buf, 0);
    }

    align4(buf);

    // Write each Label table, tracking table positions
    let mut table_positions = Vec::with_capacity(count);
    for label in labels {
        align4(buf);

        // Label VTable: size(2) + table_size(2) + 2 fields(4) = 8 bytes
        let vtable_start = buf.len();
        write_u16(buf, 8); // vtable_size
        write_u16(buf, 12); // table_size: soffset(4) + key_off(4) + value_off(4)
        write_u16(buf, 4); // field 0: key at table+4
        write_u16(buf, 8); // field 1: value at table+8

        // Label Table
        let table_start = buf.len();
        table_positions.push(table_start);
        let soffset = (table_start - vtable_start) as i32;
        write_i32(buf, soffset);

        let key_off_pos = buf.len();
        write_u32(buf, 0);

        let value_off_pos = buf.len();
        write_u32(buf, 0);

        // Key string
        align4(buf);
        let key_start = write_string(buf, label.key);

        // Value string
        align4(buf);
        let value_start = write_string(buf, label.value);

        // Patch label offsets
        patch_offset(buf, key_off_pos, key_start);
        patch_offset(buf, value_off_pos, value_start);
    }

    // Patch vector offset slots
    for (i, &table_pos) in table_positions.iter().enumerate() {
        patch_offset(buf, offsets_start + i * 4, table_pos);
    }

    vec_start
}

/// Encode a Histogram sub-table.
///
/// Histogram table fields:
/// - field 0: count `u64`
/// - field 1: sum `f64`
/// - field 2: buckets `[Bucket]` (vector of tables)
/// - field 3: min `f64`
/// - field 4: max `f64`
///
/// Returns the table start position.
fn encode_histogram(buf: &mut Vec<u8>, h: &crate::HistogramParams) -> usize {
    let has_buckets = !h.buckets.is_empty();

    // VTable: 4 + 5 fields * 2 = 14 bytes, aligned to 16
    let vtable_start = buf.len();
    write_u16(buf, 14); // vtable_size
    // table: soffset(4) + count(8) + sum(8) + buckets_offset(4) + min(8) + max(8) = 40
    write_u16(buf, 40);
    write_u16(buf, 4); // field 0: count at +4
    write_u16(buf, 12); // field 1: sum at +12
    write_u16(buf, if has_buckets { 20 } else { 0 }); // field 2: buckets at +20
    write_u16(buf, 24); // field 3: min at +24
    write_u16(buf, 32); // field 4: max at +32

    align4(buf);

    // Table
    let table_start = buf.len();
    write_i32(buf, (table_start - vtable_start) as i32);

    // count (u64) at +4
    write_u64(buf, h.count);
    // sum (f64) at +12
    write_f64(buf, h.sum);

    // buckets offset placeholder at +20
    let buckets_off_pos = buf.len();
    write_u32(buf, 0);

    // min (f64) at +24
    write_f64(buf, h.min);
    // max (f64) at +32
    write_f64(buf, h.max);

    align4(buf);

    // Buckets vector
    if has_buckets {
        let start = encode_buckets(buf, &h.buckets);
        patch_offset(buf, buckets_off_pos, start);
    }

    table_start
}

/// Encode a vector of Bucket tables.
///
/// Bucket table fields:
/// - field 0: upper_bound `f64`
/// - field 1: count `u64`
fn encode_buckets(buf: &mut Vec<u8>, buckets: &[(f64, u64)]) -> usize {
    let vec_start = buf.len();
    write_u32(buf, buckets.len() as u32);

    let slots_start = buf.len();
    for _ in 0..buckets.len() {
        write_u32(buf, 0);
    }
    align4(buf);

    let mut offsets = Vec::with_capacity(buckets.len());
    for &(upper_bound, count) in buckets {
        // Bucket VTable: 4 + 2*2 = 8 bytes
        let vtable_start = buf.len();
        write_u16(buf, 8); // vtable_size
        write_u16(buf, 20); // table_size: soffset(4) + upper_bound(8) + count(8)
        write_u16(buf, 4); // field 0: upper_bound at +4
        write_u16(buf, 12); // field 1: count at +12

        align4(buf);

        let table_start = buf.len();
        write_i32(buf, (table_start - vtable_start) as i32);

        // upper_bound (f64)
        write_f64(buf, upper_bound);
        // count (u64)
        write_u64(buf, count);

        align4(buf);
        offsets.push(table_start);
    }

    for (i, &offset) in offsets.iter().enumerate() {
        patch_offset(buf, slots_start + i * 4, offset);
    }

    vec_start
}
