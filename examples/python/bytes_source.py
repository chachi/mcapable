import mcapable

from common import sample_bytes


def main() -> None:
    data = sample_bytes()
    reader = mcapable.Reader.from_bytes(data)

    header = reader.header()
    print(f"Profile: {header.profile}")
    print(f"Library: {header.library}")

    count = 0
    stream = reader.raw_messages()
    for msg in stream:
        count += 1
        if count <= 3:
            print(
                f"Message: channel={msg.channel_id} time={msg.log_time} size={len(msg.data)}"
            )
    print(f"Total messages: {count}")


if __name__ == "__main__":
    main()
