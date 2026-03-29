import mcapable

from common import write_sample_file


def main() -> None:
    path = write_sample_file()
    reader = mcapable.Reader.from_path(str(path))

    count = 0
    stream = reader.messages().filter_channel(lambda ch: ch.topic.startswith("/example"))
    for msg in stream:
        count += 1
        if count <= 5:
            print(f"example msg: channel={msg.channel_id} time={msg.log_time}")
    print(f"example messages: {count}")


if __name__ == "__main__":
    main()
