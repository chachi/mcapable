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

  McapChunkStream* stream = mcap_reader_chunks(reader);
  if (!stream) {
    print_last_error();
    mcap_reader_free(reader);
    return 1;
  }

  McapChunk chunk = {0};
  size_t count = 0;
  while (mcap_chunk_stream_next(stream, &chunk)) {
    double ratio = chunk.uncompressed_size
        ? (double)chunk.records.len / (double)chunk.uncompressed_size
        : 1.0;
    printf("Chunk %zu:\n", count);
    printf("  Time range: %llu - %llu\n",
           (unsigned long long)chunk.message_start_time,
           (unsigned long long)chunk.message_end_time);
    printf("  Compression: %.*s\n",
           (int)chunk.compression.len,
           (char*)chunk.compression.ptr);
    printf("  Compressed size: %zu bytes\n", chunk.records.len);
    printf("  Uncompressed size: %llu bytes\n",
           (unsigned long long)chunk.uncompressed_size);
    printf("  Compression ratio: %.2f%%\n", ratio * 100.0);
    printf("  CRC32: 0x%08x\n", chunk.uncompressed_crc);
    mcap_chunk_clear(&chunk);
    if (++count >= 5) {
      printf("... (showing first 5 chunks)\n");
      break;
    }
  }
  if (count == 0) {
    printf("(no chunks in sample file)\n");
  }

  reader = mcap_chunk_stream_into_reader(stream);
  if (!reader) {
    print_last_error();
    return 1;
  }
  mcap_reader_free(reader);
  return 0;
}
