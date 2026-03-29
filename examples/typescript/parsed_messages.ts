import { Reader } from "@mcapable/wasm";

import { sampleBytes } from "./common";

function main(): void {
  const reader = Reader.fromBytes(sampleBytes());
  const stream = reader.messages();
  const parsed = stream.parsed();
  let count = 0;
  for (;;) {
    const item = parsed.next();
    if (!item) break;
    if (item instanceof Uint8Array) {
      console.log(`Message ${count}: raw bytes size=${item.length}`);
    } else {
      console.log(`Message ${count}: parsed JSON`, item);
    }
    if (++count >= 5) {
      break;
    }
  }
}

main();
