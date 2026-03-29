#include "common.h"
#include "cxx.h"
#include "mcapable-cpp/src/lib.rs.h"
#include <iostream>

int main() {
  auto data = sample_bytes();
  auto reader = mcapable::reader_from_bytes(data);

  auto stream = mcapable::reader_messages(*reader);

  mcapable::Message message{};
  size_t count = 0;
  while (mcapable::message_stream_next(*stream, message)) {
    if (message.channel_id != 1) {
      continue;
    }
    std::cout << "example msg: channel=" << message.channel_id
              << " time=" << message.log_time << "\n";
    if (++count >= 5) {
      break;
    }
  }
  std::cout << "example messages: " << count << "\n";
  return 0;
}
