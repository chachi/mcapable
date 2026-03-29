#include "common.h"
#include "cxx.h"
#include "mcapable-cpp/src/lib.rs.h"
#include <iostream>

int main() {
  auto data = sample_bytes();
  auto reader = mcapable::reader_from_bytes(data);

  auto schemas = mcapable::reader_schemas(*reader);
  auto channels = mcapable::reader_channels(*reader);

  std::cout << "Schemas: " << schemas.size() << "\n";
  std::cout << "Channels: " << channels.size() << "\n";

  std::cout << "\nChannels:\n";
  for (const auto& channel : channels) {
    std::cout << "  [" << channel.id << "] topic='" << channel.topic
              << "' encoding='" << channel.message_encoding
              << "' schema_id=" << channel.schema_id << "\n";
  }

  std::cout << "\nSchemas:\n";
  for (const auto& schema : schemas) {
    std::cout << "  [" << schema.id << "] name='" << schema.name
              << "' encoding='" << schema.encoding << "' data_len="
              << schema.data.size() << "\n";
  }

  return 0;
}
