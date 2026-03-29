import mcapable

from common import write_sample_file


def main() -> None:
    path = write_sample_file()
    reader = mcapable.Reader.from_path(str(path))

    print("Before streaming:")
    print(f"  Schemas: {len(reader.schemas())}")
    print(f"  Channels: {len(reader.channels())}")

    stream = reader.messages()
    next(iter(stream), None)
    reader = stream.into_reader()

    schemas = reader.schemas()
    channels = reader.channels()

    print("\nMetadata available after caching:")
    print(f"  Schemas: {len(schemas)}")
    print(f"  Channels: {len(channels)}")

    print("\nChannels:")
    for channel in channels:
        print(
            f"  [{channel.id}] topic='{channel.topic}' encoding='{channel.message_encoding}' schema_id={channel.schema_id}"
        )

    print("\nSchemas:")
    for schema in schemas:
        print(
            f"  [{schema.id}] name='{schema.name}' encoding='{schema.encoding}' data_len={len(schema.data)}"
        )

    stream = reader.messages()
    msg_count = 0
    for msg in stream:
        msg_count += 1
        channel = next((ch for ch in channels if ch.id == msg.channel_id), None)
        if channel:
            schema = next((sc for sc in schemas if sc.id == channel.schema_id), None)
            if schema:
                print(f"  Message on '{channel.topic}' using schema '{schema.name}'")
        if msg_count >= 3:
            break

    print(f"\nProcessed {msg_count} messages")


if __name__ == "__main__":
    main()
