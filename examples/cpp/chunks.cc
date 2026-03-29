#include "common.h"
#include "cxx.h"
#include "mcapable-cpp/src/lib.rs.h"
#include <iostream>

int main() {
  auto data = sample_bytes();
  auto reader = mcapable::reader_from_bytes(data);

  auto stream = mcapable::reader_chunks(*reader);
  mcapable::Chunk chunk{};
  size_t count = 0;
  while (mcapable::chunk_stream_next(*stream, chunk)) {
    double ratio = chunk.uncompressed_size > 0
                       ? static_cast<double>(chunk.records.size()) /
                             static_cast<double>(chunk.uncompressed_size)
                       : 1.0;
    std::cout << "Chunk " << count << ":\n";
    std::cout << "  Time range: " << chunk.message_start_time << " - "
              << chunk.message_end_time << "\n";
    std::cout << "  Compression: " << chunk.compression << "\n";
    std::cout << "  Compressed size: " << chunk.records.size() << " bytes\n";
    std::cout << "  Uncompressed size: " << chunk.uncompressed_size << " bytes\n";
    std::cout << "  Compression ratio: " << ratio * 100.0 << "%\n";
    std::cout << "  CRC32: 0x" << std::hex << chunk.uncompressed_crc << std::dec
              << "\n";
    if (++count >= 5) {
      std::cout << "... (showing first 5 chunks)\n";
      break;
    }
  }
  if (count == 0) {
    std::cout << "(no chunks in sample file)\n";
  }
  return 0;
}
