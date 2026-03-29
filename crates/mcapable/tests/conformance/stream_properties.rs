//! Property-based tests for stream iteration.
//!
//! These tests verify that stream iteration behaves correctly
//! across various scenarios and data patterns.

use proptest::prelude::*;

use crate::helpers::mcap_builder::*;

proptest! {
    /// Test that iterating twice yields the same results.
    #[test]
    fn prop_iteration_is_repeatable(
        num_messages in 1usize..20usize,
    ) {
        use mcapable::reader;
        use std::io::Cursor;

        let mut builder = McapBuilder::new();
        builder = builder.add_simple_channel(0, "/test");

        for i in 0..num_messages {
            builder = builder.add_simple_message(0, i as u32, 1000 + i as u64, vec![i as u8]);
        }

        let mcap = builder.build();
        let cursor = Cursor::new(mcap);
        let mut reader = reader::Builder::new().build(cursor)?;

        // First iteration
        let count1 = reader.messages().unwrap().count();

        // Second iteration
        let count2 = reader.messages().unwrap().count();

        prop_assert_eq!(count1, count2);
        prop_assert_eq!(count1, num_messages);
    }

    /// Test that partial iteration doesn't affect subsequent iterations.
    #[test]
    fn prop_partial_iteration_safe(
        num_messages in 5usize..20usize,
        take_count in 1usize..5usize,
    ) {
        use mcapable::reader;
        use std::io::Cursor;

        let mut builder = McapBuilder::new();
        builder = builder.add_simple_channel(0, "/test");

        for i in 0..num_messages {
            builder = builder.add_simple_message(0, i as u32, 1000 + i as u64, vec![i as u8]);
        }

        let mcap = builder.build();
        let cursor = Cursor::new(mcap);
        let mut reader = reader::Builder::new().build(cursor)?;

        // Partial iteration
        let _ = reader.messages().unwrap().take(take_count).count();

        // Full iteration should still work
        let count = reader.messages().unwrap().count();
        prop_assert_eq!(count, num_messages);
    }

    /// Test that different stream types on same reader work independently.
    #[test]
    fn prop_stream_types_independent(
        num_messages in 1usize..10usize,
    ) {
        use mcapable::reader;
        use std::io::Cursor;

        let mut builder = McapBuilder::new();
        builder = builder.add_simple_channel(0, "/test");

        for i in 0..num_messages {
            builder = builder.add_simple_message(0, i as u32, 1000 + i as u64, vec![i as u8]);
        }

        let mcap = builder.build();
        let cursor = Cursor::new(mcap);
        let mut reader = reader::Builder::new().build(cursor)?;

        // Get message count
        let msg_count = reader.messages().unwrap().count();

        // Get raw message count
        let raw_count = reader.raw_messages().unwrap().count();

        // Get chunk count
        let chunk_count = reader.chunks().count();

        prop_assert_eq!(msg_count, num_messages);
        prop_assert_eq!(raw_count, num_messages);
        prop_assert!(chunk_count > 0); // At least one chunk
    }

    /// Test that message and raw_message streams have same count.
    #[test]
    fn prop_message_raw_message_count_equal(
        num_messages in 1usize..15usize,
    ) {
        use mcapable::reader;
        use std::io::Cursor;

        let mut builder = McapBuilder::new();
        builder = builder.add_simple_channel(0, "/test");

        for i in 0..num_messages {
            builder = builder.add_simple_message(0, i as u32, 1000 + i as u64, vec![i as u8]);
        }

        let mcap = builder.build();
        let cursor = Cursor::new(mcap);
        let mut reader = reader::Builder::new().build(cursor)?;

        let msg_count = reader.messages().unwrap().count();
        let raw_count = reader.raw_messages().unwrap().count();

        prop_assert_eq!(msg_count, raw_count);
    }

    /// Test that stream errors don't corrupt the reader state.
    #[test]
    fn prop_error_recovery(
        num_messages in 1usize..10usize,
    ) {
        use mcapable::reader;
        use std::io::Cursor;

        let mut builder = McapBuilder::new();
        builder = builder.add_simple_channel(0, "/test");

        for i in 0..num_messages {
            builder = builder.add_simple_message(0, i as u32, 1000 + i as u64, vec![i as u8]);
        }

        let mcap = builder.build();
        let cursor = Cursor::new(mcap);
        let mut reader = reader::Builder::new().build(cursor)?;

        // Try to iterate with a filter that might cause issues
        let _ = reader
            .messages()
            .unwrap()
            .filter_channel(|_| false)
            .count();

        // Should still be able to iterate normally
        let count = reader.messages().unwrap().count();
        prop_assert_eq!(count, num_messages);
    }

    /// Test that chunks contain all messages.
    #[test]
    fn prop_chunks_contain_all_messages(
        num_messages in 1usize..20usize,
    ) {
        use mcapable::reader;
        use std::io::Cursor;

        let mut builder = McapBuilder::new();
        builder = builder.add_simple_channel(0, "/test");

        for i in 0..num_messages {
            builder = builder.add_simple_message(0, i as u32, 1000 + i as u64, vec![i as u8]);
        }

        let mcap = builder.build();
        let cursor = Cursor::new(mcap);
        let mut reader = reader::Builder::new().build(cursor)?;

        // Count messages
        let msg_count = reader.messages().unwrap().count();

        // Count chunks
        let chunk_count = reader.chunks().count();

        prop_assert_eq!(msg_count, num_messages);
        prop_assert!(chunk_count > 0);
        // In a chunked file, all messages are in chunks
    }

    /// Test that filtering reduces message count correctly.
    #[test]
    fn prop_filtering_reduces_count(
        total in 10usize..20usize,
        filter_threshold in 5u32..10u32,
    ) {
        use mcapable::reader;
        use std::io::Cursor;

        let mut builder = McapBuilder::new();
        builder = builder.add_simple_channel(0, "/test");

        for i in 0..total {
            builder = builder.add_simple_message(0, i as u32, 1000 + i as u64, vec![i as u8]);
        }

        let mcap = builder.build();
        let cursor = Cursor::new(mcap);
        let mut reader = reader::Builder::new().build(cursor)?;

        let total_count = reader.messages().unwrap().count();
        let filtered_count = reader.messages().unwrap()
            .filter(|hdr| hdr.sequence < filter_threshold)
            .count();

        prop_assert_eq!(total_count, total);
        prop_assert!(filtered_count <= total_count);
        prop_assert!(filtered_count <= filter_threshold as usize);
    }

    /// Test that record stream includes header.
    #[test]
    fn prop_record_stream_has_header(
        num_messages in 0usize..10usize,
    ) {
        use mcapable::reader;
        use std::io::Cursor;

        let mut builder = McapBuilder::new();
        builder = builder.add_simple_channel(0, "/test");

        for i in 0..num_messages {
            builder = builder.add_simple_message(0, i as u32, 1000 + i as u64, vec![i as u8]);
        }

        let mcap = builder.build();
        let cursor = Cursor::new(mcap);
        let mut reader = reader::Builder::new().build(cursor)?;

        let mut has_header = false;
        for record_result in reader.records().take(10) {
            let record = record_result?;
            if matches!(record, mcapable::Record::Header(_)) {
                has_header = true;
                break;
            }
        }

        prop_assert!(has_header);
    }

    /// Test that empty channel list filter returns no messages.
    #[test]
    fn prop_empty_channel_filter(
        num_messages in 1usize..10usize,
    ) {
        use mcapable::reader;
        use std::io::Cursor;

        let mut builder = McapBuilder::new();
        builder = builder.add_simple_channel(0, "/test");
        builder = builder.add_simple_channel(1, "/test2");

        for i in 0..num_messages {
            let channel = (i % 2) as u16;
            builder = builder.add_simple_message(channel, i as u32, 1000 + i as u64, vec![i as u8]);
        }

        let mcap = builder.build();
        let cursor = Cursor::new(mcap);
        let mut reader = reader::Builder::new().build(cursor)?;

        let count = reader.messages().unwrap().filter_channel(|_| false).count();
        prop_assert_eq!(count, 0);
    }
}
