import { Reader } from "@mcapable/wasm";

import { sampleBytes } from "./common";

function main(): void {
  let reader = Reader.fromBytes(sampleBytes());

  console.log("Messages in time range [0, 10]:");
  let stream = reader.messages();
  let count = 0;
  for (;;) {
    const item = stream.next();
    if (!item) break;
    const msg = item as { log_time: number; channel_id: number };
    if (msg.log_time > 10) {
      continue;
    }
    console.log(`  time=${msg.log_time} channel=${msg.channel_id}`);
    if (++count >= 5) {
      break;
    }
  }
  reader = stream.into_reader();

  console.log("\nMessages on channels [1]:");
  stream = reader.messages();
  count = 0;
  for (;;) {
    const item = stream.next();
    if (!item) break;
    const msg = item as { log_time: number; channel_id: number };
    if (msg.channel_id !== 1) {
      continue;
    }
    console.log(`  time=${msg.log_time} channel=${msg.channel_id}`);
    if (++count >= 5) {
      break;
    }
  }
  reader = stream.into_reader();

  console.log("\nMessages on channel 1 in time range [0, 10]:");
  stream = reader.messages();
  count = 0;
  for (;;) {
    const item = stream.next();
    if (!item) break;
    const msg = item as { log_time: number; channel_id: number };
    if (msg.channel_id !== 1 || msg.log_time > 10) {
      continue;
    }
    console.log(`  time=${msg.log_time} channel=${msg.channel_id}`);
    if (++count >= 5) {
      break;
    }
  }
  reader = stream.into_reader();
}

main();
