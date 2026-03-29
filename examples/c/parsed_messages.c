#include "common.h"
#include "mcapable_ffi.h"
#include <stdio.h>

static void print_last_error(void) {
  McapByteBuffer err = mcap_last_error();
  if (err.ptr && err.len) {
    fwrite(err.ptr, 1, err.len, stderr);
    fputc('\n', stderr);
    mcap_byte_buffer_free(err);
  }
}

int main(void) {
  SampleBuffer sample = sample_bytes();
  McapReader* reader = mcap_reader_from_bytes(sample.data, sample.len);
  free_sample_bytes(sample);
  if (!reader) {
    print_last_error();
    return 1;
  }

  McapMessageStream* stream = mcap_reader_messages(reader);
  if (!stream) {
    print_last_error();
    mcap_reader_free(reader);
    return 1;
  }

  McapParsedStream* parsed = mcap_message_stream_parsed(stream);
  if (!parsed) {
    print_last_error();
    return 1;
  }

  McapParsedValue value = {0};
  size_t count = 0;
  while (mcap_parsed_stream_next(parsed, &value)) {
    if (value.is_json) {
      printf("Message %zu: parsed JSON %.*s\n", count, (int)value.json.len, (char*)value.json.ptr);
    } else {
      printf("Message %zu: raw bytes size=%zu\n", count, value.bytes.len);
    }
    mcap_parsed_value_clear(&value);
    if (++count >= 5) {
      break;
    }
  }

  reader = mcap_parsed_stream_into_reader(parsed);
  if (!reader) {
    print_last_error();
    return 1;
  }
  mcap_reader_free(reader);
  return 0;
}
