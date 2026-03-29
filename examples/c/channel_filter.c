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

  McapMessage msg = {0};
  size_t count = 0;
  while (mcap_message_stream_next(stream, &msg)) {
    if (msg.channel_id != 1) {
      mcap_message_clear(&msg);
      continue;
    }
    printf("example msg: channel=%u time=%llu\n",
           msg.channel_id,
           (unsigned long long)msg.log_time);
    mcap_message_clear(&msg);
    if (++count >= 5) {
      break;
    }
  }
  printf("example messages: %zu\n", count);

  reader = mcap_message_stream_into_reader(stream);
  if (!reader) {
    print_last_error();
    return 1;
  }
  mcap_reader_free(reader);
  return 0;
}
