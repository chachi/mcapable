use bytes::Bytes;
use mcapable_core::Compression;
use mcapable_core::reader;
use mcapable_core::writer::rolling::*;
use mcapable_core::writer::{ChannelSpec, ChunkOptions, SchemaSpec, WriterBuilder};
use proptest::prelude::*;
use std::io::Cursor;
use std::sync::Arc;

const MCAP_MAGIC: &[u8; 8] = b"\x89MCAP\x30\r\n";

// ──────────────────────────────────────────────────────────
// Test infrastructure (same as rolling_writer.rs)
// ──────────────────────────────────────────────────────────

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

// ──────────────────────────────────────────────────────────
// Strategies
// ──────────────────────────────────────────────────────────

fn writer_mode_strategy() -> impl Strategy<Value = WriterBuilder> {
    prop_oneof![
        Just(WriterBuilder::new().profile("test")),
        Just(WriterBuilder::new().profile("test").chunked(ChunkOptions {
            compression: None,
            max_uncompressed_bytes: 128,
        })),
        Just(WriterBuilder::new().profile("test").chunked(ChunkOptions {
            compression: Some(Compression::Lz4),
            max_uncompressed_bytes: 128,
        })),
        Just(WriterBuilder::new().profile("test").chunked(ChunkOptions {
            compression: Some(Compression::Zstd),
            max_uncompressed_bytes: 128,
        })),
    ]
}

/// Returns (trigger_kind, trigger_value) where kind 0 = MessageCount, 1 = MaxSize.
fn trigger_params_strategy() -> impl Strategy<Value = (u8, u64)> {
    prop_oneof![(Just(0u8), 1u64..20u64), (Just(1u8), 500u64..50_000u64),]
}

fn make_trigger(kind: u8, value: u64) -> AnyTrigger {
    match kind {
        0 => AnyTrigger::new().or(MessageCount::new(value)),
        _ => AnyTrigger::new().or(MaxSize::new(value)),
    }
}

// ──────────────────────────────────────────────────────────
// Property tests
// ──────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig { cases: 32, .. ProptestConfig::default() })]

    #[test]
    fn prop_rolling_roundtrip_preserves_all_messages(
        writer_builder in writer_mode_strategy(),
        (trigger_kind, trigger_value) in trigger_params_strategy(),
        payloads in prop::collection::vec(prop::collection::vec(any::<u8>(), 0..256), 0..40),
    ) {
        let trigger = make_trigger(trigger_kind, trigger_value);
        let (factory, files) = CollectingFactory::new();
        let mut rolling = RollingWriterBuilder::new(factory, trigger)
            .writer_builder(writer_builder)
            .build()
            .unwrap();

        let mut ch = rolling
            .add_channel(
                ChannelSpec::new("/prop_topic", "raw")
                    .schema(SchemaSpec::new("TestMsg", "jsonschema", Bytes::from_static(b"{}")))
            )
            .unwrap();

        for (i, payload) in payloads.iter().enumerate() {
            ch.write(
                1000 + i as u64,
                1000 + i as u64,
                payload.clone(),
            )
            .unwrap();
        }
        drop(ch);
        rolling.finish().unwrap();
        drop(rolling);

        let files = files.lock().unwrap();

        let mut all_payloads: Vec<Vec<u8>> = Vec::new();
        let mut all_sequences: Vec<u32> = Vec::new();
        for file_data in files.iter() {
            // Every file must be valid MCAP
            prop_assert!(file_data.starts_with(MCAP_MAGIC), "missing start magic");
            prop_assert!(file_data.ends_with(MCAP_MAGIC), "missing end magic");

            let mut reader = reader::Reader::from_slice(file_data).unwrap();

            // Files with messages must have schemas and channels
            let mut file_reader = reader::Reader::from_slice(file_data).unwrap();
            let msg_count = file_reader.raw_messages().unwrap().count();
            if msg_count > 0 {
                let _load: Vec<_> = reader.messages().unwrap().collect();
                let schemas = reader.schemas();
                let channels = reader.channels();
                prop_assert!(!schemas.is_empty(), "file with messages missing schemas");
                prop_assert!(!channels.is_empty(), "file with messages missing channels");
            }

            let mut reader = reader::Reader::from_slice(file_data).unwrap();
            for msg in reader.raw_messages().unwrap() {
                let msg = msg.unwrap();
                all_payloads.push(msg.data_bytes().to_vec());
                all_sequences.push(msg.sequence);
            }
        }

        // Total messages preserved (no data loss)
        prop_assert_eq!(
            all_payloads.len(),
            payloads.len(),
            "message count mismatch"
        );

        // Payload bytes match
        for (i, (got, expected)) in all_payloads.iter().zip(payloads.iter()).enumerate() {
            prop_assert_eq!(got, expected, "payload mismatch at index {}", i);
        }

        // Sequence numbers are monotonically increasing
        for window in all_sequences.windows(2) {
            prop_assert!(
                window[1] == window[0] + 1,
                "sequence not monotonic: {} -> {}",
                window[0],
                window[1]
            );
        }
    }

    #[test]
    fn prop_rolling_multiple_channels(
        writer_builder in writer_mode_strategy(),
        msgs_per_channel in 0u64..15u64,
        num_channels in 2usize..5usize,
    ) {
        let trigger = AnyTrigger::new().or(MessageCount::new(7));
        let (factory, files) = CollectingFactory::new();

        let mut rolling = RollingWriterBuilder::new(factory, trigger)
            .writer_builder(writer_builder)
            .build()
            .unwrap();

        let mut writers: Vec<_> = (0..num_channels)
            .map(|i| {
                rolling
                    .add_channel(ChannelSpec::new(format!("/ch_{i}"), "raw"))
                    .unwrap()
            })
            .collect();

        let channel_ids: Vec<u16> = writers.iter().map(|w| w.channel_id()).collect();

        for t in 0..msgs_per_channel {
            for w in &mut writers {
                w.write(t * 1000, t * 1000, &b"x"[..]).unwrap();
            }
        }
        drop(writers);
        rolling.finish().unwrap();
        drop(rolling);

        let files = files.lock().unwrap();
        let expected_total = (msgs_per_channel as usize) * num_channels;

        // Count total messages across files
        let mut total = 0usize;
        for file_data in files.iter() {
            let mut reader = reader::Reader::from_slice(file_data).unwrap();
            total += reader.raw_messages().unwrap().count();
        }
        prop_assert_eq!(total, expected_total, "total message count mismatch");

        // Verify channel IDs present in files with messages
        for file_data in files.iter() {
            let mut reader = reader::Reader::from_slice(file_data).unwrap();
            let msg_count = reader.raw_messages().unwrap().count();
            if msg_count > 0 {
                let mut reader = reader::Reader::from_slice(file_data).unwrap();
                let _load: Vec<_> = reader.messages().unwrap().collect();
                let channels = reader.channels();
                for &id in &channel_ids {
                    prop_assert!(
                        channels.contains_key(&id),
                        "channel {} missing from file with messages",
                        id
                    );
                }
            }
        }
    }
}
