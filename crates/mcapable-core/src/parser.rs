//! Binary parsing for MCAP format using nom.
//!
//! This module provides parsers for all MCAP record types according to the
//! MCAP specification: <https://mcap.dev/specification>

use crate::format::MESSAGE_HEADER_SIZE;
use crate::support::{HashMap, Vec};
use crate::types::*;
use crate::zero_copy::ByteStr;
use crate::zero_copy::{Span, bytes_from_span, root_span};
use bytes::Bytes;
use nom::{
    IResult,
    bytes::complete::take,
    number::complete::{le_u8, le_u16, le_u32, le_u64},
    sequence::tuple,
};

#[cfg(test)]
use crate::format::MCAP_MAGIC;
#[cfg(test)]
use crate::support::str as support_str;
#[cfg(test)]
use nom::bytes::complete::tag;

fn message_header_fields(input: &[u8]) -> IResult<&[u8], (u16, u32, u64, u64)> {
    tuple((le_u16, le_u32, le_u64, le_u64))(input)
}

/// Parse a message header from a message record body slice.
///
/// `content` must contain the 22-byte message header followed by payload bytes.
pub fn parse_message_header_from_content(content: &[u8]) -> Result<MessageHeader, crate::Error> {
    let payload_len =
        content
            .len()
            .checked_sub(MESSAGE_HEADER_SIZE)
            .ok_or(crate::Error::ParseError(crate::ParseError::Opcode(
                Opcode::Message,
            )))?;

    let (_, (channel_id, sequence, log_time, publish_time)) = message_header_fields(content)
        .map_err(|_| crate::Error::ParseError(crate::ParseError::Opcode(Opcode::Message)))?;

    Ok(MessageHeader {
        channel_id,
        sequence,
        log_time,
        publish_time,
        data_size: payload_len as u64,
    })
}

/// Parse a message header and payload from a backing buffer + content range.
///
/// This is used by Stream implementations that iterate over decompressed chunk
/// buffers and want message payloads to reference the full chunk backing.
pub fn parse_message_from_backing_range(
    backing: &Bytes,
    content_start: usize,
    content_end: usize,
) -> Result<(MessageHeader, Payload), crate::Error> {
    if content_start > content_end || content_end > backing.len() {
        return Err(crate::Error::ParseError(crate::ParseError::Opcode(
            Opcode::Message,
        )));
    }
    let header_end = content_start + MESSAGE_HEADER_SIZE;
    if header_end > content_end {
        return Err(crate::Error::ParseError(crate::ParseError::Opcode(
            Opcode::Message,
        )));
    }

    let header_bytes = &backing[content_start..header_end];
    let (_, (channel_id, sequence, log_time, publish_time)) =
        message_header_fields(header_bytes)
            .map_err(|_| crate::Error::ParseError(crate::ParseError::Opcode(Opcode::Message)))?;

    let payload = Payload::from_backing_range(backing.clone(), header_end, content_end);
    let payload_len = content_end - header_end;

    Ok((
        MessageHeader {
            channel_id,
            sequence,
            log_time,
            publish_time,
            data_size: payload_len as u64,
        },
        payload,
    ))
}

/// Parse MCAP magic bytes at start/end of file.
#[cfg(test)]
pub fn magic_bytes(input: &[u8]) -> IResult<&[u8], ()> {
    let (input, _) = tag(&MCAP_MAGIC[..])(input)?;
    Ok((input, ()))
}

/// Parse a record header (opcode + length).
///
/// Returns (opcode, length) tuple.
pub fn record_header(input: &[u8]) -> IResult<&[u8], (u8, u64)> {
    let (input, (opcode, length)) = tuple((le_u8, le_u64))(input)?;
    Ok((input, (opcode, length)))
}

/// Parse a record header (opcode + length) from a `Bytes`-backed span.
#[cfg(test)]
#[allow(dead_code)]
pub fn record_header_span<'a>(input: Span<'a>) -> IResult<Span<'a>, (u8, u64)> {
    let (input, (opcode, length)) = tuple((le_u8, le_u64))(input)?;
    Ok((input, (opcode, length)))
}

/// Parse a length-prefixed string.
///
/// Format: u32 length + UTF-8 string bytes.
#[cfg(test)]
pub fn length_string(input: &[u8]) -> IResult<&[u8], &str> {
    let (input, length) = le_u32(input)?;
    let (input, bytes) = take(length)(input)?;
    let s = support_str::from_utf8(bytes).map_err(|_| {
        nom::Err::Failure(nom::error::Error::new(input, nom::error::ErrorKind::Verify))
    })?;
    Ok((input, s))
}

/// Parse a length-prefixed UTF-8 string as a zero-copy `ByteStr`.
///
/// Format: u32 length + UTF-8 string bytes.
pub fn length_string_span<'a>(input: Span<'a>) -> IResult<Span<'a>, ByteStr> {
    let (input, length) = le_u32(input)?;
    let length: usize = length as usize;
    let (input, bytes_span) = take(length)(input)?;

    let bytes = bytes_from_span(bytes_span);
    let s = ByteStr::from_utf8(bytes).map_err(|_| {
        nom::Err::Failure(nom::error::Error::new(input, nom::error::ErrorKind::Verify))
    })?;
    Ok((input, s))
}

/// Parse a key-value metadata map.
///
/// Format: u32 length + repeated (key_string, value_string) pairs.
#[cfg(test)]
pub fn metadata_map(input: &[u8]) -> IResult<&[u8], HashMap<ByteStr, ByteStr>> {
    let (input, map_length) = le_u32(input)?;
    let (remaining, map_data) = take(map_length)(input)?;

    let mut map = HashMap::new();
    let mut input = map_data;

    while !input.is_empty() {
        let (rest, (key, value)) = tuple((length_string, length_string))(input)?;
        map.insert(ByteStr::from(key), ByteStr::from(value));
        input = rest;
    }

    Ok((remaining, map))
}

/// Parse a key-value metadata map as zero-copy `ByteStr` keys/values.
///
/// Format: u32 byte length + repeated (key_string, value_string) pairs.
pub fn metadata_map_span<'a>(input: Span<'a>) -> IResult<Span<'a>, HashMap<ByteStr, ByteStr>> {
    let (input, map_length) = le_u32(input)?;
    let map_length: usize = map_length as usize;
    let (remaining, map_span) = take(map_length)(input)?;

    let mut map = HashMap::new();
    let mut cursor = map_span;

    while !cursor.fragment().is_empty() {
        let (rest, (key, value)) = tuple((length_string_span, length_string_span))(cursor)?;
        map.insert(key, value);
        cursor = rest;
    }

    Ok((remaining, map))
}

/// Parse Header record (opcode 0x01).
pub fn parse_header_record(input: Bytes) -> Result<Header, crate::Error> {
    let span = root_span(&input);
    let (span, (profile, library)) = tuple((length_string_span, length_string_span))(span)
        .map_err(|_| crate::Error::ParseError(crate::ParseError::Opcode(Opcode::Header)))?;

    let (_, metadata) = if span.fragment().is_empty() {
        (span, HashMap::new())
    } else {
        metadata_map_span(span)
            .map_err(|_| crate::Error::ParseError(crate::ParseError::Opcode(Opcode::Header)))?
    };

    Ok(Header {
        profile,
        library,
        metadata,
    })
}

/// Parse Footer record (opcode 0x02).
pub fn parse_footer_record(input: Bytes) -> Result<Footer, crate::Error> {
    let span = root_span(&input);
    let (_, (summary_start, summary_offset_start, summary_crc)): (Span<'_>, (u64, u64, u32)) =
        tuple::<_, _, nom::error::Error<_>, _>((le_u64, le_u64, le_u32))(span)
            .map_err(|_| crate::Error::ParseError(crate::ParseError::Opcode(Opcode::Footer)))?;

    Ok(Footer {
        summary_start,
        summary_offset_start,
        summary_crc,
    })
}

/// Parse Schema record (opcode 0x03).
pub fn parse_schema_record(input: Bytes) -> Result<Schema, crate::Error> {
    let span = root_span(&input);
    let (span, (id, name, encoding, data_length)): (Span<'_>, (u16, ByteStr, ByteStr, u32)) =
        tuple((le_u16, length_string_span, length_string_span, le_u32))(span)
            .map_err(|_| crate::Error::ParseError(crate::ParseError::Opcode(Opcode::Schema)))?;

    let data_length: usize = data_length as usize;
    let (_, data_span) = take::<_, _, nom::error::Error<_>>(data_length)(span)
        .map_err(|_| crate::Error::ParseError(crate::ParseError::Opcode(Opcode::Schema)))?;

    Ok(Schema {
        id,
        name,
        encoding,
        data: bytes_from_span(data_span),
    })
}

/// Parse Channel record (opcode 0x04).
pub fn parse_channel_record(input: Bytes) -> Result<Channel, crate::Error> {
    let span = root_span(&input);
    let (span, (id, schema_id, topic, message_encoding)): (Span<'_>, (u16, u16, ByteStr, ByteStr)) =
        tuple((le_u16, le_u16, length_string_span, length_string_span))(span)
            .map_err(|_| crate::Error::ParseError(crate::ParseError::Opcode(Opcode::Channel)))?;

    let (_, metadata) = metadata_map_span(span)
        .map_err(|_| crate::Error::ParseError(crate::ParseError::Opcode(Opcode::Channel)))?;

    Ok(Channel {
        id,
        topic,
        message_encoding,
        schema_id,
        metadata,
    })
}

/// Parse Message record (opcode 0x05) from owned or borrowed data.
///
/// The message header is 22 bytes:
/// - channel_id: 2 bytes
/// - sequence: 4 bytes
/// - log_time: 8 bytes
/// - publish_time: 8 bytes
/// - data: remaining bytes (no length prefix!)
pub fn parse_message_record(input: Bytes) -> Result<RawMessage, crate::Error> {
    let span = root_span(&input);
    let (_, (channel_id, sequence, log_time, publish_time)): (Span<'_>, (u16, u32, u64, u64)) =
        tuple::<_, _, nom::error::Error<_>, _>((le_u16, le_u32, le_u64, le_u64))(span)
            .map_err(|_| crate::Error::ParseError(crate::ParseError::Opcode(Opcode::Message)))?;

    let payload = Payload::from_bytes(input.slice(MESSAGE_HEADER_SIZE..input.len()));

    Ok(RawMessage::new(
        channel_id,
        sequence,
        log_time,
        publish_time,
        payload,
    ))
}

/// Parse Chunk record (opcode 0x06).
pub fn parse_chunk_record(input: Bytes) -> Result<Chunk, crate::Error> {
    let span = root_span(&input);
    let (
        span,
        (
            message_start_time,
            message_end_time,
            uncompressed_size,
            uncompressed_crc,
            compression,
            compressed_length,
        ),
    ): (Span<'_>, (u64, u64, u64, u32, ByteStr, u64)) = tuple::<_, _, nom::error::Error<_>, _>((
        le_u64,
        le_u64,
        le_u64,
        le_u32,
        length_string_span,
        le_u64,
    ))(span)
    .map_err(|_| crate::Error::ParseError(crate::ParseError::Opcode(Opcode::Chunk)))?;

    let compressed_length: usize = compressed_length
        .try_into()
        .map_err(|_| crate::Error::ParseError(crate::ParseError::Opcode(Opcode::Chunk)))?;

    let start = span.location_offset();
    let end = start + compressed_length;
    let records = if end <= span.extra.len() {
        span.extra.slice(start..end)
    } else {
        return Err(crate::Error::ParseError(crate::ParseError::Opcode(
            Opcode::Chunk,
        )));
    };

    Ok(Chunk {
        message_start_time,
        message_end_time,
        uncompressed_size,
        uncompressed_crc,
        compression,
        records,
    })
}

/// Parse MessageIndex record (opcode 0x07).
pub fn parse_message_index_record(input: Bytes) -> Result<MessageIndex, crate::Error> {
    let span = root_span(&input);
    let (span, (channel_id, records_length)): (Span<'_>, (u16, u32)) =
        tuple::<_, _, nom::error::Error<_>, _>((le_u16, le_u32))(span).map_err(|_| {
            crate::Error::ParseError(crate::ParseError::Opcode(Opcode::MessageIndex))
        })?;

    let records_length: usize = records_length as usize;
    let (_, records_span) = take::<_, _, nom::error::Error<_>>(records_length)(span)
        .map_err(|_| crate::Error::ParseError(crate::ParseError::Opcode(Opcode::MessageIndex)))?;
    let records_bytes = bytes_from_span(records_span);

    let mut records = Vec::new();
    let mut remaining: &[u8] = records_bytes.as_ref();
    while !remaining.is_empty() {
        let (rest, (timestamp, offset)) =
            tuple::<_, _, nom::error::Error<_>, _>((le_u64, le_u64))(remaining).map_err(|_| {
                crate::Error::ParseError(crate::ParseError::Opcode(Opcode::MessageIndex))
            })?;
        records.push(MessageIndexEntry { timestamp, offset });
        remaining = rest;
    }

    Ok(MessageIndex {
        channel_id,
        records,
    })
}

/// Parse ChunkIndex record (opcode 0x08).
pub fn parse_chunk_index_record(input: Bytes) -> Result<ChunkIndex, crate::Error> {
    // Format per MCAP spec:
    // message_start_time: u64
    // message_end_time: u64
    // chunk_start_offset: u64
    // chunk_length: u64
    // message_index_offsets: Map<u16, u64> (length-prefixed with u32 byte count)
    // message_index_length: u64
    // compression: String (length-prefixed)
    // compressed_size: u64
    // uncompressed_size: u64

    let span = root_span(&input);
    let (span, (message_start_time, message_end_time, chunk_start_offset, chunk_length)): (
        Span<'_>,
        (u64, u64, u64, u64),
    ) = tuple::<_, _, nom::error::Error<_>, _>((le_u64, le_u64, le_u64, le_u64))(span)
        .map_err(|_| crate::Error::ParseError(crate::ParseError::Opcode(Opcode::ChunkIndex)))?;

    let (span, map_byte_len) = le_u32::<_, nom::error::Error<_>>(span)
        .map_err(|_| crate::Error::ParseError(crate::ParseError::Opcode(Opcode::ChunkIndex)))?;
    let (span, map_span) = take::<_, _, nom::error::Error<_>>(map_byte_len as usize)(span)
        .map_err(|_| crate::Error::ParseError(crate::ParseError::Opcode(Opcode::ChunkIndex)))?;
    let map_bytes = bytes_from_span(map_span);

    let mut message_index_offsets = HashMap::new();
    let mut map_input: &[u8] = map_bytes.as_ref();
    while !map_input.is_empty() {
        let (rest, (channel_id, offset)) =
            tuple::<_, _, nom::error::Error<_>, _>((le_u16, le_u64))(map_input).map_err(|_| {
                crate::Error::ParseError(crate::ParseError::Opcode(Opcode::ChunkIndex))
            })?;
        message_index_offsets.insert(channel_id, offset);
        map_input = rest;
    }

    let (_, (message_index_length, compression, _compressed_size, uncompressed_size)): (
        Span<'_>,
        (u64, ByteStr, u64, u64),
    ) = tuple::<_, _, nom::error::Error<_>, _>((le_u64, length_string_span, le_u64, le_u64))(span)
        .map_err(|_| crate::Error::ParseError(crate::ParseError::Opcode(Opcode::ChunkIndex)))?;

    Ok(ChunkIndex {
        message_start_time,
        message_end_time,
        chunk_start_offset,
        chunk_length,
        message_index_offsets,
        message_index_length,
        uncompressed_size,
        compression,
    })
}

/// Parse Attachment record (opcode 0x0A).
pub fn parse_attachment_record(input: Bytes) -> Result<Attachment, crate::Error> {
    let span = root_span(&input);
    let (span, (log_time, create_time, name, media_type, data_length)): (
        Span<'_>,
        (u64, u64, ByteStr, ByteStr, u64),
    ) = tuple::<_, _, nom::error::Error<_>, _>((
        le_u64,
        le_u64,
        length_string_span,
        length_string_span,
        le_u64,
    ))(span)
    .map_err(|_| crate::Error::ParseError(crate::ParseError::Opcode(Opcode::Attachment)))?;

    let data_length: usize = data_length
        .try_into()
        .map_err(|_| crate::Error::ParseError(crate::ParseError::Opcode(Opcode::Attachment)))?;
    let (span, data_span) = take::<_, _, nom::error::Error<_>>(data_length)(span)
        .map_err(|_| crate::Error::ParseError(crate::ParseError::Opcode(Opcode::Attachment)))?;
    let (_, _crc) = le_u32::<_, nom::error::Error<_>>(span)
        .map_err(|_| crate::Error::ParseError(crate::ParseError::Opcode(Opcode::Attachment)))?;

    Ok(Attachment {
        log_time,
        create_time,
        name,
        media_type,
        data: bytes_from_span(data_span),
    })
}

/// Parse Statistics record (opcode 0x0B).
pub fn parse_statistics_record(input: Bytes) -> Result<Statistics, crate::Error> {
    let span = root_span(&input);
    let (
        span,
        (
            message_count,
            schema_count,
            channel_count,
            attachment_count,
            metadata_count,
            chunk_count,
            message_start_time,
            message_end_time,
        ),
    ) = tuple::<_, _, nom::error::Error<_>, _>((
        le_u64, le_u16, le_u32, le_u32, le_u32, le_u32, le_u64, le_u64,
    ))(span)
    .map_err(|_| crate::Error::ParseError(crate::ParseError::Opcode(Opcode::Statistics)))?;

    let (span, counts_len) = le_u32::<_, nom::error::Error<_>>(span)
        .map_err(|_| crate::Error::ParseError(crate::ParseError::Opcode(Opcode::Statistics)))?;
    let counts_len: usize = counts_len
        .try_into()
        .map_err(|_| crate::Error::ParseError(crate::ParseError::Opcode(Opcode::Statistics)))?;
    let (_, counts_span) = take::<_, _, nom::error::Error<_>>(counts_len)(span)
        .map_err(|_| crate::Error::ParseError(crate::ParseError::Opcode(Opcode::Statistics)))?;

    let mut channel_message_counts = Vec::new();
    let mut cursor = counts_span;
    while !cursor.fragment().is_empty() {
        let (rest, (channel_id, per_channel_count)) =
            tuple::<_, _, nom::error::Error<_>, _>((le_u16, le_u64))(cursor).map_err(|_| {
                crate::Error::ParseError(crate::ParseError::Opcode(Opcode::Statistics))
            })?;
        channel_message_counts.push(ChannelMessageCount {
            channel_id,
            message_count: per_channel_count,
        });
        cursor = rest;
    }

    Ok(Statistics {
        message_count,
        schema_count,
        channel_count,
        attachment_count,
        metadata_count,
        chunk_count,
        message_start_time,
        message_end_time,
        channel_message_counts,
    })
}

/// Parse Metadata record (opcode 0x0C).
pub fn parse_metadata_record(input: Bytes) -> Result<Metadata, crate::Error> {
    let span = root_span(&input);
    let (_, (name, metadata)): (Span<'_>, (ByteStr, HashMap<ByteStr, ByteStr>)) =
        tuple::<_, _, nom::error::Error<_>, _>((length_string_span, metadata_map_span))(span)
            .map_err(|_| crate::Error::ParseError(crate::ParseError::Opcode(Opcode::Metadata)))?;

    Ok(Metadata { name, metadata })
}

/// Parse MetadataIndex record (opcode 0x09).
pub fn parse_metadata_index_record(input: Bytes) -> Result<MetadataIndex, crate::Error> {
    let span = root_span(&input);
    let (_, (offset, length, name)): (Span<'_>, (u64, u64, ByteStr)) =
        tuple::<_, _, nom::error::Error<_>, _>((le_u64, le_u64, length_string_span))(span)
            .map_err(|_| {
                crate::Error::ParseError(crate::ParseError::Opcode(Opcode::MetadataIndex))
            })?;

    Ok(MetadataIndex {
        offset,
        length,
        name,
    })
}

/// Parse AttachmentIndex record (opcode 0x0D).
pub fn parse_attachment_index_record(input: Bytes) -> Result<AttachmentIndex, crate::Error> {
    let span = root_span(&input);
    let (_, (offset, length, log_time, create_time, data_size, name, media_type)) =
        tuple::<_, _, nom::error::Error<_>, _>((
            le_u64,
            le_u64,
            le_u64,
            le_u64,
            le_u64,
            length_string_span,
            length_string_span,
        ))(span)
        .map_err(|_| {
            crate::Error::ParseError(crate::ParseError::Opcode(Opcode::AttachmentIndex))
        })?;

    Ok(AttachmentIndex {
        offset,
        length,
        log_time,
        create_time,
        data_size,
        name,
        media_type,
    })
}

/// Parse SummaryOffset record (opcode 0x0E).
pub fn parse_summary_offset_record(input: Bytes) -> Result<SummaryOffset, crate::Error> {
    let span = root_span(&input);
    let (_, (group_opcode, group_start, group_length)): (Span<'_>, (u8, u64, u64)) =
        tuple::<_, _, nom::error::Error<_>, _>((le_u8, le_u64, le_u64))(span).map_err(|_| {
            crate::Error::ParseError(crate::ParseError::Opcode(Opcode::SummaryOffset))
        })?;

    Ok(SummaryOffset {
        group_opcode,
        group_start,
        group_length,
    })
}

/// Parse DataEnd record (opcode 0x0F).
pub fn parse_data_end_record(input: Bytes) -> Result<DataEnd, crate::Error> {
    let span = root_span(&input);
    let (_, data_section_crc): (Span<'_>, u32) = le_u32::<_, nom::error::Error<_>>(span)
        .map_err(|_| crate::Error::ParseError(crate::ParseError::Opcode(Opcode::DataEnd)))?;
    Ok(DataEnd { data_section_crc })
}

/// SummaryOffset record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryOffset {
    pub group_opcode: u8,
    pub group_start: u64,
    pub group_length: u64,
}

/// DataEnd record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataEnd {
    pub data_section_crc: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[test]
    fn test_parse_message_header_from_content_sets_data_size() {
        let mut data = Vec::new();
        data.extend_from_slice(&1u16.to_le_bytes()); // channel_id
        data.extend_from_slice(&42u32.to_le_bytes()); // sequence
        data.extend_from_slice(&100u64.to_le_bytes()); // log_time
        data.extend_from_slice(&200u64.to_le_bytes()); // publish_time
        data.extend_from_slice(b"hello"); // payload

        let header = parse_message_header_from_content(&data).unwrap();
        assert_eq!(header.channel_id, 1);
        assert_eq!(header.sequence, 42);
        assert_eq!(header.log_time, 100);
        assert_eq!(header.publish_time, 200);
        assert_eq!(header.data_size, 5);
    }

    #[test]
    fn test_parse_message_header_from_content_rejects_truncated() {
        let data = [0u8; MESSAGE_HEADER_SIZE - 1];
        let result = parse_message_header_from_content(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_message_from_backing_range_zero_copy_payload() {
        let mut data = Vec::new();
        data.extend_from_slice(&1u16.to_le_bytes()); // channel_id
        data.extend_from_slice(&42u32.to_le_bytes()); // sequence
        data.extend_from_slice(&100u64.to_le_bytes()); // log_time
        data.extend_from_slice(&200u64.to_le_bytes()); // publish_time
        data.extend_from_slice(b"hello"); // payload

        let backing = Bytes::from(data);
        let (header, payload) =
            parse_message_from_backing_range(&backing, 0, backing.len()).unwrap();
        assert_eq!(header.data_size, 5);
        assert_eq!(payload.as_slice(), b"hello");

        let base = backing.as_ref().as_ptr() as usize;
        let end = base + backing.len();
        let payload_ptr = payload.as_slice().as_ptr() as usize;
        assert!(payload_ptr >= base && payload_ptr < end);
    }

    #[test]
    fn test_parse_message_from_backing_range_rejects_out_of_bounds() {
        let backing = Bytes::from_static(b"abcd");
        let result = parse_message_from_backing_range(&backing, 0, 10);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_message_from_backing_range_rejects_truncated_header() {
        let backing = Bytes::from_static(b"abcd");
        let result = parse_message_from_backing_range(&backing, 0, 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_magic_bytes() {
        let valid = [0x89, b'M', b'C', b'A', b'P', 0x30, b'\r', b'\n'];
        assert!(magic_bytes(&valid).is_ok());

        let invalid = [0x00, b'M', b'C', b'A', b'P', 0x30, b'\r', b'\n'];
        assert!(magic_bytes(&invalid).is_err());
    }

    #[test]
    fn test_record_header() {
        // Opcode 0x01 (Header), length 0x00000000_00000010 (16 bytes)
        let data = [0x01, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let result = record_header(&data);
        assert!(result.is_ok());
        let (_, (opcode, length)) = result.unwrap();
        assert_eq!(opcode, 0x01);
        assert_eq!(length, 0x10);
    }

    #[test]
    fn test_length_string() {
        // Length 5, "hello"
        let data = [0x05, 0x00, 0x00, 0x00, b'h', b'e', b'l', b'l', b'o'];
        let result = length_string(&data);
        assert!(result.is_ok());
        let (_, s) = result.unwrap();
        assert_eq!(s, "hello");
    }

    #[test]
    fn test_length_string_empty() {
        // Length 0, empty string
        let data = [0x00, 0x00, 0x00, 0x00];
        let result = length_string(&data);
        assert!(result.is_ok());
        let (_, s) = result.unwrap();
        assert_eq!(s, "");
    }

    #[test]
    fn test_length_string_span_valid() {
        let bytes = Bytes::from_static(b"\x05\x00\x00\x00hello");
        let span = root_span(&bytes);
        let (_, s) = length_string_span(span).unwrap();
        assert_eq!(s, "hello");
    }

    #[test]
    fn test_length_string_span_rejects_invalid_utf8() {
        let bytes = Bytes::from_static(&[0x02, 0x00, 0x00, 0x00, 0xff, 0xfe]);
        let span = root_span(&bytes);
        assert!(length_string_span(span).is_err());
    }

    #[test]
    fn test_metadata_map_empty() {
        // Length 0, no entries
        let data = [0x00, 0x00, 0x00, 0x00];
        let result = metadata_map(&data);
        assert!(result.is_ok());
        let (_, map) = result.unwrap();
        assert_eq!(map.len(), 0);
    }

    #[test]
    fn test_metadata_map_single_entry() {
        // Map length: 14 bytes (4 + 3 + 4 + 3)
        // Key: length 3, "foo" (4 bytes for length + 3 bytes data)
        // Value: length 3, "bar" (4 bytes for length + 3 bytes data)
        let data = [
            0x0E, 0x00, 0x00, 0x00, // map length = 14
            0x03, 0x00, 0x00, 0x00, b'f', b'o', b'o', // key
            0x03, 0x00, 0x00, 0x00, b'b', b'a', b'r', // value
        ];
        let result = metadata_map(&data);
        assert!(result.is_ok());
        let (_, map) = result.unwrap();
        assert_eq!(map.len(), 1);
        let key = ByteStr::from("foo");
        assert_eq!(map.get(&key).map(|v| &**v), Some("bar"));
    }

    #[test]
    fn test_metadata_map_span_single_entry() {
        let bytes = Bytes::from_static(
            &[
                0x0E, 0x00, 0x00, 0x00, // map length = 14
                0x03, 0x00, 0x00, 0x00, b'f', b'o', b'o', // key
                0x03, 0x00, 0x00, 0x00, b'b', b'a', b'r', // value
            ][..],
        );
        let span = root_span(&bytes);
        let (_, map) = metadata_map_span(span).unwrap();
        assert_eq!(map.len(), 1);
        let key = ByteStr::from("foo");
        assert_eq!(map.get(&key).map(|v| &**v), Some("bar"));
    }

    #[test]
    fn test_parse_header_record() {
        // Profile: "ros2" (length 4)
        // Library: "" (length 0)
        // Metadata: empty (length 0)
        let data = [
            0x04, 0x00, 0x00, 0x00, b'r', b'o', b's', b'2', // profile
            0x00, 0x00, 0x00, 0x00, // library (empty)
            0x00, 0x00, 0x00, 0x00, // metadata (empty)
        ];
        let header = parse_header_record(Bytes::copy_from_slice(&data)).unwrap();
        assert_eq!(header.profile, "ros2");
        assert_eq!(header.library, "");
        assert_eq!(header.metadata.len(), 0);
    }

    #[test]
    fn test_parse_channel_record() {
        // Channel ID: 1
        // Schema ID: 2
        // Topic: "/test" (length 5)
        // Message encoding: "cdr" (length 3)
        // Metadata: empty (length 0)
        let data = [
            0x01, 0x00, // channel_id = 1
            0x02, 0x00, // schema_id = 2
            0x05, 0x00, 0x00, 0x00, b'/', b't', b'e', b's', b't', // topic
            0x03, 0x00, 0x00, 0x00, b'c', b'd', b'r', // message_encoding
            0x00, 0x00, 0x00, 0x00, // metadata (empty)
        ];
        let channel = parse_channel_record(Bytes::copy_from_slice(&data)).unwrap();
        assert_eq!(channel.id, 1);
        assert_eq!(channel.schema_id, 2);
        assert_eq!(channel.topic, "/test");
        assert_eq!(channel.message_encoding, "cdr");
    }

    #[test]
    fn test_parse_message_record_borrowed() {
        // Message record:
        // channel_id: 1 (u16)
        // sequence: 42 (u32)
        // log_time: 1000 (u64)
        // publish_time: 2000 (u64)
        // data: [0x01, 0x02, 0x03]
        let data = [
            0x01, 0x00, // channel_id = 1
            0x2A, 0x00, 0x00, 0x00, // sequence = 42
            0xE8, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // log_time = 1000
            0xD0, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // publish_time = 2000
            0x01, 0x02, 0x03, // data
        ];

        let msg = parse_message_record(Bytes::copy_from_slice(&data)).unwrap();
        assert_eq!(msg.channel_id, 1);
        assert_eq!(msg.sequence, 42);
        assert_eq!(msg.log_time, 1000);
        assert_eq!(msg.publish_time, 2000);
        assert_eq!(msg.data(), &[0x01, 0x02, 0x03]);
    }

    #[test]
    fn test_parse_message_record_owned() {
        // Message record with owned data
        let data = vec![
            0x02, 0x00, // channel_id = 2
            0x64, 0x00, 0x00, 0x00, // sequence = 100
            0x10, 0x27, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // log_time = 10000
            0x20, 0x4E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // publish_time = 20000
            0xAA, 0xBB, 0xCC, 0xDD, // data
        ];

        let msg = parse_message_record(Bytes::from(data)).unwrap();
        assert_eq!(msg.channel_id, 2);
        assert_eq!(msg.sequence, 100);
        assert_eq!(msg.log_time, 10000);
        assert_eq!(msg.publish_time, 20000);
        assert_eq!(msg.data(), &[0xAA, 0xBB, 0xCC, 0xDD]);
    }

    #[test]
    fn test_parse_message_record_empty_data() {
        // Message record with no data
        let data = [
            0x03, 0x00, // channel_id = 3
            0x01, 0x00, 0x00, 0x00, // sequence = 1
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // log_time = 0
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, // publish_time = 0
                  // no data
        ];

        let msg = parse_message_record(Bytes::copy_from_slice(&data)).unwrap();
        assert_eq!(msg.channel_id, 3);
        assert_eq!(msg.data_len(), 0);
    }

    #[test]
    fn test_parse_message_record_truncated() {
        // Message record that's too short (missing some header fields)
        let data = [
            0x01, 0x00, // channel_id = 1
            0x2A, 0x00, 0x00,
            0x00, // sequence = 42
                  // truncated - missing log_time and publish_time
        ];

        let result = parse_message_record(Bytes::copy_from_slice(&data));
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_message_index_record_two_entries() {
        let mut data = Vec::new();
        data.extend_from_slice(&5u16.to_le_bytes()); // channel_id
        data.extend_from_slice(&32u32.to_le_bytes()); // records_length
        data.extend_from_slice(&10u64.to_le_bytes()); // timestamp 1
        data.extend_from_slice(&100u64.to_le_bytes()); // offset 1
        data.extend_from_slice(&20u64.to_le_bytes()); // timestamp 2
        data.extend_from_slice(&200u64.to_le_bytes()); // offset 2

        let index = parse_message_index_record(Bytes::from(data)).unwrap();
        assert_eq!(index.channel_id, 5);
        assert_eq!(index.records.len(), 2);
        assert_eq!(index.records[0].timestamp, 10);
        assert_eq!(index.records[0].offset, 100);
        assert_eq!(index.records[1].timestamp, 20);
        assert_eq!(index.records[1].offset, 200);
    }

    #[test]
    fn test_parse_chunk_index_record_with_map() {
        let mut data = Vec::new();
        data.extend_from_slice(&1u64.to_le_bytes()); // message_start_time
        data.extend_from_slice(&2u64.to_le_bytes()); // message_end_time
        data.extend_from_slice(&3u64.to_le_bytes()); // chunk_start_offset
        data.extend_from_slice(&4u64.to_le_bytes()); // chunk_length

        // message_index_offsets map: 2 entries, 20 bytes total
        data.extend_from_slice(&20u32.to_le_bytes()); // map_byte_len
        data.extend_from_slice(&7u16.to_le_bytes());
        data.extend_from_slice(&50u64.to_le_bytes());
        data.extend_from_slice(&8u16.to_le_bytes());
        data.extend_from_slice(&60u64.to_le_bytes());

        data.extend_from_slice(&7u64.to_le_bytes()); // message_index_length

        // compression string "none"
        data.extend_from_slice(&4u32.to_le_bytes());
        data.extend_from_slice(b"none");

        data.extend_from_slice(&123u64.to_le_bytes()); // compressed_size (ignored)
        data.extend_from_slice(&456u64.to_le_bytes()); // uncompressed_size

        let index = parse_chunk_index_record(Bytes::from(data)).unwrap();
        assert_eq!(index.message_start_time, 1);
        assert_eq!(index.message_end_time, 2);
        assert_eq!(index.chunk_start_offset, 3);
        assert_eq!(index.chunk_length, 4);
        assert_eq!(index.compression, "none");
        assert_eq!(index.uncompressed_size, 456);
        assert_eq!(index.message_index_offsets.len(), 2);
        assert_eq!(index.message_index_offsets.get(&7), Some(&50));
        assert_eq!(index.message_index_offsets.get(&8), Some(&60));
        assert_eq!(index.message_index_length, 7);
    }

    #[test]
    fn test_parse_statistics_record_with_channel_counts() {
        // Statistics record fields:
        // message_count: u64
        // schema_count: u16
        // channel_count: u32
        // attachment_count: u32
        // metadata_count: u32
        // chunk_count: u32
        // message_start_time: u64
        // message_end_time: u64
        // channel_message_counts: len-prefixed bytes (u32 + repeated (u16,u64))
        let mut data = Vec::new();
        data.extend_from_slice(&1202840u64.to_le_bytes());
        data.extend_from_slice(&12u16.to_le_bytes());
        data.extend_from_slice(&32u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&14490u32.to_le_bytes());
        data.extend_from_slice(&1669703463001080535u64.to_le_bytes());
        data.extend_from_slice(&1669704213999570541u64.to_le_bytes());

        let mut counts = Vec::new();
        counts.extend_from_slice(&1u16.to_le_bytes());
        counts.extend_from_slice(&156658u64.to_le_bytes());
        counts.extend_from_slice(&2u16.to_le_bytes());
        counts.extend_from_slice(&37551u64.to_le_bytes());

        data.extend_from_slice(&(counts.len() as u32).to_le_bytes());
        data.extend_from_slice(&counts);

        let stats = parse_statistics_record(Bytes::from(data)).unwrap();
        assert_eq!(stats.message_count, 1202840);
        assert_eq!(stats.schema_count, 12);
        assert_eq!(stats.channel_count, 32);
        assert_eq!(stats.attachment_count, 0);
        assert_eq!(stats.metadata_count, 1);
        assert_eq!(stats.chunk_count, 14490);
        assert_eq!(stats.message_start_time, 1669703463001080535);
        assert_eq!(stats.message_end_time, 1669704213999570541);
        assert_eq!(stats.channel_message_counts.len(), 2);
        assert_eq!(stats.channel_message_counts[0].channel_id, 1);
        assert_eq!(stats.channel_message_counts[0].message_count, 156658);
        assert_eq!(stats.channel_message_counts[1].channel_id, 2);
        assert_eq!(stats.channel_message_counts[1].message_count, 37551);
    }

    #[test]
    fn test_parse_metadata_index_record_fields() {
        let mut data = Vec::new();
        data.extend_from_slice(&123u64.to_le_bytes()); // offset
        data.extend_from_slice(&456u64.to_le_bytes()); // length
        data.extend_from_slice(&4u32.to_le_bytes()); // name length
        data.extend_from_slice(b"meta");

        let idx = parse_metadata_index_record(Bytes::from(data)).unwrap();
        assert_eq!(idx.offset, 123);
        assert_eq!(idx.length, 456);
        assert_eq!(idx.name.as_str(), "meta");
    }

    #[test]
    fn test_parse_attachment_index_record_fields() {
        let mut data = Vec::new();
        data.extend_from_slice(&123u64.to_le_bytes()); // offset
        data.extend_from_slice(&456u64.to_le_bytes()); // length
        data.extend_from_slice(&1000u64.to_le_bytes()); // log_time
        data.extend_from_slice(&2000u64.to_le_bytes()); // create_time
        data.extend_from_slice(&999u64.to_le_bytes()); // data_size
        data.extend_from_slice(&4u32.to_le_bytes()); // name length
        data.extend_from_slice(b"file");
        data.extend_from_slice(&9u32.to_le_bytes()); // media_type length
        data.extend_from_slice(b"image/png");

        let idx = parse_attachment_index_record(Bytes::from(data)).unwrap();
        assert_eq!(idx.offset, 123);
        assert_eq!(idx.length, 456);
        assert_eq!(idx.log_time, 1000);
        assert_eq!(idx.create_time, 2000);
        assert_eq!(idx.data_size, 999);
        assert_eq!(idx.name.as_str(), "file");
        assert_eq!(idx.media_type.as_str(), "image/png");
    }
}
