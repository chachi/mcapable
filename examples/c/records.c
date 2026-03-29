#include "common.h"
#include "mcapable_ffi.h"
#include <stdio.h>
#include <string.h>

static void print_last_error(void) {
  McapByteBuffer err = mcap_last_error();
  if (err.ptr && err.len) {
    fwrite(err.ptr, 1, err.len, stderr);
    fputc('\n', stderr);
    mcap_byte_buffer_free(err);
  }
}

static bool kind_is(const McapRecord* record, const char* name) {
  size_t name_len = strlen(name);
  if (record->kind.len != name_len) {
    return false;
  }
  return memcmp(record->kind.ptr, name, name_len) == 0;
}

int main(void) {
  SampleBuffer sample = sample_bytes();
  McapReader* reader = mcap_reader_from_bytes(sample.data, sample.len);
  free_sample_bytes(sample);
  if (!reader) {
    print_last_error();
    return 1;
  }

  McapRecordStream* stream = mcap_reader_records(reader);
  if (!stream) {
    print_last_error();
    mcap_reader_free(reader);
    return 1;
  }

  size_t header_count = 0;
  size_t footer_count = 0;
  size_t schema_count = 0;
  size_t channel_count = 0;
  size_t message_count = 0;
  size_t chunk_count = 0;
  size_t other_count = 0;

  McapRecord record = {0};
  while (mcap_record_stream_next(stream, &record)) {
    if (kind_is(&record, "Header")) {
      header_count++;
    } else if (kind_is(&record, "Footer")) {
      footer_count++;
    } else if (kind_is(&record, "Schema")) {
      schema_count++;
    } else if (kind_is(&record, "Channel")) {
      channel_count++;
    } else if (kind_is(&record, "Message")) {
      message_count++;
    } else if (kind_is(&record, "Chunk")) {
      chunk_count++;
    } else {
      other_count++;
    }
    mcap_record_clear(&record);
  }

  printf("Record counts:\n");
  printf("  Header:   %zu\n", header_count);
  printf("  Footer:   %zu\n", footer_count);
  printf("  Schema:   %zu\n", schema_count);
  printf("  Channel:  %zu\n", channel_count);
  printf("  Message:  %zu\n", message_count);
  printf("  Chunk:    %zu\n", chunk_count);
  printf("  Other:    %zu\n", other_count);

  reader = mcap_record_stream_into_reader(stream);
  if (!reader) {
    print_last_error();
    return 1;
  }
  mcap_reader_free(reader);
  return 0;
}
