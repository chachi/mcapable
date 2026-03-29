import { Reader } from "@mcapable/wasm";

import { sampleBytes } from "./common";

function main(): void {
  const reader = Reader.fromBytes(sampleBytes());
  const header = reader.header() as { profile: string; library: string; metadata: [string, string][] };
  console.log(`MCAP Profile: ${header.profile}`);
  if (header.metadata.length > 0) {
    console.log("Header metadata:");
    for (const [key, value] of header.metadata) {
      console.log(`  ${key}: ${value}`);
    }
  }

  console.log("\nMessages:");
  const stream = reader.messages();
  let count = 0;
  for (;;) {
    const item = stream.next();
    if (!item) break;
    const message = item as {
      channel_id: number;
      sequence: number;
      log_time: number;
      data: Uint8Array;
    };
    console.log(
      `  [${message.log_time}] channel=${message.channel_id} seq=${message.sequence} size=${message.data.length}`
    );
    if (++count >= 10) {
      console.log("  ... (showing first 10)");
      break;
    }
  }
}

main();
