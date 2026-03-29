#include "common.h"
#include "cxx.h"
#include "mcapable-cpp/src/lib.rs.h"
#include <iostream>

int main() {
  auto data = sample_bytes();
  auto reader = mcapable::reader_from_bytes(data);

  auto stream = mcapable::reader_messages(*reader);

  std::cout << "Messages in time range [0, 10]:\n";
  mcapable::Message message{};
  size_t count = 0;
  while (mcapable::message_stream_next(*stream, message)) {
    if (message.log_time < 0 || message.log_time > 10) {
      continue;
    }
    std::cout << "  time=" << message.log_time << " channel=" << message.channel_id
              << "\n";
    if (++count >= 5) {
      break;
    }
  }
  reader = mcapable::message_stream_into_reader(*stream);

  stream = mcapable::reader_messages(*reader);

  std::cout << "\nMessages on channels [1]:\n";
  count = 0;
  while (mcapable::message_stream_next(*stream, message)) {
    if (message.channel_id != 1) {
      continue;
    }
    std::cout << "  time=" << message.log_time << " channel=" << message.channel_id
              << "\n";
    if (++count >= 5) {
      break;
    }
  }
  reader = mcapable::message_stream_into_reader(*stream);

  stream = mcapable::reader_messages(*reader);

  std::cout << "\nMessages on channel 1 in time range [0, 10]:\n";
  count = 0;
  while (mcapable::message_stream_next(*stream, message)) {
    if (message.channel_id != 1 || message.log_time > 10) {
      continue;
    }
    std::cout << "  time=" << message.log_time << " channel=" << message.channel_id
              << "\n";
    if (++count >= 5) {
      break;
    }
  }
  reader = mcapable::message_stream_into_reader(*stream);

  return 0;
}
