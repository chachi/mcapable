#!/bin/sh
set -euo pipefail

root_dir="$(cd "$(dirname "$0")/.." && pwd)"

cargo build -p mcapable-swift

swiftc \
  -import-objc-header "$root_dir/tests/SwiftBridge.h" \
  "$root_dir/Generated/SwiftBridgeCore.swift" \
  "$root_dir/Generated/McapableSwift/McapableSwift.swift" \
  "$root_dir/tests/main.swift" \
  -L "$root_dir/../../target/debug" \
  -lmcapable_swift \
  -o "$root_dir/../../target/swift-mcapable-tests"

"$root_dir/../../target/swift-mcapable-tests"
