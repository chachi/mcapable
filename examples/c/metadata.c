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

  McapSchema* schemas = NULL;
  size_t schema_len = 0;
  if (!mcap_reader_schemas(reader, &schemas, &schema_len)) {
    print_last_error();
    mcap_reader_free(reader);
    return 1;
  }

  McapChannel* channels = NULL;
  size_t channel_len = 0;
  if (!mcap_reader_channels(reader, &channels, &channel_len)) {
    print_last_error();
    mcap_schema_array_free(schemas, schema_len);
    mcap_reader_free(reader);
    return 1;
  }

  printf("Schemas: %zu\n", schema_len);
  printf("Channels: %zu\n", channel_len);

  printf("\nChannels:\n");
  for (size_t i = 0; i < channel_len; ++i) {
    McapChannel* channel = &channels[i];
    printf("  [%u] topic='%.*s' encoding='%.*s' schema_id=%u\n",
           channel->id,
           (int)channel->topic.len,
           (char*)channel->topic.ptr,
           (int)channel->message_encoding.len,
           (char*)channel->message_encoding.ptr,
           channel->schema_id);
  }

  printf("\nSchemas:\n");
  for (size_t i = 0; i < schema_len; ++i) {
    McapSchema* schema = &schemas[i];
    printf("  [%u] name='%.*s' encoding='%.*s' data_len=%zu\n",
           schema->id,
           (int)schema->name.len,
           (char*)schema->name.ptr,
           (int)schema->encoding.len,
           (char*)schema->encoding.ptr,
           schema->data.len);
  }

  mcap_channel_array_free(channels, channel_len);
  mcap_schema_array_free(schemas, schema_len);
  mcap_reader_free(reader);
  return 0;
}
