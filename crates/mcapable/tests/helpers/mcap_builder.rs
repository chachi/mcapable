#![allow(dead_code)]
//! Test helpers for building MCAP files using the official mcap crate.
//!
//! This is a thin wrapper around mcap::Writer that provides a builder-style API
//! for creating test MCAP files with specific characteristics.

use mcapable_core::collections::HashMap;
use std::collections::BTreeMap;
use std::io::Cursor;

/// Compression type for test MCAP files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    None,
    Lz4,
    Zstd,
}

/// Builder for creating test MCAP files.
pub struct McapBuilder {
    channels: Vec<TestChannel>,
    messages: Vec<TestMessage>,
    attachments: Vec<TestAttachment>,
    compression: Option<Compression>,
    chunked: bool,
    chunk_size: Option<usize>,
    profile: String,
    metadata: HashMap<String, String>,
}

/// Test channel configuration.
#[derive(Debug, Clone)]
pub struct TestChannel {
    pub id: u16,
    pub topic: String,
    pub message_encoding: String,
    pub schema_id: u16,
    pub schema_name: Option<String>,
    pub schema_encoding: Option<String>,
    pub schema_data: Option<Vec<u8>>,
}

/// Test message data.
#[derive(Debug, Clone)]
pub struct TestMessage {
    pub channel_id: u16,
    pub sequence: u32,
    pub log_time: u64,
    pub publish_time: u64,
    pub data: Vec<u8>,
}

/// Test attachment data.
#[derive(Debug, Clone)]
pub struct TestAttachment {
    pub log_time: u64,
    pub create_time: u64,
    pub name: String,
    pub media_type: String,
    pub data: Vec<u8>,
}

impl McapBuilder {
    /// Create a new MCAP builder.
    pub fn new() -> Self {
        Self {
            channels: Vec::new(),
            messages: Vec::new(),
            attachments: Vec::new(),
            compression: None,
            chunked: true,
            chunk_size: Some(1024 * 1024), // 1MB default
            profile: String::new(),
            metadata: HashMap::new(),
        }
    }

    /// Set compression type.
    pub fn compression(mut self, compression: Option<Compression>) -> Self {
        self.compression = compression;
        self
    }

    /// Set whether to use chunks.
    pub fn chunked(mut self, chunked: bool) -> Self {
        self.chunked = chunked;
        self
    }

    /// Set chunk size (None for single chunk).
    pub fn chunk_size(mut self, size: Option<usize>) -> Self {
        self.chunk_size = size;
        self
    }

    /// Set profile.
    pub fn profile(mut self, profile: impl Into<String>) -> Self {
        self.profile = profile.into();
        self
    }

    /// Set whether to include summary section.
    /// Note: The mcap crate always includes summaries, this is for API compatibility.
    pub fn include_summary(self, _include: bool) -> Self {
        // mcap crate handles summary automatically, ignore this setting
        self
    }

    /// Set whether to include statistics.
    /// Note: The mcap crate handles statistics automatically, this is for API compatibility.
    pub fn include_statistics(self, _include: bool) -> Self {
        // mcap crate handles statistics automatically, ignore this setting
        self
    }

    /// Add file metadata.
    pub fn add_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Add an attachment.
    pub fn add_attachment(
        mut self,
        name: impl Into<String>,
        media_type: impl Into<String>,
        log_time: u64,
        create_time: u64,
        data: Vec<u8>,
    ) -> Self {
        self.attachments.push(TestAttachment {
            log_time,
            create_time,
            name: name.into(),
            media_type: media_type.into(),
            data,
        });
        self
    }

    /// Add a channel with full configuration.
    pub fn add_channel(mut self, channel: TestChannel) -> Self {
        self.channels.push(channel);
        self
    }

    /// Add a simple channel with defaults.
    pub fn add_simple_channel(mut self, id: u16, topic: &str) -> Self {
        self.channels.push(TestChannel {
            id,
            topic: topic.to_string(),
            message_encoding: "application/octet-stream".to_string(),
            schema_id: 0,
            schema_name: None,
            schema_encoding: None,
            schema_data: None,
        });
        self
    }

    /// Add a message with full configuration.
    pub fn add_message(mut self, message: TestMessage) -> Self {
        self.messages.push(message);
        self
    }

    /// Add a simple message.
    pub fn add_simple_message(
        mut self,
        channel_id: u16,
        sequence: u32,
        log_time: u64,
        data: Vec<u8>,
    ) -> Self {
        self.messages.push(TestMessage {
            channel_id,
            sequence,
            log_time,
            publish_time: log_time,
            data,
        });
        self
    }

    /// Build the MCAP file using the official mcap crate.
    pub fn build(self) -> Vec<u8> {
        let mut buffer = Cursor::new(Vec::new());

        // Configure WriteOptions
        let mut opts = mcap::WriteOptions::new();

        if !self.profile.is_empty() {
            opts = opts.profile(&self.profile);
        }

        // Set compression
        let compression = match self.compression {
            Some(Compression::Lz4) => Some(mcap::Compression::Lz4),
            Some(Compression::Zstd) => Some(mcap::Compression::Zstd),
            Some(Compression::None) | None => None,
        };
        opts = opts.compression(compression);

        // Set chunking
        opts = opts.use_chunks(self.chunked);
        if let Some(size) = self.chunk_size {
            opts = opts.chunk_size(Some(size as u64));
        } else {
            opts = opts.chunk_size(None);
        }

        // Create writer
        let mut writer = opts
            .create(&mut buffer)
            .expect("Failed to create MCAP writer");

        // Write file metadata
        for (key, value) in &self.metadata {
            let metadata_record = mcap::records::Metadata {
                name: key.clone(),
                metadata: [(key.clone(), value.clone())].iter().cloned().collect(),
            };
            writer
                .write_metadata(&metadata_record)
                .expect("Failed to write metadata");
        }

        // Write attachments
        for attachment in &self.attachments {
            let attachment_record = mcap::Attachment {
                log_time: attachment.log_time,
                create_time: attachment.create_time,
                name: attachment.name.clone(),
                media_type: attachment.media_type.clone(),
                data: std::borrow::Cow::Owned(attachment.data.clone()),
            };
            writer
                .attach(&attachment_record)
                .expect("Failed to write attachment");
        }

        // Collect unique schemas and create Schema objects.
        //
        // Note: mcap v0.24+ requires explicit IDs on Schema/Channel structs.
        let mut schema_map: HashMap<u16, std::sync::Arc<mcap::Schema<'static>>> = HashMap::new();
        for channel in &self.channels {
            if channel.schema_id != 0
                && !schema_map.contains_key(&channel.schema_id)
                && let (Some(name), Some(encoding), Some(data)) = (
                    &channel.schema_name,
                    &channel.schema_encoding,
                    &channel.schema_data,
                )
            {
                let schema = std::sync::Arc::new(mcap::Schema {
                    id: channel.schema_id,
                    name: name.clone(),
                    encoding: encoding.clone(),
                    data: std::borrow::Cow::Owned(data.clone()),
                });
                schema_map.insert(channel.schema_id, schema);
            }
        }

        // Create Channel objects with stable IDs so mcapable can assert on them.
        let mut channel_map: HashMap<u16, std::sync::Arc<mcap::Channel<'static>>> = HashMap::new();
        for channel in &self.channels {
            let schema = if channel.schema_id == 0 {
                None
            } else {
                schema_map.get(&channel.schema_id).cloned()
            };

            let mcap_channel = std::sync::Arc::new(mcap::Channel {
                id: channel.id,
                topic: channel.topic.clone(),
                schema,
                message_encoding: channel.message_encoding.clone(),
                metadata: BTreeMap::new(),
            });

            channel_map.insert(channel.id, mcap_channel);
        }

        // Add messages
        for message in self.messages.iter() {
            let channel = channel_map
                .get(&message.channel_id)
                .expect("Message references unknown channel");

            let mcap_message = mcap::Message {
                channel: channel.clone(),
                sequence: message.sequence,
                log_time: message.log_time,
                publish_time: message.publish_time,
                data: std::borrow::Cow::Owned(message.data.clone()),
            };
            writer
                .write(&mcap_message)
                .expect("Failed to write message");
        }

        // Finish writing
        writer.finish().expect("Failed to finish MCAP file");

        // Drop writer to release the borrow on buffer
        drop(writer);

        buffer.into_inner()
    }
}

impl Default for McapBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a simple MCAP file for testing.
pub fn create_simple_mcap() -> Vec<u8> {
    McapBuilder::new()
        .chunked(true) // Chunked so RawMessage/Message streams work
        .add_simple_channel(0, "/test/topic")
        .add_simple_message(0, 1, 1000, b"message1".to_vec())
        .add_simple_message(0, 2, 2000, b"message2".to_vec())
        .add_simple_message(0, 3, 3000, b"message3".to_vec())
        .build()
}

/// Create an MCAP with multiple channels.
pub fn create_multi_channel_mcap() -> Vec<u8> {
    McapBuilder::new()
        .add_simple_channel(0, "/topic1")
        .add_simple_channel(1, "/topic2")
        .add_simple_message(0, 1, 1000, b"msg1".to_vec())
        .add_simple_message(1, 2, 2000, b"msg2".to_vec())
        .add_simple_message(0, 3, 3000, b"msg3".to_vec())
        .build()
}

/// Create an MCAP with messages in time order.
pub fn create_time_ordered_mcap() -> Vec<u8> {
    McapBuilder::new()
        .add_simple_channel(0, "/topic1")
        .add_simple_channel(1, "/topic2")
        .add_simple_message(0, 1, 1000, b"t1_msg1".to_vec())
        .add_simple_message(1, 2, 2000, b"t2_msg1".to_vec())
        .add_simple_message(0, 3, 3000, b"t1_msg2".to_vec())
        .add_simple_message(1, 4, 4000, b"t2_msg2".to_vec())
        .build()
}

/// Create an MCAP with overlapping time ranges across chunks.
pub fn create_overlapping_chunks_mcap() -> Vec<u8> {
    McapBuilder::new()
        .chunked(true)
        .chunk_size(Some(512)) // Small chunks to force multiple
        .add_simple_channel(0, "/topic1")
        .add_simple_channel(1, "/topic2")
        .add_simple_message(0, 1, 1000, vec![0; 200])
        .add_simple_message(1, 2, 1500, vec![0; 200])
        .add_simple_message(0, 3, 2000, vec![0; 200])
        .add_simple_message(1, 4, 2500, vec![0; 200])
        .add_simple_message(0, 5, 3000, vec![0; 200])
        .add_simple_message(1, 6, 3500, vec![0; 200])
        .build()
}

/// Create a large MCAP file for stress testing.
pub fn create_large_mcap(num_channels: u16, messages_per_channel: u32) -> Vec<u8> {
    let mut builder = McapBuilder::new();

    for ch in 0..num_channels {
        builder = builder.add_simple_channel(ch, &format!("/channel/{}", ch));
    }

    for i in 0..(num_channels as u32 * messages_per_channel) {
        let channel = (i % num_channels as u32) as u16;
        builder = builder.add_simple_message(channel, i, 1000 + (i as u64) * 10, vec![0u8; 128]);
    }

    builder.build()
}

/// Create an unchunked MCAP file.
pub fn create_unchunked_mcap() -> Vec<u8> {
    McapBuilder::new()
        .chunked(false)
        .add_simple_channel(0, "/test")
        .add_simple_message(0, 1, 1000, b"message1".to_vec())
        .add_simple_message(0, 2, 2000, b"message2".to_vec())
        .build()
}

/// Create a compressed MCAP file for testing compression.
pub fn create_compressed_mcap(compression: Compression) -> Vec<u8> {
    McapBuilder::new()
        .compression(Some(compression))
        .chunked(true)
        .add_simple_channel(0, "/test")
        .add_simple_message(0, 1, 1000, vec![0; 1000])
        .add_simple_message(0, 2, 2000, vec![0; 1000])
        .build()
}
