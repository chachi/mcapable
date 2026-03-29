#include "common.h"
#include "cxx.h"
#include "mcapable-cpp/src/lib.rs.h"
#include <iostream>
#include <unordered_map>

int main() {
  auto data = sample_bytes();
  auto reader = mcapable::reader_from_bytes(data);

  auto stream = mcapable::reader_records(*reader);
  mcapable::Record record{};
  std::unordered_map<std::string, size_t> counts;
  while (mcapable::record_stream_next(*stream, record)) {
    counts[std::string(record.kind.c_str())] += 1;
  }

  std::cout << "Record counts:\n";
  const char* order[] = {"Header", "Footer", "Schema", "Channel", "Message", "Chunk", "Other"};
  for (const char* key : order) {
    std::cout << "  " << key << ": " << counts[key] << "\n";
  }
  return 0;
}
