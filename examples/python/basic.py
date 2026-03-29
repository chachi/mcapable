import mcapable

from common import write_sample_file


def main() -> None:
    path = write_sample_file()
    reader = mcapable.Reader.from_path(str(path))

    header = reader.header()
    print(f"MCAP Profile: {header.profile}")
    if header.metadata:
        print("Header metadata:")
        for key, value in header.metadata:
            print(f"  {key}: {value}")

    print("\nMessages:")
    count = 0
    stream = reader.messages()
    for message in stream:
        print(
            f"  [{message.log_time}] channel={message.channel_id} seq={message.sequence} size={len(message.data)}"
        )
        count += 1
        if count >= 10:
            print("  ... (showing first 10)")
            break

    reader = stream.into_reader()
    print(f"\nLoaded {len(reader.schemas())} schemas")
    print(f"Loaded {len(reader.channels())} channels")


if __name__ == "__main__":
    main()
