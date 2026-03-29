import { Reader } from "@mcapable/wasm";

import { sampleBytes } from "./common";

function main(): void {
  const reader = Reader.fromBytes(sampleBytes());
  const stream = reader.records();
  const counts = new Map<string, number>([
    ["Header", 0],
    ["Footer", 0],
    ["Schema", 0],
    ["Channel", 0],
    ["Message", 0],
    ["Chunk", 0],
    ["Other", 0],
  ]);

  for (;;) {
    const item = stream.next();
    if (!item) break;
    const record = item as { kind: string };
    const key = counts.has(record.kind) ? record.kind : "Other";
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }

  console.log("Record counts:");
  for (const key of ["Header", "Footer", "Schema", "Channel", "Message", "Chunk", "Other"]) {
    console.log(`  ${key}: ${counts.get(key) ?? 0}`);
  }
}

main();
