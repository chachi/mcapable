use bytes::Bytes;
use mcapable_core::reader;
use mcapable_core::types::{Metadata, Record};
use mcapable_core::writer::rolling::*;
use mcapable_core::writer::{ChannelSpec, SchemaSpec};
use mcapable_core::writer::{ChunkOptions, WriterBuilder};
use mcapable_core::zero_copy::ByteStr;
use std::io::Cursor;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Shared-state factory that stores each finished file's bytes.
struct CollectingFactory {
    files: Arc<std::sync::Mutex<Vec<Vec<u8>>>>,
}

impl CollectingFactory {
    fn new() -> (Self, Arc<std::sync::Mutex<Vec<Vec<u8>>>>) {
        let files = Arc::new(std::sync::Mutex::new(Vec::new()));
        (
            Self {
                files: files.clone(),
            },
            files,
        )
    }
}

impl SinkFactory for CollectingFactory {
    type Sink = SharedCursor;

    fn create_sink(&mut self, _context: &SplitContext) -> std::io::Result<Self::Sink> {
        Ok(SharedCursor::new(self.files.clone()))
    }
}

/// A cursor that, on drop, pushes its data into a shared collection.
struct SharedCursor {
    cursor: Cursor<Vec<u8>>,
    files: Arc<std::sync::Mutex<Vec<Vec<u8>>>>,
}

impl SharedCursor {
    fn new(files: Arc<std::sync::Mutex<Vec<Vec<u8>>>>) -> Self {
        Self {
            cursor: Cursor::new(Vec::new()),
            files,
        }
    }
}

impl std::io::Write for SharedCursor {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.cursor.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.cursor.flush()
    }
}

impl std::io::Seek for SharedCursor {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.cursor.seek(pos)
    }
}

impl Drop for SharedCursor {
    fn drop(&mut self) {
        let data = std::mem::take(self.cursor.get_mut());
        if !data.is_empty() {
            self.files.lock().unwrap().push(data);
        }
    }
}

fn validate_mcap(data: &[u8]) -> reader::Reader<mcapable_core::source::BytesCursor> {
    let mut reader = reader::Reader::from_slice(data).unwrap();
    // Force loading header and summary to validate structure
    let _header = reader.header().unwrap();
    reader
}

fn count_messages(data: &[u8]) -> usize {
    let mut reader = reader::Reader::from_slice(data).unwrap();
    reader.raw_messages().unwrap().count()
}

// ──────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────

#[test]
fn single_file_no_split() {
    let (factory, files) = CollectingFactory::new();
    let trigger = MessageCount::new(1000); // won't fire

    let mut rolling = RollingWriterBuilder::new(factory, trigger)
        .writer_builder(WriterBuilder::new().profile("test"))
        .build()
        .unwrap();

    let mut ch = rolling
        .add_channel(ChannelSpec::new("/topic", "json"))
        .unwrap();

    for i in 0..5 {
        ch.write(i * 1000, i * 1000, &b"data"[..]).unwrap();
    }
    drop(ch);
    rolling.finish().unwrap();
    drop(rolling);

    let files = files.lock().unwrap();
    assert_eq!(files.len(), 1, "Should produce exactly one file");

    let mut reader = validate_mcap(&files[0]);
    let msg_count = reader.raw_messages().unwrap().count();
    assert_eq!(msg_count, 5);
}

#[test]
fn message_count_trigger_splits() {
    let (factory, files) = CollectingFactory::new();
    let trigger = MessageCount::new(3);

    let mut rolling = RollingWriterBuilder::new(factory, trigger)
        .writer_builder(WriterBuilder::new().profile("test"))
        .build()
        .unwrap();

    let mut ch = rolling
        .add_channel(ChannelSpec::new("/topic", "json"))
        .unwrap();

    // Write 10 messages. With max 3 per file:
    // File 0: msgs 0,1,2 (split after msg 2)
    // File 1: msgs 3,4,5 (split after msg 5)
    // File 2: msgs 6,7,8 (split after msg 8)
    // File 3: msg 9
    for i in 0..10u64 {
        ch.write(i * 1000, i * 1000, &b"data"[..]).unwrap();
    }
    drop(ch);
    rolling.finish().unwrap();
    drop(rolling);

    let files = files.lock().unwrap();
    assert_eq!(files.len(), 4, "Should produce 4 files");

    // Verify total messages
    let total: usize = files.iter().map(|f| count_messages(f)).sum();
    assert_eq!(total, 10);

    // First 3 files should have 3 messages each
    assert_eq!(count_messages(&files[0]), 3);
    assert_eq!(count_messages(&files[1]), 3);
    assert_eq!(count_messages(&files[2]), 3);
    // Last file has 1 message
    assert_eq!(count_messages(&files[3]), 1);

    // All files should be valid MCAPs
    for f in files.iter() {
        validate_mcap(f);
    }
}

#[test]
fn schemas_and_channels_in_every_file() {
    let (factory, files) = CollectingFactory::new();
    let trigger = MessageCount::new(2);

    let mut rolling = RollingWriterBuilder::new(factory, trigger)
        .writer_builder(WriterBuilder::new().profile("test"))
        .build()
        .unwrap();

    let mut ch = rolling
        .add_channel(ChannelSpec::new("/lidar", "cdr").schema(SchemaSpec::new(
            "PointCloud2",
            "ros2msg",
            Bytes::from_static(b"schema-data"),
        )))
        .unwrap();

    for i in 0..6u64 {
        ch.write(i * 1000, i * 1000, &b"msg"[..]).unwrap();
    }
    drop(ch);
    rolling.finish().unwrap();
    drop(rolling);

    let files = files.lock().unwrap();
    assert_eq!(files.len(), 3);

    for (i, f) in files.iter().enumerate() {
        let mut reader = validate_mcap(f);

        // Load schemas and channels by reading messages
        let _msgs: Vec<_> = reader.messages().unwrap().collect();
        let schemas = reader.schemas();
        let channels = reader.channels();

        assert!(!schemas.is_empty(), "File {i} should have schemas");
        assert!(!channels.is_empty(), "File {i} should have channels");

        // Verify the schema name matches
        let schema = schemas.values().next().unwrap();
        assert_eq!(schema.name, ByteStr::from("PointCloud2"));

        // Verify the channel topic matches
        let channel = channels.values().next().unwrap();
        assert_eq!(channel.topic, ByteStr::from("/lidar"));
    }
}

#[test]
fn channel_ids_stable_across_files() {
    let (factory, files) = CollectingFactory::new();
    let trigger = MessageCount::new(2);

    let mut rolling = RollingWriterBuilder::new(factory, trigger)
        .writer_builder(WriterBuilder::new().profile("test"))
        .build()
        .unwrap();

    let mut ch1 = rolling
        .add_channel(ChannelSpec::new("/topic_a", "json"))
        .unwrap();
    let mut ch2 = rolling
        .add_channel(ChannelSpec::new("/topic_b", "json"))
        .unwrap();

    let id1 = ch1.channel_id();
    let id2 = ch2.channel_id();

    for i in 0..6u64 {
        ch1.write(i * 1000, i * 1000, &b"a"[..]).unwrap();
        ch2.write(i * 1000, i * 1000, &b"b"[..]).unwrap();
    }
    drop(ch1);
    drop(ch2);
    rolling.finish().unwrap();
    drop(rolling);

    let files = files.lock().unwrap();
    assert!(files.len() >= 2, "Should have multiple files");

    // Every file should have channels with the same IDs
    for f in files.iter() {
        let mut reader = validate_mcap(f);
        let _msgs: Vec<_> = reader.messages().unwrap().collect();
        let channels = reader.channels();

        let ch_a = channels.get(&id1).expect("channel id1 must exist");
        assert_eq!(ch_a.topic, ByteStr::from("/topic_a"));

        let ch_b = channels.get(&id2).expect("channel id2 must exist");
        assert_eq!(ch_b.topic, ByteStr::from("/topic_b"));
    }
}

#[test]
fn sequence_numbers_monotonic_across_files() {
    let (factory, files) = CollectingFactory::new();
    let trigger = MessageCount::new(3);

    let mut rolling = RollingWriterBuilder::new(factory, trigger)
        .writer_builder(WriterBuilder::new().profile("test"))
        .build()
        .unwrap();

    let mut ch = rolling
        .add_channel(ChannelSpec::new("/topic", "json"))
        .unwrap();

    for i in 0..9u64 {
        ch.write(i * 1000, i * 1000, &b"data"[..]).unwrap();
    }
    drop(ch);
    rolling.finish().unwrap();
    drop(rolling);

    let files = files.lock().unwrap();
    let mut all_sequences: Vec<u32> = Vec::new();

    for f in files.iter() {
        let mut reader = validate_mcap(f);
        for msg in reader.raw_messages().unwrap() {
            let msg = msg.unwrap();
            all_sequences.push(msg.sequence);
        }
    }

    // Sequences should be 0, 1, 2, 3, 4, 5, 6, 7, 8
    let expected: Vec<u32> = (0..9).collect();
    assert_eq!(all_sequences, expected);
}

#[test]
fn force_split() {
    let (factory, files) = CollectingFactory::new();
    let trigger = MessageCount::new(1000); // won't fire automatically

    let mut rolling = RollingWriterBuilder::new(factory, trigger)
        .writer_builder(WriterBuilder::new().profile("test"))
        .build()
        .unwrap();

    let mut ch = rolling
        .add_channel(ChannelSpec::new("/topic", "json"))
        .unwrap();

    ch.write(1000, 1000, &b"before"[..]).unwrap();
    drop(ch);

    let closed = rolling.force_split().unwrap();
    assert_eq!(closed.file_index, 0);
    assert_eq!(closed.message_count, 1);
    assert_eq!(rolling.current_file_index(), 1);

    let mut ch = rolling
        .add_channel(ChannelSpec::new("/topic2", "json"))
        .unwrap();
    ch.write(2000, 2000, &b"after"[..]).unwrap();
    drop(ch);

    rolling.finish().unwrap();
    drop(rolling);

    let files = files.lock().unwrap();
    assert_eq!(files.len(), 2);

    assert_eq!(count_messages(&files[0]), 1);
    assert_eq!(count_messages(&files[1]), 1);
}

#[test]
fn any_trigger_first_to_fire() {
    let (factory, files) = CollectingFactory::new();
    // MessageCount(5) should fire before MaxSize(very large)
    let trigger = AnyTrigger::new()
        .or(MaxSize::new(u64::MAX))
        .or(MessageCount::new(5));

    let mut rolling = RollingWriterBuilder::new(factory, trigger)
        .writer_builder(WriterBuilder::new().profile("test"))
        .build()
        .unwrap();

    let mut ch = rolling
        .add_channel(ChannelSpec::new("/topic", "json"))
        .unwrap();

    for i in 0..12u64 {
        ch.write(i * 1000, i * 1000, &b"data"[..]).unwrap();
    }
    drop(ch);
    rolling.finish().unwrap();
    drop(rolling);

    let files = files.lock().unwrap();
    // 12 messages / 5 per file = 3 files (5, 5, 2)
    assert_eq!(files.len(), 3);
    assert_eq!(count_messages(&files[0]), 5);
    assert_eq!(count_messages(&files[1]), 5);
    assert_eq!(count_messages(&files[2]), 2);
}

#[test]
fn multiple_channel_writers_across_splits() {
    let (factory, files) = CollectingFactory::new();
    let trigger = MessageCount::new(4);

    let mut rolling = RollingWriterBuilder::new(factory, trigger)
        .writer_builder(WriterBuilder::new().profile("test"))
        .build()
        .unwrap();

    let mut ch_a = rolling.add_channel(ChannelSpec::new("/a", "json")).unwrap();
    let mut ch_b = rolling.add_channel(ChannelSpec::new("/b", "json")).unwrap();

    // Interleave writes: 8 total messages (4 per channel)
    for i in 0..4u64 {
        ch_a.write(i * 1000, i * 1000, &b"aa"[..]).unwrap();
        ch_b.write(i * 1000, i * 1000, &b"bb"[..]).unwrap();
    }
    drop(ch_a);
    drop(ch_b);
    rolling.finish().unwrap();
    drop(rolling);

    let files = files.lock().unwrap();
    let total: usize = files.iter().map(|f| count_messages(f)).sum();
    assert_eq!(total, 8, "All 8 messages should be present across files");

    // All files should be valid
    for f in files.iter() {
        validate_mcap(f);
    }
}

#[test]
fn on_split_callback() {
    let split_count = Arc::new(AtomicUsize::new(0));
    let split_count_clone = split_count.clone();

    let (factory, _files) = CollectingFactory::new();
    let trigger = MessageCount::new(3);

    let mut rolling = RollingWriterBuilder::new(factory, trigger)
        .writer_builder(WriterBuilder::new().profile("test"))
        .on_split(move |_ctx| {
            split_count_clone.fetch_add(1, Ordering::SeqCst);
        })
        .build()
        .unwrap();

    let mut ch = rolling
        .add_channel(ChannelSpec::new("/topic", "json"))
        .unwrap();

    for i in 0..10u64 {
        ch.write(i * 1000, i * 1000, &b"data"[..]).unwrap();
    }
    drop(ch);
    rolling.finish().unwrap();
    drop(rolling);

    // 10 messages / 3 per file = 3 splits (after file 0, 1, 2)
    assert_eq!(split_count.load(Ordering::SeqCst), 3);
}

#[test]
fn chunked_rolling() {
    let (factory, files) = CollectingFactory::new();
    let trigger = MessageCount::new(5);

    let mut rolling = RollingWriterBuilder::new(factory, trigger)
        .writer_builder(
            WriterBuilder::new()
                .profile("test")
                .chunked(ChunkOptions::default()),
        )
        .build()
        .unwrap();

    let mut ch = rolling
        .add_channel(ChannelSpec::new("/sensor", "cdr").schema(SchemaSpec::new(
            "SensorData",
            "ros2msg",
            Bytes::from_static(b"schema"),
        )))
        .unwrap();

    for i in 0..15u64 {
        ch.write(i * 1_000_000, i * 1_000_000, &b"payload-data"[..])
            .unwrap();
    }
    drop(ch);
    rolling.finish().unwrap();
    drop(rolling);

    let files = files.lock().unwrap();
    assert_eq!(files.len(), 3);

    for (i, f) in files.iter().enumerate() {
        let mut reader = validate_mcap(f);

        // Verify summary exists (chunked files should have summaries)
        let summary = reader.summary().unwrap();
        assert!(summary.is_some(), "File {i} should have a summary section");

        let msg_count = count_messages(f);
        assert_eq!(msg_count, 5, "File {i} should have 5 messages");
    }
}

#[test]
fn current_file_stats() {
    let (factory, _files) = CollectingFactory::new();
    let trigger = MessageCount::new(5);

    let mut rolling = RollingWriterBuilder::new(factory, trigger)
        .writer_builder(WriterBuilder::new().profile("test"))
        .build()
        .unwrap();

    assert_eq!(rolling.current_file_index(), 0);
    assert_eq!(rolling.current_file_message_count(), 0);

    let mut ch = rolling
        .add_channel(ChannelSpec::new("/topic", "json"))
        .unwrap();

    ch.write(1000, 1000, &b"a"[..]).unwrap();
    assert_eq!(rolling.current_file_message_count(), 1);

    ch.write(2000, 2000, &b"b"[..]).unwrap();
    assert_eq!(rolling.current_file_message_count(), 2);

    // Write 3 more to reach 5 messages total
    for i in 3..5u64 {
        ch.write(i * 1000, i * 1000, &b"x"[..]).unwrap();
    }
    assert_eq!(rolling.current_file_message_count(), 4);

    // The 5th message pushes count to 5
    ch.write(5000, 5000, &b"x"[..]).unwrap();
    assert_eq!(rolling.current_file_message_count(), 5);

    // The 6th write triggers the split (check-before-write)
    ch.write(6000, 6000, &b"y"[..]).unwrap();
    assert_eq!(rolling.current_file_index(), 1);
    assert_eq!(rolling.current_file_message_count(), 1);
}

#[test]
fn finish_is_idempotent() {
    let (factory, files) = CollectingFactory::new();
    let trigger = MessageCount::new(1000);

    let mut rolling = RollingWriterBuilder::new(factory, trigger)
        .writer_builder(WriterBuilder::new().profile("test"))
        .build()
        .unwrap();

    let mut ch = rolling
        .add_channel(ChannelSpec::new("/topic", "json"))
        .unwrap();
    ch.write(1000, 1000, &b"data"[..]).unwrap();
    drop(ch);

    rolling.finish().unwrap();
    rolling.finish().unwrap(); // Should not panic or error

    drop(rolling);
    let files = files.lock().unwrap();
    assert_eq!(files.len(), 1);
}

#[test]
fn write_after_finish_errors() {
    let (factory, _files) = CollectingFactory::new();
    let trigger = MessageCount::new(1000);

    let mut rolling = RollingWriterBuilder::new(factory, trigger)
        .writer_builder(WriterBuilder::new().profile("test"))
        .build()
        .unwrap();

    let mut ch = rolling
        .add_channel(ChannelSpec::new("/topic", "json"))
        .unwrap();
    ch.write(1000, 1000, &b"data"[..]).unwrap();

    rolling.finish().unwrap();

    // Writing after finish should error
    let result = ch.write(2000, 2000, &b"nope"[..]);
    assert!(result.is_err());
}

#[test]
fn empty_file_on_finish() {
    let (factory, files) = CollectingFactory::new();
    let trigger = MessageCount::new(1000);

    let mut rolling = RollingWriterBuilder::new(factory, trigger)
        .writer_builder(WriterBuilder::new().profile("test"))
        .build()
        .unwrap();

    // No channels, no messages -- just finish
    rolling.finish().unwrap();
    drop(rolling);

    let files = files.lock().unwrap();
    assert_eq!(files.len(), 1);
    // The file should still be valid (header + footer)
    validate_mcap(&files[0]);
}

// ──────────────────────────────────────────────────────────
// Additional API path tests
// ──────────────────────────────────────────────────────────

fn collect_records(data: &[u8]) -> Vec<Record> {
    let mut reader = reader::Reader::from_slice(data).unwrap();
    reader.records().map(|r| r.unwrap()).collect()
}

#[test]
fn attachment_writer_writes_to_current_file() {
    let (factory, files) = CollectingFactory::new();
    let trigger = MessageCount::new(1000);

    let mut rolling = RollingWriterBuilder::new(factory, trigger)
        .writer_builder(WriterBuilder::new().profile("test"))
        .build()
        .unwrap();

    let mut ch = rolling
        .add_channel(ChannelSpec::new("/topic", "json"))
        .unwrap();
    ch.write(1000, 1000, &b"msg"[..]).unwrap();
    drop(ch);

    let mut aw = rolling.attachment_writer();
    aw.write(
        1000,
        1000,
        ByteStr::from("config.json"),
        ByteStr::from("application/json"),
        Bytes::from_static(b"{\"key\":\"value\"}"),
    )
    .unwrap();
    drop(aw);

    rolling.finish().unwrap();
    drop(rolling);

    let files = files.lock().unwrap();
    let records = collect_records(&files[0]);
    let attachments: Vec<_> = records
        .iter()
        .filter_map(|r| match r {
            Record::Attachment(a) => Some(a),
            _ => None,
        })
        .collect();
    assert_eq!(attachments.len(), 1);
    assert_eq!(attachments[0].name, ByteStr::from("config.json"));
    assert_eq!(
        attachments[0].data,
        Bytes::from_static(b"{\"key\":\"value\"}")
    );
}

#[test]
fn write_metadata_to_current_file() {
    let (factory, files) = CollectingFactory::new();
    let trigger = MessageCount::new(1000);

    let mut rolling = RollingWriterBuilder::new(factory, trigger)
        .writer_builder(WriterBuilder::new().profile("test"))
        .build()
        .unwrap();

    let mut meta_map = mcapable_core::collections::HashMap::new();
    meta_map.insert(ByteStr::from("version"), ByteStr::from("1.0"));
    rolling
        .write_metadata(&Metadata {
            name: ByteStr::from("run_info"),
            metadata: meta_map,
        })
        .unwrap();

    rolling.finish().unwrap();
    drop(rolling);

    let files = files.lock().unwrap();
    let records = collect_records(&files[0]);
    let metadata: Vec<_> = records
        .iter()
        .filter_map(|r| match r {
            Record::Metadata(m) => Some(m),
            _ => None,
        })
        .collect();
    assert_eq!(metadata.len(), 1);
    assert_eq!(metadata[0].name, ByteStr::from("run_info"));
}

#[test]
fn attachment_not_re_emitted_on_split() {
    let (factory, files) = CollectingFactory::new();
    let trigger = MessageCount::new(2);

    let mut rolling = RollingWriterBuilder::new(factory, trigger)
        .writer_builder(WriterBuilder::new().profile("test"))
        .build()
        .unwrap();

    let mut ch = rolling
        .add_channel(ChannelSpec::new("/topic", "json"))
        .unwrap();
    ch.write(1000, 1000, &b"a"[..]).unwrap();

    // Write attachment to file 0
    let mut aw = rolling.attachment_writer();
    aw.write(
        1000,
        1000,
        ByteStr::from("cal.bin"),
        ByteStr::from("application/octet-stream"),
        Bytes::from_static(b"calibration"),
    )
    .unwrap();
    drop(aw);

    // Write more to trigger split
    ch.write(2000, 2000, &b"b"[..]).unwrap();
    ch.write(3000, 3000, &b"c"[..]).unwrap();
    ch.write(4000, 4000, &b"d"[..]).unwrap();
    drop(ch);
    rolling.finish().unwrap();
    drop(rolling);

    let files = files.lock().unwrap();
    assert!(files.len() >= 2);

    // File 0 should have the attachment
    let records_0 = collect_records(&files[0]);
    let att_count_0 = records_0
        .iter()
        .filter(|r| matches!(r, Record::Attachment(_)))
        .count();
    assert_eq!(att_count_0, 1);

    // File 1 should NOT have the attachment
    let records_1 = collect_records(&files[1]);
    let att_count_1 = records_1
        .iter()
        .filter(|r| matches!(r, Record::Attachment(_)))
        .count();
    assert_eq!(att_count_1, 0);
}

#[test]
fn metadata_not_re_emitted_on_split() {
    let (factory, files) = CollectingFactory::new();
    let trigger = MessageCount::new(2);

    let mut rolling = RollingWriterBuilder::new(factory, trigger)
        .writer_builder(WriterBuilder::new().profile("test"))
        .build()
        .unwrap();

    let mut ch = rolling
        .add_channel(ChannelSpec::new("/topic", "json"))
        .unwrap();
    ch.write(1000, 1000, &b"a"[..]).unwrap();

    // Write metadata to file 0
    rolling
        .write_metadata(&Metadata {
            name: ByteStr::from("info"),
            metadata: mcapable_core::collections::HashMap::new(),
        })
        .unwrap();

    // Trigger split
    ch.write(2000, 2000, &b"b"[..]).unwrap();
    ch.write(3000, 3000, &b"c"[..]).unwrap();
    ch.write(4000, 4000, &b"d"[..]).unwrap();
    drop(ch);
    rolling.finish().unwrap();
    drop(rolling);

    let files = files.lock().unwrap();
    assert!(files.len() >= 2);

    let meta_count = |data: &[u8]| {
        collect_records(data)
            .iter()
            .filter(|r| matches!(r, Record::Metadata(_)))
            .count()
    };
    assert_eq!(meta_count(&files[0]), 1);
    assert_eq!(meta_count(&files[1]), 0);
}

#[test]
fn write_with_sequence_explicit() {
    let (factory, files) = CollectingFactory::new();
    let trigger = MessageCount::new(1000);

    let mut rolling = RollingWriterBuilder::new(factory, trigger)
        .writer_builder(WriterBuilder::new().profile("test"))
        .build()
        .unwrap();

    let mut ch = rolling
        .add_channel(ChannelSpec::new("/topic", "json"))
        .unwrap();
    ch.write_with_sequence(1000, 1000, &b"data"[..], 42)
        .unwrap();
    drop(ch);
    rolling.finish().unwrap();
    drop(rolling);

    let files = files.lock().unwrap();
    let mut reader = validate_mcap(&files[0]);
    let msg = reader.raw_messages().unwrap().next().unwrap().unwrap();
    assert_eq!(msg.sequence, 42);
}

#[test]
fn starting_sequence_offset() {
    let (factory, files) = CollectingFactory::new();
    let trigger = MessageCount::new(1000);

    let mut rolling = RollingWriterBuilder::new(factory, trigger)
        .writer_builder(WriterBuilder::new().profile("test"))
        .build()
        .unwrap();

    let mut ch = rolling
        .add_channel(ChannelSpec::new("/topic", "json"))
        .unwrap()
        .starting_sequence(100);

    for i in 0..3u64 {
        ch.write(i * 1000, i * 1000, &b"data"[..]).unwrap();
    }
    drop(ch);
    rolling.finish().unwrap();
    drop(rolling);

    let files = files.lock().unwrap();
    let mut reader = validate_mcap(&files[0]);
    let sequences: Vec<u32> = reader
        .raw_messages()
        .unwrap()
        .map(|m| m.unwrap().sequence)
        .collect();
    assert_eq!(sequences, vec![100, 101, 102]);
}

#[test]
fn force_split_with_empty_file() {
    let (factory, files) = CollectingFactory::new();
    let trigger = MessageCount::new(1000);

    let mut rolling = RollingWriterBuilder::new(factory, trigger)
        .writer_builder(WriterBuilder::new().profile("test"))
        .build()
        .unwrap();

    let mut ch = rolling
        .add_channel(ChannelSpec::new("/topic", "json"))
        .unwrap();

    // Force split with no messages in current file
    let closed = rolling.force_split().unwrap();
    assert_eq!(closed.file_index, 0);
    assert_eq!(closed.message_count, 0);
    assert_eq!(closed.log_time_range, None);
    assert!(closed.file_size > 0); // header + footer at minimum

    // Write to the new file
    ch.write(1000, 1000, &b"msg"[..]).unwrap();
    drop(ch);
    rolling.finish().unwrap();
    drop(rolling);

    let files = files.lock().unwrap();
    assert_eq!(files.len(), 2);
    validate_mcap(&files[0]);
    validate_mcap(&files[1]);
    assert_eq!(count_messages(&files[0]), 0);
    assert_eq!(count_messages(&files[1]), 1);
}

#[test]
fn closed_file_context_field_accuracy() {
    let (factory, _files) = CollectingFactory::new();
    let trigger = MessageCount::new(1000);

    let mut rolling = RollingWriterBuilder::new(factory, trigger)
        .writer_builder(WriterBuilder::new().profile("test"))
        .build()
        .unwrap();

    let mut ch = rolling
        .add_channel(ChannelSpec::new("/topic", "json"))
        .unwrap();

    ch.write(1000, 1000, &b"a"[..]).unwrap();
    ch.write(5000, 5000, &b"b"[..]).unwrap();
    ch.write(3000, 3000, &b"c"[..]).unwrap();
    drop(ch);

    let closed = rolling.force_split().unwrap();
    assert_eq!(closed.file_index, 0);
    assert_eq!(closed.message_count, 3);
    assert_eq!(closed.log_time_range, Some((1000, 5000)));
    assert!(closed.file_size > 0);
}

#[test]
fn add_channel_after_finish_errors() {
    let (factory, _files) = CollectingFactory::new();
    let trigger = MessageCount::new(1000);

    let mut rolling = RollingWriterBuilder::new(factory, trigger)
        .writer_builder(WriterBuilder::new().profile("test"))
        .build()
        .unwrap();

    rolling.finish().unwrap();

    let result = rolling.add_channel(ChannelSpec::new("/topic", "json"));
    assert!(result.is_err());
}

/// Override channels are routed to dedicated uncompressed chunk streams,
/// just like the base Writer.
#[test]
fn rolling_writer_routes_override_channel_to_uncompressed_chunks() {
    use mcapable_core::Compression;
    use mcapable_core::writer::SchemaSpec;

    let (factory, files) = CollectingFactory::new();

    let mut rolling = RollingWriterBuilder::new(factory, MessageCount::new(1_000))
        .writer_builder(WriterBuilder::new().chunked(ChunkOptions {
            compression: Some(Compression::Zstd),
            max_uncompressed_bytes: 64,
            include_crc: true,
        }))
        .build()
        .unwrap();

    let schema = SchemaSpec::new("pkg/Msg", "raw", Bytes::from_static(b""));
    let mut default_ch = rolling
        .add_channel(ChannelSpec::new("/telemetry", "raw").schema(schema.clone()))
        .unwrap();
    let mut override_ch = rolling
        .add_channel(
            ChannelSpec::new("/video", "h264")
                .schema(schema)
                .uncompressed_chunks(),
        )
        .unwrap();

    for i in 0..6u64 {
        default_ch
            .write(1000 + i, 1000 + i, vec![b'a'; 80])
            .unwrap();
        override_ch
            .write(1000 + i, 1000 + i, vec![b'b'; 80])
            .unwrap();
    }
    drop(default_ch);
    drop(override_ch);
    rolling.finish().unwrap();
    drop(rolling);

    let files = files.lock().unwrap();
    assert_eq!(files.len(), 1, "single file expected (no split triggered)");
    let bytes = &files[0];

    let mut reader = mcapable_core::reader::Reader::from_slice(bytes).unwrap();
    let summary = reader.summary().unwrap().expect("summary");

    let zstd_count = summary
        .chunk_indexes
        .iter()
        .filter(|ci| ci.compression.as_ref() == "zstd")
        .count();
    let none_count = summary
        .chunk_indexes
        .iter()
        .filter(|ci| ci.compression.as_ref().is_empty())
        .count();
    assert!(
        zstd_count > 0,
        "expected at least one zstd chunk for default channel"
    );
    assert!(
        none_count > 0,
        "expected at least one uncompressed chunk for override channel"
    );
}

/// After a forced split, the new file independently re-registers the
/// override channel so its messages still land in uncompressed chunks.
#[test]
fn rolling_writer_re_registers_override_after_split() {
    use mcapable_core::Compression;
    use mcapable_core::writer::SchemaSpec;

    let (factory, files) = CollectingFactory::new();

    let mut rolling = RollingWriterBuilder::new(factory, MessageCount::new(10_000))
        .writer_builder(WriterBuilder::new().chunked(ChunkOptions {
            compression: Some(Compression::Zstd),
            max_uncompressed_bytes: 64,
            include_crc: true,
        }))
        .build()
        .unwrap();

    let schema = SchemaSpec::new("pkg/Msg", "raw", Bytes::from_static(b""));
    let mut override_ch = rolling
        .add_channel(
            ChannelSpec::new("/video", "h264")
                .schema(schema)
                .uncompressed_chunks(),
        )
        .unwrap();

    // Write into file 0
    for i in 0..4u64 {
        override_ch
            .write(1000 + i, 1000 + i, vec![b'a'; 80])
            .unwrap();
    }
    rolling.force_split().unwrap();
    // Write into file 1
    for i in 0..4u64 {
        override_ch
            .write(2000 + i, 2000 + i, vec![b'b'; 80])
            .unwrap();
    }
    drop(override_ch);
    rolling.finish().unwrap();
    drop(rolling);

    let files = files.lock().unwrap();
    assert_eq!(files.len(), 2, "expected two files after one split");

    for (idx, file_bytes) in files.iter().enumerate() {
        let mut reader = mcapable_core::reader::Reader::from_slice(file_bytes).unwrap();
        let summary = reader.summary().unwrap().expect("summary");
        let none_count = summary
            .chunk_indexes
            .iter()
            .filter(|ci| ci.compression.as_ref().is_empty())
            .count();
        assert!(
            none_count > 0,
            "file {idx} must contain at least one uncompressed chunk after re-registration",
        );
    }
}

/// Override chunks must not span file boundaries: a partial override chunk
/// at split time gets drained into the closing file, and the next file
/// starts cleanly.
#[test]
fn rolling_writer_override_chunks_do_not_span_files() {
    let (factory, files) = CollectingFactory::new();

    let mut rolling = RollingWriterBuilder::new(factory, MessageCount::new(10_000))
        .writer_builder(WriterBuilder::new().chunked(ChunkOptions {
            compression: None,
            max_uncompressed_bytes: 1 << 20, // huge default — won't flush
            include_crc: true,
        }))
        .build()
        .unwrap();

    let mut override_ch = rolling
        .add_channel(
            ChannelSpec::new("/video", "h264").chunk_override(ChunkOptions {
                compression: None,
                max_uncompressed_bytes: 1 << 20, // huge — won't flush mid-write
                include_crc: true,
            }),
        )
        .unwrap();

    // Write a few messages — well under threshold, so the override chunk is
    // partially filled when we force the split.
    override_ch.write(1, 1, vec![b'a'; 16]).unwrap();
    override_ch.write(2, 2, vec![b'b'; 16]).unwrap();
    rolling.force_split().unwrap();
    // Write into file 1.
    override_ch.write(3, 3, vec![b'c'; 16]).unwrap();
    drop(override_ch);
    rolling.finish().unwrap();
    drop(rolling);

    let files = files.lock().unwrap();
    assert_eq!(files.len(), 2);

    // File 0 must contain exactly one uncompressed chunk (the partial fill
    // drained at split-time finish).
    let mut reader0 = mcapable_core::reader::Reader::from_slice(&files[0]).unwrap();
    let summary0 = reader0.summary().unwrap().expect("summary 0");
    let none_count0 = summary0
        .chunk_indexes
        .iter()
        .filter(|ci| ci.compression.as_ref().is_empty())
        .count();
    assert_eq!(
        none_count0, 1,
        "file 0 must have exactly 1 uncompressed chunk"
    );

    let log_times0: Vec<u64> = reader0
        .raw_messages()
        .unwrap()
        .map(|m| m.unwrap().log_time)
        .collect();
    assert_eq!(
        log_times0,
        vec![1, 2],
        "file 0 must contain the 2 pre-split override messages",
    );

    // File 1 must independently produce its own uncompressed chunk for the
    // post-split message.
    let mut reader1 = mcapable_core::reader::Reader::from_slice(&files[1]).unwrap();
    let summary1 = reader1.summary().unwrap().expect("summary 1");
    let none_count1 = summary1
        .chunk_indexes
        .iter()
        .filter(|ci| ci.compression.as_ref().is_empty())
        .count();
    assert_eq!(
        none_count1, 1,
        "file 1 must have exactly 1 uncompressed chunk"
    );

    let log_times1: Vec<u64> = reader1
        .raw_messages()
        .unwrap()
        .map(|m| m.unwrap().log_time)
        .collect();
    assert_eq!(
        log_times1,
        vec![3],
        "file 1 must contain only the post-split override message",
    );
}
