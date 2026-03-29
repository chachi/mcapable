# C ABI Wrapper Plan

Goals
- Provide a stable, minimal C ABI for reading MCAP files from dynamic languages.
- Prefer simple C data shapes (plain structs, buffers) with explicit allocation/free.
- Keep ownership explicit: caller owns buffers returned by the ABI.

Scope (initial)
- Reader from in-memory bytes.
- Message stream and parsed message stream.
- Error retrieval via `mcap_last_error`.

Type mapping
- `ByteStr` -> `char*` UTF-8 buffer (length + pointer).
- `Bytes` payload -> `McapByteBuffer` (pointer + length).
- `bool` return values for iteration; errors returned via `mcap_last_error`.

API outline
- Reader lifecycle:
  - `mcap_reader_from_bytes(data, len) -> McapReader*`
  - `mcap_reader_free(reader)`
- Message stream:
  - `mcap_reader_messages(reader) -> McapMessageStream*`
  - `mcap_message_stream_next(stream, out_message) -> bool`
  - `mcap_message_stream_free(stream)`
- Parsed message stream:
  - `mcap_message_stream_parsed(stream) -> McapParsedStream*`
  - `mcap_parsed_stream_next(parsed, out_value) -> bool`
  - `mcap_parsed_stream_free(parsed)`
- Buffer cleanup:
  - `mcap_byte_buffer_free(buf)`
  - `mcap_message_clear(message)`
  - `mcap_parsed_value_clear(value)`
- Error reporting:
  - `mcap_last_error() -> McapByteBuffer` (UTF-8 error string)

Parsed output
- JSON-encoded messages return UTF-8 JSON bytes via `McapParsedValue.json` with `is_json = true`.
- Non-JSON messages return raw bytes via `McapParsedValue.bytes` with `is_json = false`.

Future expansion
- Reader from file path / file descriptor.
- Streams: records, raw messages, chunks.
- Metadata access: header, schemas, channels, attachments.
- Filtering: time range, channel ID list.
