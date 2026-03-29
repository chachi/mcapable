const assert = require("node:assert/strict");
const path = require("node:path");
const test = require("node:test");

const wasmPath = path.join(__dirname, "..", "..", "pkg", "mcapable_wasm.js");
// eslint-disable-next-line import/no-dynamic-require
const wasm = require(wasmPath);

const SAMPLE_BASE64 =
  "iU1DQVAwDQoBGwAAAAAAAAAHAAAAZXhhbXBsZQgAAABtY2FwYWJsZQAAAAADNwAAAAAAAAABAA4AAABleGFtcGxlX3NjaGVtYQoAAABqc29uc2NoZW1hEQAAAHsidHlwZSI6Im9iamVjdCJ9BBwAAAAAAAAAAQABAAgAAAAvZXhhbXBsZQQAAABqc29uAAAAAAUnAAAAAAAAAAEAAQAAAAEAAAAAAAAAAQAAAAAAAAB7ImhlbGxvIjoid29ybGQifQ8EAAAAAAAAAAAAAAADNwAAAAAAAAABAA4AAABleGFtcGxlX3NjaGVtYQoAAABqc29uc2NoZW1hEQAAAHsidHlwZSI6Im9iamVjdCJ9BBwAAAAAAAAAAQABAAgAAAAvZXhhbXBsZQQAAABqc29uAAAAAAs4AAAAAAAAAAEAAAAAAAAAAQABAAAAAAAAAAAAAAAAAAAAAQAAAAAAAAABAAAAAAAAAAoAAAABAAEAAAAAAAAADhEAAAAAAAAAA84AAAAAAAAAQAAAAAAAAAAOEQAAAAAAAAAEDgEAAAAAAAAlAAAAAAAAAA4RAAAAAAAAAAszAQAAAAAAAEEAAAAAAAAAAhQAAAAAAAAAzgAAAAAAAAB0AQAAAAAAAHCmwIyJTUNBUDANCg==";

function sampleBytes() {
  return Uint8Array.from(Buffer.from(SAMPLE_BASE64, "base64"));
}

test("Reader header and streams", () => {
  const reader = wasm.Reader.fromBytes(sampleBytes());
  const header = reader.header();
  assert.equal(header.profile, "example");

  const schemas = reader.schemas();
  const channels = reader.channels();
  assert.equal(schemas.length, 1);
  assert.equal(channels.length, 1);

  const stream = reader.messages();
  const message = stream.next();
  assert.ok(message);
  assert.equal(message.channel_id, 1);
  assert.ok(message.data.length > 0);
});
