use bytes::Bytes;
use mcapable_core::Error;
use mcapable_core::reader::{Builder as ReaderBuilder, Reader};
use mcapable_core::source::{BytesCursor, BytesSource};
use mcapable_core::zero_copy::ByteStr;
use mcapable_core::{
    Chunk, Message, MessageMetadata, RawMessage, Record, RecordMetadata, RecordSource,
};
use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyIOError, PyRuntimeError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use pyo3::types::{PyDict, PyList};
use pyo3::{BoundObject, IntoPyObject};
use serde_json::Value as JsonValue;
use std::cell::RefCell;
use std::rc::Rc;
type ReaderHandle = Reader<Box<dyn BytesSource>>;

create_exception!(mcapable, McapError, PyException);
create_exception!(mcapable, InvalidMagicError, McapError);
create_exception!(mcapable, InvalidRecordError, McapError);
create_exception!(mcapable, UnsupportedVersionError, McapError);
create_exception!(mcapable, InvalidCompressionError, McapError);
create_exception!(mcapable, SchemaNotFoundError, McapError);
create_exception!(mcapable, ChannelNotFoundError, McapError);
create_exception!(mcapable, InvalidSeekTimeError, McapError);
create_exception!(mcapable, InvalidSummaryError, McapError);
create_exception!(mcapable, ParseError, McapError);
create_exception!(mcapable, UnexpectedEofError, McapError);
create_exception!(mcapable, InvalidOpcodeError, McapError);
create_exception!(mcapable, CrcMismatchError, McapError);
create_exception!(mcapable, DecompressionError, McapError);

struct RawMessageStreamInner {
    reader: Box<ReaderHandle>,
    stream: Option<mcapable_core::Stream<'static, RawMessage>>,
}

struct MessageStreamInner {
    reader: Box<ReaderHandle>,
    stream: Option<mcapable_core::Stream<'static, Message>>,
}

struct ChunkStreamInner {
    reader: Box<ReaderHandle>,
    stream: Option<mcapable_core::Stream<'static, Chunk>>,
}

struct RecordStreamInner {
    reader: Box<ReaderHandle>,
    stream: Option<mcapable_core::Stream<'static, Record>>,
}

struct MessageMetadataStreamInner {
    reader: Box<ReaderHandle>,
    stream: Option<mcapable_core::Stream<'static, MessageMetadata>>,
}

struct RecordMetadataStreamInner {
    reader: Box<ReaderHandle>,
    stream: Option<mcapable_core::Stream<'static, RecordMetadata>>,
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

fn parser_kind_from_str(kind: &str) -> PyResult<ParserKind> {
    match kind.to_ascii_lowercase().as_str() {
        "json" => Ok(ParserKind::Json),
        "bytes" => Ok(ParserKind::Bytes),
        "schema_json" => Ok(ParserKind::SchemaJson),
        "schema_bytes" => Ok(ParserKind::SchemaBytes),
        _ => Err(PyRuntimeError::new_err(format!(
            "unsupported parser kind: {kind}"
        ))),
    }
}

struct ParsedMessageStreamInner {
    reader: Box<ReaderHandle>,
    stream: Option<mcapable_core::ParsedStream<'static, ParsedValue>>,
}

fn map_error(err: Error) -> PyErr {
    match err {
        Error::Io(io) => PyIOError::new_err(io.to_string()),
        Error::InvalidMagic => InvalidMagicError::new_err("Invalid MCAP magic bytes"),
        Error::InvalidRecord(message) => InvalidRecordError::new_err(message),
        Error::UnsupportedVersion(message) => UnsupportedVersionError::new_err(message),
        Error::InvalidCompression(message) => InvalidCompressionError::new_err(message),
        Error::SchemaNotFound(id) => SchemaNotFoundError::new_err(id.to_string()),
        Error::ChannelNotFound(id) => ChannelNotFoundError::new_err(id.to_string()),
        Error::InvalidSeekTime(message) => InvalidSeekTimeError::new_err(message),
        Error::InvalidSummary(message) => InvalidSummaryError::new_err(message),
        Error::ParseError(message) => ParseError::new_err(message.to_string()),
        Error::UnexpectedEof(offset) => UnexpectedEofError::new_err(offset.to_string()),
        Error::InvalidOpcode(opcode) => InvalidOpcodeError::new_err(format!("{opcode:#x}")),
        Error::CrcMismatch { expected, actual } => {
            CrcMismatchError::new_err(format!("{expected:#x} != {actual:#x}"))
        }
        Error::DecompressionError(message) => DecompressionError::new_err(message),
    }
}

fn take_filter_error(filter_error: &Rc<RefCell<Option<PyErr>>>) -> Option<PyErr> {
    filter_error.borrow_mut().take()
}

fn json_value_to_py(py: Python<'_>, value: JsonValue) -> PyObject {
    fn to_object<'py, T>(py: Python<'py>, value: T) -> PyObject
    where
        T: IntoPyObject<'py>,
    {
        value
            .into_pyobject(py)
            .map(|obj| obj.into_any().unbind())
            .unwrap_or_else(|_| py.None())
    }

    match value {
        JsonValue::Null => py.None(),
        JsonValue::Bool(value) => to_object(py, value),
        JsonValue::Number(value) => {
            if let Some(int) = value.as_i64() {
                to_object(py, int)
            } else if let Some(uint) = value.as_u64() {
                to_object(py, uint)
            } else if let Some(float) = value.as_f64() {
                to_object(py, float)
            } else {
                py.None()
            }
        }
        JsonValue::String(value) => to_object(py, value),
        JsonValue::Array(values) => {
            let list = PyList::empty(py);
            for value in values {
                let item = json_value_to_py(py, value);
                if list.append(item).is_err() {
                    return py.None();
                }
            }
            list.into_any().unbind()
        }
        JsonValue::Object(values) => {
            let dict = PyDict::new(py);
            for (key, value) in values {
                let item = json_value_to_py(py, value);
                if dict.set_item(key, item).is_err() {
                    return py.None();
                }
            }
            dict.into_any().unbind()
        }
    }
}

fn apply_filter_channel<T>(
    stream: mcapable_core::Stream<'static, T>,
    filter_error: Rc<RefCell<Option<PyErr>>>,
    predicate: Py<PyAny>,
) -> mcapable_core::Stream<'static, T> {
    stream.filter_channel(move |channel| {
        if filter_error.borrow().is_some() {
            return false;
        }
        Python::with_gil(|py| {
            let py_channel = PyChannel {
                id: channel.id,
                topic: byte_str_to_string(&channel.topic),
                message_encoding: byte_str_to_string(&channel.message_encoding),
                schema_id: channel.schema_id,
                metadata: metadata_to_vec(&channel.metadata),
            };
            let py_channel = match Py::new(py, py_channel) {
                Ok(value) => value,
                Err(err) => {
                    *filter_error.borrow_mut() = Some(err);
                    return false;
                }
            };
            let result = predicate.call1(py, (py_channel,));
            match result.and_then(|value| value.extract::<bool>(py)) {
                Ok(value) => value,
                Err(err) => {
                    *filter_error.borrow_mut() = Some(err);
                    false
                }
            }
        })
    })
}

fn byte_str_to_string(value: &ByteStr) -> String {
    value.as_ref().to_string()
}

fn metadata_to_vec(
    metadata: &mcapable_core::collections::HashMap<ByteStr, ByteStr>,
) -> Vec<(String, String)> {
    metadata
        .iter()
        .map(|(k, v)| (byte_str_to_string(k), byte_str_to_string(v)))
        .collect()
}

#[pyclass(name = "Header")]
#[derive(Clone)]
struct PyHeader {
    #[pyo3(get)]
    profile: String,
    #[pyo3(get)]
    library: String,
    metadata: Vec<(String, String)>,
}

#[pymethods]
impl PyHeader {
    #[getter]
    fn metadata(&self) -> Vec<(String, String)> {
        self.metadata.clone()
    }
}

#[pyclass(name = "Schema")]
#[derive(Clone)]
struct PySchema {
    #[pyo3(get)]
    id: u16,
    #[pyo3(get)]
    name: String,
    #[pyo3(get)]
    encoding: String,
    data: Bytes,
}

#[pymethods]
impl PySchema {
    #[getter]
    fn data(&self, py: Python<'_>) -> Py<PyBytes> {
        PyBytes::new(py, self.data.as_ref()).into()
    }
}

#[pyclass(name = "Channel")]
#[derive(Clone)]
struct PyChannel {
    #[pyo3(get)]
    id: u16,
    #[pyo3(get)]
    topic: String,
    #[pyo3(get)]
    message_encoding: String,
    #[pyo3(get)]
    schema_id: u16,
    metadata: Vec<(String, String)>,
}

#[pyclass(name = "Metadata")]
#[derive(Clone)]
struct PyMetadata {
    #[pyo3(get)]
    name: String,
    metadata: Vec<(String, String)>,
}

#[pymethods]
impl PyMetadata {
    #[getter]
    fn metadata(&self) -> Vec<(String, String)> {
        self.metadata.clone()
    }
}

#[pyclass(name = "Attachment")]
#[derive(Clone)]
struct PyAttachment {
    #[pyo3(get)]
    log_time: u64,
    #[pyo3(get)]
    create_time: u64,
    #[pyo3(get)]
    name: String,
    #[pyo3(get)]
    media_type: String,
    data: Bytes,
}

#[pymethods]
impl PyAttachment {
    #[getter]
    fn data(&self, py: Python<'_>) -> Py<PyBytes> {
        PyBytes::new(py, self.data.as_ref()).into()
    }
}

#[pymethods]
impl PyChannel {
    #[getter]
    fn metadata(&self) -> Vec<(String, String)> {
        self.metadata.clone()
    }
}

#[pyclass(name = "RawMessage")]
#[derive(Clone)]
struct PyRawMessage {
    #[pyo3(get)]
    channel_id: u16,
    #[pyo3(get)]
    sequence: u32,
    #[pyo3(get)]
    log_time: u64,
    #[pyo3(get)]
    publish_time: u64,
    data: Bytes,
}

#[pymethods]
impl PyRawMessage {
    #[getter]
    fn data(&self, py: Python<'_>) -> Py<PyBytes> {
        PyBytes::new(py, self.data.as_ref()).into()
    }
}

#[pyclass(name = "Message")]
#[derive(Clone)]
struct PyMessage {
    #[pyo3(get)]
    channel_id: u16,
    #[pyo3(get)]
    sequence: u32,
    #[pyo3(get)]
    log_time: u64,
    #[pyo3(get)]
    publish_time: u64,
    data: Bytes,
}

#[pyclass(name = "Chunk")]
#[derive(Clone)]
struct PyChunk {
    #[pyo3(get)]
    message_start_time: u64,
    #[pyo3(get)]
    message_end_time: u64,
    #[pyo3(get)]
    uncompressed_size: u64,
    #[pyo3(get)]
    uncompressed_crc: u32,
    #[pyo3(get)]
    compression: String,
    records: Bytes,
}

#[pymethods]
impl PyChunk {
    #[getter]
    fn records(&self, py: Python<'_>) -> Py<PyBytes> {
        PyBytes::new(py, self.records.as_ref()).into()
    }
}

#[pyclass(name = "MessageMetadata")]
#[derive(Clone)]
struct PyMessageMetadata {
    #[pyo3(get)]
    channel_id: u16,
    #[pyo3(get)]
    sequence: u32,
    #[pyo3(get)]
    log_time: u64,
    #[pyo3(get)]
    publish_time: u64,
    #[pyo3(get)]
    data_size: u64,
}

#[pyclass(name = "RecordMetadata")]
struct PyRecordMetadata {
    #[pyo3(get)]
    opcode: String,
    #[pyo3(get)]
    length: u64,
    #[pyo3(get)]
    total_len: u64,
    #[pyo3(get)]
    offset: u64,
    #[pyo3(get)]
    source: String,
    message: Option<PyMessageMetadata>,
}

#[pymethods]
impl PyRecordMetadata {
    #[getter]
    fn message(&self) -> Option<PyMessageMetadata> {
        self.message.clone()
    }
}

#[pyclass(name = "Record")]
struct PyRecord {
    #[pyo3(get)]
    kind: String,
    header: Option<PyHeader>,
    schema: Option<PySchema>,
    channel: Option<PyChannel>,
    message: Option<PyMessage>,
    chunk: Option<PyChunk>,
    attachment: Option<PyAttachment>,
    metadata: Option<PyMetadata>,
}

#[pymethods]
impl PyRecord {
    #[getter]
    fn header(&self) -> Option<PyHeader> {
        self.header.clone()
    }

    #[getter]
    fn schema(&self) -> Option<PySchema> {
        self.schema.clone()
    }

    #[getter]
    fn channel(&self) -> Option<PyChannel> {
        self.channel.clone()
    }

    #[getter]
    fn message(&self) -> Option<PyMessage> {
        self.message.clone()
    }

    #[getter]
    fn chunk(&self) -> Option<PyChunk> {
        self.chunk.clone()
    }

    #[getter]
    fn attachment(&self) -> Option<PyAttachment> {
        self.attachment.clone()
    }

    #[getter]
    fn metadata(&self) -> Option<PyMetadata> {
        self.metadata.clone()
    }
}

impl PyRecord {
    fn from_record(record: Record) -> Self {
        match record {
            Record::Header(header) => PyRecord {
                kind: "Header".to_string(),
                header: Some(PyHeader {
                    profile: byte_str_to_string(&header.profile),
                    library: byte_str_to_string(&header.library),
                    metadata: metadata_to_vec(&header.metadata),
                }),
                schema: None,
                channel: None,
                message: None,
                chunk: None,
                attachment: None,
                metadata: None,
            },
            Record::Footer(_) => PyRecord {
                kind: "Footer".to_string(),
                header: None,
                schema: None,
                channel: None,
                message: None,
                chunk: None,
                attachment: None,
                metadata: None,
            },
            Record::Schema(schema) => PyRecord {
                kind: "Schema".to_string(),
                header: None,
                schema: Some(PySchema {
                    id: schema.id,
                    name: byte_str_to_string(&schema.name),
                    encoding: byte_str_to_string(&schema.encoding),
                    data: schema.data,
                }),
                channel: None,
                message: None,
                chunk: None,
                attachment: None,
                metadata: None,
            },
            Record::Channel(channel) => PyRecord {
                kind: "Channel".to_string(),
                header: None,
                schema: None,
                channel: Some(PyChannel {
                    id: channel.id,
                    topic: byte_str_to_string(&channel.topic),
                    message_encoding: byte_str_to_string(&channel.message_encoding),
                    schema_id: channel.schema_id,
                    metadata: metadata_to_vec(&channel.metadata),
                }),
                message: None,
                chunk: None,
                attachment: None,
                metadata: None,
            },
            Record::Message(message) => PyRecord {
                kind: "Message".to_string(),
                header: None,
                schema: None,
                channel: None,
                message: Some(PyMessage {
                    channel_id: message.channel_id,
                    sequence: message.sequence,
                    log_time: message.log_time,
                    publish_time: message.publish_time,
                    data: message.data_bytes(),
                }),
                chunk: None,
                attachment: None,
                metadata: None,
            },
            Record::Chunk(chunk) => PyRecord {
                kind: "Chunk".to_string(),
                header: None,
                schema: None,
                channel: None,
                message: None,
                chunk: Some(PyChunk {
                    message_start_time: chunk.message_start_time,
                    message_end_time: chunk.message_end_time,
                    uncompressed_size: chunk.uncompressed_size,
                    uncompressed_crc: chunk.uncompressed_crc,
                    compression: byte_str_to_string(&chunk.compression),
                    records: chunk.records,
                }),
                attachment: None,
                metadata: None,
            },
            Record::Attachment(attachment) => PyRecord {
                kind: "Attachment".to_string(),
                header: None,
                schema: None,
                channel: None,
                message: None,
                chunk: None,
                attachment: Some(PyAttachment {
                    log_time: attachment.log_time,
                    create_time: attachment.create_time,
                    name: byte_str_to_string(&attachment.name),
                    media_type: byte_str_to_string(&attachment.media_type),
                    data: attachment.data,
                }),
                metadata: None,
            },
            Record::Metadata(metadata) => PyRecord {
                kind: "Metadata".to_string(),
                header: None,
                schema: None,
                channel: None,
                message: None,
                chunk: None,
                attachment: None,
                metadata: Some(PyMetadata {
                    name: byte_str_to_string(&metadata.name),
                    metadata: metadata_to_vec(&metadata.metadata),
                }),
            },
            Record::SummaryOffset => PyRecord {
                kind: "SummaryOffset".to_string(),
                header: None,
                schema: None,
                channel: None,
                message: None,
                chunk: None,
                attachment: None,
                metadata: None,
            },
            Record::DataEnd => PyRecord {
                kind: "DataEnd".to_string(),
                header: None,
                schema: None,
                channel: None,
                message: None,
                chunk: None,
                attachment: None,
                metadata: None,
            },
            _ => PyRecord {
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
}
#[pymethods]
impl PyMessage {
    #[getter]
    fn data(&self, py: Python<'_>) -> Py<PyBytes> {
        PyBytes::new(py, self.data.as_ref()).into()
    }
}

#[pyclass(name = "ReaderBuilder")]
struct PyReaderBuilder {
    validate_end_magic: bool,
}

#[pymethods]
impl PyReaderBuilder {
    #[new]
    fn new() -> Self {
        Self {
            validate_end_magic: true,
        }
    }

    fn validate_end_magic(&mut self, validate: bool) {
        self.validate_end_magic = validate;
    }

    fn build_from_path(&self, path: &str) -> PyResult<PyReader> {
        let file = std::fs::File::open(path).map_err(|e| PyIOError::new_err(e.to_string()))?;
        let source: Box<dyn BytesSource> =
            Box::new(mcapable_core::source::ArenaBytesSource::new(file));
        let reader = ReaderBuilder::new()
            .validate_end_magic(self.validate_end_magic)
            .build(source)
            .map_err(map_error)?;
        Ok(PyReader {
            inner: Some(reader),
        })
    }

    fn build_from_bytes(&self, bytes: &Bound<'_, PyBytes>) -> PyResult<PyReader> {
        let data = Bytes::copy_from_slice(bytes.as_bytes());
        let source: Box<dyn BytesSource> = Box::new(BytesCursor::new(data));
        let reader = ReaderBuilder::new()
            .validate_end_magic(self.validate_end_magic)
            .build(source)
            .map_err(map_error)?;
        Ok(PyReader {
            inner: Some(reader),
        })
    }
}

#[pyclass(unsendable, name = "Reader")]
struct PyReader {
    inner: Option<ReaderHandle>,
}

#[pymethods]
impl PyReader {
    #[staticmethod]
    fn from_path(path: &str) -> PyResult<Self> {
        let file = std::fs::File::open(path).map_err(|e| PyIOError::new_err(e.to_string()))?;
        let source: Box<dyn BytesSource> =
            Box::new(mcapable_core::source::ArenaBytesSource::new(file));
        let reader = ReaderBuilder::new().build(source).map_err(map_error)?;
        Ok(Self {
            inner: Some(reader),
        })
    }

    #[staticmethod]
    fn from_bytes(bytes: &Bound<'_, PyBytes>) -> PyResult<Self> {
        let data = Bytes::copy_from_slice(bytes.as_bytes());
        let source: Box<dyn BytesSource> = Box::new(BytesCursor::new(data));
        let reader = ReaderBuilder::new().build(source).map_err(map_error)?;
        Ok(Self {
            inner: Some(reader),
        })
    }

    fn header(&mut self) -> PyResult<PyHeader> {
        let reader = self
            .inner
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("reader is already in use"))?;
        let header = reader.header().map_err(map_error)?;
        Ok(PyHeader {
            profile: byte_str_to_string(&header.profile),
            library: byte_str_to_string(&header.library),
            metadata: metadata_to_vec(&header.metadata),
        })
    }

    fn schemas(&mut self) -> PyResult<Vec<PySchema>> {
        let reader = self
            .inner
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("reader is already in use"))?;
        let schemas = reader.schemas();
        Ok(schemas
            .values()
            .map(|schema| PySchema {
                id: schema.id,
                name: byte_str_to_string(&schema.name),
                encoding: byte_str_to_string(&schema.encoding),
                data: schema.data.clone(),
            })
            .collect())
    }

    fn channels(&mut self) -> PyResult<Vec<PyChannel>> {
        let reader = self
            .inner
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("reader is already in use"))?;
        let channels = reader.channels();
        Ok(channels
            .values()
            .map(|channel| PyChannel {
                id: channel.id,
                topic: byte_str_to_string(&channel.topic),
                message_encoding: byte_str_to_string(&channel.message_encoding),
                schema_id: channel.schema_id,
                metadata: metadata_to_vec(&channel.metadata),
            })
            .collect())
    }

    fn metadata(&mut self, name: &str) -> PyResult<Option<PyMetadata>> {
        let reader = self
            .inner
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("reader is already in use"))?;
        Ok(reader.metadata(name).map(|metadata| PyMetadata {
            name: byte_str_to_string(&metadata.name),
            metadata: metadata_to_vec(&metadata.metadata),
        }))
    }

    fn all_metadata(&mut self) -> PyResult<Vec<PyMetadata>> {
        let reader = self
            .inner
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("reader is already in use"))?;
        let all = reader.all_metadata();
        Ok(all
            .values()
            .map(|metadata| PyMetadata {
                name: byte_str_to_string(&metadata.name),
                metadata: metadata_to_vec(&metadata.metadata),
            })
            .collect())
    }

    fn attachment(&mut self, name: &str) -> PyResult<Option<PyAttachment>> {
        let reader = self
            .inner
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("reader is already in use"))?;
        Ok(reader.attachment(name).map(|attachment| PyAttachment {
            log_time: attachment.log_time,
            create_time: attachment.create_time,
            name: byte_str_to_string(&attachment.name),
            media_type: byte_str_to_string(&attachment.media_type),
            data: attachment.data.clone(),
        }))
    }

    fn all_attachments(&mut self) -> PyResult<Vec<PyAttachment>> {
        let reader = self
            .inner
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("reader is already in use"))?;
        let all = reader.all_attachments();
        Ok(all
            .values()
            .map(|attachment| PyAttachment {
                log_time: attachment.log_time,
                create_time: attachment.create_time,
                name: byte_str_to_string(&attachment.name),
                media_type: byte_str_to_string(&attachment.media_type),
                data: attachment.data.clone(),
            })
            .collect())
    }

    fn raw_messages(&mut self) -> PyResult<PyRawMessageStream> {
        let reader = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("reader is already in use"))?;
        let mut reader = Box::new(reader);
        let stream = reader.raw_messages().map_err(map_error)?;
        // Safety: stream borrows from reader owned by this struct; we drop stream before reader.
        let stream = unsafe {
            std::mem::transmute::<
                mcapable_core::Stream<'_, RawMessage>,
                mcapable_core::Stream<'static, RawMessage>,
            >(stream)
        };
        Ok(PyRawMessageStream {
            inner: Some(RawMessageStreamInner {
                reader,
                stream: Some(stream),
            }),
            filter_error: Rc::new(RefCell::new(None)),
        })
    }

    fn messages(&mut self) -> PyResult<PyMessageStream> {
        let reader = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("reader is already in use"))?;
        let mut reader = Box::new(reader);
        let stream = reader.messages().map_err(map_error)?;
        // Safety: stream borrows from reader owned by this struct; we drop stream before reader.
        let stream = unsafe {
            std::mem::transmute::<
                mcapable_core::Stream<'_, Message>,
                mcapable_core::Stream<'static, Message>,
            >(stream)
        };
        Ok(PyMessageStream {
            inner: Some(MessageStreamInner {
                reader,
                stream: Some(stream),
            }),
            filter_error: Rc::new(RefCell::new(None)),
        })
    }

    fn chunks(&mut self) -> PyResult<PyChunkStream> {
        let reader = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("reader is already in use"))?;
        let mut reader = Box::new(reader);
        let stream = reader.chunks();
        let stream = unsafe {
            std::mem::transmute::<
                mcapable_core::Stream<'_, Chunk>,
                mcapable_core::Stream<'static, Chunk>,
            >(stream)
        };
        Ok(PyChunkStream {
            inner: Some(ChunkStreamInner {
                reader,
                stream: Some(stream),
            }),
            filter_error: Rc::new(RefCell::new(None)),
        })
    }

    fn records(&mut self) -> PyResult<PyRecordStream> {
        let reader = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("reader is already in use"))?;
        let mut reader = Box::new(reader);
        let stream = reader.records();
        let stream = unsafe {
            std::mem::transmute::<
                mcapable_core::Stream<'_, Record>,
                mcapable_core::Stream<'static, Record>,
            >(stream)
        };
        Ok(PyRecordStream {
            inner: Some(RecordStreamInner {
                reader,
                stream: Some(stream),
            }),
            filter_error: Rc::new(RefCell::new(None)),
        })
    }

    fn message_metadata(&mut self) -> PyResult<PyMessageMetadataStream> {
        let reader = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("reader is already in use"))?;
        let mut reader = Box::new(reader);
        let stream = reader.message_metadata().map_err(map_error)?;
        let stream = unsafe {
            std::mem::transmute::<
                mcapable_core::Stream<'_, MessageMetadata>,
                mcapable_core::Stream<'static, MessageMetadata>,
            >(stream)
        };
        Ok(PyMessageMetadataStream {
            inner: Some(MessageMetadataStreamInner {
                reader,
                stream: Some(stream),
            }),
            filter_error: Rc::new(RefCell::new(None)),
        })
    }

    fn record_metadata(&mut self) -> PyResult<PyRecordMetadataStream> {
        let reader = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("reader is already in use"))?;
        let mut reader = Box::new(reader);
        let stream = reader.record_metadata();
        let stream = unsafe {
            std::mem::transmute::<
                mcapable_core::Stream<'_, RecordMetadata>,
                mcapable_core::Stream<'static, RecordMetadata>,
            >(stream)
        };
        Ok(PyRecordMetadataStream {
            inner: Some(RecordMetadataStreamInner {
                reader,
                stream: Some(stream),
            }),
            filter_error: Rc::new(RefCell::new(None)),
        })
    }
}

#[pyclass(unsendable, name = "RawMessageStream")]
struct PyRawMessageStream {
    inner: Option<RawMessageStreamInner>,
    filter_error: Rc<RefCell<Option<PyErr>>>,
}

#[pymethods]
impl PyRawMessageStream {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn time_range<'py>(
        mut slf: PyRefMut<'py, Self>,
        start: u64,
        end: u64,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let inner = slf
            .inner
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        let stream = inner
            .stream
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        inner.stream = Some(stream.time_range(start, end));
        Ok(slf)
    }

    fn filter_channel<'py>(
        mut slf: PyRefMut<'py, Self>,
        predicate: Py<PyAny>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let filter_error = slf.filter_error.clone();
        let inner = slf
            .inner
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        let stream = inner
            .stream
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        inner.stream = Some(apply_filter_channel(stream, filter_error, predicate));
        Ok(slf)
    }

    fn __next__(&mut self) -> PyResult<Option<PyRawMessage>> {
        if let Some(err) = take_filter_error(&self.filter_error) {
            return Err(err);
        }
        let inner = match self.inner.as_mut() {
            Some(inner) => inner,
            None => return Ok(None),
        };
        let stream = match inner.stream.as_mut() {
            Some(stream) => stream,
            None => return Ok(None),
        };
        let result = match stream.next() {
            Some(Ok(raw)) => Ok(Some(PyRawMessage {
                channel_id: raw.channel_id,
                sequence: raw.sequence,
                log_time: raw.log_time,
                publish_time: raw.publish_time,
                data: raw.data_bytes(),
            })),
            Some(Err(err)) => Err(map_error(err)),
            None => Ok(None),
        };
        if let Some(err) = take_filter_error(&self.filter_error) {
            return Err(err);
        }
        result
    }

    #[allow(clippy::wrong_self_convention)]
    fn into_reader(&mut self) -> PyResult<PyReader> {
        let inner = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        let RawMessageStreamInner { reader, stream } = inner;
        drop(stream);
        Ok(PyReader {
            inner: Some(*reader),
        })
    }
}

#[pyclass(unsendable, name = "MessageStream")]
struct PyMessageStream {
    inner: Option<MessageStreamInner>,
    filter_error: Rc<RefCell<Option<PyErr>>>,
}

#[pymethods]
impl PyMessageStream {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn time_range<'py>(
        mut slf: PyRefMut<'py, Self>,
        start: u64,
        end: u64,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let inner = slf
            .inner
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        let stream = inner
            .stream
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        inner.stream = Some(stream.time_range(start, end));
        Ok(slf)
    }

    fn filter_channel<'py>(
        mut slf: PyRefMut<'py, Self>,
        predicate: Py<PyAny>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let filter_error = slf.filter_error.clone();
        let inner = slf
            .inner
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        let stream = inner
            .stream
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        inner.stream = Some(apply_filter_channel(stream, filter_error, predicate));
        Ok(slf)
    }

    fn parsed(&mut self) -> PyResult<PyParsedMessageStream> {
        let inner = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        let stream = inner
            .stream
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        let reader = inner.reader;
        let parsed = stream
            .parsed::<ParsedValue>()
            .default_parsers(
                |value| Ok(ParsedValue::Json(value)),
                |data| Ok(ParsedValue::Bytes(data)),
            )
            .build();
        Ok(PyParsedMessageStream {
            inner: Some(ParsedMessageStreamInner {
                reader,
                stream: Some(parsed),
            }),
        })
    }

    fn parsed_with(&mut self, parsers: Vec<(String, String)>) -> PyResult<PyParsedMessageStream> {
        let inner = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        let stream = inner
            .stream
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        let reader = inner.reader;
        let mut builder = stream.parsed::<ParsedValue>();
        for (encoding, kind) in parsers {
            match parser_kind_from_str(&kind)? {
                ParserKind::Json => {
                    builder = builder.parser_message_encoding(encoding, |data| {
                        serde_json::from_slice(data.as_ref())
                            .map(ParsedValue::Json)
                            .map_err(|err| Error::InvalidRecord(err.to_string()))
                    });
                }
                ParserKind::Bytes => {
                    builder = builder
                        .parser_message_encoding(encoding, |data| Ok(ParsedValue::Bytes(data)));
                }
                ParserKind::SchemaJson => {
                    builder = builder.parser_schema_encoding(encoding, |data| {
                        serde_json::from_slice(data.as_ref())
                            .map(ParsedValue::Json)
                            .map_err(|err| Error::InvalidRecord(err.to_string()))
                    });
                }
                ParserKind::SchemaBytes => {
                    builder = builder
                        .parser_schema_encoding(encoding, |data| Ok(ParsedValue::Bytes(data)));
                }
            }
        }
        let parsed = builder
            .default_parsers(
                |value| Ok(ParsedValue::Json(value)),
                |data| Ok(ParsedValue::Bytes(data)),
            )
            .build();
        Ok(PyParsedMessageStream {
            inner: Some(ParsedMessageStreamInner {
                reader,
                stream: Some(parsed),
            }),
        })
    }

    fn __next__(&mut self) -> PyResult<Option<PyMessage>> {
        if let Some(err) = take_filter_error(&self.filter_error) {
            return Err(err);
        }
        let inner = match self.inner.as_mut() {
            Some(inner) => inner,
            None => return Ok(None),
        };
        let stream = match inner.stream.as_mut() {
            Some(stream) => stream,
            None => return Ok(None),
        };
        let result = match stream.next() {
            Some(Ok(message)) => Ok(Some(PyMessage {
                channel_id: message.channel_id,
                sequence: message.sequence,
                log_time: message.log_time,
                publish_time: message.publish_time,
                data: message.data_bytes(),
            })),
            Some(Err(err)) => Err(map_error(err)),
            None => Ok(None),
        };
        if let Some(err) = take_filter_error(&self.filter_error) {
            return Err(err);
        }
        result
    }

    #[allow(clippy::wrong_self_convention)]
    fn into_reader(&mut self) -> PyResult<PyReader> {
        let inner = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        let MessageStreamInner { reader, stream } = inner;
        drop(stream);
        Ok(PyReader {
            inner: Some(*reader),
        })
    }
}

#[pyclass(unsendable, name = "ParsedMessageStream")]
struct PyParsedMessageStream {
    inner: Option<ParsedMessageStreamInner>,
}

#[pymethods]
impl PyParsedMessageStream {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self, py: Python<'_>) -> PyResult<Option<PyObject>> {
        let inner = match self.inner.as_mut() {
            Some(inner) => inner,
            None => return Ok(None),
        };
        let stream = match inner.stream.as_mut() {
            Some(stream) => stream,
            None => return Ok(None),
        };
        match stream.next() {
            Some(Ok(parsed)) => {
                let value = match parsed {
                    ParsedValue::Json(value) => json_value_to_py(py, value),
                    ParsedValue::Bytes(bytes) => {
                        PyBytes::new(py, bytes.as_ref()).into_any().unbind()
                    }
                };
                Ok(Some(value))
            }
            Some(Err(err)) => Err(map_error(err)),
            None => Ok(None),
        }
    }

    #[allow(clippy::wrong_self_convention)]
    fn into_reader(&mut self) -> PyResult<PyReader> {
        let inner = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        let ParsedMessageStreamInner { reader, stream } = inner;
        drop(stream);
        Ok(PyReader {
            inner: Some(*reader),
        })
    }
}

#[pyclass(unsendable, name = "ChunkStream")]
struct PyChunkStream {
    inner: Option<ChunkStreamInner>,
    filter_error: Rc<RefCell<Option<PyErr>>>,
}

#[pymethods]
impl PyChunkStream {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn time_range<'py>(
        mut slf: PyRefMut<'py, Self>,
        start: u64,
        end: u64,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let inner = slf
            .inner
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        let stream = inner
            .stream
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        inner.stream = Some(stream.time_range(start, end));
        Ok(slf)
    }

    fn filter_channel<'py>(
        mut slf: PyRefMut<'py, Self>,
        predicate: Py<PyAny>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let filter_error = slf.filter_error.clone();
        let inner = slf
            .inner
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        let stream = inner
            .stream
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        inner.stream = Some(apply_filter_channel(stream, filter_error, predicate));
        Ok(slf)
    }

    fn __next__(&mut self) -> PyResult<Option<PyChunk>> {
        if let Some(err) = take_filter_error(&self.filter_error) {
            return Err(err);
        }
        let inner = match self.inner.as_mut() {
            Some(inner) => inner,
            None => return Ok(None),
        };
        let stream = match inner.stream.as_mut() {
            Some(stream) => stream,
            None => return Ok(None),
        };
        let result = match stream.next() {
            Some(Ok(chunk)) => Ok(Some(PyChunk {
                message_start_time: chunk.message_start_time,
                message_end_time: chunk.message_end_time,
                uncompressed_size: chunk.uncompressed_size,
                uncompressed_crc: chunk.uncompressed_crc,
                compression: byte_str_to_string(&chunk.compression),
                records: chunk.records,
            })),
            Some(Err(err)) => Err(map_error(err)),
            None => Ok(None),
        };
        if let Some(err) = take_filter_error(&self.filter_error) {
            return Err(err);
        }
        result
    }

    #[allow(clippy::wrong_self_convention)]
    fn into_reader(&mut self) -> PyResult<PyReader> {
        let inner = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        let ChunkStreamInner { reader, stream } = inner;
        drop(stream);
        Ok(PyReader {
            inner: Some(*reader),
        })
    }
}

#[pyclass(unsendable, name = "RecordStream")]
struct PyRecordStream {
    inner: Option<RecordStreamInner>,
    filter_error: Rc<RefCell<Option<PyErr>>>,
}

#[pymethods]
impl PyRecordStream {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn time_range<'py>(
        mut slf: PyRefMut<'py, Self>,
        start: u64,
        end: u64,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let inner = slf
            .inner
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        let stream = inner
            .stream
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        inner.stream = Some(stream.time_range(start, end));
        Ok(slf)
    }

    fn filter_channel<'py>(
        mut slf: PyRefMut<'py, Self>,
        predicate: Py<PyAny>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let filter_error = slf.filter_error.clone();
        let inner = slf
            .inner
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        let stream = inner
            .stream
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        inner.stream = Some(apply_filter_channel(stream, filter_error, predicate));
        Ok(slf)
    }

    fn __next__(&mut self) -> PyResult<Option<PyRecord>> {
        if let Some(err) = take_filter_error(&self.filter_error) {
            return Err(err);
        }
        let inner = match self.inner.as_mut() {
            Some(inner) => inner,
            None => return Ok(None),
        };
        let stream = match inner.stream.as_mut() {
            Some(stream) => stream,
            None => return Ok(None),
        };
        let result = match stream.next() {
            Some(Ok(record)) => Ok(Some(PyRecord::from_record(record))),
            Some(Err(err)) => Err(map_error(err)),
            None => Ok(None),
        };
        if let Some(err) = take_filter_error(&self.filter_error) {
            return Err(err);
        }
        result
    }

    #[allow(clippy::wrong_self_convention)]
    fn into_reader(&mut self) -> PyResult<PyReader> {
        let inner = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        let RecordStreamInner { reader, stream } = inner;
        drop(stream);
        Ok(PyReader {
            inner: Some(*reader),
        })
    }
}

#[pyclass(unsendable, name = "MessageMetadataStream")]
struct PyMessageMetadataStream {
    inner: Option<MessageMetadataStreamInner>,
    filter_error: Rc<RefCell<Option<PyErr>>>,
}

#[pymethods]
impl PyMessageMetadataStream {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn time_range<'py>(
        mut slf: PyRefMut<'py, Self>,
        start: u64,
        end: u64,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let inner = slf
            .inner
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        let stream = inner
            .stream
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        inner.stream = Some(stream.time_range(start, end));
        Ok(slf)
    }

    fn filter_channel<'py>(
        mut slf: PyRefMut<'py, Self>,
        predicate: Py<PyAny>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let filter_error = slf.filter_error.clone();
        let inner = slf
            .inner
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        let stream = inner
            .stream
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        inner.stream = Some(apply_filter_channel(stream, filter_error, predicate));
        Ok(slf)
    }

    fn __next__(&mut self) -> PyResult<Option<PyMessageMetadata>> {
        if let Some(err) = take_filter_error(&self.filter_error) {
            return Err(err);
        }
        let inner = match self.inner.as_mut() {
            Some(inner) => inner,
            None => return Ok(None),
        };
        let stream = match inner.stream.as_mut() {
            Some(stream) => stream,
            None => return Ok(None),
        };
        let result = match stream.next() {
            Some(Ok(message)) => Ok(Some(PyMessageMetadata {
                channel_id: message.channel_id,
                sequence: message.sequence,
                log_time: message.log_time,
                publish_time: message.publish_time,
                data_size: message.data_size,
            })),
            Some(Err(err)) => Err(map_error(err)),
            None => Ok(None),
        };
        if let Some(err) = take_filter_error(&self.filter_error) {
            return Err(err);
        }
        result
    }

    #[allow(clippy::wrong_self_convention)]
    fn into_reader(&mut self) -> PyResult<PyReader> {
        let inner = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        let MessageMetadataStreamInner { reader, stream } = inner;
        drop(stream);
        Ok(PyReader {
            inner: Some(*reader),
        })
    }
}

#[pyclass(unsendable, name = "RecordMetadataStream")]
struct PyRecordMetadataStream {
    inner: Option<RecordMetadataStreamInner>,
    filter_error: Rc<RefCell<Option<PyErr>>>,
}

#[pymethods]
impl PyRecordMetadataStream {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn time_range<'py>(
        mut slf: PyRefMut<'py, Self>,
        start: u64,
        end: u64,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let inner = slf
            .inner
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        let stream = inner
            .stream
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        inner.stream = Some(stream.time_range(start, end));
        Ok(slf)
    }

    fn filter_channel<'py>(
        mut slf: PyRefMut<'py, Self>,
        predicate: Py<PyAny>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        let filter_error = slf.filter_error.clone();
        let inner = slf
            .inner
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        let stream = inner
            .stream
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        inner.stream = Some(apply_filter_channel(stream, filter_error, predicate));
        Ok(slf)
    }

    fn __next__(&mut self) -> PyResult<Option<PyRecordMetadata>> {
        if let Some(err) = take_filter_error(&self.filter_error) {
            return Err(err);
        }
        let inner = match self.inner.as_mut() {
            Some(inner) => inner,
            None => return Ok(None),
        };
        let stream = match inner.stream.as_mut() {
            Some(stream) => stream,
            None => return Ok(None),
        };
        let result = match stream.next() {
            Some(Ok(record)) => Ok(Some(PyRecordMetadata {
                opcode: format!("{:?}", record.opcode),
                length: record.length,
                total_len: record.total_len,
                offset: record.offset,
                source: match record.source {
                    RecordSource::File => "File".to_string(),
                    RecordSource::Chunk { chunk_offset } => {
                        format!("Chunk({chunk_offset})")
                    }
                },
                message: record.message.map(|message| PyMessageMetadata {
                    channel_id: message.channel_id,
                    sequence: message.sequence,
                    log_time: message.log_time,
                    publish_time: message.publish_time,
                    data_size: message.data_size,
                }),
            })),
            Some(Err(err)) => Err(map_error(err)),
            None => Ok(None),
        };
        if let Some(err) = take_filter_error(&self.filter_error) {
            return Err(err);
        }
        result
    }

    #[allow(clippy::wrong_self_convention)]
    fn into_reader(&mut self) -> PyResult<PyReader> {
        let inner = self
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("stream is already consumed"))?;
        let RecordMetadataStreamInner { reader, stream } = inner;
        drop(stream);
        Ok(PyReader {
            inner: Some(*reader),
        })
    }
}

#[pymodule]
fn mcapable(_py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyReader>()?;
    module.add_class::<PyReaderBuilder>()?;
    module.add_class::<PyHeader>()?;
    module.add_class::<PySchema>()?;
    module.add_class::<PyChannel>()?;
    module.add_class::<PyMetadata>()?;
    module.add_class::<PyAttachment>()?;
    module.add_class::<PyRawMessage>()?;
    module.add_class::<PyMessage>()?;
    module.add_class::<PyChunk>()?;
    module.add_class::<PyMessageMetadata>()?;
    module.add_class::<PyRecordMetadata>()?;
    module.add_class::<PyRecord>()?;
    module.add_class::<PyRawMessageStream>()?;
    module.add_class::<PyMessageStream>()?;
    module.add_class::<PyParsedMessageStream>()?;
    module.add_class::<PyChunkStream>()?;
    module.add_class::<PyRecordStream>()?;
    module.add_class::<PyMessageMetadataStream>()?;
    module.add_class::<PyRecordMetadataStream>()?;
    module.add("McapError", _py.get_type::<McapError>())?;
    module.add("InvalidMagicError", _py.get_type::<InvalidMagicError>())?;
    module.add("InvalidRecordError", _py.get_type::<InvalidRecordError>())?;
    module.add(
        "UnsupportedVersionError",
        _py.get_type::<UnsupportedVersionError>(),
    )?;
    module.add(
        "InvalidCompressionError",
        _py.get_type::<InvalidCompressionError>(),
    )?;
    module.add("SchemaNotFoundError", _py.get_type::<SchemaNotFoundError>())?;
    module.add(
        "ChannelNotFoundError",
        _py.get_type::<ChannelNotFoundError>(),
    )?;
    module.add(
        "InvalidSeekTimeError",
        _py.get_type::<InvalidSeekTimeError>(),
    )?;
    module.add("InvalidSummaryError", _py.get_type::<InvalidSummaryError>())?;
    module.add("ParseError", _py.get_type::<ParseError>())?;
    module.add("UnexpectedEofError", _py.get_type::<UnexpectedEofError>())?;
    module.add("InvalidOpcodeError", _py.get_type::<InvalidOpcodeError>())?;
    module.add("CrcMismatchError", _py.get_type::<CrcMismatchError>())?;
    module.add("DecompressionError", _py.get_type::<DecompressionError>())?;
    module.add("__version__", env!("CARGO_PKG_VERSION"))?;
    module.add("__doc__", "mcapable Python bindings")?;
    Ok(())
}
