#include "common.h"
#include "cxx.h"
#include "mcapable-cpp/src/lib.rs.h"
#include <iostream>

int main() {
  auto data = sample_bytes();
  auto reader = mcapable::reader_from_bytes(data);
  auto header = mcapable::reader_header(*reader);
  std::cout << "Profile: " << header.profile << "\n";
  std::cout << "Library: " << header.library << "\n";

  auto stream = mcapable::reader_raw_messages(*reader);

  mcapable::RawMessage msg{};
  size_t count = 0;
  while (mcapable::raw_message_stream_next(*stream, msg)) {
    if (++count <= 3) {
      std::cout << "Message: channel=" << msg.channel_id << " time=" << msg.log_time
                << " size=" << msg.data.size() << "\n";
    }
  }
  std::cout << "Total messages: " << count << "\n";
  return 0;
}
