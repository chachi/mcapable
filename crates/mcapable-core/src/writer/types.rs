use crate::types::{Channel, Schema};
use crate::zero_copy::ByteStr;
use bytes::Bytes;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct SchemaKey {
    pub(crate) name: ByteStr,
    pub(crate) encoding: ByteStr,
    pub(crate) data: Bytes,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ChannelStats {
    pub(crate) message_count: u64,
    pub(crate) message_start_time: u64,
    pub(crate) message_end_time: u64,
    pub(crate) has_messages: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ChunkIndexInfo {
    pub(crate) message_start_time: u64,
    pub(crate) message_end_time: u64,
    pub(crate) chunk_start_offset: u64,
    pub(crate) chunk_length: u64,
    pub(crate) compression: ByteStr,
    pub(crate) compressed_size: u64,
    pub(crate) uncompressed_size: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct AttachmentIndexInfo {
    pub(crate) offset: u64,
    pub(crate) length: u64,
    pub(crate) log_time: u64,
    pub(crate) create_time: u64,
    pub(crate) data_size: u64,
    pub(crate) name: ByteStr,
    pub(crate) media_type: ByteStr,
}

#[derive(Debug, Clone)]
pub(crate) struct MetadataIndexInfo {
    pub(crate) offset: u64,
    pub(crate) length: u64,
    pub(crate) name: ByteStr,
}

#[derive(Debug, Clone)]
pub(crate) struct SummaryGroup {
    pub(crate) opcode: crate::types::Opcode,
    pub(crate) start: u64,
    pub(crate) length: u64,
}

pub(crate) fn should_write_summary(
    schemas: &crate::support::HashMap<u16, Schema>,
    channels: &crate::support::HashMap<u16, Channel>,
    chunk_indexes: &[ChunkIndexInfo],
    attachment_indexes: &[AttachmentIndexInfo],
    metadata_indexes: &[MetadataIndexInfo],
    channel_stats: &[ChannelStats],
) -> bool {
    !schemas.is_empty()
        || !channels.is_empty()
        || !chunk_indexes.is_empty()
        || !attachment_indexes.is_empty()
        || !metadata_indexes.is_empty()
        || channel_stats.iter().any(|s| s.has_messages)
}
