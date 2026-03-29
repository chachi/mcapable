package mcapable

/*
#cgo CFLAGS: -I${SRCDIR}/../../examples/c
#cgo LDFLAGS: -L${SRCDIR}/../../target/debug -lmcapable_ffi -Wl,-rpath,${SRCDIR}/../../target/debug
#include <stdlib.h>
#include "mcapable_ffi.h"
*/
import "C"

import (
	"encoding/json"
	"errors"
	"unsafe"
)

type Reader struct {
	ptr *C.McapReader
}

type Header struct {
	Profile  string
	Library  string
	Metadata map[string]string
}

type Schema struct {
	ID       uint16
	Name     string
	Encoding string
	Data     []byte
}

type Channel struct {
	ID              uint16
	Topic           string
	MessageEncoding string
	SchemaID        uint16
	Metadata        map[string]string
}

type Message struct {
	ChannelID   uint16
	Sequence    uint32
	LogTime     uint64
	PublishTime uint64
	Data        []byte
}

type RawMessage struct {
	ChannelID   uint16
	Sequence    uint32
	LogTime     uint64
	PublishTime uint64
	Data        []byte
}

type Chunk struct {
	MessageStartTime uint64
	MessageEndTime   uint64
	UncompressedSize uint64
	UncompressedCRC  uint32
	Compression      string
	Records          []byte
}

type Record struct {
	Kind string
}

type ParsedMessage struct {
	IsJSON bool
	JSON   any
	Bytes  []byte
}

type ParserKind int

const (
	ParserJSON ParserKind = iota
	ParserBytes
)

type ParserSpec struct {
	Encoding string
	Kind     ParserKind
}

type MessageStream struct {
	ptr *C.McapMessageStream
}

type RawMessageStream struct {
	ptr *C.McapRawMessageStream
}

type ChunkStream struct {
	ptr *C.McapChunkStream
}

type RecordStream struct {
	ptr *C.McapRecordStream
}

type ParsedStream struct {
	ptr *C.McapParsedStream
}

func NewReaderFromBytes(data []byte) (*Reader, error) {
	var ptr *C.uchar
	if len(data) > 0 {
		ptr = (*C.uchar)(unsafe.Pointer(&data[0]))
	}
	reader := C.mcap_reader_from_bytes(ptr, C.size_t(len(data)))
	if reader == nil {
		return nil, lastErrorOr("failed to create reader")
	}
	return &Reader{ptr: reader}, nil
}

func (r *Reader) Close() {
	if r == nil || r.ptr == nil {
		return
	}
	C.mcap_reader_free(r.ptr)
	r.ptr = nil
}

func (r *Reader) Header() (Header, error) {
	if r == nil || r.ptr == nil {
		return Header{}, errors.New("reader is closed")
	}
	var header C.McapHeader
	if ok := C.mcap_reader_header(r.ptr, &header); !ok {
		return Header{}, lastErrorOr("failed to read header")
	}
	out := Header{
		Profile:  byteBufferToString(header.profile),
		Library:  byteBufferToString(header.library),
		Metadata: keyValuesFromSlice(header.metadata, header.metadata_len),
	}
	C.mcap_header_clear(&header)
	return out, nil
}

func (r *Reader) Schemas() ([]Schema, error) {
	if r == nil || r.ptr == nil {
		return nil, errors.New("reader is closed")
	}
	var ptr *C.McapSchema
	var length C.size_t
	if ok := C.mcap_reader_schemas(r.ptr, &ptr, &length); !ok {
		return nil, lastErrorOr("failed to read schemas")
	}
	defer C.mcap_schema_array_free(ptr, length)
	count := int(length)
	if ptr == nil || count == 0 {
		return nil, nil
	}
	items := unsafe.Slice(ptr, count)
	out := make([]Schema, 0, count)
	for _, schema := range items {
		out = append(out, Schema{
			ID:       uint16(schema.id),
			Name:     byteBufferToString(schema.name),
			Encoding: byteBufferToString(schema.encoding),
			Data:     byteBufferToBytes(schema.data),
		})
	}
	return out, nil
}

func (r *Reader) Channels() ([]Channel, error) {
	if r == nil || r.ptr == nil {
		return nil, errors.New("reader is closed")
	}
	var ptr *C.McapChannel
	var length C.size_t
	if ok := C.mcap_reader_channels(r.ptr, &ptr, &length); !ok {
		return nil, lastErrorOr("failed to read channels")
	}
	defer C.mcap_channel_array_free(ptr, length)
	count := int(length)
	if ptr == nil || count == 0 {
		return nil, nil
	}
	items := unsafe.Slice(ptr, count)
	out := make([]Channel, 0, count)
	for _, channel := range items {
		out = append(out, Channel{
			ID:              uint16(channel.id),
			Topic:           byteBufferToString(channel.topic),
			MessageEncoding: byteBufferToString(channel.message_encoding),
			SchemaID:        uint16(channel.schema_id),
			Metadata:        keyValuesFromSlice(channel.metadata, channel.metadata_len),
		})
	}
	return out, nil
}

func (r *Reader) Messages() (*MessageStream, error) {
	if r == nil || r.ptr == nil {
		return nil, errors.New("reader is closed")
	}
	stream := C.mcap_reader_messages(r.ptr)
	if stream == nil {
		return nil, lastErrorOr("failed to open message stream")
	}
	r.ptr = nil
	return &MessageStream{ptr: stream}, nil
}

func (r *Reader) RawMessages() (*RawMessageStream, error) {
	if r == nil || r.ptr == nil {
		return nil, errors.New("reader is closed")
	}
	stream := C.mcap_reader_raw_messages(r.ptr)
	if stream == nil {
		return nil, lastErrorOr("failed to open raw message stream")
	}
	r.ptr = nil
	return &RawMessageStream{ptr: stream}, nil
}

func (r *Reader) Chunks() (*ChunkStream, error) {
	if r == nil || r.ptr == nil {
		return nil, errors.New("reader is closed")
	}
	stream := C.mcap_reader_chunks(r.ptr)
	if stream == nil {
		return nil, lastErrorOr("failed to open chunk stream")
	}
	r.ptr = nil
	return &ChunkStream{ptr: stream}, nil
}

func (r *Reader) Records() (*RecordStream, error) {
	if r == nil || r.ptr == nil {
		return nil, errors.New("reader is closed")
	}
	stream := C.mcap_reader_records(r.ptr)
	if stream == nil {
		return nil, lastErrorOr("failed to open record stream")
	}
	r.ptr = nil
	return &RecordStream{ptr: stream}, nil
}

func (s *MessageStream) Close() {
	if s == nil || s.ptr == nil {
		return
	}
	C.mcap_message_stream_free(s.ptr)
	s.ptr = nil
}

func (s *MessageStream) Next() (*Message, error) {
	if s == nil || s.ptr == nil {
		return nil, errors.New("message stream is closed")
	}
	var msg C.McapMessage
	if ok := C.mcap_message_stream_next(s.ptr, &msg); !ok {
		if err := lastError(); err != nil {
			return nil, err
		}
		return nil, nil
	}
	out := &Message{
		ChannelID:   uint16(msg.channel_id),
		Sequence:    uint32(msg.sequence),
		LogTime:     uint64(msg.log_time),
		PublishTime: uint64(msg.publish_time),
		Data:        byteBufferToBytes(msg.data),
	}
	C.mcap_message_clear(&msg)
	return out, nil
}

func (s *MessageStream) Parsed() (*ParsedStream, error) {
	if s == nil || s.ptr == nil {
		return nil, errors.New("message stream is closed")
	}
	parsed := C.mcap_message_stream_parsed(s.ptr)
	s.ptr = nil
	if parsed == nil {
		return nil, lastErrorOr("failed to create parsed stream")
	}
	return &ParsedStream{ptr: parsed}, nil
}

func (s *MessageStream) ParsedWith(parsers []ParserSpec) (*ParsedStream, error) {
	if s == nil || s.ptr == nil {
		return nil, errors.New("message stream is closed")
	}
	if len(parsers) == 0 {
		return s.Parsed()
	}
	specs := make([]C.McapParserSpec, len(parsers))
	cStrings := make([]*C.char, len(parsers))
	for i, parser := range parsers {
		cstr := C.CString(parser.Encoding)
		cStrings[i] = cstr
		specs[i] = C.McapParserSpec{
			encoding: cstr,
			kind:     C.McapParserKind(parser.Kind),
		}
	}
	defer func() {
		for _, cstr := range cStrings {
			C.free(unsafe.Pointer(cstr))
		}
	}()
	parsed := C.mcap_message_stream_parsed_with(
		s.ptr,
		(*C.McapParserSpec)(unsafe.Pointer(&specs[0])),
		C.size_t(len(specs)),
	)
	s.ptr = nil
	if parsed == nil {
		return nil, lastErrorOr("failed to create parsed stream")
	}
	return &ParsedStream{ptr: parsed}, nil
}

func (s *MessageStream) IntoReader() (*Reader, error) {
	if s == nil || s.ptr == nil {
		return nil, errors.New("message stream is closed")
	}
	reader := C.mcap_message_stream_into_reader(s.ptr)
	s.ptr = nil
	if reader == nil {
		return nil, lastErrorOr("failed to recover reader")
	}
	return &Reader{ptr: reader}, nil
}

func (s *RawMessageStream) Close() {
	if s == nil || s.ptr == nil {
		return
	}
	C.mcap_raw_message_stream_free(s.ptr)
	s.ptr = nil
}

func (s *RawMessageStream) Next() (*RawMessage, error) {
	if s == nil || s.ptr == nil {
		return nil, errors.New("raw message stream is closed")
	}
	var msg C.McapRawMessage
	if ok := C.mcap_raw_message_stream_next(s.ptr, &msg); !ok {
		if err := lastError(); err != nil {
			return nil, err
		}
		return nil, nil
	}
	out := &RawMessage{
		ChannelID:   uint16(msg.channel_id),
		Sequence:    uint32(msg.sequence),
		LogTime:     uint64(msg.log_time),
		PublishTime: uint64(msg.publish_time),
		Data:        byteBufferToBytes(msg.data),
	}
	C.mcap_raw_message_clear(&msg)
	return out, nil
}

func (s *RawMessageStream) IntoReader() (*Reader, error) {
	if s == nil || s.ptr == nil {
		return nil, errors.New("raw message stream is closed")
	}
	reader := C.mcap_raw_message_stream_into_reader(s.ptr)
	s.ptr = nil
	if reader == nil {
		return nil, lastErrorOr("failed to recover reader")
	}
	return &Reader{ptr: reader}, nil
}

func (s *ChunkStream) Close() {
	if s == nil || s.ptr == nil {
		return
	}
	C.mcap_chunk_stream_free(s.ptr)
	s.ptr = nil
}

func (s *ChunkStream) Next() (*Chunk, error) {
	if s == nil || s.ptr == nil {
		return nil, errors.New("chunk stream is closed")
	}
	var chunk C.McapChunk
	if ok := C.mcap_chunk_stream_next(s.ptr, &chunk); !ok {
		if err := lastError(); err != nil {
			return nil, err
		}
		return nil, nil
	}
	out := &Chunk{
		MessageStartTime: uint64(chunk.message_start_time),
		MessageEndTime:   uint64(chunk.message_end_time),
		UncompressedSize: uint64(chunk.uncompressed_size),
		UncompressedCRC:  uint32(chunk.uncompressed_crc),
		Compression:      byteBufferToString(chunk.compression),
		Records:          byteBufferToBytes(chunk.records),
	}
	C.mcap_chunk_clear(&chunk)
	return out, nil
}

func (s *ChunkStream) IntoReader() (*Reader, error) {
	if s == nil || s.ptr == nil {
		return nil, errors.New("chunk stream is closed")
	}
	reader := C.mcap_chunk_stream_into_reader(s.ptr)
	s.ptr = nil
	if reader == nil {
		return nil, lastErrorOr("failed to recover reader")
	}
	return &Reader{ptr: reader}, nil
}

func (s *RecordStream) Close() {
	if s == nil || s.ptr == nil {
		return
	}
	C.mcap_record_stream_free(s.ptr)
	s.ptr = nil
}

func (s *RecordStream) Next() (*Record, error) {
	if s == nil || s.ptr == nil {
		return nil, errors.New("record stream is closed")
	}
	var record C.McapRecord
	if ok := C.mcap_record_stream_next(s.ptr, &record); !ok {
		if err := lastError(); err != nil {
			return nil, err
		}
		return nil, nil
	}
	out := &Record{Kind: byteBufferToString(record.kind)}
	C.mcap_record_clear(&record)
	return out, nil
}

func (s *RecordStream) IntoReader() (*Reader, error) {
	if s == nil || s.ptr == nil {
		return nil, errors.New("record stream is closed")
	}
	reader := C.mcap_record_stream_into_reader(s.ptr)
	s.ptr = nil
	if reader == nil {
		return nil, lastErrorOr("failed to recover reader")
	}
	return &Reader{ptr: reader}, nil
}

func (s *ParsedStream) Close() {
	if s == nil || s.ptr == nil {
		return
	}
	C.mcap_parsed_stream_free(s.ptr)
	s.ptr = nil
}

func (s *ParsedStream) Next() (*ParsedMessage, error) {
	if s == nil || s.ptr == nil {
		return nil, errors.New("parsed stream is closed")
	}
	var value C.McapParsedValue
	if ok := C.mcap_parsed_stream_next(s.ptr, &value); !ok {
		if err := lastError(); err != nil {
			return nil, err
		}
		return nil, nil
	}
	defer C.mcap_parsed_value_clear(&value)
	if value.is_json != C.bool(false) {
		data := byteBufferToBytes(value.json)
		var parsed any
		if err := json.Unmarshal(data, &parsed); err != nil {
			return nil, err
		}
		return &ParsedMessage{IsJSON: true, JSON: parsed}, nil
	}
	return &ParsedMessage{IsJSON: false, Bytes: byteBufferToBytes(value.bytes)}, nil
}

func (s *ParsedStream) IntoReader() (*Reader, error) {
	if s == nil || s.ptr == nil {
		return nil, errors.New("parsed stream is closed")
	}
	reader := C.mcap_parsed_stream_into_reader(s.ptr)
	s.ptr = nil
	if reader == nil {
		return nil, lastErrorOr("failed to recover reader")
	}
	return &Reader{ptr: reader}, nil
}

func lastError() error {
	buf := C.mcap_last_error()
	if buf.ptr == nil || buf.len == 0 {
		return nil
	}
	defer C.mcap_byte_buffer_free(buf)
	return errors.New(C.GoStringN((*C.char)(unsafe.Pointer(buf.ptr)), C.int(buf.len)))
}

func lastErrorOr(message string) error {
	if err := lastError(); err != nil {
		return err
	}
	return errors.New(message)
}

func byteBufferToBytes(buf C.McapByteBuffer) []byte {
	if buf.ptr == nil || buf.len == 0 {
		return nil
	}
	return C.GoBytes(unsafe.Pointer(buf.ptr), C.int(buf.len))
}

func byteBufferToString(buf C.McapByteBuffer) string {
	if buf.ptr == nil || buf.len == 0 {
		return ""
	}
	return C.GoStringN((*C.char)(unsafe.Pointer(buf.ptr)), C.int(buf.len))
}

func keyValuesFromSlice(ptr *C.McapKeyValue, length C.size_t) map[string]string {
	count := int(length)
	if ptr == nil || count == 0 {
		return nil
	}
	items := unsafe.Slice(ptr, count)
	out := make(map[string]string, count)
	for _, item := range items {
		key := byteBufferToString(item.key)
		value := byteBufferToString(item.value)
		out[key] = value
	}
	return out
}
