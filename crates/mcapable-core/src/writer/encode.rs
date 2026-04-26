#![allow(dead_code)] // Encoding helpers are used incrementally as writer features land.

use crate::error::{Error, Result};
use crate::support::HashMap;
use crate::zero_copy::ByteStr;
use std::io::Write;

pub(crate) fn push_le_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn push_le_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn push_le_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn push_record_header(out: &mut Vec<u8>, opcode: crate::types::Opcode, length: u64) {
    out.push(opcode.as_u8());
    push_le_u64(out, length);
}

pub(crate) fn push_len_prefixed_bytes(out: &mut Vec<u8>, bytes: &[u8]) -> Result<()> {
    let len: u32 = bytes
        .len()
        .try_into()
        .map_err(|_| Error::InvalidRecord("length exceeds u32".to_string()))?;
    push_le_u32(out, len);
    out.extend_from_slice(bytes);
    Ok(())
}

pub(crate) fn push_len_prefixed_str(out: &mut Vec<u8>, value: &ByteStr) -> Result<()> {
    push_len_prefixed_bytes(out, value.as_bytes())
}

pub(crate) fn push_metadata_map(
    out: &mut Vec<u8>,
    metadata: &HashMap<ByteStr, ByteStr>,
) -> Result<()> {
    let mut body = Vec::new();
    for (key, value) in metadata {
        push_len_prefixed_str(&mut body, key)?;
        push_len_prefixed_str(&mut body, value)?;
    }
    push_len_prefixed_bytes(out, &body)?;
    Ok(())
}

pub(crate) fn write_magic(out: &mut impl Write) -> std::io::Result<()> {
    out.write_all(&crate::format::MCAP_MAGIC)
}

pub(crate) fn write_record(
    out: &mut impl Write,
    opcode: crate::types::Opcode,
    payload: &[u8],
) -> std::io::Result<()> {
    let mut header = [0u8; 9];
    header[0] = opcode.as_u8();
    header[1..9].copy_from_slice(&(payload.len() as u64).to_le_bytes());
    out.write_all(&header)?;
    out.write_all(payload)?;
    Ok(())
}

pub(crate) fn write_record_header(
    out: &mut impl Write,
    opcode: crate::types::Opcode,
    length: u64,
) -> std::io::Result<()> {
    let mut header = [0u8; 9];
    header[0] = opcode.as_u8();
    header[1..9].copy_from_slice(&length.to_le_bytes());
    out.write_all(&header)
}

pub(crate) fn encode_footer(
    summary_start: u64,
    summary_offset_start: u64,
    summary_crc: u32,
) -> [u8; 20] {
    let mut buf = [0u8; 20];
    buf[0..8].copy_from_slice(&summary_start.to_le_bytes());
    buf[8..16].copy_from_slice(&summary_offset_start.to_le_bytes());
    buf[16..20].copy_from_slice(&summary_crc.to_le_bytes());
    buf
}

pub(crate) fn encode_data_end(data_section_crc: u32) -> [u8; 4] {
    data_section_crc.to_le_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;
    use bytes::Bytes;

    #[test]
    fn record_header_round_trip() {
        let mut buf = Vec::new();
        push_record_header(&mut buf, crate::types::Opcode::Schema, 1234);
        let (_, (opcode, length)) = parser::record_header(&buf).unwrap();
        assert_eq!(opcode, crate::types::Opcode::Schema.as_u8());
        assert_eq!(length, 1234);
    }

    #[test]
    fn magic_bytes_written() {
        let mut buf = Vec::new();
        write_magic(&mut buf).unwrap();
        assert_eq!(buf.as_slice(), &crate::format::MCAP_MAGIC);
    }

    #[test]
    fn footer_round_trip() {
        let payload = encode_footer(10, 20, 0x12345678);
        let parsed = parser::parse_footer_record(Bytes::copy_from_slice(&payload)).unwrap();
        assert_eq!(parsed.summary_start, 10);
        assert_eq!(parsed.summary_offset_start, 20);
        assert_eq!(parsed.summary_crc, 0x12345678);
    }

    #[test]
    fn data_end_round_trip() {
        let payload = encode_data_end(0xaabbccdd);
        let parsed = parser::parse_data_end_record(Bytes::copy_from_slice(&payload)).unwrap();
        assert_eq!(parsed.data_section_crc, 0xaabbccdd);
    }

    #[test]
    fn header_round_trip_with_metadata() {
        let mut payload = Vec::new();
        let profile = ByteStr::from("ros2");
        let library = ByteStr::from("mcapable");
        push_len_prefixed_str(&mut payload, &profile).unwrap();
        push_len_prefixed_str(&mut payload, &library).unwrap();

        let mut metadata = HashMap::default();
        metadata.insert(ByteStr::from("key1"), ByteStr::from("value1"));
        metadata.insert(ByteStr::from("key2"), ByteStr::from("value2"));
        push_metadata_map(&mut payload, &metadata).unwrap();

        let parsed = parser::parse_header_record(Bytes::from(payload)).unwrap();
        assert_eq!(parsed.profile, profile);
        assert_eq!(parsed.library, library);
        assert_eq!(parsed.metadata, metadata);
    }
}
