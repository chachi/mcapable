use bytes::Bytes;
use cxx::CxxString;
use mcapable_core::reader::{Builder as CoreReaderBuilder, Reader as CoreReader};
use mcapable_core::source::{ArenaBytesSource, BytesCursor, BytesSource};
use mcapable_core::zero_copy::ByteStr;
use mcapable_core::{
    Attachment, Channel, Chunk, Header, Message, MessageMetadata, Metadata, RawMessage, Record,
    RecordMetadata, RecordSource, Schema,
};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

type ReaderHandle = CoreReader<Box<dyn BytesSource>>;

type CxxResult<T> = Result<T, String>;

#[cxx::bridge(namespace = "mcapable")]
mod ffi {
    struct KeyValue {
        key: String,
        value: String,
    }

    struct Header {
        profile: String,
        library: String,
        metadata: Vec<KeyValue>,
    }

    struct Schema {
        id: u16,
        name: String,
        encoding: String,
        data: Vec<u8>,
    }

    struct Channel {
        id: u16,
        topic: String,
        message_encoding: String,
        schema_id: u16,
        metadata: Vec<KeyValue>,
    }

    struct Metadata {
        name: String,
        metadata: Vec<KeyValue>,
    }

    struct Attachment {
        log_time: u64,
        create_time: u64,
        name: String,
        media_type: String,
        data: Vec<u8>,
    }

    struct RawMessage {
        channel_id: u16,
        sequence: u32,
        log_time: u64,
        publish_time: u64,
        data: Vec<u8>,
    }

    struct Message {
        channel_id: u16,
        sequence: u32,
        log_time: u64,
        publish_time: u64,
        data: Vec<u8>,
    }

    struct ParsedMessage {
        is_json: bool,
        json: String,
        bytes: Vec<u8>,
    }

    struct ParserSpec {
        encoding: String,
        kind: String,
    }

    struct Chunk {
        message_start_time: u64,
        message_end_time: u64,
        uncompressed_size: u64,
        uncompressed_crc: u32,
        compression: String,
        records: Vec<u8>,
    }

    struct MessageMetadata {
        channel_id: u16,
        sequence: u32,
        log_time: u64,
        publish_time: u64,
        data_size: u64,
    }

    struct RecordMetadata {
        opcode: String,
        length: u64,
        total_len: u64,
        offset: u64,
        source: String,
        message: MessageMetadata,
        has_message: bool,
    }

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

    extern "Rust" {
        type ReaderBuilder;
        type Reader;
        type RawMessageStream;
        type MessageStream;
        type ParsedMessageStream;
        type ChunkStream;
        type RecordStream;
        type MessageMetadataStream;
        type RecordMetadataStream;

        fn reader_builder_new() -> Box<ReaderBuilder>;
        fn reader_builder_validate_end_magic(builder: &mut ReaderBuilder, validate: bool);
        fn reader_builder_build_from_path(
            builder: &ReaderBuilder,
            path: &CxxString,
        ) -> Result<Box<Reader>>;
        fn reader_builder_build_from_bytes(
            builder: &ReaderBuilder,
            bytes: &Vec<u8>,
        ) -> Result<Box<Reader>>;

        fn reader_from_path(path: &CxxString) -> Result<Box<Reader>>;
        fn reader_from_bytes(bytes: &Vec<u8>) -> Result<Box<Reader>>;

        fn reader_header(reader: &mut Reader) -> Result<Header>;
        fn reader_schemas(reader: &mut Reader) -> Vec<Schema>;
        fn reader_channels(reader: &mut Reader) -> Vec<Channel>;
        fn reader_metadata(reader: &mut Reader, name: &CxxString) -> Result<Metadata>;
        fn reader_all_metadata(reader: &mut Reader) -> Vec<Metadata>;
        fn reader_attachment(reader: &mut Reader, name: &CxxString) -> Result<Attachment>;
        fn reader_all_attachments(reader: &mut Reader) -> Vec<Attachment>;

        fn reader_raw_messages(reader: &mut Reader) -> Result<Box<RawMessageStream>>;
        fn reader_messages(reader: &mut Reader) -> Result<Box<MessageStream>>;
        fn reader_chunks(reader: &mut Reader) -> Box<ChunkStream>;
        fn reader_records(reader: &mut Reader) -> Box<RecordStream>;
        fn reader_message_metadata(reader: &mut Reader) -> Result<Box<MessageMetadataStream>>;
        fn reader_record_metadata(reader: &mut Reader) -> Box<RecordMetadataStream>;

        fn message_stream_parsed(stream: &mut MessageStream) -> Result<Box<ParsedMessageStream>>;
        fn message_stream_parsed_with(
            stream: &mut MessageStream,
            parsers: &[ParserSpec],
        ) -> Result<Box<ParsedMessageStream>>;

        fn raw_message_stream_time_range(
            stream: &mut RawMessageStream,
            start: u64,
            end: u64,
        ) -> Result<()>;
        fn message_stream_time_range(
            stream: &mut MessageStream,
            start: u64,
            end: u64,
        ) -> Result<()>;
        fn chunk_stream_time_range(stream: &mut ChunkStream, start: u64, end: u64) -> Result<()>;
        fn record_stream_time_range(stream: &mut RecordStream, start: u64, end: u64) -> Result<()>;
        fn message_metadata_stream_time_range(
            stream: &mut MessageMetadataStream,
            start: u64,
            end: u64,
        ) -> Result<()>;
        fn record_metadata_stream_time_range(
            stream: &mut RecordMetadataStream,
            start: u64,
            end: u64,
        ) -> Result<()>;

        fn raw_message_stream_next(
            stream: &mut RawMessageStream,
            out: &mut RawMessage,
        ) -> Result<bool>;
        fn message_stream_next(stream: &mut MessageStream, out: &mut Message) -> Result<bool>;
        fn chunk_stream_next(stream: &mut ChunkStream, out: &mut Chunk) -> Result<bool>;
        fn record_stream_next(stream: &mut RecordStream, out: &mut Record) -> Result<bool>;
        fn message_metadata_stream_next(
            stream: &mut MessageMetadataStream,
            out: &mut MessageMetadata,
        ) -> Result<bool>;
        fn record_metadata_stream_next(
            stream: &mut RecordMetadataStream,
            out: &mut RecordMetadata,
        ) -> Result<bool>;
        fn parsed_message_stream_next(
            stream: &mut ParsedMessageStream,
            out: &mut ParsedMessage,
        ) -> Result<bool>;

        fn raw_message_stream_into_reader(stream: &mut RawMessageStream) -> Result<Box<Reader>>;
        fn message_stream_into_reader(stream: &mut MessageStream) -> Result<Box<Reader>>;
        fn chunk_stream_into_reader(stream: &mut ChunkStream) -> Result<Box<Reader>>;
        fn record_stream_into_reader(stream: &mut RecordStream) -> Result<Box<Reader>>;
        fn message_metadata_stream_into_reader(
            stream: &mut MessageMetadataStream,
        ) -> Result<Box<Reader>>;
        fn record_metadata_stream_into_reader(
            stream: &mut RecordMetadataStream,
        ) -> Result<Box<Reader>>;
        fn parsed_message_stream_into_reader(
            stream: &mut ParsedMessageStream,
        ) -> Result<Box<Reader>>;
    }
}

struct ReaderBuilder {
    validate_end_magic: bool,
}

struct Reader {
    inner: Option<ReaderHandle>,
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

fn parser_kind_from_str(kind: &str) -> CxxResult<ParserKind> {
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
    specs: &[ffi::ParserSpec],
) -> CxxResult<mcapable_core::ParsedStreamBuilder<'static, ParsedValue>> {
    for spec in specs {
        match parser_kind_from_str(&spec.kind)? {
            ParserKind::Json => {
                builder = builder.parser_message_encoding(spec.encoding.clone(), |data| {
                    serde_json::from_slice(data.as_ref())
                        .map(ParsedValue::Json)
                        .map_err(|err| mcapable_core::Error::InvalidRecord(err.to_string()))
                });
            }
            ParserKind::Bytes => {
                builder = builder.parser_message_encoding(spec.encoding.clone(), |data| {
                    Ok(ParsedValue::Bytes(data))
                });
            }
            ParserKind::SchemaJson => {
                builder = builder.parser_schema_encoding(spec.encoding.clone(), |data| {
                    serde_json::from_slice(data.as_ref())
                        .map(ParsedValue::Json)
                        .map_err(|err| mcapable_core::Error::InvalidRecord(err.to_string()))
                });
            }
            ParserKind::SchemaBytes => {
                builder = builder.parser_schema_encoding(spec.encoding.clone(), |data| {
                    Ok(ParsedValue::Bytes(data))
                });
            }
        }
    }
    Ok(builder)
}

struct ParsedMessageStreamInner {
    reader: Box<ReaderHandle>,
    stream: Option<mcapable_core::ParsedStream<'static, ParsedValue>>,
}

struct ParsedMessageStream {
    inner: Option<ParsedMessageStreamInner>,
}

fn map_err(err: mcapable_core::Error) -> String {
    err.to_string()
}

fn err_message(message: &str) -> String {
    message.to_string()
}

fn byte_str_to_string(value: &ByteStr) -> String {
    value.as_ref().to_string()
}

fn metadata_to_vec(metadata: &HashMap<ByteStr, ByteStr>) -> Vec<ffi::KeyValue> {
    metadata
        .iter()
        .map(|(k, v)| ffi::KeyValue {
            key: byte_str_to_string(k),
            value: byte_str_to_string(v),
        })
        .collect()
}

fn empty_header() -> ffi::Header {
    ffi::Header {
        profile: String::new(),
        library: String::new(),
        metadata: Vec::new(),
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
        metadata: Vec::new(),
    }
}

fn empty_metadata() -> ffi::Metadata {
    ffi::Metadata {
        name: String::new(),
        metadata: Vec::new(),
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

fn header_to_ffi(header: Header) -> ffi::Header {
    ffi::Header {
        profile: byte_str_to_string(&header.profile),
        library: byte_str_to_string(&header.library),
        metadata: metadata_to_vec(&header.metadata),
    }
}

fn schema_to_ffi(schema: Schema) -> ffi::Schema {
    ffi::Schema {
        id: schema.id,
        name: byte_str_to_string(&schema.name),
        encoding: byte_str_to_string(&schema.encoding),
        data: schema.data.to_vec(),
    }
}

fn channel_to_ffi(channel: Channel) -> ffi::Channel {
    ffi::Channel {
        id: channel.id,
        topic: byte_str_to_string(&channel.topic),
        message_encoding: byte_str_to_string(&channel.message_encoding),
        schema_id: channel.schema_id,
        metadata: metadata_to_vec(&channel.metadata),
    }
}

fn metadata_record_to_ffi(metadata: Metadata) -> ffi::Metadata {
    ffi::Metadata {
        name: byte_str_to_string(&metadata.name),
        metadata: metadata_to_vec(&metadata.metadata),
    }
}

fn attachment_to_ffi(attachment: Attachment) -> ffi::Attachment {
    ffi::Attachment {
        log_time: attachment.log_time,
        create_time: attachment.create_time,
        name: byte_str_to_string(&attachment.name),
        media_type: byte_str_to_string(&attachment.media_type),
        data: attachment.data.to_vec(),
    }
}

fn raw_message_to_ffi(raw: RawMessage) -> ffi::RawMessage {
    ffi::RawMessage {
        channel_id: raw.channel_id,
        sequence: raw.sequence,
        log_time: raw.log_time,
        publish_time: raw.publish_time,
        data: raw.data_bytes().to_vec(),
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

fn parsed_value_to_ffi(value: ParsedValue) -> ffi::ParsedMessage {
    match value {
        ParsedValue::Json(value) => ffi::ParsedMessage {
            is_json: true,
            json: value.to_string(),
            bytes: Vec::new(),
        },
        ParsedValue::Bytes(bytes) => ffi::ParsedMessage {
            is_json: false,
            json: String::new(),
            bytes: bytes.to_vec(),
        },
    }
}

fn chunk_to_ffi(chunk: Chunk) -> ffi::Chunk {
    ffi::Chunk {
        message_start_time: chunk.message_start_time,
        message_end_time: chunk.message_end_time,
        uncompressed_size: chunk.uncompressed_size,
        uncompressed_crc: chunk.uncompressed_crc,
        compression: byte_str_to_string(&chunk.compression),
        records: chunk.records.to_vec(),
    }
}

fn message_metadata_to_ffi(message: MessageMetadata) -> ffi::MessageMetadata {
    ffi::MessageMetadata {
        channel_id: message.channel_id,
        sequence: message.sequence,
        log_time: message.log_time,
        publish_time: message.publish_time,
        data_size: message.data_size,
    }
}

fn record_metadata_to_ffi(record: RecordMetadata) -> ffi::RecordMetadata {
    let (message, has_message) = match record.message {
        Some(message) => (message_metadata_to_ffi(message), true),
        None => (empty_message_metadata(), false),
    };
    ffi::RecordMetadata {
        opcode: format!("{:?}", record.opcode),
        length: record.length,
        total_len: record.total_len,
        offset: record.offset,
        source: match record.source {
            RecordSource::File => "File".to_string(),
            RecordSource::Chunk { chunk_offset } => format!("Chunk({chunk_offset})"),
        },
        message,
        has_message,
    }
}

fn record_to_ffi(record: Record) -> ffi::Record {
    match record {
        Record::Header(header) => ffi::Record {
            kind: "Header".to_string(),
            header: header_to_ffi(header),
            has_header: true,
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
        },
        Record::Footer(_) => ffi::Record {
            kind: "Footer".to_string(),
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
        },
        Record::Schema(schema) => ffi::Record {
            kind: "Schema".to_string(),
            header: empty_header(),
            has_header: false,
            schema: schema_to_ffi(schema),
            has_schema: true,
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
        },
        Record::Channel(channel) => ffi::Record {
            kind: "Channel".to_string(),
            header: empty_header(),
            has_header: false,
            schema: empty_schema(),
            has_schema: false,
            channel: channel_to_ffi(channel),
            has_channel: true,
            message: empty_message(),
            has_message: false,
            chunk: empty_chunk(),
            has_chunk: false,
            attachment: empty_attachment(),
            has_attachment: false,
            metadata: empty_metadata(),
            has_metadata: false,
        },
        Record::Message(message) => ffi::Record {
            kind: "Message".to_string(),
            header: empty_header(),
            has_header: false,
            schema: empty_schema(),
            has_schema: false,
            channel: empty_channel(),
            has_channel: false,
            message: message_to_ffi(message),
            has_message: true,
            chunk: empty_chunk(),
            has_chunk: false,
            attachment: empty_attachment(),
            has_attachment: false,
            metadata: empty_metadata(),
            has_metadata: false,
        },
        Record::Chunk(chunk) => ffi::Record {
            kind: "Chunk".to_string(),
            header: empty_header(),
            has_header: false,
            schema: empty_schema(),
            has_schema: false,
            channel: empty_channel(),
            has_channel: false,
            message: empty_message(),
            has_message: false,
            chunk: chunk_to_ffi(chunk),
            has_chunk: true,
            attachment: empty_attachment(),
            has_attachment: false,
            metadata: empty_metadata(),
            has_metadata: false,
        },
        Record::Attachment(attachment) => ffi::Record {
            kind: "Attachment".to_string(),
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
            attachment: attachment_to_ffi(attachment),
            has_attachment: true,
            metadata: empty_metadata(),
            has_metadata: false,
        },
        Record::Metadata(metadata) => ffi::Record {
            kind: "Metadata".to_string(),
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
            metadata: metadata_record_to_ffi(metadata),
            has_metadata: true,
        },
        Record::SummaryOffset => ffi::Record {
            kind: "SummaryOffset".to_string(),
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
        },
        Record::DataEnd => ffi::Record {
            kind: "DataEnd".to_string(),
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
        },
        _ => ffi::Record {
            kind: "Other".to_string(),
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
        },
    }
}

fn reader_builder_new() -> Box<ReaderBuilder> {
    Box::new(ReaderBuilder {
        validate_end_magic: true,
    })
}

fn reader_builder_validate_end_magic(builder: &mut ReaderBuilder, validate: bool) {
    builder.validate_end_magic = validate;
}

fn reader_builder_build_from_path(
    builder: &ReaderBuilder,
    path: &CxxString,
) -> CxxResult<Box<Reader>> {
    let file = std::fs::File::open(path.to_string())
        .map_err(|err| format!("Failed to open {path}: {err}"))?;
    let source: Box<dyn BytesSource> = Box::new(ArenaBytesSource::new(file));
    let reader = CoreReaderBuilder::new()
        .validate_end_magic(builder.validate_end_magic)
        .build(source)
        .map_err(map_err)?;
    Ok(Box::new(Reader {
        inner: Some(reader),
    }))
}

#[allow(clippy::ptr_arg)]
fn reader_builder_build_from_bytes(
    builder: &ReaderBuilder,
    bytes: &Vec<u8>,
) -> CxxResult<Box<Reader>> {
    let data = bytes.clone().into();
    let source: Box<dyn BytesSource> = Box::new(BytesCursor::new(data));
    let reader = CoreReaderBuilder::new()
        .validate_end_magic(builder.validate_end_magic)
        .build(source)
        .map_err(map_err)?;
    Ok(Box::new(Reader {
        inner: Some(reader),
    }))
}

fn reader_from_path(path: &CxxString) -> CxxResult<Box<Reader>> {
    reader_builder_build_from_path(&ReaderBuilder::new(), path)
}

#[allow(clippy::ptr_arg)]
fn reader_from_bytes(bytes: &Vec<u8>) -> CxxResult<Box<Reader>> {
    reader_builder_build_from_bytes(&ReaderBuilder::new(), bytes)
}

impl ReaderBuilder {
    fn new() -> Self {
        Self {
            validate_end_magic: true,
        }
    }
}

fn reader_header(reader: &mut Reader) -> CxxResult<ffi::Header> {
    let inner = reader
        .inner
        .as_mut()
        .ok_or_else(|| err_message("reader is already in use"))?;
    let header = inner.header().map_err(map_err)?;
    Ok(header_to_ffi(header))
}

fn reader_schemas(reader: &mut Reader) -> Vec<ffi::Schema> {
    let inner = match reader.inner.as_mut() {
        Some(inner) => inner,
        None => return Vec::new(),
    };
    inner
        .schemas()
        .values()
        .cloned()
        .map(schema_to_ffi)
        .collect()
}

fn reader_channels(reader: &mut Reader) -> Vec<ffi::Channel> {
    let inner = match reader.inner.as_mut() {
        Some(inner) => inner,
        None => return Vec::new(),
    };
    inner
        .channels()
        .values()
        .cloned()
        .map(channel_to_ffi)
        .collect()
}

fn reader_metadata(reader: &mut Reader, name: &CxxString) -> CxxResult<ffi::Metadata> {
    let inner = reader
        .inner
        .as_mut()
        .ok_or_else(|| err_message("reader is already in use"))?;
    let metadata = inner
        .metadata(name.to_string().as_str())
        .ok_or_else(|| err_message("metadata not found"))?;
    Ok(metadata_record_to_ffi(metadata))
}

fn reader_all_metadata(reader: &mut Reader) -> Vec<ffi::Metadata> {
    let inner = match reader.inner.as_mut() {
        Some(inner) => inner,
        None => return Vec::new(),
    };
    inner
        .all_metadata()
        .values()
        .cloned()
        .map(metadata_record_to_ffi)
        .collect()
}

fn reader_attachment(reader: &mut Reader, name: &CxxString) -> CxxResult<ffi::Attachment> {
    let inner = reader
        .inner
        .as_mut()
        .ok_or_else(|| err_message("reader is already in use"))?;
    let attachment = inner
        .attachment(name.to_string().as_str())
        .ok_or_else(|| err_message("attachment not found"))?;
    Ok(attachment_to_ffi(attachment))
}

fn reader_all_attachments(reader: &mut Reader) -> Vec<ffi::Attachment> {
    let inner = match reader.inner.as_mut() {
        Some(inner) => inner,
        None => return Vec::new(),
    };
    inner
        .all_attachments()
        .values()
        .cloned()
        .map(attachment_to_ffi)
        .collect()
}

fn reader_raw_messages(reader: &mut Reader) -> CxxResult<Box<RawMessageStream>> {
    let reader = reader
        .inner
        .take()
        .ok_or_else(|| err_message("reader is already in use"))?;
    let mut reader = Box::new(reader);
    let stream = reader.raw_messages().map_err(map_err)?;
    let stream = unsafe {
        std::mem::transmute::<
            mcapable_core::Stream<'_, RawMessage>,
            mcapable_core::Stream<'static, RawMessage>,
        >(stream)
    };
    Ok(Box::new(RawMessageStream {
        inner: Some(RawMessageStreamInner {
            reader,
            stream: Some(stream),
        }),
    }))
}

fn reader_messages(reader: &mut Reader) -> CxxResult<Box<MessageStream>> {
    let reader = reader
        .inner
        .take()
        .ok_or_else(|| err_message("reader is already in use"))?;
    let mut reader = Box::new(reader);
    let stream = reader.messages().map_err(map_err)?;
    let stream = unsafe {
        std::mem::transmute::<
            mcapable_core::Stream<'_, Message>,
            mcapable_core::Stream<'static, Message>,
        >(stream)
    };
    Ok(Box::new(MessageStream {
        inner: Some(MessageStreamInner {
            reader,
            stream: Some(stream),
        }),
    }))
}

fn message_stream_parsed(stream: &mut MessageStream) -> CxxResult<Box<ParsedMessageStream>> {
    let inner = stream
        .inner
        .take()
        .ok_or_else(|| err_message("stream is already consumed"))?;
    let stream = inner
        .stream
        .ok_or_else(|| err_message("stream is already consumed"))?;
    let reader = inner.reader;
    let parsed = stream
        .parsed::<ParsedValue>()
        .default_parsers(
            |value| Ok(ParsedValue::Json(value)),
            |data| Ok(ParsedValue::Bytes(data)),
        )
        .build();
    Ok(Box::new(ParsedMessageStream {
        inner: Some(ParsedMessageStreamInner {
            reader,
            stream: Some(parsed),
        }),
    }))
}

fn message_stream_parsed_with(
    stream: &mut MessageStream,
    parsers: &[ffi::ParserSpec],
) -> CxxResult<Box<ParsedMessageStream>> {
    let inner = stream
        .inner
        .take()
        .ok_or_else(|| err_message("stream is already consumed"))?;
    let stream = inner
        .stream
        .ok_or_else(|| err_message("stream is already consumed"))?;
    let reader = inner.reader;
    let builder = stream.parsed::<ParsedValue>();
    let parsed = apply_parser_specs(builder, parsers)?
        .default_parsers(
            |value| Ok(ParsedValue::Json(value)),
            |data| Ok(ParsedValue::Bytes(data)),
        )
        .build();
    Ok(Box::new(ParsedMessageStream {
        inner: Some(ParsedMessageStreamInner {
            reader,
            stream: Some(parsed),
        }),
    }))
}

fn reader_chunks(reader: &mut Reader) -> Box<ChunkStream> {
    let reader = match reader.inner.take() {
        Some(reader) => reader,
        None => {
            return Box::new(ChunkStream { inner: None });
        }
    };
    let mut reader = Box::new(reader);
    let stream = reader.chunks();
    let stream = unsafe {
        std::mem::transmute::<mcapable_core::Stream<'_, Chunk>, mcapable_core::Stream<'static, Chunk>>(
            stream,
        )
    };
    Box::new(ChunkStream {
        inner: Some(ChunkStreamInner {
            reader,
            stream: Some(stream),
        }),
    })
}

fn reader_records(reader: &mut Reader) -> Box<RecordStream> {
    let reader = match reader.inner.take() {
        Some(reader) => reader,
        None => {
            return Box::new(RecordStream { inner: None });
        }
    };
    let mut reader = Box::new(reader);
    let stream = reader.records();
    let stream = unsafe {
        std::mem::transmute::<
            mcapable_core::Stream<'_, Record>,
            mcapable_core::Stream<'static, Record>,
        >(stream)
    };
    Box::new(RecordStream {
        inner: Some(RecordStreamInner {
            reader,
            stream: Some(stream),
        }),
    })
}

fn reader_message_metadata(reader: &mut Reader) -> CxxResult<Box<MessageMetadataStream>> {
    let reader = reader
        .inner
        .take()
        .ok_or_else(|| err_message("reader is already in use"))?;
    let mut reader = Box::new(reader);
    let stream = reader.message_metadata().map_err(map_err)?;
    let stream = unsafe {
        std::mem::transmute::<
            mcapable_core::Stream<'_, MessageMetadata>,
            mcapable_core::Stream<'static, MessageMetadata>,
        >(stream)
    };
    Ok(Box::new(MessageMetadataStream {
        inner: Some(MessageMetadataStreamInner {
            reader,
            stream: Some(stream),
        }),
    }))
}

fn reader_record_metadata(reader: &mut Reader) -> Box<RecordMetadataStream> {
    let reader = match reader.inner.take() {
        Some(reader) => reader,
        None => {
            return Box::new(RecordMetadataStream { inner: None });
        }
    };
    let mut reader = Box::new(reader);
    let stream = reader.record_metadata();
    let stream = unsafe {
        std::mem::transmute::<
            mcapable_core::Stream<'_, RecordMetadata>,
            mcapable_core::Stream<'static, RecordMetadata>,
        >(stream)
    };
    Box::new(RecordMetadataStream {
        inner: Some(RecordMetadataStreamInner {
            reader,
            stream: Some(stream),
        }),
    })
}

fn raw_message_stream_time_range(
    stream: &mut RawMessageStream,
    start: u64,
    end: u64,
) -> CxxResult<()> {
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

fn message_stream_time_range(stream: &mut MessageStream, start: u64, end: u64) -> CxxResult<()> {
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

fn chunk_stream_time_range(stream: &mut ChunkStream, start: u64, end: u64) -> CxxResult<()> {
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

fn record_stream_time_range(stream: &mut RecordStream, start: u64, end: u64) -> CxxResult<()> {
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

fn message_metadata_stream_time_range(
    stream: &mut MessageMetadataStream,
    start: u64,
    end: u64,
) -> CxxResult<()> {
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

fn record_metadata_stream_time_range(
    stream: &mut RecordMetadataStream,
    start: u64,
    end: u64,
) -> CxxResult<()> {
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

fn raw_message_stream_next(
    stream: &mut RawMessageStream,
    out: &mut ffi::RawMessage,
) -> CxxResult<bool> {
    let inner = match stream.inner.as_mut() {
        Some(inner) => inner,
        None => return Ok(false),
    };
    let stream = match inner.stream.as_mut() {
        Some(stream) => stream,
        None => return Ok(false),
    };
    match stream.next() {
        Some(Ok(raw)) => {
            *out = raw_message_to_ffi(raw);
            Ok(true)
        }
        Some(Err(err)) => Err(map_err(err)),
        None => Ok(false),
    }
}

fn message_stream_next(stream: &mut MessageStream, out: &mut ffi::Message) -> CxxResult<bool> {
    let inner = match stream.inner.as_mut() {
        Some(inner) => inner,
        None => return Ok(false),
    };
    let stream = match inner.stream.as_mut() {
        Some(stream) => stream,
        None => return Ok(false),
    };
    match stream.next() {
        Some(Ok(message)) => {
            *out = message_to_ffi(message);
            Ok(true)
        }
        Some(Err(err)) => Err(map_err(err)),
        None => Ok(false),
    }
}

fn chunk_stream_next(stream: &mut ChunkStream, out: &mut ffi::Chunk) -> CxxResult<bool> {
    let inner = match stream.inner.as_mut() {
        Some(inner) => inner,
        None => return Ok(false),
    };
    let stream = match inner.stream.as_mut() {
        Some(stream) => stream,
        None => return Ok(false),
    };
    match stream.next() {
        Some(Ok(chunk)) => {
            *out = chunk_to_ffi(chunk);
            Ok(true)
        }
        Some(Err(err)) => Err(map_err(err)),
        None => Ok(false),
    }
}

fn record_stream_next(stream: &mut RecordStream, out: &mut ffi::Record) -> CxxResult<bool> {
    let inner = match stream.inner.as_mut() {
        Some(inner) => inner,
        None => return Ok(false),
    };
    let stream = match inner.stream.as_mut() {
        Some(stream) => stream,
        None => return Ok(false),
    };
    match stream.next() {
        Some(Ok(record)) => {
            *out = record_to_ffi(record);
            Ok(true)
        }
        Some(Err(err)) => Err(map_err(err)),
        None => Ok(false),
    }
}

fn message_metadata_stream_next(
    stream: &mut MessageMetadataStream,
    out: &mut ffi::MessageMetadata,
) -> CxxResult<bool> {
    let inner = match stream.inner.as_mut() {
        Some(inner) => inner,
        None => return Ok(false),
    };
    let stream = match inner.stream.as_mut() {
        Some(stream) => stream,
        None => return Ok(false),
    };
    match stream.next() {
        Some(Ok(message)) => {
            *out = message_metadata_to_ffi(message);
            Ok(true)
        }
        Some(Err(err)) => Err(map_err(err)),
        None => Ok(false),
    }
}

fn record_metadata_stream_next(
    stream: &mut RecordMetadataStream,
    out: &mut ffi::RecordMetadata,
) -> CxxResult<bool> {
    let inner = match stream.inner.as_mut() {
        Some(inner) => inner,
        None => return Ok(false),
    };
    let stream = match inner.stream.as_mut() {
        Some(stream) => stream,
        None => return Ok(false),
    };
    match stream.next() {
        Some(Ok(record)) => {
            *out = record_metadata_to_ffi(record);
            Ok(true)
        }
        Some(Err(err)) => Err(map_err(err)),
        None => Ok(false),
    }
}

fn parsed_message_stream_next(
    stream: &mut ParsedMessageStream,
    out: &mut ffi::ParsedMessage,
) -> CxxResult<bool> {
    let inner = match stream.inner.as_mut() {
        Some(inner) => inner,
        None => return Ok(false),
    };
    let stream = match inner.stream.as_mut() {
        Some(stream) => stream,
        None => return Ok(false),
    };
    match stream.next() {
        Some(Ok(parsed)) => {
            *out = parsed_value_to_ffi(parsed);
            Ok(true)
        }
        Some(Err(err)) => Err(map_err(err)),
        None => Ok(false),
    }
}

fn raw_message_stream_into_reader(stream: &mut RawMessageStream) -> CxxResult<Box<Reader>> {
    let inner = stream
        .inner
        .take()
        .ok_or_else(|| err_message("stream is already consumed"))?;
    Ok(Box::new(Reader {
        inner: Some(*inner.reader),
    }))
}

fn message_stream_into_reader(stream: &mut MessageStream) -> CxxResult<Box<Reader>> {
    let inner = stream
        .inner
        .take()
        .ok_or_else(|| err_message("stream is already consumed"))?;
    Ok(Box::new(Reader {
        inner: Some(*inner.reader),
    }))
}

fn chunk_stream_into_reader(stream: &mut ChunkStream) -> CxxResult<Box<Reader>> {
    let inner = stream
        .inner
        .take()
        .ok_or_else(|| err_message("stream is already consumed"))?;
    Ok(Box::new(Reader {
        inner: Some(*inner.reader),
    }))
}

fn record_stream_into_reader(stream: &mut RecordStream) -> CxxResult<Box<Reader>> {
    let inner = stream
        .inner
        .take()
        .ok_or_else(|| err_message("stream is already consumed"))?;
    Ok(Box::new(Reader {
        inner: Some(*inner.reader),
    }))
}

fn message_metadata_stream_into_reader(
    stream: &mut MessageMetadataStream,
) -> CxxResult<Box<Reader>> {
    let inner = stream
        .inner
        .take()
        .ok_or_else(|| err_message("stream is already consumed"))?;
    Ok(Box::new(Reader {
        inner: Some(*inner.reader),
    }))
}

fn record_metadata_stream_into_reader(stream: &mut RecordMetadataStream) -> CxxResult<Box<Reader>> {
    let inner = stream
        .inner
        .take()
        .ok_or_else(|| err_message("stream is already consumed"))?;
    Ok(Box::new(Reader {
        inner: Some(*inner.reader),
    }))
}

fn parsed_message_stream_into_reader(stream: &mut ParsedMessageStream) -> CxxResult<Box<Reader>> {
    let inner = stream
        .inner
        .take()
        .ok_or_else(|| err_message("stream is already consumed"))?;
    Ok(Box::new(Reader {
        inner: Some(*inner.reader),
    }))
}
