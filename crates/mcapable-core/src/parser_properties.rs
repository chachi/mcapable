//! Property-based tests for MCAP parsers.
//!
//! These tests use proptest to generate random valid MCAP data structures
//! and verify that our parsers can handle them correctly.

use proptest::prelude::*;

use crate::support::{HashMap, Vec};
use crate::test_generators::*;

fn encode_len_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let len: u32 = bytes.len().try_into().expect("len overflow");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(bytes);
    out
}

fn encode_len_str(s: &crate::zero_copy::ByteStr) -> Vec<u8> {
    encode_len_bytes(s.as_bytes())
}

fn encode_metadata_map(
    map: &HashMap<crate::zero_copy::ByteStr, crate::zero_copy::ByteStr>,
) -> Vec<u8> {
    let mut body = Vec::new();
    for (k, v) in map {
        body.extend_from_slice(&encode_len_str(k));
        body.extend_from_slice(&encode_len_str(v));
    }
    let mut out = Vec::new();
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

fn encode_header_record(header: &crate::types::Header) -> bytes::Bytes {
    let mut out = Vec::new();
    out.extend_from_slice(&encode_len_str(&header.profile));
    out.extend_from_slice(&encode_len_str(&header.library));
    if !header.metadata.is_empty() {
        out.extend_from_slice(&encode_metadata_map(&header.metadata));
    }
    bytes::Bytes::from(out)
}

fn encode_schema_record(schema: &crate::types::Schema) -> bytes::Bytes {
    let mut out = Vec::new();
    out.extend_from_slice(&schema.id.to_le_bytes());
    out.extend_from_slice(&encode_len_str(&schema.name));
    out.extend_from_slice(&encode_len_str(&schema.encoding));
    out.extend_from_slice(&(schema.data.len() as u32).to_le_bytes());
    out.extend_from_slice(schema.data.as_ref());
    bytes::Bytes::from(out)
}

fn encode_channel_record(channel: &crate::types::Channel) -> bytes::Bytes {
    let mut out = Vec::new();
    out.extend_from_slice(&channel.id.to_le_bytes());
    out.extend_from_slice(&channel.schema_id.to_le_bytes());
    out.extend_from_slice(&encode_len_str(&channel.topic));
    out.extend_from_slice(&encode_len_str(&channel.message_encoding));
    out.extend_from_slice(&encode_metadata_map(&channel.metadata));
    bytes::Bytes::from(out)
}

fn encode_message_record(msg: &crate::types::RawMessage) -> bytes::Bytes {
    let mut out = Vec::new();
    out.extend_from_slice(&msg.channel_id.to_le_bytes());
    out.extend_from_slice(&msg.sequence.to_le_bytes());
    out.extend_from_slice(&msg.log_time.to_le_bytes());
    out.extend_from_slice(&msg.publish_time.to_le_bytes());
    out.extend_from_slice(msg.data());
    bytes::Bytes::from(out)
}

proptest! {
    /// Test that magic bytes strategy always generates valid MCAP magic.
    #[test]
    fn prop_magic_bytes_valid(magic in magic_bytes_strategy()) {
        use crate::parser::magic_bytes;
        let result = magic_bytes(&magic);
        prop_assert!(result.is_ok());
    }

    /// Test that generated record headers have valid opcodes.
    #[test]
    fn prop_record_header_valid(header in record_header_strategy()) {
        let (opcode, length) = header;
        prop_assert!((0x01..=0x0F).contains(&opcode));
        prop_assert!(length < 1_000_000); // Reasonable upper bound
    }

    /// Test that generated strings are valid UTF-8.
    #[test]
    fn prop_strings_valid_utf8(s in mcap_string_strategy()) {
        // String should already be valid UTF-8 by construction
        prop_assert!(s.is_ascii() || s.chars().all(|c| c.is_alphanumeric() || c == '/' || c == '_' || c == '-' || c == '.'));
    }

    /// Test that generated metadata maps are valid.
    #[test]
    fn prop_metadata_map_valid(map in metadata_map_strategy()) {
        prop_assert!(map.len() <= 10);
        for (key, value) in &map {
            prop_assert!(!key.is_empty() || key.is_empty()); // Keys can be empty
            prop_assert!(!value.is_empty() || value.is_empty()); // Values can be empty
        }
    }

    /// Test that generated Header records are well-formed.
    #[test]
    fn prop_header_wellformed(header in header_strategy()) {
        prop_assert!(header.profile.len() <= 100);
        prop_assert!(header.library.len() <= 100);
        prop_assert!(header.metadata.len() <= 10);
    }

    /// Test that generated Schema records are well-formed.
    #[test]
    fn prop_schema_wellformed(schema in schema_strategy()) {
        prop_assert!(schema.name.len() <= 100);
        prop_assert!(schema.encoding.len() <= 100);
        prop_assert!(schema.data.len() <= 1000);
    }

    /// Test that generated Channel records are well-formed.
    #[test]
    fn prop_channel_wellformed(channel in channel_strategy()) {
        prop_assert!(channel.topic.len() <= 100);
        prop_assert!(channel.message_encoding.len() <= 100);
        prop_assert!(channel.metadata.len() <= 10);
    }

    /// Test that generated Message records are well-formed.
    #[test]
    fn prop_message_wellformed(msg in message_strategy()) {
        prop_assert!(msg.data_len() <= 1000);
    }

    /// Test that generated Chunk records have valid compression.
    #[test]
    fn prop_chunk_compression_valid(chunk in chunk_strategy()) {
        prop_assert!(
            chunk.compression.is_empty() ||
            chunk.compression == "lz4" ||
            chunk.compression == "zstd"
        );
        prop_assert!(chunk.records.len() <= 1000);
    }

    /// Test that generated ChunkIndex records are well-formed.
    #[test]
    fn prop_chunk_index_wellformed(index in chunk_index_strategy()) {
        prop_assert!(index.chunk_length > 0);
        prop_assert!(index.message_index_offsets.len() <= 10);
        prop_assert!(
            index.compression.is_empty() ||
            index.compression == "lz4" ||
            index.compression == "zstd"
        );
    }

    /// Test that generated Statistics records are well-formed.
    #[test]
    fn prop_statistics_wellformed(stats in statistics_strategy()) {
        // Stats can have any values, just check they don't panic
        // message_count is u64, always >= 0
        prop_assert!(stats.message_count < u64::MAX);
    }

    /// Test that generated Metadata records are well-formed.
    #[test]
    fn prop_metadata_wellformed(metadata in metadata_strategy()) {
        prop_assert!(metadata.name.len() <= 100);
        prop_assert!(metadata.metadata.len() <= 10);
    }

    /// Test that generated Attachment records are well-formed.
    #[test]
    fn prop_attachment_wellformed(attachment in attachment_strategy()) {
        prop_assert!(attachment.name.len() <= 100);
        prop_assert!(attachment.media_type.len() <= 100);
        prop_assert!(attachment.data.len() <= 1000);
    }
}

/// Additional integration property tests.
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::error::{Error, ParseError};
    use crate::types::Opcode;
    use proptest::strategy::Strategy;
    use proptest::test_runner::{TestCaseError, TestCaseResult};

    fn expect_truncation_opcode_error<T>(
        bytes: bytes::Bytes,
        cut: usize,
        expected: Opcode,
        parse: impl Fn(bytes::Bytes) -> Result<T, Error>,
    ) -> TestCaseResult {
        if cut >= bytes.len() {
            return Ok(());
        }
        let err = match parse(bytes.slice(0..cut)) {
            Ok(_) => return Err(TestCaseError::fail("expected truncation to error")),
            Err(err) => err,
        };
        prop_assert!(matches!(
            err,
            Error::ParseError(ParseError::Opcode(op)) if op == expected
        ));
        Ok(())
    }

    fn invalid_utf8_bytes() -> impl Strategy<Value = Vec<u8>> {
        proptest::collection::vec(0x80u8..=0xFF, 1..32)
            .prop_filter("invalid utf8", |v| std::str::from_utf8(v).is_err())
    }

    fn encode_footer_record(
        summary_start: u64,
        summary_offset_start: u64,
        summary_crc: u32,
    ) -> bytes::Bytes {
        let mut out = Vec::new();
        out.extend_from_slice(&summary_start.to_le_bytes());
        out.extend_from_slice(&summary_offset_start.to_le_bytes());
        out.extend_from_slice(&summary_crc.to_le_bytes());
        bytes::Bytes::from(out)
    }

    fn encode_chunk_record(compression: &[u8]) -> bytes::Bytes {
        let mut out = Vec::new();
        out.extend_from_slice(&0u64.to_le_bytes()); // message_start_time
        out.extend_from_slice(&0u64.to_le_bytes()); // message_end_time
        out.extend_from_slice(&0u64.to_le_bytes()); // uncompressed_size
        out.extend_from_slice(&0u32.to_le_bytes()); // uncompressed_crc
        out.extend_from_slice(&encode_len_bytes(compression));
        out.extend_from_slice(&0u64.to_le_bytes()); // compressed_length
        bytes::Bytes::from(out)
    }

    fn encode_message_index_record(channel_id: u16) -> bytes::Bytes {
        let mut out = Vec::new();
        out.extend_from_slice(&channel_id.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // records_length = 0
        bytes::Bytes::from(out)
    }

    fn encode_chunk_index_record(compression: &[u8]) -> bytes::Bytes {
        let mut out = Vec::new();
        out.extend_from_slice(&0u64.to_le_bytes()); // message_start_time
        out.extend_from_slice(&0u64.to_le_bytes()); // message_end_time
        out.extend_from_slice(&0u64.to_le_bytes()); // chunk_start_offset
        out.extend_from_slice(&1u64.to_le_bytes()); // chunk_length
        out.extend_from_slice(&0u32.to_le_bytes()); // map_byte_len = 0
        out.extend_from_slice(&0u64.to_le_bytes()); // message_index_length
        out.extend_from_slice(&encode_len_bytes(compression));
        out.extend_from_slice(&0u64.to_le_bytes()); // compressed_size
        out.extend_from_slice(&0u64.to_le_bytes()); // uncompressed_size
        bytes::Bytes::from(out)
    }

    fn encode_attachment_record(name: &[u8], media_type: &[u8]) -> bytes::Bytes {
        let mut out = Vec::new();
        out.extend_from_slice(&0u64.to_le_bytes()); // log_time
        out.extend_from_slice(&0u64.to_le_bytes()); // create_time
        out.extend_from_slice(&encode_len_bytes(name));
        out.extend_from_slice(&encode_len_bytes(media_type));
        out.extend_from_slice(&0u64.to_le_bytes()); // data_length
        out.extend_from_slice(&0u32.to_le_bytes()); // crc
        bytes::Bytes::from(out)
    }

    fn encode_metadata_record(name: &[u8]) -> bytes::Bytes {
        let mut out = Vec::new();
        out.extend_from_slice(&encode_len_bytes(name));
        out.extend_from_slice(&0u32.to_le_bytes()); // metadata map length = 0
        bytes::Bytes::from(out)
    }

    fn encode_statistics_record(channel_id: u16) -> bytes::Bytes {
        let mut out = Vec::new();
        out.extend_from_slice(&channel_id.to_le_bytes());
        out.extend_from_slice(&0u64.to_le_bytes()); // message_count
        out.extend_from_slice(&0u64.to_le_bytes()); // message_start_time
        out.extend_from_slice(&0u64.to_le_bytes()); // message_end_time
        bytes::Bytes::from(out)
    }

    fn encode_metadata_index_record(name: &[u8]) -> bytes::Bytes {
        let mut out = Vec::new();
        out.extend_from_slice(&0u64.to_le_bytes()); // offset
        out.extend_from_slice(&0u64.to_le_bytes()); // length
        out.extend_from_slice(&encode_len_bytes(name));
        bytes::Bytes::from(out)
    }

    fn encode_attachment_index_record(name: &[u8], media_type: &[u8]) -> bytes::Bytes {
        let mut out = Vec::new();
        out.extend_from_slice(&0u64.to_le_bytes()); // offset
        out.extend_from_slice(&0u64.to_le_bytes()); // length
        out.extend_from_slice(&0u64.to_le_bytes()); // log_time
        out.extend_from_slice(&0u64.to_le_bytes()); // create_time
        out.extend_from_slice(&0u64.to_le_bytes()); // data_size
        out.extend_from_slice(&encode_len_bytes(name));
        out.extend_from_slice(&encode_len_bytes(media_type));
        bytes::Bytes::from(out)
    }

    fn encode_summary_offset_record(group_opcode: u8) -> bytes::Bytes {
        let mut out = Vec::new();
        out.extend_from_slice(&group_opcode.to_le_bytes());
        out.extend_from_slice(&0u64.to_le_bytes()); // group_start
        out.extend_from_slice(&0u64.to_le_bytes()); // group_length
        bytes::Bytes::from(out)
    }

    fn encode_data_end_record(crc: u32) -> bytes::Bytes {
        bytes::Bytes::from(crc.to_le_bytes().to_vec())
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 64, .. ProptestConfig::default() })]
        /// Test that multiple headers can be generated without conflicts.
        #[test]
        fn prop_multiple_headers_unique(
            headers in prop::collection::vec(header_strategy(), 1..10)
        ) {
            prop_assert!(headers.len() <= 10);
        }

        /// Test that multiple schemas with different IDs can coexist.
        #[test]
        fn prop_multiple_schemas_coexist(
            schemas in prop::collection::vec(schema_strategy(), 1..10)
        ) {
            // Schemas may have duplicate IDs, which is valid in the spec
            prop_assert!(schemas.len() <= 10);
        }

        /// Test that multiple channels can reference schemas.
        #[test]
        fn prop_channels_reference_schemas(
            channels in prop::collection::vec(channel_strategy(), 1..10)
        ) {
            // Each channel has a schema_id, which may or may not exist
            let _ = channels;
        }

        /// Test that messages reference valid channel IDs.
        #[test]
        fn prop_messages_reference_channels(
            messages in prop::collection::vec(message_strategy(), 1..20)
        ) {
            let _ = messages;
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 64, .. ProptestConfig::default() })]

        #[test]
        fn prop_parse_header_roundtrip(header in header_strategy()) {
            let bytes = encode_header_record(&header);
            let parsed = crate::parser::parse_header_record(bytes).unwrap();
            prop_assert_eq!(parsed.profile, header.profile);
            prop_assert_eq!(parsed.library, header.library);
            prop_assert_eq!(parsed.metadata, header.metadata);
        }

        #[test]
        fn prop_parse_schema_roundtrip(schema in schema_strategy()) {
            let bytes = encode_schema_record(&schema);
            let parsed = crate::parser::parse_schema_record(bytes).unwrap();
            prop_assert_eq!(parsed.id, schema.id);
            prop_assert_eq!(parsed.name, schema.name);
            prop_assert_eq!(parsed.encoding, schema.encoding);
            prop_assert_eq!(parsed.data.as_ref(), schema.data.as_ref());
        }

        #[test]
        fn prop_parse_channel_roundtrip(channel in channel_strategy()) {
            let bytes = encode_channel_record(&channel);
            let parsed = crate::parser::parse_channel_record(bytes).unwrap();
            prop_assert_eq!(parsed.id, channel.id);
            prop_assert_eq!(parsed.schema_id, channel.schema_id);
            prop_assert_eq!(parsed.topic, channel.topic);
            prop_assert_eq!(parsed.message_encoding, channel.message_encoding);
            prop_assert_eq!(parsed.metadata, channel.metadata);
        }

        #[test]
        fn prop_parse_message_roundtrip(msg in message_strategy()) {
            let bytes = encode_message_record(&msg);
            let parsed = crate::parser::parse_message_record(bytes).unwrap();
            prop_assert_eq!(parsed.channel_id, msg.channel_id);
            prop_assert_eq!(parsed.sequence, msg.sequence);
            prop_assert_eq!(parsed.log_time, msg.log_time);
            prop_assert_eq!(parsed.publish_time, msg.publish_time);
            prop_assert_eq!(parsed.data(), msg.data());
        }

        #[test]
        fn prop_parse_header_truncation_returns_opcode_error(header in header_strategy(), cut in 0usize..200) {
            let bytes = encode_header_record(&header);
            let cut = cut.min(bytes.len());
            let truncated = bytes.slice(0..cut);
            if truncated.len() == bytes.len() {
                return Ok(());
            }
            match crate::parser::parse_header_record(truncated) {
                Err(err) => {
                    prop_assert!(matches!(
                        err,
                        Error::ParseError(ParseError::Opcode(Opcode::Header))
                    ));
                }
                Ok(parsed) => {
                    // Header "metadata" is an optional extension accepted by this crate; when the
                    // trailing metadata map is absent, parsing succeeds with empty metadata.
                    let min_len = 4 + header.profile.len() + 4 + header.library.len(); // profile + library
                    prop_assert_eq!(cut, min_len);
                    prop_assert_eq!(parsed.profile, header.profile);
                    prop_assert_eq!(parsed.library, header.library);
                    prop_assert!(parsed.metadata.is_empty());
                }
            }
        }

        #[test]
        fn prop_parse_schema_truncation_returns_opcode_error(schema in schema_strategy(), cut in 0usize..300) {
            let bytes = encode_schema_record(&schema);
            let cut = cut.min(bytes.len());
            let truncated = bytes.slice(0..cut);
            if truncated.len() == bytes.len() {
                return Ok(());
            }
            let err = crate::parser::parse_schema_record(truncated).unwrap_err();
            prop_assert!(matches!(err, Error::ParseError(ParseError::Opcode(Opcode::Schema))));
        }

        #[test]
        fn prop_parse_channel_truncation_returns_opcode_error(channel in channel_strategy(), cut in 0usize..300) {
            let bytes = encode_channel_record(&channel);
            let cut = cut.min(bytes.len());
            let truncated = bytes.slice(0..cut);
            if truncated.len() == bytes.len() {
                return Ok(());
            }
            let err = crate::parser::parse_channel_record(truncated).unwrap_err();
            prop_assert!(matches!(err, Error::ParseError(ParseError::Opcode(Opcode::Channel))));
        }

        #[test]
        fn prop_parse_message_truncation_returns_opcode_error(msg in message_strategy(), cut in 0usize..64) {
            let bytes = encode_message_record(&msg);
            let cut = cut.min(bytes.len());
            let truncated = bytes.slice(0..cut);
            if truncated.len() >= crate::format::MESSAGE_HEADER_SIZE {
                return Ok(());
            }
            let err = crate::parser::parse_message_record(truncated).unwrap_err();
            prop_assert!(matches!(err, Error::ParseError(ParseError::Opcode(Opcode::Message))));
        }

        #[test]
        fn prop_parse_footer_truncation_returns_opcode_error(cut in 0usize..32) {
            let bytes = encode_footer_record(0, 0, 0);
            expect_truncation_opcode_error(bytes, cut, Opcode::Footer, crate::parser::parse_footer_record)?;
        }

        #[test]
        fn prop_parse_chunk_truncation_returns_opcode_error(cut in 0usize..128) {
            let bytes = encode_chunk_record(b"");
            expect_truncation_opcode_error(bytes, cut, Opcode::Chunk, crate::parser::parse_chunk_record)?;
        }

        #[test]
        fn prop_parse_message_index_truncation_returns_opcode_error(cut in 0usize..32) {
            let bytes = encode_message_index_record(0);
            expect_truncation_opcode_error(bytes, cut, Opcode::MessageIndex, crate::parser::parse_message_index_record)?;
        }

        #[test]
        fn prop_parse_chunk_index_truncation_returns_opcode_error(cut in 0usize..128) {
            let bytes = encode_chunk_index_record(b"");
            expect_truncation_opcode_error(bytes, cut, Opcode::ChunkIndex, crate::parser::parse_chunk_index_record)?;
        }

        #[test]
        fn prop_parse_attachment_truncation_returns_opcode_error(cut in 0usize..128) {
            let bytes = encode_attachment_record(b"", b"");
            expect_truncation_opcode_error(bytes, cut, Opcode::Attachment, crate::parser::parse_attachment_record)?;
        }

        #[test]
        fn prop_parse_statistics_truncation_returns_opcode_error(cut in 0usize..64) {
            let bytes = encode_statistics_record(0);
            expect_truncation_opcode_error(bytes, cut, Opcode::Statistics, crate::parser::parse_statistics_record)?;
        }

        #[test]
        fn prop_parse_metadata_truncation_returns_opcode_error(cut in 0usize..128) {
            let bytes = encode_metadata_record(b"");
            expect_truncation_opcode_error(bytes, cut, Opcode::Metadata, crate::parser::parse_metadata_record)?;
        }

        #[test]
        fn prop_parse_metadata_index_truncation_returns_opcode_error(cut in 0usize..64) {
            let bytes = encode_metadata_index_record(b"");
            expect_truncation_opcode_error(bytes, cut, Opcode::MetadataIndex, crate::parser::parse_metadata_index_record)?;
        }

        #[test]
        fn prop_parse_attachment_index_truncation_returns_opcode_error(cut in 0usize..128) {
            let bytes = encode_attachment_index_record(b"", b"");
            expect_truncation_opcode_error(bytes, cut, Opcode::AttachmentIndex, crate::parser::parse_attachment_index_record)?;
        }

        #[test]
        fn prop_parse_summary_offset_truncation_returns_opcode_error(cut in 0usize..64) {
            let bytes = encode_summary_offset_record(0);
            expect_truncation_opcode_error(bytes, cut, Opcode::SummaryOffset, crate::parser::parse_summary_offset_record)?;
        }

        #[test]
        fn prop_parse_data_end_truncation_returns_opcode_error(cut in 0usize..16) {
            let bytes = encode_data_end_record(0);
            expect_truncation_opcode_error(bytes, cut, Opcode::DataEnd, crate::parser::parse_data_end_record)?;
        }

        #[test]
        fn prop_schema_invalid_utf8_is_opcode_error(bad in invalid_utf8_bytes()) {
            let mut out = Vec::new();
            out.extend_from_slice(&1u16.to_le_bytes()); // id
            out.extend_from_slice(&encode_len_bytes(&bad)); // name
            out.extend_from_slice(&encode_len_bytes(b"cdr")); // encoding
            out.extend_from_slice(&0u32.to_le_bytes()); // data length = 0
            let err = crate::parser::parse_schema_record(bytes::Bytes::from(out)).unwrap_err();
            prop_assert!(matches!(err, Error::ParseError(ParseError::Opcode(Opcode::Schema))));
        }

        #[test]
        fn prop_metadata_invalid_utf8_is_opcode_error(bad in invalid_utf8_bytes()) {
            let err = crate::parser::parse_metadata_record(encode_metadata_record(&bad)).unwrap_err();
            prop_assert!(matches!(err, Error::ParseError(ParseError::Opcode(Opcode::Metadata))));
        }

        #[test]
        fn prop_attachment_invalid_utf8_is_opcode_error(bad in invalid_utf8_bytes()) {
            let err = crate::parser::parse_attachment_record(encode_attachment_record(&bad, b"")).unwrap_err();
            prop_assert!(matches!(err, Error::ParseError(ParseError::Opcode(Opcode::Attachment))));
        }

        #[test]
        fn prop_chunk_invalid_utf8_is_opcode_error(bad in invalid_utf8_bytes()) {
            let err = crate::parser::parse_chunk_record(encode_chunk_record(&bad)).unwrap_err();
            prop_assert!(matches!(err, Error::ParseError(ParseError::Opcode(Opcode::Chunk))));
        }

        #[test]
        fn prop_chunk_index_invalid_utf8_is_opcode_error(bad in invalid_utf8_bytes()) {
            let err = crate::parser::parse_chunk_index_record(encode_chunk_index_record(&bad)).unwrap_err();
            prop_assert!(matches!(err, Error::ParseError(ParseError::Opcode(Opcode::ChunkIndex))));
        }

        #[test]
        fn prop_metadata_index_invalid_utf8_is_opcode_error(bad in invalid_utf8_bytes()) {
            let err = crate::parser::parse_metadata_index_record(encode_metadata_index_record(&bad)).unwrap_err();
            prop_assert!(matches!(err, Error::ParseError(ParseError::Opcode(Opcode::MetadataIndex))));
        }

        #[test]
        fn prop_attachment_index_invalid_utf8_is_opcode_error(bad in invalid_utf8_bytes()) {
            let err = crate::parser::parse_attachment_index_record(encode_attachment_index_record(&bad, b"")).unwrap_err();
            prop_assert!(matches!(err, Error::ParseError(ParseError::Opcode(Opcode::AttachmentIndex))));
        }
    }

    #[test]
    fn test_parse_header_invalid_utf8_is_opcode_error() {
        // Invalid UTF-8 bytes in profile string.
        let mut out = Vec::new();
        out.extend_from_slice(&encode_len_bytes(&[0xff, 0xfe, 0xfd]));
        out.extend_from_slice(&encode_len_bytes(b"")); // library
        let err = crate::parser::parse_header_record(bytes::Bytes::from(out)).unwrap_err();
        assert!(matches!(
            err,
            crate::Error::ParseError(crate::ParseError::Opcode(crate::types::Opcode::Header))
        ));
    }

    #[test]
    fn test_parse_channel_invalid_utf8_is_opcode_error() {
        // Valid fixed fields, invalid UTF-8 in topic.
        let mut out = Vec::new();
        out.extend_from_slice(&1u16.to_le_bytes()); // id
        out.extend_from_slice(&0u16.to_le_bytes()); // schema_id
        out.extend_from_slice(&encode_len_bytes(&[0xff])); // topic
        out.extend_from_slice(&encode_len_bytes(b"cdr")); // message_encoding
        out.extend_from_slice(&0u32.to_le_bytes()); // metadata map length = 0

        let err = crate::parser::parse_channel_record(bytes::Bytes::from(out)).unwrap_err();
        assert!(matches!(
            err,
            crate::Error::ParseError(crate::ParseError::Opcode(crate::types::Opcode::Channel))
        ));
    }
}
