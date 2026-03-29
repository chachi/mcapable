# mcapable-cpp

C++ bindings for the mcapable MCAP reader, built with `cxx`.

## Build

```bash
cargo build -p mcapable-cpp
```

The `cxx` bridge generates headers under:

```
target/cxxbridge/mcapable-cpp/src/lib.rs.h
target/cxxbridge/rust/cxx.h
```

## Usage (C++)

```cpp
#include "cxx.h"
#include "mcapable-cpp/src/lib.rs.h"

void read_mcap() {
  auto reader = mcapable::reader_from_path("data.mcap");
  if (!reader) {
    throw std::runtime_error(reader.error().what());
  }
  auto stream = mcapable::reader_messages(*reader);
  if (!stream) {
    throw std::runtime_error(stream.error().what());
  }

  mcapable::Message message{};
  while (mcapable::message_stream_next(*stream, message)) {
    // use message fields
  }
}
```
