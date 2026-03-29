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

  printf("Chunk analysis:\n");
  McapChunkStream* chunk_stream = mcap_reader_chunks(reader);
  if (!chunk_stream) {
    print_last_error();
    mcap_reader_free(reader);
    return 1;
  }
  McapChunk chunk = {0};
  size_t chunk_count = 0;
  uint64_t total_uncompressed = 0;
  while (mcap_chunk_stream_next(chunk_stream, &chunk)) {
    chunk_count += 1;
    total_uncompressed += chunk.uncompressed_size;
    mcap_chunk_clear(&chunk);
  }
  printf("  %zu chunks\n", chunk_count);
  printf("  %llu bytes uncompressed total\n", (unsigned long long)total_uncompressed);
  reader = mcap_chunk_stream_into_reader(chunk_stream);
  if (!reader) {
    print_last_error();
    return 1;
  }

  printf("\nMessage counts per channel:\n");
  McapMessageStream* msg_stream = mcap_reader_messages(reader);
  if (!msg_stream) {
    print_last_error();
    mcap_reader_free(reader);
    return 1;
  }
  size_t channel_counts[16] = {0};
  McapMessage msg = {0};
  while (mcap_message_stream_next(msg_stream, &msg)) {
    if (msg.channel_id < 16) {
      channel_counts[msg.channel_id] += 1;
    }
    mcap_message_clear(&msg);
  }
  for (size_t i = 0; i < 16; ++i) {
    if (channel_counts[i] > 0) {
      printf("  channel %zu: %zu messages\n", i, channel_counts[i]);
    }
  }
  reader = mcap_message_stream_into_reader(msg_stream);
  if (!reader) {
    print_last_error();
    return 1;
  }

  printf("\nTime range analysis:\n");
  McapRawMessageStream* raw_stream = mcap_reader_raw_messages(reader);
  if (!raw_stream) {
    print_last_error();
    mcap_reader_free(reader);
    return 1;
  }
  McapRawMessage raw = {0};
  bool got = false;
  uint64_t min_time = 0;
  uint64_t max_time = 0;
  while (mcap_raw_message_stream_next(raw_stream, &raw)) {
    if (!got) {
      min_time = raw.log_time;
      max_time = raw.log_time;
      got = true;
    } else {
      if (raw.log_time < min_time) min_time = raw.log_time;
      if (raw.log_time > max_time) max_time = raw.log_time;
    }
    mcap_raw_message_clear(&raw);
  }
  if (got) {
    printf("  start: %llu\n", (unsigned long long)min_time);
    printf("  end:   %llu\n", (unsigned long long)max_time);
    printf("  duration: %llu\n", (unsigned long long)(max_time - min_time));
  }
  reader = mcap_raw_message_stream_into_reader(raw_stream);
  if (!reader) {
    print_last_error();
    return 1;
  }

  printf("\nStream type comparison:\n");
  McapRecordStream* record_stream = mcap_reader_records(reader);
  if (!record_stream) {
    print_last_error();
    mcap_reader_free(reader);
    return 1;
  }
  McapRecord record = {0};
  size_t record_count = 0;
  while (mcap_record_stream_next(record_stream, &record)) {
    record_count += 1;
    mcap_record_clear(&record);
  }
  printf("  Records: %zu\n", record_count);
  reader = mcap_record_stream_into_reader(record_stream);
  if (!reader) {
    print_last_error();
    return 1;
  }

  chunk_stream = mcap_reader_chunks(reader);
  if (!chunk_stream) {
    print_last_error();
    mcap_reader_free(reader);
    return 1;
  }
  chunk_count = 0;
  while (mcap_chunk_stream_next(chunk_stream, &chunk)) {
    chunk_count += 1;
    mcap_chunk_clear(&chunk);
  }
  printf("  Chunks: %zu\n", chunk_count);
  reader = mcap_chunk_stream_into_reader(chunk_stream);
  if (!reader) {
    print_last_error();
    return 1;
  }

  raw_stream = mcap_reader_raw_messages(reader);
  if (!raw_stream) {
    print_last_error();
    mcap_reader_free(reader);
    return 1;
  }
  size_t raw_count = 0;
  while (mcap_raw_message_stream_next(raw_stream, &raw)) {
    raw_count += 1;
    mcap_raw_message_clear(&raw);
  }
  printf("  Raw messages: %zu\n", raw_count);
  reader = mcap_raw_message_stream_into_reader(raw_stream);
  if (!reader) {
    print_last_error();
    return 1;
  }

  msg_stream = mcap_reader_messages(reader);
  if (!msg_stream) {
    print_last_error();
    mcap_reader_free(reader);
    return 1;
  }
  size_t msg_count = 0;
  while (mcap_message_stream_next(msg_stream, &msg)) {
    msg_count += 1;
    mcap_message_clear(&msg);
  }
  printf("  Messages: %zu\n", msg_count);
  reader = mcap_message_stream_into_reader(msg_stream);
  if (!reader) {
    print_last_error();
    return 1;
  }
  mcap_reader_free(reader);
  return 0;
}
