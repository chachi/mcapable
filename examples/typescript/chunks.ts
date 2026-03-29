import { Reader } from "@mcapable/wasm";

import { sampleBytes } from "./common";

function main(): void {
  const reader = Reader.fromBytes(sampleBytes());
  const stream = reader.chunks();
  let count = 0;
  for (;;) {
    const item = stream.next();
    if (!item) break;
    const chunk = item as {
      message_start_time: number;
      message_end_time: number;
      uncompressed_size: number;
      uncompressed_crc: number;
      compression: string;
      records: Uint8Array;
    };
    const ratio = chunk.uncompressed_size
      ? chunk.records.length / chunk.uncompressed_size
      : 1.0;
    console.log(`Chunk ${count}:`);
    console.log(`  Time range: ${chunk.message_start_time} - ${chunk.message_end_time}`);
    console.log(`  Compression: ${chunk.compression}`);
    console.log(`  Compressed size: ${chunk.records.length} bytes`);
    console.log(`  Uncompressed size: ${chunk.uncompressed_size} bytes`);
    console.log(`  Compression ratio: ${(ratio * 100).toFixed(2)}%`);
    console.log(`  CRC32: 0x${chunk.uncompressed_crc.toString(16).padStart(8, "0")}`);
    if (++count >= 5) {
      console.log("... (showing first 5 chunks)");
      break;
    }
  }
  if (count === 0) {
    console.log("(no chunks in sample file)");
  }
}

main();
