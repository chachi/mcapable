# Default recipe - show available commands
default:
    @just --list

# Build in debug mode
build:
    cargo build

# Build in release mode
release:
    cargo build --release

# Run cargo check
check:
    cargo check --workspace --all-targets

# Check no_std core build for WASM
check-wasm:
    cargo check -p mcapable-core --no-default-features --features alloc --target wasm32-unknown-unknown

# Check no_std core build for embedded targets
check-nostd:
    cargo check -p mcapable-core --no-default-features --features alloc --target thumbv7em-none-eabihf

# Run all no_std build checks
check-nostd-all: check-wasm check-nostd

# Run clippy lints
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Format code (always fixes)
fmt:
    cargo fmt

# Check formatting without modifying (for CI)
fmt-check:
    cargo fmt -- --check

# Run all tests
test:
    cargo nextest run --workspace

# Run all examples
examples: examples-rust examples-python examples-cpp examples-c examples-typescript examples-go examples-swift

examples-rust:
    @for example in $(find examples/rust -maxdepth 1 -type f -name "*.rs" ! -name "common.rs" ! -name "common_*.rs" -exec basename {} .rs \;); do \
        cargo run -p mcapable --example "$example"; \
    done

examples-python:
    @py=python3.13; \
    venv="$(pwd)/target/venv-examples"; \
    if [ ! -x "$venv/bin/python" ] || ! "$venv/bin/python" -V 2>&1 | grep -q "Python 3.13"; then \
        rm -rf "$venv"; \
        "$py" -m venv "$venv"; \
    fi; \
    "$venv/bin/python" -m pip install -U pip >/dev/null; \
    "$venv/bin/python" -m pip install maturin >/dev/null; \
    cd crates/mcapable-py && VIRTUAL_ENV="$venv" PATH="$venv/bin:$PATH" "$venv/bin/python" -m maturin develop; \
    cd ../..; \
    for example in $(find examples/python -maxdepth 1 -type f -name "*.py" ! -name "common.py" -exec basename {} \;); do \
        "$venv/bin/python" "examples/python/$example"; \
    done

examples-cpp:
    @if command -v xcrun >/dev/null 2>&1; then \
        export MACOSX_DEPLOYMENT_TARGET="`xcrun --show-sdk-version`"; \
    fi; \
    cargo build -p mcapable-cpp; \
    for example in $(find examples/cpp -maxdepth 1 -type f -name "*.cc" ! -name "common.h" -exec basename {} .cc \;); do \
        c++ -std=c++17 -Iexamples/cpp -Itarget/cxxbridge -Itarget/cxxbridge/rust -Ltarget/debug -lmcapable_cpp -Wl,-rpath,$(pwd)/target/debug "examples/cpp/$example.cc" -o "target/examples-cpp-$example"; \
        "target/examples-cpp-$example"; \
    done

examples-c:
    @cargo build -p mcapable-ffi
    @for example in $(find examples/c -maxdepth 1 -type f -name "*.c" ! -name "common.h" ! -name "mcapable_ffi.h" -exec basename {} .c \;); do \
        cc -std=c11 -Iexamples/c -Ltarget/debug -lmcapable_ffi -Wl,-rpath,$(pwd)/target/debug "examples/c/$example.c" -o "target/examples-c-$example"; \
        "target/examples-c-$example"; \
    done

examples-typescript:
    @if ! command -v wasm-pack >/dev/null 2>&1; then \
        cargo install wasm-pack --locked; \
    fi
    @cd crates/mcapable-wasm && npm install && npm run build
    @mkdir -p examples/typescript/node_modules/@mcapable
    @ln -sf "$(pwd)/crates/mcapable-wasm/pkg" "examples/typescript/node_modules/@mcapable/wasm"
    @for example in $(find examples/typescript -maxdepth 1 -type f -name "*.ts" ! -name "common.ts" -exec basename {} .ts \;); do \
        NODE_PATH=crates/mcapable-wasm/pkg npx --yes tsx "examples/typescript/$example.ts"; \
    done

examples-go:
    @cargo build -p mcapable-ffi
    @for example in $(find examples/go -maxdepth 1 -type d ! -name "common" ! -name "go" -exec basename {} \;); do \
        (cd examples/go && go run "./$example"); \
    done

examples-swift:
    @swift_target="`swiftc -print-target-info | python3 -c 'import json,sys; print(json.load(sys.stdin)[\"target\"][\"triple\"])'`"; \
    swift_deploy="`echo $swift_target | python3 -c 'import sys; target=sys.stdin.read().strip(); print(target.split(\"macosx\",1)[-1] or \"15.0\")'`"; \
    export MACOSX_DEPLOYMENT_TARGET="$swift_deploy"; \
    cargo build -p mcapable-swift; \
    for example in $(find examples/swift -maxdepth 1 -type f -name "*.swift" ! -name "common.swift" -exec basename {} .swift \;); do \
        swiftc -target "$swift_target" \
            -import-objc-header "examples/swift/SwiftBridge.h" \
            "crates/mcapable-swift/Generated/SwiftBridgeCore.swift" \
            "crates/mcapable-swift/Generated/McapableSwift/McapableSwift.swift" \
            "examples/swift/common.swift" \
            "examples/swift/$example.swift" \
            -L "target/debug" \
            -lmcapable_swift \
            -o "target/examples-swift-$example"; \
        "target/examples-swift-$example"; \
    done

# Run tests with output
test-verbose:
    cargo nextest run --no-capture

# Generate documentation
doc:
    cargo doc --no-deps

# Open documentation in browser
doc-open:
    cargo doc --no-deps --open

# Clean build artifacts
clean:
    cargo clean

# Run tests with coverage check (must be >80%)
test-coverage:
    cargo llvm-cov --lcov --output-path lcov.info
    @echo "Checking coverage threshold..."
    @coverage=$$(cargo llvm-cov --summary-only 2>/dev/null | grep 'TOTAL' | awk '{print $$10}' | sed 's/%//' || echo "0"); \
    echo "Coverage: $${coverage}%"; \
    if [ "$${coverage%.*}" -lt "80" ]; then \
        echo "❌ Coverage $${coverage}% is below 80% threshold"; \
        exit 1; \
    else \
        echo "✅ Coverage $${coverage}% meets 80% threshold"; \
    fi

# Run all checks including coverage
all-with-coverage: fmt clippy test test-coverage

# Run all checks (format, lint, test)
all: fmt clippy test

# Pre-commit checks (format, lint, test)
pre-commit: fmt clippy test
    @echo "All pre-commit checks passed!"

# Fix formatting and clippy issues
fix:
    cargo fmt
    cargo clippy --fix --allow-dirty

# Watch for changes and run check
watch:
    cargo watch -x check

# Watch for changes and run tests
watch-test:
    cargo watch -x test

# Generate code coverage report (HTML)
coverage:
    cargo llvm-cov --html --open

# Generate code coverage report (terminal output)
coverage-text:
    cargo llvm-cov

# Generate code coverage for CI (lcov format)
coverage-lcov:
    cargo llvm-cov --lcov --output-path lcov.info

# Generate code coverage for CI (cobertura XML)
coverage-cobertura:
    cargo llvm-cov --cobertura --output-path cobertura.xml

# Clean coverage artifacts
coverage-clean:
    cargo llvm-cov clean

# Code quality metrics
metrics:
    @echo "=== Code Metrics ==="
    @echo ""
    @echo "Lines of code:"
    @find src -name "*.rs" -exec wc -l {} + | tail -1
    @echo ""
    @echo "Number of files:"
    @find src -name "*.rs" | wc -l
    @echo ""
    @echo "TODO markers:"
    @grep -r "TODO\|FIXME\|XXX\|HACK" src --include="*.rs" || echo "None found"
    @echo ""
    @echo "Public API items:"
    @grep -r "^pub " src --include="*.rs" | wc -l

# Run all quality checks (for CI)
ci: fmt-check clippy test coverage-lcov metrics
