import { Reader } from "@mcapable/wasm";

import { sampleBytes } from "./common";

function main(): void {
  const reader = Reader.fromBytes(sampleBytes());
  const schemas = reader.schemas() as Array<{ id: number; name: string; encoding: string; data: Uint8Array }>;
  const channels = reader.channels() as Array<{ id: number; topic: string; message_encoding: string; schema_id: number }>;

  console.log(`Schemas: ${schemas.length}`);
  console.log(`Channels: ${channels.length}`);

  console.log("\nChannels:");
  for (const channel of channels) {
    console.log(
      `  [${channel.id}] topic='${channel.topic}' encoding='${channel.message_encoding}' schema_id=${channel.schema_id}`
    );
  }

  console.log("\nSchemas:");
  for (const schema of schemas) {
    console.log(
      `  [${schema.id}] name='${schema.name}' encoding='${schema.encoding}' data_len=${schema.data.length}`
    );
  }
}

main();
