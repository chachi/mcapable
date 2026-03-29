import { Reader } from "@mcapable/wasm";

import { sampleBytes } from "./common";

function main(): void {
  const reader = Reader.fromBytes(sampleBytes());
  const header = reader.header() as { profile: string; library: string };
  console.log(`Profile: ${header.profile}`);
  console.log(`Library: ${header.library}`);

  const stream = reader.raw_messages();
  let count = 0;
  for (;;) {
    const item = stream.next();
    if (!item) break;
    const msg = item as { channel_id: number; log_time: number; data: Uint8Array };
    count += 1;
    if (count <= 3) {
      console.log(`Message: channel=${msg.channel_id} time=${msg.log_time} size=${msg.data.length}`);
    }
  }
  console.log(`Total messages: ${count}`);
}

main();
