#ifndef MCAPABLE_FFI_H
#define MCAPABLE_FFI_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

typedef struct McapReader McapReader;
typedef struct McapMessageStream McapMessageStream;
typedef struct McapRawMessageStream McapRawMessageStream;
typedef struct McapChunkStream McapChunkStream;
typedef struct McapRecordStream McapRecordStream;
typedef struct McapParsedStream McapParsedStream;

typedef struct {
  uint8_t* ptr;
  size_t len;
  size_t cap;
} McapByteBuffer;

typedef struct {
  McapByteBuffer key;
  McapByteBuffer value;
} McapKeyValue;

typedef struct {
  McapByteBuffer profile;
  McapByteBuffer library;
  McapKeyValue* metadata;
  size_t metadata_len;
} McapHeader;

typedef struct {
  uint16_t id;
  McapByteBuffer name;
  McapByteBuffer encoding;
  McapByteBuffer data;
} McapSchema;

typedef struct {
  uint16_t id;
  McapByteBuffer topic;
  McapByteBuffer message_encoding;
  uint16_t schema_id;
  McapKeyValue* metadata;
  size_t metadata_len;
} McapChannel;

typedef struct {
  uint16_t channel_id;
  uint32_t sequence;
  uint64_t log_time;
  uint64_t publish_time;
  McapByteBuffer data;
} McapMessage;

typedef struct {
  uint16_t channel_id;
  uint32_t sequence;
  uint64_t log_time;
  uint64_t publish_time;
  McapByteBuffer data;
} McapRawMessage;

typedef struct {
  uint64_t message_start_time;
  uint64_t message_end_time;
  uint64_t uncompressed_size;
  uint32_t uncompressed_crc;
  McapByteBuffer compression;
  McapByteBuffer records;
} McapChunk;

typedef struct {
  McapByteBuffer kind;
} McapRecord;

typedef struct {
  bool is_json;
  McapByteBuffer json;
  McapByteBuffer bytes;
} McapParsedValue;

typedef enum {
  MCAP_PARSER_JSON = 0,
  MCAP_PARSER_BYTES = 1
} McapParserKind;

typedef struct {
  const char* encoding;
  McapParserKind kind;
} McapParserSpec;

McapByteBuffer mcap_last_error(void);
void mcap_byte_buffer_free(McapByteBuffer buf);
void mcap_key_value_clear(McapKeyValue* kv);
void mcap_key_value_array_free(McapKeyValue* items, size_t len);

McapReader* mcap_reader_from_bytes(const uint8_t* data, size_t len);
void mcap_reader_free(McapReader* reader);
bool mcap_reader_header(McapReader* reader, McapHeader* out_header);
bool mcap_reader_schemas(McapReader* reader, McapSchema** out_items, size_t* out_len);
bool mcap_reader_channels(McapReader* reader, McapChannel** out_items, size_t* out_len);

McapMessageStream* mcap_reader_messages(McapReader* reader);
McapRawMessageStream* mcap_reader_raw_messages(McapReader* reader);
McapChunkStream* mcap_reader_chunks(McapReader* reader);
McapRecordStream* mcap_reader_records(McapReader* reader);
void mcap_message_stream_free(McapMessageStream* stream);
void mcap_raw_message_stream_free(McapRawMessageStream* stream);
void mcap_chunk_stream_free(McapChunkStream* stream);
void mcap_record_stream_free(McapRecordStream* stream);
bool mcap_message_stream_next(McapMessageStream* stream, McapMessage* out_message);
bool mcap_raw_message_stream_next(McapRawMessageStream* stream, McapRawMessage* out_message);
bool mcap_chunk_stream_next(McapChunkStream* stream, McapChunk* out_chunk);
bool mcap_record_stream_next(McapRecordStream* stream, McapRecord* out_record);

McapReader* mcap_message_stream_into_reader(McapMessageStream* stream);
McapReader* mcap_raw_message_stream_into_reader(McapRawMessageStream* stream);
McapReader* mcap_chunk_stream_into_reader(McapChunkStream* stream);
McapReader* mcap_record_stream_into_reader(McapRecordStream* stream);
McapReader* mcap_parsed_stream_into_reader(McapParsedStream* stream);

void mcap_message_clear(McapMessage* message);
void mcap_raw_message_clear(McapRawMessage* message);
void mcap_chunk_clear(McapChunk* chunk);
void mcap_record_clear(McapRecord* record);
void mcap_header_clear(McapHeader* header);
void mcap_schema_array_free(McapSchema* items, size_t len);
void mcap_channel_array_free(McapChannel* items, size_t len);

McapParsedStream* mcap_message_stream_parsed(McapMessageStream* stream);
McapParsedStream* mcap_message_stream_parsed_with(McapMessageStream* stream,
                                                  const McapParserSpec* specs,
                                                  size_t specs_len);
void mcap_parsed_stream_free(McapParsedStream* stream);
bool mcap_parsed_stream_next(McapParsedStream* stream, McapParsedValue* out_value);
void mcap_parsed_value_clear(McapParsedValue* value);

#endif
