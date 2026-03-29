use bytes::Bytes;
use mcapable_core::reader::{Builder as ReaderBuilder, Reader as CoreReader};
use mcapable_core::source::BytesCursor;
use mcapable_core::zero_copy::ByteStr;
use mcapable_core::{Chunk, Message, RawMessage, Record};
use serde_json::Value as JsonValue;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CStr;
use std::os::raw::c_char;
use std::ptr;

thread_local! {
    static LAST_ERROR: RefCell<Option<Vec<u8>>> = const { RefCell::new(None) };
}

type ReaderHandle = CoreReader<BytesCursor>;

type MessageStream = mcapable_core::Stream<'static, Message>;
type RawMessageStream = mcapable_core::Stream<'static, RawMessage>;
type ChunkStream = mcapable_core::Stream<'static, Chunk>;
type RecordStream = mcapable_core::Stream<'static, Record>;

type ParsedStream = mcapable_core::ParsedStream<'static, ParsedValue>;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct McapByteBuffer {
    ptr: *mut u8,
    len: usize,
    cap: usize,
}

impl McapByteBuffer {
    fn empty() -> Self {
        Self {
            ptr: ptr::null_mut(),
            len: 0,
            cap: 0,
        }
    }

    fn from_vec(mut data: Vec<u8>) -> Self {
        if data.is_empty() {
            return Self::empty();
        }
        let len = data.len();
        let cap = data.capacity();
        let ptr = data.as_mut_ptr();
        std::mem::forget(data);
        Self { ptr, len, cap }
    }
}

fn byte_buffer_from_str(value: &str) -> McapByteBuffer {
    McapByteBuffer::from_vec(value.as_bytes().to_vec())
}

fn vec_into_array<T>(mut items: Vec<T>) -> (*mut T, usize) {
    if items.is_empty() {
        return (ptr::null_mut(), 0);
    }
    let len = items.len();
    let ptr = items.as_mut_ptr();
    std::mem::forget(items);
    (ptr, len)
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_byte_buffer_free(buf: McapByteBuffer) {
    if buf.ptr.is_null() || buf.len == 0 {
        return;
    }
    unsafe {
        drop(Vec::from_raw_parts(buf.ptr, buf.len, buf.cap));
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct McapKeyValue {
    key: McapByteBuffer,
    value: McapByteBuffer,
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_key_value_clear(kv: *mut McapKeyValue) {
    if kv.is_null() {
        return;
    }
    unsafe {
        let kv = &mut *kv;
        mcap_byte_buffer_free(kv.key);
        mcap_byte_buffer_free(kv.value);
        kv.key = McapByteBuffer::empty();
        kv.value = McapByteBuffer::empty();
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_key_value_array_free(items: *mut McapKeyValue, len: usize) {
    if items.is_null() || len == 0 {
        return;
    }
    unsafe {
        let slice = std::slice::from_raw_parts_mut(items, len);
        for kv in slice.iter_mut() {
            mcap_byte_buffer_free(kv.key);
            mcap_byte_buffer_free(kv.value);
        }
        drop(Vec::from_raw_parts(items, len, len));
    }
}

#[repr(C)]
pub struct McapHeader {
    profile: McapByteBuffer,
    library: McapByteBuffer,
    metadata: *mut McapKeyValue,
    metadata_len: usize,
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_header_clear(header: *mut McapHeader) {
    if header.is_null() {
        return;
    }
    unsafe {
        let header = &mut *header;
        mcap_byte_buffer_free(header.profile);
        mcap_byte_buffer_free(header.library);
        mcap_key_value_array_free(header.metadata, header.metadata_len);
        header.profile = McapByteBuffer::empty();
        header.library = McapByteBuffer::empty();
        header.metadata = ptr::null_mut();
        header.metadata_len = 0;
    }
}

#[repr(C)]
pub struct McapSchema {
    id: u16,
    name: McapByteBuffer,
    encoding: McapByteBuffer,
    data: McapByteBuffer,
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_schema_array_free(items: *mut McapSchema, len: usize) {
    if items.is_null() || len == 0 {
        return;
    }
    unsafe {
        let slice = std::slice::from_raw_parts_mut(items, len);
        for schema in slice.iter_mut() {
            mcap_byte_buffer_free(schema.name);
            mcap_byte_buffer_free(schema.encoding);
            mcap_byte_buffer_free(schema.data);
        }
        drop(Vec::from_raw_parts(items, len, len));
    }
}

#[repr(C)]
pub struct McapChannel {
    id: u16,
    topic: McapByteBuffer,
    message_encoding: McapByteBuffer,
    schema_id: u16,
    metadata: *mut McapKeyValue,
    metadata_len: usize,
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_channel_array_free(items: *mut McapChannel, len: usize) {
    if items.is_null() || len == 0 {
        return;
    }
    unsafe {
        let slice = std::slice::from_raw_parts_mut(items, len);
        for channel in slice.iter_mut() {
            mcap_byte_buffer_free(channel.topic);
            mcap_byte_buffer_free(channel.message_encoding);
            mcap_key_value_array_free(channel.metadata, channel.metadata_len);
        }
        drop(Vec::from_raw_parts(items, len, len));
    }
}

#[repr(C)]
pub struct McapRawMessage {
    channel_id: u16,
    sequence: u32,
    log_time: u64,
    publish_time: u64,
    data: McapByteBuffer,
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_raw_message_clear(message: *mut McapRawMessage) {
    if message.is_null() {
        return;
    }
    unsafe {
        let message = &mut *message;
        mcap_byte_buffer_free(message.data);
        message.data = McapByteBuffer::empty();
    }
}

#[repr(C)]
pub struct McapChunk {
    message_start_time: u64,
    message_end_time: u64,
    uncompressed_size: u64,
    uncompressed_crc: u32,
    compression: McapByteBuffer,
    records: McapByteBuffer,
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_chunk_clear(chunk: *mut McapChunk) {
    if chunk.is_null() {
        return;
    }
    unsafe {
        let chunk = &mut *chunk;
        mcap_byte_buffer_free(chunk.compression);
        mcap_byte_buffer_free(chunk.records);
        chunk.compression = McapByteBuffer::empty();
        chunk.records = McapByteBuffer::empty();
    }
}

#[repr(C)]
pub struct McapRecord {
    kind: McapByteBuffer,
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_record_clear(record: *mut McapRecord) {
    if record.is_null() {
        return;
    }
    unsafe {
        let record = &mut *record;
        mcap_byte_buffer_free(record.kind);
        record.kind = McapByteBuffer::empty();
    }
}

#[repr(C)]
pub struct McapMessage {
    channel_id: u16,
    sequence: u32,
    log_time: u64,
    publish_time: u64,
    data: McapByteBuffer,
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_message_clear(message: *mut McapMessage) {
    if message.is_null() {
        return;
    }
    unsafe {
        let message = &mut *message;
        mcap_byte_buffer_free(message.data);
        message.data = McapByteBuffer::empty();
    }
}

#[repr(C)]
pub struct McapParsedValue {
    is_json: bool,
    json: McapByteBuffer,
    bytes: McapByteBuffer,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub enum McapParserKind {
    Json = 0,
    Bytes = 1,
    SchemaJson = 2,
    SchemaBytes = 3,
}

#[repr(C)]
pub struct McapParserSpec {
    encoding: *const c_char,
    kind: McapParserKind,
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_parsed_value_clear(value: *mut McapParsedValue) {
    if value.is_null() {
        return;
    }
    unsafe {
        let value = &mut *value;
        mcap_byte_buffer_free(value.json);
        mcap_byte_buffer_free(value.bytes);
        value.json = McapByteBuffer::empty();
        value.bytes = McapByteBuffer::empty();
    }
}

pub struct McapReader {
    reader: Box<ReaderHandle>,
}

pub struct McapMessageStream {
    stream: MessageStream,
    reader: *mut ReaderHandle,
}

pub struct McapRawMessageStream {
    stream: RawMessageStream,
    reader: *mut ReaderHandle,
}

pub struct McapChunkStream {
    stream: ChunkStream,
    reader: *mut ReaderHandle,
}

pub struct McapRecordStream {
    stream: RecordStream,
    reader: *mut ReaderHandle,
}

pub struct McapParsedStream {
    stream: ParsedStream,
    #[allow(dead_code)] // Keeps the reader alive while the parsed stream borrows it.
    reader: *mut ReaderHandle,
}

enum ParsedValue {
    Json(JsonValue),
    Bytes(Bytes),
}

fn set_last_error(message: String) {
    LAST_ERROR.with(|slot| {
        *slot.borrow_mut() = Some(message.into_bytes());
    });
}

fn take_last_error() -> Option<Vec<u8>> {
    LAST_ERROR.with(|slot| slot.borrow_mut().take())
}

fn apply_parser_specs<'a>(
    mut builder: mcapable_core::ParsedStreamBuilder<'a, ParsedValue>,
    specs: *const McapParserSpec,
    len: usize,
) -> Result<mcapable_core::ParsedStreamBuilder<'a, ParsedValue>, String> {
    if specs.is_null() || len == 0 {
        return Ok(builder);
    }
    let specs = unsafe { std::slice::from_raw_parts(specs, len) };
    for spec in specs {
        if spec.encoding.is_null() {
            return Err("null encoding pointer".to_string());
        }
        let encoding = unsafe { CStr::from_ptr(spec.encoding) }
            .to_str()
            .map_err(|_| "encoding is not valid UTF-8".to_string())?
            .to_string();
        builder = match spec.kind {
            McapParserKind::Json => builder.parser_message_encoding(encoding, |data| {
                serde_json::from_slice(data.as_ref())
                    .map(ParsedValue::Json)
                    .map_err(|err| mcapable_core::Error::InvalidRecord(err.to_string()))
            }),
            McapParserKind::Bytes => {
                builder.parser_message_encoding(encoding, |data| Ok(ParsedValue::Bytes(data)))
            }
            McapParserKind::SchemaJson => builder.parser_schema_encoding(encoding, |data| {
                serde_json::from_slice(data.as_ref())
                    .map(ParsedValue::Json)
                    .map_err(|err| mcapable_core::Error::InvalidRecord(err.to_string()))
            }),
            McapParserKind::SchemaBytes => {
                builder.parser_schema_encoding(encoding, |data| Ok(ParsedValue::Bytes(data)))
            }
        };
    }
    Ok(builder)
}

fn key_values_from_map(map: &HashMap<ByteStr, ByteStr>) -> Vec<McapKeyValue> {
    map.iter()
        .map(|(key, value)| McapKeyValue {
            key: byte_buffer_from_str(key.as_ref()),
            value: byte_buffer_from_str(value.as_ref()),
        })
        .collect()
}

fn preload_metadata(reader: &mut ReaderHandle) -> Result<(), String> {
    let mut stream = reader.messages().map_err(|err| err.to_string())?;
    let _ = stream.next();
    drop(stream);
    Ok(())
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_last_error() -> McapByteBuffer {
    match take_last_error() {
        Some(data) => McapByteBuffer::from_vec(data),
        None => McapByteBuffer::empty(),
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_reader_from_bytes(data: *const u8, len: usize) -> *mut McapReader {
    if data.is_null() && len > 0 {
        set_last_error("null data pointer".to_string());
        return ptr::null_mut();
    }
    let slice = if len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(data, len) }
    };
    let bytes = Bytes::copy_from_slice(slice);
    let reader = match ReaderBuilder::new().build(BytesCursor::new(bytes)) {
        Ok(reader) => reader,
        Err(err) => {
            set_last_error(err.to_string());
            return ptr::null_mut();
        }
    };
    Box::into_raw(Box::new(McapReader {
        reader: Box::new(reader),
    }))
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_reader_free(reader: *mut McapReader) {
    if reader.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(reader));
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_reader_header(reader: *mut McapReader, out: *mut McapHeader) -> bool {
    if reader.is_null() || out.is_null() {
        set_last_error("null reader or output".to_string());
        return false;
    }
    let reader = unsafe { &mut *reader };
    let header = match reader.reader.as_mut().header() {
        Ok(header) => header,
        Err(err) => {
            set_last_error(err.to_string());
            return false;
        }
    };
    let metadata = key_values_from_map(&header.metadata);
    let (metadata_ptr, metadata_len) = vec_into_array(metadata);
    unsafe {
        *out = McapHeader {
            profile: byte_buffer_from_str(header.profile.as_ref()),
            library: byte_buffer_from_str(header.library.as_ref()),
            metadata: metadata_ptr,
            metadata_len,
        };
    }
    true
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_reader_schemas(
    reader: *mut McapReader,
    out_items: *mut *mut McapSchema,
    out_len: *mut usize,
) -> bool {
    if reader.is_null() || out_items.is_null() || out_len.is_null() {
        set_last_error("null reader or output".to_string());
        return false;
    }
    let reader = unsafe { &mut *reader };
    if let Err(err) = preload_metadata(reader.reader.as_mut()) {
        set_last_error(err);
        return false;
    }
    let schemas: Vec<McapSchema> = reader
        .reader
        .as_mut()
        .schemas()
        .values()
        .map(|schema| McapSchema {
            id: schema.id,
            name: byte_buffer_from_str(schema.name.as_ref()),
            encoding: byte_buffer_from_str(schema.encoding.as_ref()),
            data: McapByteBuffer::from_vec(schema.data.as_ref().to_vec()),
        })
        .collect();
    let (ptr, len) = vec_into_array(schemas);
    unsafe {
        *out_items = ptr;
        *out_len = len;
    }
    true
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_reader_channels(
    reader: *mut McapReader,
    out_items: *mut *mut McapChannel,
    out_len: *mut usize,
) -> bool {
    if reader.is_null() || out_items.is_null() || out_len.is_null() {
        set_last_error("null reader or output".to_string());
        return false;
    }
    let reader = unsafe { &mut *reader };
    if let Err(err) = preload_metadata(reader.reader.as_mut()) {
        set_last_error(err);
        return false;
    }
    let channels: Vec<McapChannel> = reader
        .reader
        .as_mut()
        .channels()
        .values()
        .map(|channel| {
            let metadata = key_values_from_map(&channel.metadata);
            let (metadata_ptr, metadata_len) = vec_into_array(metadata);
            McapChannel {
                id: channel.id,
                topic: byte_buffer_from_str(channel.topic.as_ref()),
                message_encoding: byte_buffer_from_str(channel.message_encoding.as_ref()),
                schema_id: channel.schema_id,
                metadata: metadata_ptr,
                metadata_len,
            }
        })
        .collect();
    let (ptr, len) = vec_into_array(channels);
    unsafe {
        *out_items = ptr;
        *out_len = len;
    }
    true
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_reader_messages(reader: *mut McapReader) -> *mut McapMessageStream {
    if reader.is_null() {
        set_last_error("null reader".to_string());
        return ptr::null_mut();
    }
    let reader = unsafe { Box::from_raw(reader) };
    let reader = reader.reader;
    let reader_ptr = Box::into_raw(reader);
    let stream = match unsafe { &mut *reader_ptr }.messages() {
        Ok(stream) => stream,
        Err(err) => {
            set_last_error(err.to_string());
            return ptr::null_mut();
        }
    };
    let stream = unsafe {
        std::mem::transmute::<
            mcapable_core::Stream<'_, Message>,
            mcapable_core::Stream<'static, Message>,
        >(stream)
    };
    Box::into_raw(Box::new(McapMessageStream {
        reader: reader_ptr,
        stream,
    }))
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_reader_raw_messages(
    reader: *mut McapReader,
) -> *mut McapRawMessageStream {
    if reader.is_null() {
        set_last_error("null reader".to_string());
        return ptr::null_mut();
    }
    let reader = unsafe { Box::from_raw(reader) };
    let reader = reader.reader;
    let reader_ptr = Box::into_raw(reader);
    let stream = match unsafe { &mut *reader_ptr }.raw_messages() {
        Ok(stream) => stream,
        Err(err) => {
            set_last_error(err.to_string());
            return ptr::null_mut();
        }
    };
    let stream = unsafe {
        std::mem::transmute::<
            mcapable_core::Stream<'_, RawMessage>,
            mcapable_core::Stream<'static, RawMessage>,
        >(stream)
    };
    Box::into_raw(Box::new(McapRawMessageStream {
        reader: reader_ptr,
        stream,
    }))
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_reader_chunks(reader: *mut McapReader) -> *mut McapChunkStream {
    if reader.is_null() {
        set_last_error("null reader".to_string());
        return ptr::null_mut();
    }
    let reader = unsafe { Box::from_raw(reader) };
    let reader = reader.reader;
    let reader_ptr = Box::into_raw(reader);
    let stream = unsafe { &mut *reader_ptr }.chunks();
    let stream = unsafe {
        std::mem::transmute::<mcapable_core::Stream<'_, Chunk>, mcapable_core::Stream<'static, Chunk>>(
            stream,
        )
    };
    Box::into_raw(Box::new(McapChunkStream {
        reader: reader_ptr,
        stream,
    }))
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_reader_records(reader: *mut McapReader) -> *mut McapRecordStream {
    if reader.is_null() {
        set_last_error("null reader".to_string());
        return ptr::null_mut();
    }
    let reader = unsafe { Box::from_raw(reader) };
    let reader = reader.reader;
    let reader_ptr = Box::into_raw(reader);
    let stream = unsafe { &mut *reader_ptr }.records();
    let stream = unsafe {
        std::mem::transmute::<
            mcapable_core::Stream<'_, Record>,
            mcapable_core::Stream<'static, Record>,
        >(stream)
    };
    Box::into_raw(Box::new(McapRecordStream {
        reader: reader_ptr,
        stream,
    }))
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_message_stream_free(stream: *mut McapMessageStream) {
    if stream.is_null() {
        return;
    }
    unsafe {
        let stream = Box::from_raw(stream);
        let reader_ptr = stream.reader;
        drop(stream);
        drop(Box::from_raw(reader_ptr));
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_raw_message_stream_free(stream: *mut McapRawMessageStream) {
    if stream.is_null() {
        return;
    }
    unsafe {
        let stream = Box::from_raw(stream);
        let reader_ptr = stream.reader;
        drop(stream);
        drop(Box::from_raw(reader_ptr));
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_chunk_stream_free(stream: *mut McapChunkStream) {
    if stream.is_null() {
        return;
    }
    unsafe {
        let stream = Box::from_raw(stream);
        let reader_ptr = stream.reader;
        drop(stream);
        drop(Box::from_raw(reader_ptr));
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_record_stream_free(stream: *mut McapRecordStream) {
    if stream.is_null() {
        return;
    }
    unsafe {
        let stream = Box::from_raw(stream);
        let reader_ptr = stream.reader;
        drop(stream);
        drop(Box::from_raw(reader_ptr));
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_message_stream_next(
    stream: *mut McapMessageStream,
    out_message: *mut McapMessage,
) -> bool {
    if stream.is_null() || out_message.is_null() {
        set_last_error("null stream or output".to_string());
        return false;
    }
    let stream = unsafe { &mut *stream };
    match stream.stream.next() {
        Some(Ok(message)) => {
            let data = message.data_bytes().to_vec();
            unsafe {
                *out_message = McapMessage {
                    channel_id: message.channel_id,
                    sequence: message.sequence,
                    log_time: message.log_time,
                    publish_time: message.publish_time,
                    data: McapByteBuffer::from_vec(data),
                };
            }
            true
        }
        Some(Err(err)) => {
            set_last_error(err.to_string());
            false
        }
        None => false,
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_raw_message_stream_next(
    stream: *mut McapRawMessageStream,
    out_message: *mut McapRawMessage,
) -> bool {
    if stream.is_null() || out_message.is_null() {
        set_last_error("null stream or output".to_string());
        return false;
    }
    let stream = unsafe { &mut *stream };
    match stream.stream.next() {
        Some(Ok(message)) => {
            let data = message.data_bytes().to_vec();
            unsafe {
                *out_message = McapRawMessage {
                    channel_id: message.channel_id,
                    sequence: message.sequence,
                    log_time: message.log_time,
                    publish_time: message.publish_time,
                    data: McapByteBuffer::from_vec(data),
                };
            }
            true
        }
        Some(Err(err)) => {
            set_last_error(err.to_string());
            false
        }
        None => false,
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_chunk_stream_next(
    stream: *mut McapChunkStream,
    out_chunk: *mut McapChunk,
) -> bool {
    if stream.is_null() || out_chunk.is_null() {
        set_last_error("null stream or output".to_string());
        return false;
    }
    let stream = unsafe { &mut *stream };
    match stream.stream.next() {
        Some(Ok(chunk)) => {
            unsafe {
                *out_chunk = McapChunk {
                    message_start_time: chunk.message_start_time,
                    message_end_time: chunk.message_end_time,
                    uncompressed_size: chunk.uncompressed_size,
                    uncompressed_crc: chunk.uncompressed_crc,
                    compression: byte_buffer_from_str(chunk.compression.as_ref()),
                    records: McapByteBuffer::from_vec(chunk.records.to_vec()),
                };
            }
            true
        }
        Some(Err(err)) => {
            set_last_error(err.to_string());
            false
        }
        None => false,
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_record_stream_next(
    stream: *mut McapRecordStream,
    out_record: *mut McapRecord,
) -> bool {
    if stream.is_null() || out_record.is_null() {
        set_last_error("null stream or output".to_string());
        return false;
    }
    let stream = unsafe { &mut *stream };
    match stream.stream.next() {
        Some(Ok(record)) => {
            let kind = match record {
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
            unsafe {
                *out_record = McapRecord {
                    kind: byte_buffer_from_str(kind),
                };
            }
            true
        }
        Some(Err(err)) => {
            set_last_error(err.to_string());
            false
        }
        None => false,
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_message_stream_parsed(
    stream: *mut McapMessageStream,
) -> *mut McapParsedStream {
    unsafe { mcap_message_stream_parsed_with(stream, ptr::null(), 0) }
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_message_stream_parsed_with(
    stream: *mut McapMessageStream,
    specs: *const McapParserSpec,
    specs_len: usize,
) -> *mut McapParsedStream {
    if stream.is_null() {
        set_last_error("null stream".to_string());
        return ptr::null_mut();
    }
    let stream = unsafe { Box::from_raw(stream) };
    let McapMessageStream { stream, reader } = *stream;
    let builder = stream.parsed::<ParsedValue>();
    let builder = match apply_parser_specs(builder, specs, specs_len) {
        Ok(builder) => builder,
        Err(err) => {
            set_last_error(err);
            return ptr::null_mut();
        }
    };
    let parsed = builder
        .default_parsers(
            |value| Ok(ParsedValue::Json(value)),
            |data| Ok(ParsedValue::Bytes(data)),
        )
        .build();
    Box::into_raw(Box::new(McapParsedStream {
        stream: parsed,
        reader,
    }))
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_parsed_stream_free(stream: *mut McapParsedStream) {
    if stream.is_null() {
        return;
    }
    unsafe {
        let stream = Box::from_raw(stream);
        let reader_ptr = stream.reader;
        drop(stream);
        drop(Box::from_raw(reader_ptr));
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_parsed_stream_next(
    stream: *mut McapParsedStream,
    out_value: *mut McapParsedValue,
) -> bool {
    if stream.is_null() || out_value.is_null() {
        set_last_error("null stream or output".to_string());
        return false;
    }
    let stream = unsafe { &mut *stream };
    match stream.stream.next() {
        Some(Ok(parsed)) => {
            let (is_json, json, bytes) = match parsed {
                ParsedValue::Json(value) => (
                    true,
                    serde_json::to_vec(&value).unwrap_or_default(),
                    Vec::new(),
                ),
                ParsedValue::Bytes(bytes) => (false, Vec::new(), bytes.to_vec()),
            };
            unsafe {
                *out_value = McapParsedValue {
                    is_json,
                    json: McapByteBuffer::from_vec(json),
                    bytes: McapByteBuffer::from_vec(bytes),
                };
            }
            true
        }
        Some(Err(err)) => {
            set_last_error(err.to_string());
            false
        }
        None => false,
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_message_stream_into_reader(
    stream: *mut McapMessageStream,
) -> *mut McapReader {
    if stream.is_null() {
        set_last_error("null stream".to_string());
        return ptr::null_mut();
    }
    let stream = unsafe { Box::from_raw(stream) };
    let reader_ptr = stream.reader;
    drop(stream);
    let reader = unsafe { Box::from_raw(reader_ptr) };
    Box::into_raw(Box::new(McapReader { reader }))
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_raw_message_stream_into_reader(
    stream: *mut McapRawMessageStream,
) -> *mut McapReader {
    if stream.is_null() {
        set_last_error("null stream".to_string());
        return ptr::null_mut();
    }
    let stream = unsafe { Box::from_raw(stream) };
    let reader_ptr = stream.reader;
    drop(stream);
    let reader = unsafe { Box::from_raw(reader_ptr) };
    Box::into_raw(Box::new(McapReader { reader }))
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_chunk_stream_into_reader(
    stream: *mut McapChunkStream,
) -> *mut McapReader {
    if stream.is_null() {
        set_last_error("null stream".to_string());
        return ptr::null_mut();
    }
    let stream = unsafe { Box::from_raw(stream) };
    let reader_ptr = stream.reader;
    drop(stream);
    let reader = unsafe { Box::from_raw(reader_ptr) };
    Box::into_raw(Box::new(McapReader { reader }))
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_record_stream_into_reader(
    stream: *mut McapRecordStream,
) -> *mut McapReader {
    if stream.is_null() {
        set_last_error("null stream".to_string());
        return ptr::null_mut();
    }
    let stream = unsafe { Box::from_raw(stream) };
    let reader_ptr = stream.reader;
    drop(stream);
    let reader = unsafe { Box::from_raw(reader_ptr) };
    Box::into_raw(Box::new(McapReader { reader }))
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_parsed_stream_into_reader(
    stream: *mut McapParsedStream,
) -> *mut McapReader {
    if stream.is_null() {
        set_last_error("null stream".to_string());
        return ptr::null_mut();
    }
    let stream = unsafe { Box::from_raw(stream) };
    let reader_ptr = stream.reader;
    drop(stream);
    let reader = unsafe { Box::from_raw(reader_ptr) };
    Box::into_raw(Box::new(McapReader { reader }))
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_parsed_value_is_json(value: *const McapParsedValue) -> bool {
    if value.is_null() {
        return false;
    }
    unsafe { (*value).is_json }
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_parsed_value_json(value: *const McapParsedValue) -> McapByteBuffer {
    if value.is_null() {
        return McapByteBuffer::empty();
    }
    unsafe { (*value).json }
}

#[unsafe(no_mangle)]
/// # Safety
/// Caller must provide valid pointers and uphold the C ABI contract.
pub unsafe extern "C" fn mcap_parsed_value_bytes(value: *const McapParsedValue) -> McapByteBuffer {
    if value.is_null() {
        return McapByteBuffer::empty();
    }
    unsafe { (*value).bytes }
}
