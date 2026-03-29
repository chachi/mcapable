#include "common.h"
#include "cxx.h"
#include "mcapable-cpp/src/lib.rs.h"
#include <iostream>

int main() {
  auto data = sample_bytes();
  auto reader = mcapable::reader_from_bytes(data);
  auto header = mcapable::reader_header(*reader);
  std::cout << "MCAP Profile: " << header.profile << "\n";
  if (!header.metadata.empty()) {
    std::cout << "Header metadata:\n";
    for (const auto& kv : header.metadata) {
      std::cout << "  " << kv.key << ": " << kv.value << "\n";
    }
  }

  auto stream = mcapable::reader_messages(*reader);

  mcapable::Message message{};
  size_t count = 0;
  while (mcapable::message_stream_next(*stream, message)) {
    std::cout << "  [" << message.log_time << "] channel=" << message.channel_id
              << " seq=" << message.sequence << " size=" << message.data.size() << "\n";
    if (++count >= 10) {
      std::cout << "  ... (showing first 10)\n";
      break;
    }
  }
  return 0;
}
