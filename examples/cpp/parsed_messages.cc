#include "common.h"
#include "cxx.h"
#include "mcapable-cpp/src/lib.rs.h"
#include <iostream>

int main() {
  auto data = sample_bytes();
  auto reader = mcapable::reader_from_bytes(data);

  auto stream = mcapable::reader_messages(*reader);

  auto parsed = mcapable::message_stream_parsed(*stream);

  mcapable::ParsedMessage value{};
  size_t count = 0;
  while (mcapable::parsed_message_stream_next(*parsed, value)) {
    if (value.is_json) {
      std::cout << "Message " << count << ": parsed JSON " << value.json << "\n";
    } else {
      std::cout << "Message " << count << ": raw bytes size=" << value.bytes.size() << "\n";
    }
    if (++count >= 5) {
      break;
    }
  }

  return 0;
}
