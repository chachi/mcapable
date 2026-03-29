#include "common.h"
#include "cxx.h"
#include "mcapable-cpp/src/lib.rs.h"
#include <iostream>
#include <unordered_map>

int main() {
  auto data = sample_bytes();
  auto reader = mcapable::reader_from_bytes(data);

  std::cout << "Chunk analysis:\n";
  auto chunk_stream = mcapable::reader_chunks(*reader);
  mcapable::Chunk chunk{};
  size_t chunk_count = 0;
  uint64_t total_uncompressed = 0;
  while (mcapable::chunk_stream_next(*chunk_stream, chunk)) {
    chunk_count += 1;
    total_uncompressed += chunk.uncompressed_size;
  }
  std::cout << "  " << chunk_count << " chunks\n";
  std::cout << "  " << total_uncompressed << " bytes uncompressed total\n";
  reader = mcapable::chunk_stream_into_reader(*chunk_stream);

  std::cout << "\nMessage counts per channel:\n";
  auto msg_stream = mcapable::reader_messages(*reader);
  std::unordered_map<uint16_t, size_t> counts;
  mcapable::Message message{};
  while (mcapable::message_stream_next(*msg_stream, message)) {
    counts[message.channel_id] += 1;
  }
  for (const auto& kv : counts) {
    std::cout << "  channel " << kv.first << ": " << kv.second << " messages\n";
  }
  reader = mcapable::message_stream_into_reader(*msg_stream);

  std::cout << "\nTime range analysis:\n";
  auto raw_stream = mcapable::reader_raw_messages(*reader);
  mcapable::RawMessage raw{};
  bool got = false;
  uint64_t min_time = 0;
  uint64_t max_time = 0;
  while (mcapable::raw_message_stream_next(*raw_stream, raw)) {
    if (!got) {
      min_time = raw.log_time;
      max_time = raw.log_time;
      got = true;
    } else {
      if (raw.log_time < min_time) min_time = raw.log_time;
      if (raw.log_time > max_time) max_time = raw.log_time;
    }
  }
  if (got) {
    std::cout << "  start: " << min_time << "\n";
    std::cout << "  end:   " << max_time << "\n";
    std::cout << "  duration: " << (max_time - min_time) << "\n";
  }
  reader = mcapable::raw_message_stream_into_reader(*raw_stream);

  std::cout << "\nStream type comparison:\n";
  auto record_stream = mcapable::reader_records(*reader);
  mcapable::Record record{};
  size_t record_count = 0;
  while (mcapable::record_stream_next(*record_stream, record)) {
    record_count += 1;
  }
  std::cout << "  Records: " << record_count << "\n";
  reader = mcapable::record_stream_into_reader(*record_stream);

  auto chunk_stream2 = mcapable::reader_chunks(*reader);
  chunk_count = 0;
  while (mcapable::chunk_stream_next(*chunk_stream2, chunk)) {
    chunk_count += 1;
  }
  std::cout << "  Chunks: " << chunk_count << "\n";
  reader = mcapable::chunk_stream_into_reader(*chunk_stream2);

  auto raw_stream2 = mcapable::reader_raw_messages(*reader);
  size_t raw_count = 0;
  while (mcapable::raw_message_stream_next(*raw_stream2, raw)) {
    raw_count += 1;
  }
  std::cout << "  Raw messages: " << raw_count << "\n";
  reader = mcapable::raw_message_stream_into_reader(*raw_stream2);

  auto msg_stream2 = mcapable::reader_messages(*reader);
  size_t msg_count = 0;
  while (mcapable::message_stream_next(*msg_stream2, message)) {
    msg_count += 1;
  }
  std::cout << "  Messages: " << msg_count << "\n";

  return 0;
}
