use bytes::Bytes;
use js_sys::Uint8Array;
use mcapable_core::reader::{Builder as ReaderBuilder, Reader as CoreReader};
use mcapable_core::source::BytesCursor;
use mcapable_core::{
    Attachment, Channel, Chunk, Header, Message, MessageMetadata, Metadata, RawMessage, Record,
    RecordMetadata, RecordSource, Schema,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use wasm_bindgen::prelude::*;

type ReaderHandle = CoreReader<BytesCursor>;

type CoreStream<T> = mcapable_core::Stream<'static, T>;

type ParsedStream<T> = mcapable_core::ParsedStream<'static, T>;

fn map_err(err: mcapable_core::Error) -> JsValue {
    JsValue::from_str(&err.to_string())
}

fn to_js_value<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(value).map_err(|err| JsValue::from_str(&err.to_string()))
}

fn apply_parser_specs(
    mut builder: mcapable_core::ParsedStreamBuilder<'static, ParsedValue>,
    specs: Vec<ParserSpec>,
) -> Result<mcapable_core::ParsedStreamBuilder<'static, ParsedValue>, JsValue> {
    for spec in specs {
        match parser_kind_from_str(&spec.kind)? {
            ParserKind::Json => {
                builder = builder.parser_message_encoding(spec.encoding, |data| {
                    serde_json::from_slice(data.as_ref())
                        .map(ParsedValue::Json)
                        .map_err(|err| mcapable_core::Error::InvalidRecord(err.to_string()))
                });
            }
            ParserKind::Bytes => {
                builder = builder
                    .parser_message_encoding(spec.encoding, |data| Ok(ParsedValue::Bytes(data)));
            }
            ParserKind::SchemaJson => {
                builder = builder.parser_schema_encoding(spec.encoding, |data| {
                    serde_json::from_slice(data.as_ref())
                        .map(ParsedValue::Json)
                        .map_err(|err| mcapable_core::Error::InvalidRecord(err.to_string()))
                });
            }
            ParserKind::SchemaBytes => {
                builder = builder
                    .parser_schema_encoding(spec.encoding, |data| Ok(ParsedValue::Bytes(data)));
            }
        }
    }
    Ok(builder)
}

#[derive(Serialize)]
struct JsHeader {
    profile: String,
    library: String,
    metadata: Vec<(String, String)>,
}

#[derive(Serialize)]
struct JsSchema {
    id: u16,
    name: String,
    encoding: String,
    data: Vec<u8>,
}

#[derive(Serialize)]
struct JsChannel {
    id: u16,
    topic: String,
    message_encoding: String,
    schema_id: u16,
    metadata: Vec<(String, String)>,
}

#[derive(Serialize)]
struct JsMetadata {
    name: String,
    metadata: Vec<(String, String)>,
}

#[derive(Serialize)]
struct JsAttachment {
    log_time: u64,
    create_time: u64,
    name: String,
    media_type: String,
    data: Vec<u8>,
}

#[derive(Serialize)]
struct JsRawMessage {
    channel_id: u16,
    sequence: u32,
    log_time: u64,
    publish_time: u64,
    data: Vec<u8>,
}

#[derive(Serialize)]
struct JsMessage {
    channel_id: u16,
    sequence: u32,
    log_time: u64,
    publish_time: u64,
    data: Vec<u8>,
}

#[derive(Serialize)]
struct JsChunk {
    message_start_time: u64,
    message_end_time: u64,
    uncompressed_size: u64,
    uncompressed_crc: u32,
    compression: String,
    records: Vec<u8>,
}

#[derive(Serialize)]
struct JsMessageMetadata {
    channel_id: u16,
    sequence: u32,
    log_time: u64,
    publish_time: u64,
    data_size: u64,
}

#[derive(Serialize)]
struct JsRecordMetadata {
    opcode: String,
    length: u64,
    total_len: u64,
    offset: u64,
    source: String,
    message: Option<JsMessageMetadata>,
}

#[derive(Serialize)]
struct JsRecord {
    kind: String,
    header: Option<JsHeader>,
    schema: Option<JsSchema>,
    channel: Option<JsChannel>,
    message: Option<JsMessage>,
    chunk: Option<JsChunk>,
    attachment: Option<JsAttachment>,
    metadata: Option<JsMetadata>,
}

fn metadata_to_vec(
    metadata: &std::collections::HashMap<
        mcapable_core::zero_copy::ByteStr,
        mcapable_core::zero_copy::ByteStr,
    >,
) -> Vec<(String, String)> {
    metadata
        .iter()
        .map(|(k, v)| (k.as_ref().to_string(), v.as_ref().to_string()))
        .collect()
}

fn header_to_js(header: Header) -> JsHeader {
    JsHeader {
        profile: header.profile.as_ref().to_string(),
        library: header.library.as_ref().to_string(),
        metadata: metadata_to_vec(&header.metadata),
    }
}

fn schema_to_js(schema: Schema) -> JsSchema {
    JsSchema {
        id: schema.id,
        name: schema.name.as_ref().to_string(),
        encoding: schema.encoding.as_ref().to_string(),
        data: schema.data.to_vec(),
    }
}

fn channel_to_js(channel: Channel) -> JsChannel {
    JsChannel {
        id: channel.id,
        topic: channel.topic.as_ref().to_string(),
        message_encoding: channel.message_encoding.as_ref().to_string(),
        schema_id: channel.schema_id,
        metadata: metadata_to_vec(&channel.metadata),
    }
}

fn metadata_to_js(metadata: Metadata) -> JsMetadata {
    JsMetadata {
        name: metadata.name.as_ref().to_string(),
        metadata: metadata_to_vec(&metadata.metadata),
    }
}

fn attachment_to_js(attachment: Attachment) -> JsAttachment {
    JsAttachment {
        log_time: attachment.log_time,
        create_time: attachment.create_time,
        name: attachment.name.as_ref().to_string(),
        media_type: attachment.media_type.as_ref().to_string(),
        data: attachment.data.to_vec(),
    }
}

fn raw_message_to_js(message: RawMessage) -> JsRawMessage {
    JsRawMessage {
        channel_id: message.channel_id,
        sequence: message.sequence,
        log_time: message.log_time,
        publish_time: message.publish_time,
        data: message.data_bytes().to_vec(),
    }
}

fn message_to_js(message: Message) -> JsMessage {
    JsMessage {
        channel_id: message.channel_id,
        sequence: message.sequence,
        log_time: message.log_time,
        publish_time: message.publish_time,
        data: message.data_bytes().to_vec(),
    }
}

fn chunk_to_js(chunk: Chunk) -> JsChunk {
    JsChunk {
        message_start_time: chunk.message_start_time,
        message_end_time: chunk.message_end_time,
        uncompressed_size: chunk.uncompressed_size,
        uncompressed_crc: chunk.uncompressed_crc,
        compression: chunk.compression.as_ref().to_string(),
        records: chunk.records.to_vec(),
    }
}

fn message_metadata_to_js(message: MessageMetadata) -> JsMessageMetadata {
    JsMessageMetadata {
        channel_id: message.channel_id,
        sequence: message.sequence,
        log_time: message.log_time,
        publish_time: message.publish_time,
        data_size: message.data_size,
    }
}

fn record_metadata_to_js(record: RecordMetadata) -> JsRecordMetadata {
    JsRecordMetadata {
        opcode: format!("{:?}", record.opcode),
        length: record.length,
        total_len: record.total_len,
        offset: record.offset,
        source: match record.source {
            RecordSource::File => "File".to_string(),
            RecordSource::Chunk { chunk_offset } => format!("Chunk({chunk_offset})"),
        },
        message: record.message.map(message_metadata_to_js),
    }
}

fn record_to_js(record: Record) -> JsRecord {
    match record {
        Record::Header(header) => JsRecord {
            kind: "Header".to_string(),
            header: Some(header_to_js(header)),
            schema: None,
            channel: None,
            message: None,
            chunk: None,
            attachment: None,
            metadata: None,
        },
        Record::Footer(_) => JsRecord {
            kind: "Footer".to_string(),
            header: None,
            schema: None,
            channel: None,
            message: None,
            chunk: None,
            attachment: None,
            metadata: None,
        },
        Record::Schema(schema) => JsRecord {
            kind: "Schema".to_string(),
            header: None,
            schema: Some(schema_to_js(schema)),
            channel: None,
            message: None,
            chunk: None,
            attachment: None,
            metadata: None,
        },
        Record::Channel(channel) => JsRecord {
            kind: "Channel".to_string(),
            header: None,
            schema: None,
            channel: Some(channel_to_js(channel)),
            message: None,
            chunk: None,
            attachment: None,
            metadata: None,
        },
        Record::Message(message) => JsRecord {
            kind: "Message".to_string(),
            header: None,
            schema: None,
            channel: None,
            message: Some(message_to_js(message)),
            chunk: None,
            attachment: None,
            metadata: None,
        },
        Record::Chunk(chunk) => JsRecord {
            kind: "Chunk".to_string(),
            header: None,
            schema: None,
            channel: None,
            message: None,
            chunk: Some(chunk_to_js(chunk)),
            attachment: None,
            metadata: None,
        },
        Record::Attachment(attachment) => JsRecord {
            kind: "Attachment".to_string(),
            header: None,
            schema: None,
            channel: None,
            message: None,
            chunk: None,
            attachment: Some(attachment_to_js(attachment)),
            metadata: None,
        },
        Record::Metadata(metadata) => JsRecord {
            kind: "Metadata".to_string(),
            header: None,
            schema: None,
            channel: None,
            message: None,
            chunk: None,
            attachment: None,
            metadata: Some(metadata_to_js(metadata)),
        },
        Record::SummaryOffset => JsRecord {
            kind: "SummaryOffset".to_string(),
            header: None,
            schema: None,
            channel: None,
            message: None,
            chunk: None,
            attachment: None,
            metadata: None,
        },
        Record::DataEnd => JsRecord {
            kind: "DataEnd".to_string(),
            header: None,
            schema: None,
            channel: None,
            message: None,
            chunk: None,
            attachment: None,
            metadata: None,
        },
        _ => JsRecord {
            kind: "Other".to_string(),
            header: None,
            schema: None,
            channel: None,
            message: None,
            chunk: None,
            attachment: None,
            metadata: None,
        },
    }
}

enum ParsedValue {
    Json(JsonValue),
    Bytes(Bytes),
}

#[derive(Deserialize)]
struct ParserSpec {
    encoding: String,
    kind: String,
}

enum ParserKind {
    Json,
    Bytes,
    SchemaJson,
    SchemaBytes,
}

fn parser_kind_from_str(kind: &str) -> Result<ParserKind, JsValue> {
    match kind.to_ascii_lowercase().as_str() {
        "json" => Ok(ParserKind::Json),
        "bytes" => Ok(ParserKind::Bytes),
        "schema_json" => Ok(ParserKind::SchemaJson),
        "schema_bytes" => Ok(ParserKind::SchemaBytes),
        _ => Err(JsValue::from_str(&format!(
            "unsupported parser kind: {kind}"
        ))),
    }
}

struct MessageStreamInner {
    reader: Box<ReaderHandle>,
    stream: Option<CoreStream<Message>>,
}

struct RawMessageStreamInner {
    reader: Box<ReaderHandle>,
    stream: Option<CoreStream<RawMessage>>,
}

struct ChunkStreamInner {
    reader: Box<ReaderHandle>,
    stream: Option<CoreStream<Chunk>>,
}

struct RecordStreamInner {
    reader: Box<ReaderHandle>,
    stream: Option<CoreStream<Record>>,
}

struct MessageMetadataStreamInner {
    reader: Box<ReaderHandle>,
    stream: Option<CoreStream<MessageMetadata>>,
}

struct RecordMetadataStreamInner {
    reader: Box<ReaderHandle>,
    stream: Option<CoreStream<RecordMetadata>>,
}

struct ParsedMessageStreamInner {
    reader: Box<ReaderHandle>,
    stream: Option<ParsedStream<ParsedValue>>,
}

#[wasm_bindgen]
pub struct Reader {
    inner: Option<ReaderHandle>,
}

#[wasm_bindgen]
impl Reader {
    #[wasm_bindgen(js_name = fromBytes)]
    pub fn from_bytes(bytes: Uint8Array) -> Result<Reader, JsValue> {
        let data = bytes.to_vec();
        let data = Bytes::copy_from_slice(&data);
        let reader = ReaderBuilder::new()
            .build(BytesCursor::new(data))
            .map_err(map_err)?;
        Ok(Reader {
            inner: Some(reader),
        })
    }

    pub fn header(&mut self) -> Result<JsValue, JsValue> {
        let reader = self
            .inner
            .as_mut()
            .ok_or_else(|| JsValue::from_str("reader is already in use"))?;
        let header = reader.header().map_err(map_err)?;
        to_js_value(&header_to_js(header))
    }

    pub fn schemas(&mut self) -> Result<JsValue, JsValue> {
        let reader = self
            .inner
            .as_mut()
            .ok_or_else(|| JsValue::from_str("reader is already in use"))?;
        let schemas: Vec<JsSchema> = reader
            .schemas()
            .values()
            .cloned()
            .map(schema_to_js)
            .collect();
        to_js_value(&schemas)
    }

    pub fn channels(&mut self) -> Result<JsValue, JsValue> {
        let reader = self
            .inner
            .as_mut()
            .ok_or_else(|| JsValue::from_str("reader is already in use"))?;
        let channels: Vec<JsChannel> = reader
            .channels()
            .values()
            .cloned()
            .map(channel_to_js)
            .collect();
        to_js_value(&channels)
    }

    pub fn raw_messages(&mut self) -> Result<RawMessageStream, JsValue> {
        let reader = self
            .inner
            .take()
            .ok_or_else(|| JsValue::from_str("reader is already in use"))?;
        let mut reader = Box::new(reader);
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

    pub fn messages(&mut self) -> Result<MessageStream, JsValue> {
        let reader = self
            .inner
            .take()
            .ok_or_else(|| JsValue::from_str("reader is already in use"))?;
        let mut reader = Box::new(reader);
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

    pub fn chunks(&mut self) -> Result<ChunkStream, JsValue> {
        let reader = self
            .inner
            .take()
            .ok_or_else(|| JsValue::from_str("reader is already in use"))?;
        let mut reader = Box::new(reader);
        let stream = reader.chunks();
        let stream = unsafe {
            std::mem::transmute::<
                mcapable_core::Stream<'_, Chunk>,
                mcapable_core::Stream<'static, Chunk>,
            >(stream)
        };
        Ok(ChunkStream {
            inner: Some(ChunkStreamInner {
                reader,
                stream: Some(stream),
            }),
        })
    }

    pub fn records(&mut self) -> Result<RecordStream, JsValue> {
        let reader = self
            .inner
            .take()
            .ok_or_else(|| JsValue::from_str("reader is already in use"))?;
        let mut reader = Box::new(reader);
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

    pub fn message_metadata(&mut self) -> Result<MessageMetadataStream, JsValue> {
        let reader = self
            .inner
            .take()
            .ok_or_else(|| JsValue::from_str("reader is already in use"))?;
        let mut reader = Box::new(reader);
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

    pub fn record_metadata(&mut self) -> Result<RecordMetadataStream, JsValue> {
        let reader = self
            .inner
            .take()
            .ok_or_else(|| JsValue::from_str("reader is already in use"))?;
        let mut reader = Box::new(reader);
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

    #[wasm_bindgen(js_name = parsedMessages)]
    pub fn parsed_messages(&mut self) -> Result<ParsedMessageStream, JsValue> {
        let reader = self
            .inner
            .take()
            .ok_or_else(|| JsValue::from_str("reader is already in use"))?;
        let mut reader = Box::new(reader);
        let stream = reader.messages().map_err(map_err)?;
        let stream = unsafe {
            std::mem::transmute::<
                mcapable_core::Stream<'_, Message>,
                mcapable_core::Stream<'static, Message>,
            >(stream)
        };
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

    #[wasm_bindgen(js_name = parsedMessagesWith)]
    pub fn parsed_messages_with(&mut self, specs: JsValue) -> Result<ParsedMessageStream, JsValue> {
        let reader = self
            .inner
            .take()
            .ok_or_else(|| JsValue::from_str("reader is already in use"))?;
        let mut reader = Box::new(reader);
        let stream = reader.messages().map_err(map_err)?;
        let stream = unsafe {
            std::mem::transmute::<
                mcapable_core::Stream<'_, Message>,
                mcapable_core::Stream<'static, Message>,
            >(stream)
        };
        let specs: Vec<ParserSpec> = serde_wasm_bindgen::from_value(specs)
            .map_err(|err| JsValue::from_str(&err.to_string()))?;
        let builder = stream.parsed::<ParsedValue>();
        let parsed = apply_parser_specs(builder, specs)?
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
}

#[wasm_bindgen]
pub struct RawMessageStream {
    inner: Option<RawMessageStreamInner>,
}

#[wasm_bindgen]
impl RawMessageStream {
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<JsValue>, JsValue> {
        let inner = match self.inner.as_mut() {
            Some(inner) => inner,
            None => return Ok(None),
        };
        let stream = match inner.stream.as_mut() {
            Some(stream) => stream,
            None => return Ok(None),
        };
        match stream.next() {
            Some(Ok(raw)) => to_js_value(&raw_message_to_js(raw)).map(Some),
            Some(Err(err)) => Err(map_err(err)),
            None => Ok(None),
        }
    }

    pub fn into_reader(&mut self) -> Result<Reader, JsValue> {
        let inner = self
            .inner
            .take()
            .ok_or_else(|| JsValue::from_str("stream is already consumed"))?;
        Ok(Reader {
            inner: Some(*inner.reader),
        })
    }
}

#[wasm_bindgen]
pub struct MessageStream {
    inner: Option<MessageStreamInner>,
}

#[wasm_bindgen]
impl MessageStream {
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<JsValue>, JsValue> {
        let inner = match self.inner.as_mut() {
            Some(inner) => inner,
            None => return Ok(None),
        };
        let stream = match inner.stream.as_mut() {
            Some(stream) => stream,
            None => return Ok(None),
        };
        match stream.next() {
            Some(Ok(message)) => to_js_value(&message_to_js(message)).map(Some),
            Some(Err(err)) => Err(map_err(err)),
            None => Ok(None),
        }
    }

    #[wasm_bindgen(js_name = parsed)]
    pub fn parsed(&mut self) -> Result<ParsedMessageStream, JsValue> {
        let inner = self
            .inner
            .take()
            .ok_or_else(|| JsValue::from_str("stream is already consumed"))?;
        let stream = inner
            .stream
            .ok_or_else(|| JsValue::from_str("stream is already consumed"))?;
        let reader = inner.reader;
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

    #[wasm_bindgen(js_name = parsedWith)]
    pub fn parsed_with(&mut self, specs: JsValue) -> Result<ParsedMessageStream, JsValue> {
        let inner = self
            .inner
            .take()
            .ok_or_else(|| JsValue::from_str("stream is already consumed"))?;
        let stream = inner
            .stream
            .ok_or_else(|| JsValue::from_str("stream is already consumed"))?;
        let reader = inner.reader;
        let specs: Vec<ParserSpec> = serde_wasm_bindgen::from_value(specs)
            .map_err(|err| JsValue::from_str(&err.to_string()))?;
        let builder = stream.parsed::<ParsedValue>();
        let parsed = apply_parser_specs(builder, specs)?
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

    pub fn into_reader(&mut self) -> Result<Reader, JsValue> {
        let inner = self
            .inner
            .take()
            .ok_or_else(|| JsValue::from_str("stream is already consumed"))?;
        Ok(Reader {
            inner: Some(*inner.reader),
        })
    }
}

#[wasm_bindgen]
pub struct ChunkStream {
    inner: Option<ChunkStreamInner>,
}

#[wasm_bindgen]
impl ChunkStream {
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<JsValue>, JsValue> {
        let inner = match self.inner.as_mut() {
            Some(inner) => inner,
            None => return Ok(None),
        };
        let stream = match inner.stream.as_mut() {
            Some(stream) => stream,
            None => return Ok(None),
        };
        match stream.next() {
            Some(Ok(chunk)) => to_js_value(&chunk_to_js(chunk)).map(Some),
            Some(Err(err)) => Err(map_err(err)),
            None => Ok(None),
        }
    }

    pub fn into_reader(&mut self) -> Result<Reader, JsValue> {
        let inner = self
            .inner
            .take()
            .ok_or_else(|| JsValue::from_str("stream is already consumed"))?;
        Ok(Reader {
            inner: Some(*inner.reader),
        })
    }
}

#[wasm_bindgen]
pub struct RecordStream {
    inner: Option<RecordStreamInner>,
}

#[wasm_bindgen]
impl RecordStream {
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<JsValue>, JsValue> {
        let inner = match self.inner.as_mut() {
            Some(inner) => inner,
            None => return Ok(None),
        };
        let stream = match inner.stream.as_mut() {
            Some(stream) => stream,
            None => return Ok(None),
        };
        match stream.next() {
            Some(Ok(record)) => to_js_value(&record_to_js(record)).map(Some),
            Some(Err(err)) => Err(map_err(err)),
            None => Ok(None),
        }
    }

    pub fn into_reader(&mut self) -> Result<Reader, JsValue> {
        let inner = self
            .inner
            .take()
            .ok_or_else(|| JsValue::from_str("stream is already consumed"))?;
        Ok(Reader {
            inner: Some(*inner.reader),
        })
    }
}

#[wasm_bindgen]
pub struct MessageMetadataStream {
    inner: Option<MessageMetadataStreamInner>,
}

#[wasm_bindgen]
impl MessageMetadataStream {
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<JsValue>, JsValue> {
        let inner = match self.inner.as_mut() {
            Some(inner) => inner,
            None => return Ok(None),
        };
        let stream = match inner.stream.as_mut() {
            Some(stream) => stream,
            None => return Ok(None),
        };
        match stream.next() {
            Some(Ok(message)) => to_js_value(&message_metadata_to_js(message)).map(Some),
            Some(Err(err)) => Err(map_err(err)),
            None => Ok(None),
        }
    }

    pub fn into_reader(&mut self) -> Result<Reader, JsValue> {
        let inner = self
            .inner
            .take()
            .ok_or_else(|| JsValue::from_str("stream is already consumed"))?;
        Ok(Reader {
            inner: Some(*inner.reader),
        })
    }
}

#[wasm_bindgen]
pub struct RecordMetadataStream {
    inner: Option<RecordMetadataStreamInner>,
}

#[wasm_bindgen]
impl RecordMetadataStream {
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<JsValue>, JsValue> {
        let inner = match self.inner.as_mut() {
            Some(inner) => inner,
            None => return Ok(None),
        };
        let stream = match inner.stream.as_mut() {
            Some(stream) => stream,
            None => return Ok(None),
        };
        match stream.next() {
            Some(Ok(record)) => to_js_value(&record_metadata_to_js(record)).map(Some),
            Some(Err(err)) => Err(map_err(err)),
            None => Ok(None),
        }
    }

    pub fn into_reader(&mut self) -> Result<Reader, JsValue> {
        let inner = self
            .inner
            .take()
            .ok_or_else(|| JsValue::from_str("stream is already consumed"))?;
        Ok(Reader {
            inner: Some(*inner.reader),
        })
    }
}

#[wasm_bindgen]
pub struct ParsedMessageStream {
    inner: Option<ParsedMessageStreamInner>,
}

#[wasm_bindgen]
impl ParsedMessageStream {
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<JsValue>, JsValue> {
        let inner = match self.inner.as_mut() {
            Some(inner) => inner,
            None => return Ok(None),
        };
        let stream = match inner.stream.as_mut() {
            Some(stream) => stream,
            None => return Ok(None),
        };
        match stream.next() {
            Some(Ok(parsed)) => match parsed {
                ParsedValue::Json(value) => serde_wasm_bindgen::to_value(&value)
                    .map(Some)
                    .map_err(|err| JsValue::from_str(&err.to_string())),
                ParsedValue::Bytes(bytes) => {
                    let data = Uint8Array::from(bytes.as_ref());
                    Ok(Some(data.into()))
                }
            },
            Some(Err(err)) => Err(map_err(err)),
            None => Ok(None),
        }
    }

    pub fn into_reader(&mut self) -> Result<Reader, JsValue> {
        let inner = self
            .inner
            .take()
            .ok_or_else(|| JsValue::from_str("stream is already consumed"))?;
        Ok(Reader {
            inner: Some(*inner.reader),
        })
    }
}
