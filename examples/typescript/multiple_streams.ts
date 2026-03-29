import { Reader } from "@mcapable/wasm";

import { sampleBytes } from "./common";

function main(): void {
  let reader = Reader.fromBytes(sampleBytes());

  console.log("Chunk analysis:");
  let chunkStream = reader.chunks();
  let chunkCount = 0;
  let totalUncompressed = 0;
  for (;;) {
    const item = chunkStream.next();
    if (!item) break;
    const chunk = item as { uncompressed_size: number };
    chunkCount += 1;
    totalUncompressed += chunk.uncompressed_size;
  }
  console.log(`  ${chunkCount} chunks`);
  console.log(`  ${totalUncompressed} bytes uncompressed total`);
  reader = chunkStream.into_reader();

  console.log("\nMessage counts per channel:");
  let msgStream = reader.messages();
  const counts = new Map<number, number>();
  for (;;) {
    const item = msgStream.next();
    if (!item) break;
    const msg = item as { channel_id: number };
    counts.set(msg.channel_id, (counts.get(msg.channel_id) ?? 0) + 1);
  }
  for (const [channelId, count] of counts.entries()) {
    console.log(`  channel ${channelId}: ${count} messages`);
  }
  reader = msgStream.into_reader();

  console.log("\nTime range analysis:");
  let rawStream = reader.raw_messages();
  let minTime: number | null = null;
  let maxTime: number | null = null;
  for (;;) {
    const item = rawStream.next();
    if (!item) break;
    const msg = item as { log_time: number };
    minTime = minTime === null ? msg.log_time : Math.min(minTime, msg.log_time);
    maxTime = maxTime === null ? msg.log_time : Math.max(maxTime, msg.log_time);
  }
  if (minTime !== null && maxTime !== null) {
    console.log(`  start: ${minTime}`);
    console.log(`  end:   ${maxTime}`);
    console.log(`  duration: ${maxTime - minTime}`);
  }
  reader = rawStream.into_reader();

  console.log("\nStream type comparison:");
  let recordStream = reader.records();
  let recordCount = 0;
  while (recordStream.next()) {
    recordCount += 1;
  }
  console.log(`  Records: ${recordCount}`);
  reader = recordStream.into_reader();

  chunkStream = reader.chunks();
  chunkCount = 0;
  while (chunkStream.next()) {
    chunkCount += 1;
  }
  console.log(`  Chunks: ${chunkCount}`);
  reader = chunkStream.into_reader();

  rawStream = reader.raw_messages();
  let rawCount = 0;
  while (rawStream.next()) {
    rawCount += 1;
  }
  console.log(`  Raw messages: ${rawCount}`);
  reader = rawStream.into_reader();

  msgStream = reader.messages();
  let msgCount = 0;
  while (msgStream.next()) {
    msgCount += 1;
  }
  console.log(`  Messages: ${msgCount}`);
}

main();
