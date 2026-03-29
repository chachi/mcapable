import { Reader } from "@mcapable/wasm";

import { sampleBytes } from "./common";

function main(): void {
  const reader = Reader.fromBytes(sampleBytes());
  const stream = reader.messages();
  let count = 0;
  for (;;) {
    const item = stream.next();
    if (!item) break;
    const msg = item as { channel_id: number; log_time: number };
    if (msg.channel_id !== 1) {
      continue;
    }
    console.log(`example msg: channel=${msg.channel_id} time=${msg.log_time}`);
    if (++count >= 5) {
      break;
    }
  }
  console.log(`example messages: ${count}`);
}

main();
