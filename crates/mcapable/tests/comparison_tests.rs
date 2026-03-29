//! Comparison tests between mcap (upstream) and mcapable (our implementation).
//!
//! These tests create MCAP files using the upstream mcap crate and then validate
//! that mcapable parses them correctly.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::Arc;

/// Create a simple MCAP file with the upstream library.
fn create_simple_mcap(message_count: usize, message_data: &[u8]) -> Vec<u8> {
    let mut buffer = Vec::new();
    {
        let mut writer = mcap::WriteOptions::new()
            .compression(None)
            .create(Cursor::new(&mut buffer))
            .expect("Failed to create writer");

        let schema = Arc::new(mcap::Schema {
            id: 1,
            name: "TestSchema".to_string(),
            encoding: "raw".to_string(),
            data: Cow::Borrowed(b"{}"),
        });

        let channel = Arc::new(mcap::Channel {
            id: 1,
            topic: "/test".to_string(),
            message_encoding: "raw".to_string(),
            metadata: BTreeMap::new(),
            schema: Some(schema),
        });

        for i in 0..message_count {
            let message = mcap::Message {
                channel: channel.clone(),
                sequence: i as u32,
                log_time: 1000 + i as u64,
                publish_time: 1000 + i as u64,
                data: Cow::Borrowed(message_data),
            };
            writer.write(&message).expect("Failed to write message");
        }

        writer.finish().expect("Failed to finish");
    }
    buffer
}

/// Create MCAP with multiple channels.
fn create_multi_channel_mcap(channels: usize, messages_per_channel: usize) -> Vec<u8> {
    let mut buffer = Vec::new();
    {
        let mut writer = mcap::WriteOptions::new()
            .compression(None)
            .create(Cursor::new(&mut buffer))
            .expect("Failed to create writer");

        // Create schemas and channels
        let mut channel_arcs = Vec::new();
        for ch_id in 0..channels {
            let schema = Arc::new(mcap::Schema {
                id: (ch_id as u16) + 1,
                name: format!("Schema{}", ch_id),
                encoding: "raw".to_string(),
                data: Cow::Borrowed(b"{}"),
            });

            let channel = Arc::new(mcap::Channel {
                id: (ch_id as u16) + 1,
                topic: format!("/channel{}", ch_id),
                message_encoding: "raw".to_string(),
                metadata: BTreeMap::new(),
                schema: Some(schema),
            });
            channel_arcs.push(channel);
        }

        // Write messages interleaved across channels
        for msg_idx in 0..messages_per_channel {
            for (ch_idx, channel) in channel_arcs.iter().enumerate() {
                let msg_data = vec![ch_idx as u8];
                let message = mcap::Message {
                    channel: channel.clone(),
                    sequence: msg_idx as u32,
                    log_time: 1000 + (msg_idx * channels + ch_idx) as u64,
                    publish_time: 1000 + (msg_idx * channels + ch_idx) as u64,
                    data: Cow::Owned(msg_data),
                };
                writer.write(&message).expect("Failed to write message");
            }
        }

        writer.finish().expect("Failed to finish");
    }
    buffer
}

// ============================================================================
// Low-level parsing tests
// ============================================================================

mod record_parsing {
    use super::*;

    #[test]
    fn test_record_iterator_on_mcap_file() {
        let mcap_data = create_simple_mcap(3, &[0xAA, 0xBB, 0xCC]);
        let mut reader = mcapable::Reader::from_slice(&mcap_data).unwrap();
        let first = reader
            .record_metadata()
            .next()
            .expect("expected at least one record")
            .unwrap();
        assert_eq!(first.opcode, mcapable::Opcode::Header);
    }

    #[test]
    fn test_parse_channel_from_mcap() {
        let mcap_data = create_simple_mcap(1, &[0xAA]);
        let mut reader = mcapable::Reader::from_slice(&mcap_data).unwrap();
        let mut found_channel = false;
        for record in reader.records() {
            if let mcapable::Record::Channel(channel) = record.unwrap() {
                assert_eq!(channel.topic, "/test");
                assert_eq!(channel.message_encoding, "raw");
                found_channel = true;
                break;
            }
        }
        assert!(found_channel);
    }

    #[test]
    fn test_parse_schema_from_mcap() {
        let mcap_data = create_simple_mcap(1, &[0xAA]);
        let mut reader = mcapable::Reader::from_slice(&mcap_data).unwrap();
        let mut found_schema = false;
        for record in reader.records() {
            if let mcapable::Record::Schema(schema) = record.unwrap() {
                assert_eq!(schema.name, "TestSchema");
                assert_eq!(schema.encoding, "raw");
                found_schema = true;
                break;
            }
        }
        assert!(found_schema);
    }

    #[test]
    fn test_parse_footer_from_mcap() {
        let mcap_data = create_simple_mcap(1, &[0xAA]);
        let mut reader = mcapable::Reader::from_slice(&mcap_data).unwrap();
        let footer = reader.footer().unwrap().expect("expected footer");
        assert!(footer.summary_start > 0);
    }
}

// ============================================================================
// Chunk parsing tests
// ============================================================================

mod chunk_parsing {
    use super::*;

    #[test]
    fn test_parse_chunk_header() {
        let mcap_data = create_simple_mcap(3, &[0xAA]);
        let mut reader = mcapable::Reader::from_slice(&mcap_data).unwrap();
        let chunk = reader.chunks().next().expect("expected chunk").unwrap();
        assert!(chunk.message_start_time > 0);
        assert!(chunk.message_end_time >= chunk.message_start_time);
        assert!(chunk.uncompressed_size > 0);
        assert!(!chunk.records.is_empty());
    }

    #[test]
    fn test_iterate_chunk_contents() {
        let mcap_data = create_simple_mcap(3, &[0xAA, 0xBB]);
        let mut reader = mcapable::Reader::from_slice(&mcap_data).unwrap();
        let msg_count = reader.raw_messages().unwrap().count();
        assert_eq!(msg_count, 3);
        let channels = reader.channels();
        assert!(!channels.is_empty());
    }

    #[test]
    fn test_parse_messages_in_chunk() {
        let message_data = [0xDE, 0xAD, 0xBE, 0xEF];
        let mcap_data = create_simple_mcap(3, &message_data);
        let mut reader = mcapable::Reader::from_slice(&mcap_data).unwrap();
        let mut messages = Vec::new();
        for msg in reader.raw_messages().unwrap() {
            let msg = msg.unwrap();
            messages.push(msg);
        }
        assert_eq!(messages.len(), 3);
        for (i, msg) in messages.iter().enumerate() {
            assert_eq!(msg.sequence, i as u32);
            assert_eq!(msg.log_time, 1000 + i as u64);
            assert_eq!(msg.data_bytes().as_ref(), &message_data);
        }
    }
}

// ============================================================================
// Multi-channel tests
// ============================================================================

mod multi_channel {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_multi_channel_parsing() {
        let mcap_data = create_multi_channel_mcap(3, 5);
        let mut reader = mcapable::Reader::from_slice(&mcap_data).unwrap();
        let _ = reader.raw_messages().unwrap().count();
        let channels: HashMap<u16, mcapable::ByteStr> = reader
            .channels()
            .iter()
            .map(|(id, ch)| (*id, ch.topic.clone()))
            .collect();
        assert!(channels.len() >= 3);
    }

    #[test]
    fn test_message_channel_ids() {
        let mcap_data = create_multi_channel_mcap(3, 5);
        let mut reader = mcapable::Reader::from_slice(&mcap_data).unwrap();
        let mut channel_counts: HashMap<u16, usize> = HashMap::new();
        for msg in reader.raw_messages().unwrap() {
            let msg = msg.unwrap();
            *channel_counts.entry(msg.channel_id).or_insert(0) += 1;
        }
        for (_ch_id, count) in channel_counts {
            assert_eq!(count, 5);
        }
    }
}

// ============================================================================
// Comparison with mcap crate reading
// ============================================================================

mod comparison {
    use super::*;

    #[test]
    fn test_compare_message_counts() {
        let mcap_data = create_simple_mcap(10, &[1, 2, 3, 4, 5]);

        // Count messages with upstream mcap
        let upstream_count = mcap::MessageStream::new(&mcap_data).unwrap().count();

        let mut reader = mcapable::Reader::from_slice(&mcap_data).unwrap();
        let our_count = reader.raw_messages().unwrap().count();

        assert_eq!(upstream_count, 10);
        assert_eq!(our_count, upstream_count, "Message counts should match");
    }

    /// Owned version of parsed message for testing
    #[derive(Debug)]
    struct OwnedMessage {
        _channel_id: u16,
        sequence: u32,
        log_time: u64,
        publish_time: u64,
        data: Vec<u8>,
    }

    #[test]
    fn test_compare_message_data() {
        let message_data = vec![0xCA, 0xFE, 0xBA, 0xBE];
        let mcap_data = create_simple_mcap(5, &message_data);

        // Read with upstream mcap
        let upstream_messages: Vec<_> = mcap::MessageStream::new(&mcap_data)
            .unwrap()
            .map(|m| m.unwrap())
            .collect();

        let mut reader = mcapable::Reader::from_slice(&mcap_data).unwrap();
        let mut our_messages: Vec<OwnedMessage> = Vec::new();
        for msg in reader.raw_messages().unwrap() {
            let msg = msg.unwrap();
            our_messages.push(OwnedMessage {
                _channel_id: msg.channel_id,
                sequence: msg.sequence,
                log_time: msg.log_time,
                publish_time: msg.publish_time,
                data: msg.data_bytes().as_ref().to_vec(),
            });
        }

        // Compare
        assert_eq!(upstream_messages.len(), our_messages.len());

        for (upstream, ours) in upstream_messages.iter().zip(our_messages.iter()) {
            assert_eq!(upstream.sequence, ours.sequence, "Sequences should match");
            assert_eq!(upstream.log_time, ours.log_time, "Log times should match");
            assert_eq!(
                upstream.publish_time, ours.publish_time,
                "Publish times should match"
            );
            assert_eq!(upstream.data.as_ref(), &ours.data[..], "Data should match");
        }
    }
}
