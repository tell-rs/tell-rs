/// Write a u16 in little-endian format.
#[inline]
pub fn write_u16(buf: &mut Vec<u8>, value: u16) {
    buf.extend_from_slice(&value.to_le_bytes());
}

/// Write a u32 in little-endian format.
#[inline]
pub fn write_u32(buf: &mut Vec<u8>, value: u32) {
    buf.extend_from_slice(&value.to_le_bytes());
}

/// Write a u64 in little-endian format.
#[inline]
pub fn write_u64(buf: &mut Vec<u8>, value: u64) {
    buf.extend_from_slice(&value.to_le_bytes());
}

/// Write an f64 (double) in little-endian IEEE 754 format.
#[inline]
pub fn write_f64(buf: &mut Vec<u8>, value: f64) {
    buf.extend_from_slice(&value.to_le_bytes());
}

/// Write an i32 in little-endian format.
#[inline]
pub fn write_i32(buf: &mut Vec<u8>, value: i32) {
    buf.extend_from_slice(&value.to_le_bytes());
}

/// Align buffer to 4-byte boundary with zero padding.
#[inline]
pub fn align4(buf: &mut Vec<u8>) {
    while !buf.len().is_multiple_of(4) {
        buf.push(0);
    }
}

/// Write a FlatBuffer vector of bytes: [u32 length][data].
#[inline]
pub fn write_byte_vector(buf: &mut Vec<u8>, data: &[u8]) -> usize {
    let start = buf.len();
    write_u32(buf, data.len() as u32);
    buf.extend_from_slice(data);
    start
}

/// Write a FlatBuffer string: [u32 length][data][null terminator].
#[inline]
pub fn write_string(buf: &mut Vec<u8>, s: &str) -> usize {
    let start = buf.len();
    write_u32(buf, s.len() as u32);
    buf.extend_from_slice(s.as_bytes());
    buf.push(0); // null terminator
    start
}

/// Write a relative offset into the buffer at a given position.
#[inline]
pub fn patch_offset(buf: &mut [u8], offset_pos: usize, target: usize) {
    let rel = (target - offset_pos) as u32;
    buf[offset_pos..offset_pos + 4].copy_from_slice(&rel.to_le_bytes());
}
