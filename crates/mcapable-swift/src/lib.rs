#![allow(clippy::unnecessary_cast)]

use bytes::Bytes;
use mcapable_core::collections::HashMap;
use mcapable_core::reader::{Builder as CoreReaderBuilder, Reader as CoreReader};
use mcapable_core::source::{ArenaBytesSource, BytesCursor, BytesSource};
use mcapable_core::zero_copy::ByteStr;
use mcapable_core::{
    Attachment, Channel, Chunk, Header, Message, MessageMetadata, Metadata, RawMessage, Record,
    RecordMetadata, RecordSource, Schema,
};
use serde_json::Value as JsonValue;

#[allow(clippy::unnecessary_cast)]
#[swift_bridge::bridge]
mod ffi {
    #[swift_bridge(swift_repr = "struct")]
    struct Header {
        profile: String,
        library: String,
        metadata_keys: Vec<String>,
        metadata_values: Vec<String>,
    }

    #[swift_bridge(swift_repr = "struct")]
    struct Schema {
        id: u16,
        name: String,
        encoding: String,
        data: Vec<u8>,
    }

    #[swift_bridge(swift_repr = "struct")]
    struct Channel {
        id: u16,
        topic: String,
        message_encoding: String,
        schema_id: u16,
        metadata_keys: Vec<String>,
        metadata_values: Vec<String>,
    }

    #[swift_bridge(swift_repr = "struct")]
    struct Metadata {
        name: String,
        metadata_keys: Vec<String>,
        metadata_values: Vec<String>,
    }

    #[swift_bridge(swift_repr = "struct")]
    struct Attachment {
        log_time: u64,
        create_time: u64,
        name: String,
        media_type: String,
        data: Vec<u8>,
    }

    #[swift_bridge(swift_repr = "struct")]
    struct RawMessage {
        channel_id: u16,
        sequence: u32,
        log_time: u64,
        publish_time: u64,
        data: Vec<u8>,
    }

    #[swift_bridge(swift_repr = "struct")]
    struct RawMessageResult {
        has_value: bool,
        value: RawMessage,
    }

    #[swift_bridge(swift_repr = "struct")]
    struct Message {
        channel_id: u16,
        sequence: u32,
        log_time: u64,
        publish_time: u64,
        data: Vec<u8>,
    }

    #[swift_bridge(swift_repr = "struct")]
    struct MessageResult {
        has_value: bool,
        value: Message,
    }

    #[swift_bridge(swift_repr = "struct")]
    struct ParsedMessage {
        is_json: bool,
        json: String,
        bytes: Vec<u8>,
    }

    #[swift_bridge(swift_repr = "struct")]
    struct ParsedMessageResult {
        has_value: bool,
        value: ParsedMessage,
    }

    #[swift_bridge(swift_repr = "struct")]
    struct Chunk {
        message_start_time: u64,
        message_end_time: u64,
        uncompressed_size: u64,
        uncompressed_crc: u32,
        compression: String,
        records: Vec<u8>,
    }

    #[swift_bridge(swift_repr = "struct")]
    struct ChunkResult {
        has_value: bool,
        value: Chunk,
    }

    #[swift_bridge(swift_repr = "struct")]
    struct MessageMetadata {
        channel_id: u16,
        sequence: u32,
        log_time: u64,
        publish_time: u64,
        data_size: u64,
    }

    #[swift_bridge(swift_repr = "struct")]
    struct RecordMetadata {
        opcode: String,
        length: u64,
        total_len: u64,
        offset: u64,
        source: String,
        message: MessageMetadata,
        has_message: bool,
    }

    #[swift_bridge(swift_repr = "struct")]
    struct RecordMetadataResult {
        has_value: bool,
        value: RecordMetadata,
    }

    #[swift_bridge(swift_repr = "struct")]
    struct Record {
        kind: String,
        header: Header,
        has_header: bool,
        schema: Schema,
        has_schema: bool,
        channel: Channel,
        has_channel: bool,
        message: Message,
        has_message: bool,
        chunk: Chunk,
        has_chunk: bool,
        attachment: Attachment,
        has_attachment: bool,
        metadata: Metadata,
        has_metadata: bool,
    }

    #[swift_bridge(swift_repr = "struct")]
    struct RecordResult {
        has_value: bool,
        value: Record,
    }

    #[swift_bridge(swift_repr = "struct")]
    struct MessageMetadataResult {
        has_value: bool,
        value: MessageMetadata,
    }

    extern "Rust" {
        type ReaderBuilder;
        type Reader;
        type SchemaList;
        type ChannelList;
        type MetadataList;
        type AttachmentList;
        type RawMessageStream;
        type MessageStream;
        type ParsedMessageStream;
        type ChunkStream;
        type RecordStream;
        type MessageMetadataStream;
        type RecordMetadataStream;

        fn reader_builder_new() -> ReaderBuilder;
        fn reader_builder_validate_end_magic(builder: &mut ReaderBuilder, validate: bool);
        fn reader_builder_build_from_path(
            builder: &ReaderBuilder,
            path: String,
        ) -> Result<Reader, String>;
        fn reader_builder_build_from_bytes(
            builder: &ReaderBuilder,
            bytes: Vec<u8>,
        ) -> Result<Reader, String>;

        fn reader_from_path(path: String) -> Result<Reader, String>;
        fn reader_from_bytes(bytes: Vec<u8>) -> Result<Reader, String>;

        fn reader_header(reader: &mut Reader) -> Result<Header, String>;
        fn reader_schemas(reader: &mut Reader) -> SchemaList;
        fn reader_channels(reader: &mut Reader) -> ChannelList;
        fn reader_metadata(reader: &mut Reader, name: String) -> Result<Metadata, String>;
        fn reader_all_metadata(reader: &mut Reader) -> MetadataList;
        fn reader_attachment(reader: &mut Reader, name: String) -> Result<Attachment, String>;
        fn reader_all_attachments(reader: &mut Reader) -> AttachmentList;

        fn schema_list_len(list: &SchemaList) -> usize;
        fn schema_list_get(list: &SchemaList, index: usize) -> Result<Schema, String>;
        fn channel_list_len(list: &ChannelList) -> usize;
        fn channel_list_get(list: &ChannelList, index: usize) -> Result<Channel, String>;
        fn metadata_list_len(list: &MetadataList) -> usize;
        fn metadata_list_get(list: &MetadataList, index: usize) -> Result<Metadata, String>;
        fn attachment_list_len(list: &AttachmentList) -> usize;
        fn attachment_list_get(list: &AttachmentList, index: usize) -> Result<Attachment, String>;
        fn reader_raw_messages(reader: &mut Reader) -> Result<RawMessageStream, String>;
        fn reader_messages(reader: &mut Reader) -> Result<MessageStream, String>;
        fn reader_chunks(reader: &mut Reader) -> Result<ChunkStream, String>;
        fn reader_records(reader: &mut Reader) -> Result<RecordStream, String>;
        fn reader_message_metadata(reader: &mut Reader) -> Result<MessageMetadataStream, String>;
        fn reader_record_metadata(reader: &mut Reader) -> Result<RecordMetadataStream, String>;

        fn raw_message_stream_next(
            stream: &mut RawMessageStream,
        ) -> Result<RawMessageResult, String>;
        fn raw_message_stream_time_range(
            stream: &mut RawMessageStream,
            start: u64,
            end: u64,
        ) -> Result<(), String>;
        fn raw_message_stream_into_reader(stream: &mut RawMessageStream) -> Result<Reader, String>;

        fn message_stream_next(stream: &mut MessageStream) -> Result<MessageResult, String>;
        fn message_stream_time_range(
            stream: &mut MessageStream,
            start: u64,
            end: u64,
        ) -> Result<(), String>;
        fn message_stream_parsed(stream: &mut MessageStream)
        -> Result<ParsedMessageStream, String>;
        fn message_stream_parsed_with(
            stream: &mut MessageStream,
            encodings: Vec<String>,
            kinds: Vec<String>,
        ) -> Result<ParsedMessageStream, String>;
        fn message_stream_into_reader(stream: &mut MessageStream) -> Result<Reader, String>;

        fn parsed_message_stream_next(
            stream: &mut ParsedMessageStream,
        ) -> Result<ParsedMessageResult, String>;
        fn parsed_message_stream_into_reader(
            stream: &mut ParsedMessageStream,
        ) -> Result<Reader, String>;

        fn chunk_stream_next(stream: &mut ChunkStream) -> Result<ChunkResult, String>;
        fn chunk_stream_time_range(
            stream: &mut ChunkStream,
            start: u64,
            end: u64,
        ) -> Result<(), String>;
        fn chunk_stream_into_reader(stream: &mut ChunkStream) -> Result<Reader, String>;

        fn record_stream_next(stream: &mut RecordStream) -> Result<RecordResult, String>;
        fn record_stream_time_range(
            stream: &mut RecordStream,
            start: u64,
            end: u64,
        ) -> Result<(), String>;
        fn record_stream_into_reader(stream: &mut RecordStream) -> Result<Reader, String>;

        fn message_metadata_stream_next(
            stream: &mut MessageMetadataStream,
        ) -> Result<MessageMetadataResult, String>;
        fn message_metadata_stream_time_range(
            stream: &mut MessageMetadataStream,
            start: u64,
            end: u64,
        ) -> Result<(), String>;
        fn message_metadata_stream_into_reader(
            stream: &mut MessageMetadataStream,
        ) -> Result<Reader, String>;

        fn record_metadata_stream_next(
            stream: &mut RecordMetadataStream,
        ) -> Result<RecordMetadataResult, String>;
        fn record_metadata_stream_time_range(
            stream: &mut RecordMetadataStream,
            start: u64,
            end: u64,
        ) -> Result<(), String>;
        fn record_metadata_stream_into_reader(
            stream: &mut RecordMetadataStream,
        ) -> Result<Reader, String>;
    }
}

type ReaderHandle = CoreReader<Box<dyn BytesSource>>;

enum ParsedValue {
    Json(JsonValue),
    Bytes(Bytes),
}

enum ParserKind {
    Json,
    Bytes,
    SchemaJson,
    SchemaBytes,
}

fn parser_kind_from_str(kind: &str) -> Result<ParserKind, String> {
    match kind.to_ascii_lowercase().as_str() {
        "json" => Ok(ParserKind::Json),
        "bytes" => Ok(ParserKind::Bytes),
        "schema_json" => Ok(ParserKind::SchemaJson),
        "schema_bytes" => Ok(ParserKind::SchemaBytes),
        _ => Err(format!("unsupported parser kind: {kind}")),
    }
}

fn apply_parser_specs(
    mut builder: mcapable_core::ParsedStreamBuilder<'static, ParsedValue>,
    encodings: Vec<String>,
    kinds: Vec<String>,
) -> Result<mcapable_core::ParsedStreamBuilder<'static, ParsedValue>, String> {
    if encodings.len() != kinds.len() {
        return Err("parser encodings and kinds must match in length".to_string());
    }
    for (encoding, kind) in encodings.into_iter().zip(kinds.into_iter()) {
        match parser_kind_from_str(&kind)? {
            ParserKind::Json => {
                builder = builder.parser_message_encoding(encoding, |data| {
                    serde_json::from_slice(data.as_ref())
                        .map(ParsedValue::Json)
                        .map_err(|err| mcapable_core::Error::InvalidRecord(err.to_string()))
                });
            }
            ParserKind::Bytes => {
                builder =
                    builder.parser_message_encoding(encoding, |data| Ok(ParsedValue::Bytes(data)));
            }
            ParserKind::SchemaJson => {
                builder = builder.parser_schema_encoding(encoding, |data| {
                    serde_json::from_slice(data.as_ref())
                        .map(ParsedValue::Json)
                        .map_err(|err| mcapable_core::Error::InvalidRecord(err.to_string()))
                });
            }
            ParserKind::SchemaBytes => {
                builder =
                    builder.parser_schema_encoding(encoding, |data| Ok(ParsedValue::Bytes(data)));
            }
        }
    }
    Ok(builder)
}

struct ReaderBuilder {
    validate_end_magic: bool,
}

struct Reader {
    inner: Option<ReaderHandle>,
}

struct SchemaList {
    items: Vec<Schema>,
}

struct ChannelList {
    items: Vec<Channel>,
}

struct MetadataList {
    items: Vec<Metadata>,
}

struct AttachmentList {
    items: Vec<Attachment>,
}

struct RawMessageStreamInner {
    reader: Box<ReaderHandle>,
    stream: Option<mcapable_core::Stream<'static, RawMessage>>,
}

struct RawMessageStream {
    inner: Option<RawMessageStreamInner>,
}

struct MessageStreamInner {
    reader: Box<ReaderHandle>,
    stream: Option<mcapable_core::Stream<'static, Message>>,
}

struct MessageStream {
    inner: Option<MessageStreamInner>,
}

struct ParsedMessageStreamInner {
    reader: Box<ReaderHandle>,
    stream: Option<mcapable_core::ParsedStream<'static, ParsedValue>>,
}

struct ParsedMessageStream {
    inner: Option<ParsedMessageStreamInner>,
}

struct ChunkStreamInner {
    reader: Box<ReaderHandle>,
    stream: Option<mcapable_core::Stream<'static, Chunk>>,
}

struct ChunkStream {
    inner: Option<ChunkStreamInner>,
}

struct RecordStreamInner {
    reader: Box<ReaderHandle>,
    stream: Option<mcapable_core::Stream<'static, Record>>,
}

struct RecordStream {
    inner: Option<RecordStreamInner>,
}

struct MessageMetadataStreamInner {
    reader: Box<ReaderHandle>,
    stream: Option<mcapable_core::Stream<'static, MessageMetadata>>,
}

struct MessageMetadataStream {
    inner: Option<MessageMetadataStreamInner>,
}

struct RecordMetadataStreamInner {
    reader: Box<ReaderHandle>,
    stream: Option<mcapable_core::Stream<'static, RecordMetadata>>,
}

struct RecordMetadataStream {
    inner: Option<RecordMetadataStreamInner>,
}

fn err_message(message: &str) -> String {
    message.to_string()
}

fn map_err(err: mcapable_core::Error) -> String {
    err.to_string()
}

fn key_values_from_map(map: &HashMap<ByteStr, ByteStr>) -> (Vec<String>, Vec<String>) {
    let mut keys = Vec::with_capacity(map.len());
    let mut values = Vec::with_capacity(map.len());
    for (key, value) in map.iter() {
        keys.push(key.as_ref().to_string());
        values.push(value.as_ref().to_string());
    }
    (keys, values)
}

fn header_to_ffi(header: Header) -> ffi::Header {
    let (metadata_keys, metadata_values) = key_values_from_map(&header.metadata);
    ffi::Header {
        profile: header.profile.as_ref().to_string(),
        library: header.library.as_ref().to_string(),
        metadata_keys,
        metadata_values,
    }
}

fn schema_to_ffi(schema: Schema) -> ffi::Schema {
    ffi::Schema {
        id: schema.id,
        name: schema.name.as_ref().to_string(),
        encoding: schema.encoding.as_ref().to_string(),
        data: schema.data.as_ref().to_vec(),
    }
}

fn channel_to_ffi(channel: Channel) -> ffi::Channel {
    let (metadata_keys, metadata_values) = key_values_from_map(&channel.metadata);
    ffi::Channel {
        id: channel.id,
        topic: channel.topic.as_ref().to_string(),
        message_encoding: channel.message_encoding.as_ref().to_string(),
        schema_id: channel.schema_id,
        metadata_keys,
        metadata_values,
    }
}

fn metadata_record_to_ffi(metadata: Metadata) -> ffi::Metadata {
    let (metadata_keys, metadata_values) = key_values_from_map(&metadata.metadata);
    ffi::Metadata {
        name: metadata.name.as_ref().to_string(),
        metadata_keys,
        metadata_values,
    }
}

fn attachment_to_ffi(attachment: Attachment) -> ffi::Attachment {
    ffi::Attachment {
        log_time: attachment.log_time,
        create_time: attachment.create_time,
        name: attachment.name.as_ref().to_string(),
        media_type: attachment.media_type.as_ref().to_string(),
        data: attachment.data.as_ref().to_vec(),
    }
}

fn raw_message_to_ffi(message: RawMessage) -> ffi::RawMessage {
    ffi::RawMessage {
        channel_id: message.channel_id,
        sequence: message.sequence,
        log_time: message.log_time,
        publish_time: message.publish_time,
        data: message.data_bytes().to_vec(),
    }
}

fn message_to_ffi(message: Message) -> ffi::Message {
    ffi::Message {
        channel_id: message.channel_id,
        sequence: message.sequence,
        log_time: message.log_time,
        publish_time: message.publish_time,
        data: message.data_bytes().to_vec(),
    }
}

fn chunk_to_ffi(chunk: Chunk) -> ffi::Chunk {
    ffi::Chunk {
        message_start_time: chunk.message_start_time,
        message_end_time: chunk.message_end_time,
        uncompressed_size: chunk.uncompressed_size,
        uncompressed_crc: chunk.uncompressed_crc,
        compression: chunk.compression.as_ref().to_string(),
        records: chunk.records.as_ref().to_vec(),
    }
}

fn message_metadata_to_ffi(metadata: MessageMetadata) -> ffi::MessageMetadata {
    ffi::MessageMetadata {
        channel_id: metadata.channel_id,
        sequence: metadata.sequence,
        log_time: metadata.log_time,
        publish_time: metadata.publish_time,
        data_size: metadata.data_size,
    }
}

fn empty_header() -> ffi::Header {
    ffi::Header {
        profile: String::new(),
        library: String::new(),
        metadata_keys: Vec::new(),
        metadata_values: Vec::new(),
    }
}

fn empty_schema() -> ffi::Schema {
    ffi::Schema {
        id: 0,
        name: String::new(),
        encoding: String::new(),
        data: Vec::new(),
    }
}

fn empty_channel() -> ffi::Channel {
    ffi::Channel {
        id: 0,
        topic: String::new(),
        message_encoding: String::new(),
        schema_id: 0,
        metadata_keys: Vec::new(),
        metadata_values: Vec::new(),
    }
}

fn empty_metadata() -> ffi::Metadata {
    ffi::Metadata {
        name: String::new(),
        metadata_keys: Vec::new(),
        metadata_values: Vec::new(),
    }
}

fn empty_attachment() -> ffi::Attachment {
    ffi::Attachment {
        log_time: 0,
        create_time: 0,
        name: String::new(),
        media_type: String::new(),
        data: Vec::new(),
    }
}

fn empty_message() -> ffi::Message {
    ffi::Message {
        channel_id: 0,
        sequence: 0,
        log_time: 0,
        publish_time: 0,
        data: Vec::new(),
    }
}

fn empty_raw_message() -> ffi::RawMessage {
    ffi::RawMessage {
        channel_id: 0,
        sequence: 0,
        log_time: 0,
        publish_time: 0,
        data: Vec::new(),
    }
}

fn empty_chunk() -> ffi::Chunk {
    ffi::Chunk {
        message_start_time: 0,
        message_end_time: 0,
        uncompressed_size: 0,
        uncompressed_crc: 0,
        compression: String::new(),
        records: Vec::new(),
    }
}

fn empty_message_metadata() -> ffi::MessageMetadata {
    ffi::MessageMetadata {
        channel_id: 0,
        sequence: 0,
        log_time: 0,
        publish_time: 0,
        data_size: 0,
    }
}

fn empty_parsed_message() -> ffi::ParsedMessage {
    ffi::ParsedMessage {
        is_json: false,
        json: String::new(),
        bytes: Vec::new(),
    }
}

fn empty_record_metadata() -> ffi::RecordMetadata {
    ffi::RecordMetadata {
        opcode: String::new(),
        length: 0,
        total_len: 0,
        offset: 0,
        source: String::new(),
        message: empty_message_metadata(),
        has_message: false,
    }
}

fn empty_record() -> ffi::Record {
    ffi::Record {
        kind: String::new(),
        header: empty_header(),
        has_header: false,
        schema: empty_schema(),
        has_schema: false,
        channel: empty_channel(),
        has_channel: false,
        message: empty_message(),
        has_message: false,
        chunk: empty_chunk(),
        has_chunk: false,
        attachment: empty_attachment(),
        has_attachment: false,
        metadata: empty_metadata(),
        has_metadata: false,
    }
}

fn record_metadata_to_ffi(metadata: RecordMetadata) -> ffi::RecordMetadata {
    let source = match metadata.source {
        RecordSource::File => "File".to_string(),
        RecordSource::Chunk { chunk_offset } => format!("Chunk({chunk_offset})"),
    };
    let (message, has_message) = match metadata.message {
        Some(message) => (message_metadata_to_ffi(message), true),
        None => (empty_message_metadata(), false),
    };
    ffi::RecordMetadata {
        opcode: format!("{:?}", metadata.opcode),
        length: metadata.length,
        total_len: metadata.total_len,
        offset: metadata.offset,
        source,
        message,
        has_message,
    }
}

fn record_to_ffi(record: Record) -> ffi::Record {
    let kind = match &record {
        Record::Header(_) => "Header",
        Record::Footer(_) => "Footer",
        Record::Schema(_) => "Schema",
        Record::Channel(_) => "Channel",
        Record::Message(_) => "Message",
        Record::Chunk(_) => "Chunk",
        Record::Attachment(_) => "Attachment",
        Record::Metadata(_) => "Metadata",
        Record::SummaryOffset => "SummaryOffset",
        Record::DataEnd => "DataEnd",
        _ => "Other",
    };

    let mut header = empty_header();
    let mut has_header = false;
    let mut schema = empty_schema();
    let mut has_schema = false;
    let mut channel = empty_channel();
    let mut has_channel = false;
    let mut message = empty_message();
    let mut has_message = false;
    let mut chunk = empty_chunk();
    let mut has_chunk = false;
    let mut attachment = empty_attachment();
    let mut has_attachment = false;
    let mut metadata = empty_metadata();
    let mut has_metadata = false;

    match record {
        Record::Header(value) => {
            header = header_to_ffi(value);
            has_header = true;
        }
        Record::Schema(value) => {
            schema = schema_to_ffi(value);
            has_schema = true;
        }
        Record::Channel(value) => {
            channel = channel_to_ffi(value);
            has_channel = true;
        }
        Record::Message(value) => {
            message = message_to_ffi(value);
            has_message = true;
        }
        Record::Chunk(value) => {
            chunk = chunk_to_ffi(value);
            has_chunk = true;
        }
        Record::Attachment(value) => {
            attachment = attachment_to_ffi(value);
            has_attachment = true;
        }
        Record::Metadata(value) => {
            metadata = metadata_record_to_ffi(value);
            has_metadata = true;
        }
        _ => {}
    }

    ffi::Record {
        kind: kind.to_string(),
        header,
        has_header,
        schema,
        has_schema,
        channel,
        has_channel,
        message,
        has_message,
        chunk,
        has_chunk,
        attachment,
        has_attachment,
        metadata,
        has_metadata,
    }
}

fn reader_builder_new() -> ReaderBuilder {
    ReaderBuilder {
        validate_end_magic: true,
    }
}

fn reader_builder_validate_end_magic(builder: &mut ReaderBuilder, validate: bool) {
    builder.validate_end_magic = validate;
}

fn reader_builder_build_from_path(builder: &ReaderBuilder, path: String) -> Result<Reader, String> {
    let file = std::fs::File::open(&path).map_err(|err| format!("Failed to open {path}: {err}"))?;
    let source: Box<dyn BytesSource> = Box::new(ArenaBytesSource::new(file));
    let reader = CoreReaderBuilder::new()
        .validate_end_magic(builder.validate_end_magic)
        .build(source)
        .map_err(map_err)?;
    Ok(Reader {
        inner: Some(reader),
    })
}

fn reader_builder_build_from_bytes(
    builder: &ReaderBuilder,
    bytes: Vec<u8>,
) -> Result<Reader, String> {
    let data: Bytes = bytes.into();
    let source: Box<dyn BytesSource> = Box::new(BytesCursor::new(data));
    let reader = CoreReaderBuilder::new()
        .validate_end_magic(builder.validate_end_magic)
        .build(source)
        .map_err(map_err)?;
    Ok(Reader {
        inner: Some(reader),
    })
}

fn reader_from_path(path: String) -> Result<Reader, String> {
    reader_builder_build_from_path(&reader_builder_new(), path)
}

fn reader_from_bytes(bytes: Vec<u8>) -> Result<Reader, String> {
    reader_builder_build_from_bytes(&reader_builder_new(), bytes)
}

fn reader_header(reader: &mut Reader) -> Result<ffi::Header, String> {
    let inner = reader
        .inner
        .as_mut()
        .ok_or_else(|| err_message("reader is already in use"))?;
    let header = inner.header().map_err(map_err)?;
    Ok(header_to_ffi(header))
}

fn reader_schemas(reader: &mut Reader) -> SchemaList {
    let inner = match reader.inner.as_mut() {
        Some(inner) => inner,
        None => {
            return SchemaList { items: Vec::new() };
        }
    };
    SchemaList {
        items: inner.schemas().values().cloned().collect(),
    }
}

fn reader_channels(reader: &mut Reader) -> ChannelList {
    let inner = match reader.inner.as_mut() {
        Some(inner) => inner,
        None => {
            return ChannelList { items: Vec::new() };
        }
    };
    ChannelList {
        items: inner.channels().values().cloned().collect(),
    }
}

fn reader_metadata(reader: &mut Reader, name: String) -> Result<ffi::Metadata, String> {
    let inner = reader
        .inner
        .as_mut()
        .ok_or_else(|| err_message("reader is already in use"))?;
    let metadata = inner
        .metadata(name.as_str())
        .ok_or_else(|| err_message("metadata not found"))?;
    Ok(metadata_record_to_ffi(metadata))
}

fn reader_all_metadata(reader: &mut Reader) -> MetadataList {
    let inner = match reader.inner.as_mut() {
        Some(inner) => inner,
        None => {
            return MetadataList { items: Vec::new() };
        }
    };
    MetadataList {
        items: inner.all_metadata().values().cloned().collect(),
    }
}

fn reader_attachment(reader: &mut Reader, name: String) -> Result<ffi::Attachment, String> {
    let inner = reader
        .inner
        .as_mut()
        .ok_or_else(|| err_message("reader is already in use"))?;
    let attachment = inner
        .attachment(name.as_str())
        .ok_or_else(|| err_message("attachment not found"))?;
    Ok(attachment_to_ffi(attachment))
}

fn reader_all_attachments(reader: &mut Reader) -> AttachmentList {
    let inner = match reader.inner.as_mut() {
        Some(inner) => inner,
        None => {
            return AttachmentList { items: Vec::new() };
        }
    };
    AttachmentList {
        items: inner.all_attachments().values().cloned().collect(),
    }
}

fn schema_list_len(list: &SchemaList) -> usize {
    list.items.len()
}

fn schema_list_get(list: &SchemaList, index: usize) -> Result<ffi::Schema, String> {
    list.items
        .get(index)
        .cloned()
        .map(schema_to_ffi)
        .ok_or_else(|| err_message("schema index out of bounds"))
}

fn channel_list_len(list: &ChannelList) -> usize {
    list.items.len()
}

fn channel_list_get(list: &ChannelList, index: usize) -> Result<ffi::Channel, String> {
    list.items
        .get(index)
        .cloned()
        .map(channel_to_ffi)
        .ok_or_else(|| err_message("channel index out of bounds"))
}

fn metadata_list_len(list: &MetadataList) -> usize {
    list.items.len()
}

fn metadata_list_get(list: &MetadataList, index: usize) -> Result<ffi::Metadata, String> {
    list.items
        .get(index)
        .cloned()
        .map(metadata_record_to_ffi)
        .ok_or_else(|| err_message("metadata index out of bounds"))
}

fn attachment_list_len(list: &AttachmentList) -> usize {
    list.items.len()
}

fn attachment_list_get(list: &AttachmentList, index: usize) -> Result<ffi::Attachment, String> {
    list.items
        .get(index)
        .cloned()
        .map(attachment_to_ffi)
        .ok_or_else(|| err_message("attachment index out of bounds"))
}

fn reader_raw_messages(reader: &mut Reader) -> Result<RawMessageStream, String> {
    let inner = reader
        .inner
        .take()
        .ok_or_else(|| err_message("reader is already in use"))?;
    let mut reader = Box::new(inner);
    let stream = reader.raw_messages().map_err(map_err)?;
    let stream = unsafe {
        std::mem::transmute::<
            mcapable_core::Stream<'_, RawMessage>,
            mcapable_core::Stream<'static, RawMessage>,
        >(stream)
    };
    Ok(RawMessageStream {
        inner: Some(RawMessageStreamInner {
            reader,
            stream: Some(stream),
        }),
    })
}

fn reader_messages(reader: &mut Reader) -> Result<MessageStream, String> {
    let inner = reader
        .inner
        .take()
        .ok_or_else(|| err_message("reader is already in use"))?;
    let mut reader = Box::new(inner);
    let stream = reader.messages().map_err(map_err)?;
    let stream = unsafe {
        std::mem::transmute::<
            mcapable_core::Stream<'_, Message>,
            mcapable_core::Stream<'static, Message>,
        >(stream)
    };
    Ok(MessageStream {
        inner: Some(MessageStreamInner {
            reader,
            stream: Some(stream),
        }),
    })
}

fn reader_chunks(reader: &mut Reader) -> Result<ChunkStream, String> {
    let inner = reader
        .inner
        .take()
        .ok_or_else(|| err_message("reader is already in use"))?;
    let mut reader = Box::new(inner);
    let stream = reader.chunks();
    let stream = unsafe {
        std::mem::transmute::<mcapable_core::Stream<'_, Chunk>, mcapable_core::Stream<'static, Chunk>>(
            stream,
        )
    };
    Ok(ChunkStream {
        inner: Some(ChunkStreamInner {
            reader,
            stream: Some(stream),
        }),
    })
}

fn reader_records(reader: &mut Reader) -> Result<RecordStream, String> {
    let inner = reader
        .inner
        .take()
        .ok_or_else(|| err_message("reader is already in use"))?;
    let mut reader = Box::new(inner);
    let stream = reader.records();
    let stream = unsafe {
        std::mem::transmute::<
            mcapable_core::Stream<'_, Record>,
            mcapable_core::Stream<'static, Record>,
        >(stream)
    };
    Ok(RecordStream {
        inner: Some(RecordStreamInner {
            reader,
            stream: Some(stream),
        }),
    })
}

fn reader_message_metadata(reader: &mut Reader) -> Result<MessageMetadataStream, String> {
    let inner = reader
        .inner
        .take()
        .ok_or_else(|| err_message("reader is already in use"))?;
    let mut reader = Box::new(inner);
    let stream = reader.message_metadata().map_err(map_err)?;
    let stream = unsafe {
        std::mem::transmute::<
            mcapable_core::Stream<'_, MessageMetadata>,
            mcapable_core::Stream<'static, MessageMetadata>,
        >(stream)
    };
    Ok(MessageMetadataStream {
        inner: Some(MessageMetadataStreamInner {
            reader,
            stream: Some(stream),
        }),
    })
}

fn reader_record_metadata(reader: &mut Reader) -> Result<RecordMetadataStream, String> {
    let inner = reader
        .inner
        .take()
        .ok_or_else(|| err_message("reader is already in use"))?;
    let mut reader = Box::new(inner);
    let stream = reader.record_metadata();
    let stream = unsafe {
        std::mem::transmute::<
            mcapable_core::Stream<'_, RecordMetadata>,
            mcapable_core::Stream<'static, RecordMetadata>,
        >(stream)
    };
    Ok(RecordMetadataStream {
        inner: Some(RecordMetadataStreamInner {
            reader,
            stream: Some(stream),
        }),
    })
}

fn raw_message_stream_next(stream: &mut RawMessageStream) -> Result<ffi::RawMessageResult, String> {
    let inner = match stream.inner.as_mut() {
        Some(inner) => inner,
        None => {
            return Ok(ffi::RawMessageResult {
                has_value: false,
                value: empty_raw_message(),
            });
        }
    };
    let stream = match inner.stream.as_mut() {
        Some(stream) => stream,
        None => {
            return Ok(ffi::RawMessageResult {
                has_value: false,
                value: empty_raw_message(),
            });
        }
    };
    match stream.next() {
        Some(Ok(message)) => Ok(ffi::RawMessageResult {
            has_value: true,
            value: raw_message_to_ffi(message),
        }),
        Some(Err(err)) => Err(map_err(err)),
        None => Ok(ffi::RawMessageResult {
            has_value: false,
            value: empty_raw_message(),
        }),
    }
}

fn raw_message_stream_time_range(
    stream: &mut RawMessageStream,
    start: u64,
    end: u64,
) -> Result<(), String> {
    let inner = stream
        .inner
        .as_mut()
        .ok_or_else(|| err_message("stream is already consumed"))?;
    let current = inner
        .stream
        .take()
        .ok_or_else(|| err_message("stream is already consumed"))?;
    inner.stream = Some(current.time_range(start, end));
    Ok(())
}

fn raw_message_stream_into_reader(stream: &mut RawMessageStream) -> Result<Reader, String> {
    let inner = stream
        .inner
        .take()
        .ok_or_else(|| err_message("stream is already consumed"))?;
    let RawMessageStreamInner { reader, stream } = inner;
    drop(stream);
    Ok(Reader {
        inner: Some(*reader),
    })
}

fn message_stream_next(stream: &mut MessageStream) -> Result<ffi::MessageResult, String> {
    let inner = match stream.inner.as_mut() {
        Some(inner) => inner,
        None => {
            return Ok(ffi::MessageResult {
                has_value: false,
                value: empty_message(),
            });
        }
    };
    let stream = match inner.stream.as_mut() {
        Some(stream) => stream,
        None => {
            return Ok(ffi::MessageResult {
                has_value: false,
                value: empty_message(),
            });
        }
    };
    match stream.next() {
        Some(Ok(message)) => Ok(ffi::MessageResult {
            has_value: true,
            value: message_to_ffi(message),
        }),
        Some(Err(err)) => Err(map_err(err)),
        None => Ok(ffi::MessageResult {
            has_value: false,
            value: empty_message(),
        }),
    }
}

fn message_stream_time_range(
    stream: &mut MessageStream,
    start: u64,
    end: u64,
) -> Result<(), String> {
    let inner = stream
        .inner
        .as_mut()
        .ok_or_else(|| err_message("stream is already consumed"))?;
    let current = inner
        .stream
        .take()
        .ok_or_else(|| err_message("stream is already consumed"))?;
    inner.stream = Some(current.time_range(start, end));
    Ok(())
}

fn message_stream_parsed(stream: &mut MessageStream) -> Result<ParsedMessageStream, String> {
    let inner = stream
        .inner
        .take()
        .ok_or_else(|| err_message("stream is already consumed"))?;
    let MessageStreamInner { reader, stream } = inner;
    let stream = stream.ok_or_else(|| err_message("stream is already consumed"))?;
    let parsed = stream
        .parsed::<ParsedValue>()
        .default_parsers(
            |value| Ok(ParsedValue::Json(value)),
            |data| Ok(ParsedValue::Bytes(data)),
        )
        .build();
    Ok(ParsedMessageStream {
        inner: Some(ParsedMessageStreamInner {
            reader,
            stream: Some(parsed),
        }),
    })
}

fn message_stream_parsed_with(
    stream: &mut MessageStream,
    encodings: Vec<String>,
    kinds: Vec<String>,
) -> Result<ParsedMessageStream, String> {
    let inner = stream
        .inner
        .take()
        .ok_or_else(|| err_message("stream is already consumed"))?;
    let stream = inner
        .stream
        .ok_or_else(|| err_message("stream is already consumed"))?;
    let reader = inner.reader;
    let builder = stream.parsed::<ParsedValue>();
    let parsed = apply_parser_specs(builder, encodings, kinds)?
        .default_parsers(
            |value| Ok(ParsedValue::Json(value)),
            |data| Ok(ParsedValue::Bytes(data)),
        )
        .build();
    Ok(ParsedMessageStream {
        inner: Some(ParsedMessageStreamInner {
            reader,
            stream: Some(parsed),
        }),
    })
}

fn message_stream_into_reader(stream: &mut MessageStream) -> Result<Reader, String> {
    let inner = stream
        .inner
        .take()
        .ok_or_else(|| err_message("stream is already consumed"))?;
    let MessageStreamInner { reader, stream } = inner;
    drop(stream);
    Ok(Reader {
        inner: Some(*reader),
    })
}

fn parsed_message_stream_next(
    stream: &mut ParsedMessageStream,
) -> Result<ffi::ParsedMessageResult, String> {
    let inner = match stream.inner.as_mut() {
        Some(inner) => inner,
        None => {
            return Ok(ffi::ParsedMessageResult {
                has_value: false,
                value: empty_parsed_message(),
            });
        }
    };
    let stream = match inner.stream.as_mut() {
        Some(stream) => stream,
        None => {
            return Ok(ffi::ParsedMessageResult {
                has_value: false,
                value: empty_parsed_message(),
            });
        }
    };
    match stream.next() {
        Some(Ok(parsed)) => {
            let value = match parsed {
                ParsedValue::Json(value) => ffi::ParsedMessage {
                    is_json: true,
                    json: value.to_string(),
                    bytes: Vec::new(),
                },
                ParsedValue::Bytes(bytes) => ffi::ParsedMessage {
                    is_json: false,
                    json: String::new(),
                    bytes: bytes.as_ref().to_vec(),
                },
            };
            Ok(ffi::ParsedMessageResult {
                has_value: true,
                value,
            })
        }
        Some(Err(err)) => Err(map_err(err)),
        None => Ok(ffi::ParsedMessageResult {
            has_value: false,
            value: empty_parsed_message(),
        }),
    }
}

fn parsed_message_stream_into_reader(stream: &mut ParsedMessageStream) -> Result<Reader, String> {
    let inner = stream
        .inner
        .take()
        .ok_or_else(|| err_message("stream is already consumed"))?;
    let ParsedMessageStreamInner { reader, stream } = inner;
    drop(stream);
    Ok(Reader {
        inner: Some(*reader),
    })
}

fn chunk_stream_next(stream: &mut ChunkStream) -> Result<ffi::ChunkResult, String> {
    let inner = match stream.inner.as_mut() {
        Some(inner) => inner,
        None => {
            return Ok(ffi::ChunkResult {
                has_value: false,
                value: empty_chunk(),
            });
        }
    };
    let stream = match inner.stream.as_mut() {
        Some(stream) => stream,
        None => {
            return Ok(ffi::ChunkResult {
                has_value: false,
                value: empty_chunk(),
            });
        }
    };
    match stream.next() {
        Some(Ok(chunk)) => Ok(ffi::ChunkResult {
            has_value: true,
            value: chunk_to_ffi(chunk),
        }),
        Some(Err(err)) => Err(map_err(err)),
        None => Ok(ffi::ChunkResult {
            has_value: false,
            value: empty_chunk(),
        }),
    }
}

fn chunk_stream_time_range(stream: &mut ChunkStream, start: u64, end: u64) -> Result<(), String> {
    let inner = stream
        .inner
        .as_mut()
        .ok_or_else(|| err_message("stream is already consumed"))?;
    let current = inner
        .stream
        .take()
        .ok_or_else(|| err_message("stream is already consumed"))?;
    inner.stream = Some(current.time_range(start, end));
    Ok(())
}

fn chunk_stream_into_reader(stream: &mut ChunkStream) -> Result<Reader, String> {
    let inner = stream
        .inner
        .take()
        .ok_or_else(|| err_message("stream is already consumed"))?;
    let ChunkStreamInner { reader, stream } = inner;
    drop(stream);
    Ok(Reader {
        inner: Some(*reader),
    })
}

fn record_stream_next(stream: &mut RecordStream) -> Result<ffi::RecordResult, String> {
    let inner = match stream.inner.as_mut() {
        Some(inner) => inner,
        None => {
            return Ok(ffi::RecordResult {
                has_value: false,
                value: empty_record(),
            });
        }
    };
    let stream = match inner.stream.as_mut() {
        Some(stream) => stream,
        None => {
            return Ok(ffi::RecordResult {
                has_value: false,
                value: empty_record(),
            });
        }
    };
    match stream.next() {
        Some(Ok(record)) => Ok(ffi::RecordResult {
            has_value: true,
            value: record_to_ffi(record),
        }),
        Some(Err(err)) => Err(map_err(err)),
        None => Ok(ffi::RecordResult {
            has_value: false,
            value: empty_record(),
        }),
    }
}

fn record_stream_time_range(stream: &mut RecordStream, start: u64, end: u64) -> Result<(), String> {
    let inner = stream
        .inner
        .as_mut()
        .ok_or_else(|| err_message("stream is already consumed"))?;
    let current = inner
        .stream
        .take()
        .ok_or_else(|| err_message("stream is already consumed"))?;
    inner.stream = Some(current.time_range(start, end));
    Ok(())
}

fn record_stream_into_reader(stream: &mut RecordStream) -> Result<Reader, String> {
    let inner = stream
        .inner
        .take()
        .ok_or_else(|| err_message("stream is already consumed"))?;
    let RecordStreamInner { reader, stream } = inner;
    drop(stream);
    Ok(Reader {
        inner: Some(*reader),
    })
}

fn message_metadata_stream_next(
    stream: &mut MessageMetadataStream,
) -> Result<ffi::MessageMetadataResult, String> {
    let inner = match stream.inner.as_mut() {
        Some(inner) => inner,
        None => {
            return Ok(ffi::MessageMetadataResult {
                has_value: false,
                value: empty_message_metadata(),
            });
        }
    };
    let stream = match inner.stream.as_mut() {
        Some(stream) => stream,
        None => {
            return Ok(ffi::MessageMetadataResult {
                has_value: false,
                value: empty_message_metadata(),
            });
        }
    };
    match stream.next() {
        Some(Ok(metadata)) => Ok(ffi::MessageMetadataResult {
            has_value: true,
            value: message_metadata_to_ffi(metadata),
        }),
        Some(Err(err)) => Err(map_err(err)),
        None => Ok(ffi::MessageMetadataResult {
            has_value: false,
            value: empty_message_metadata(),
        }),
    }
}

fn message_metadata_stream_time_range(
    stream: &mut MessageMetadataStream,
    start: u64,
    end: u64,
) -> Result<(), String> {
    let inner = stream
        .inner
        .as_mut()
        .ok_or_else(|| err_message("stream is already consumed"))?;
    let current = inner
        .stream
        .take()
        .ok_or_else(|| err_message("stream is already consumed"))?;
    inner.stream = Some(current.time_range(start, end));
    Ok(())
}

fn message_metadata_stream_into_reader(
    stream: &mut MessageMetadataStream,
) -> Result<Reader, String> {
    let inner = stream
        .inner
        .take()
        .ok_or_else(|| err_message("stream is already consumed"))?;
    let MessageMetadataStreamInner { reader, stream } = inner;
    drop(stream);
    Ok(Reader {
        inner: Some(*reader),
    })
}

fn record_metadata_stream_next(
    stream: &mut RecordMetadataStream,
) -> Result<ffi::RecordMetadataResult, String> {
    let inner = match stream.inner.as_mut() {
        Some(inner) => inner,
        None => {
            return Ok(ffi::RecordMetadataResult {
                has_value: false,
                value: empty_record_metadata(),
            });
        }
    };
    let stream = match inner.stream.as_mut() {
        Some(stream) => stream,
        None => {
            return Ok(ffi::RecordMetadataResult {
                has_value: false,
                value: empty_record_metadata(),
            });
        }
    };
    match stream.next() {
        Some(Ok(metadata)) => Ok(ffi::RecordMetadataResult {
            has_value: true,
            value: record_metadata_to_ffi(metadata),
        }),
        Some(Err(err)) => Err(map_err(err)),
        None => Ok(ffi::RecordMetadataResult {
            has_value: false,
            value: empty_record_metadata(),
        }),
    }
}

fn record_metadata_stream_time_range(
    stream: &mut RecordMetadataStream,
    start: u64,
    end: u64,
) -> Result<(), String> {
    let inner = stream
        .inner
        .as_mut()
        .ok_or_else(|| err_message("stream is already consumed"))?;
    let current = inner
        .stream
        .take()
        .ok_or_else(|| err_message("stream is already consumed"))?;
    inner.stream = Some(current.time_range(start, end));
    Ok(())
}

fn record_metadata_stream_into_reader(stream: &mut RecordMetadataStream) -> Result<Reader, String> {
    let inner = stream
        .inner
        .take()
        .ok_or_else(|| err_message("stream is already consumed"))?;
    let RecordMetadataStreamInner { reader, stream } = inner;
    drop(stream);
    Ok(Reader {
        inner: Some(*reader),
    })
}
