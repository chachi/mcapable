import mcapable

from common import write_sample_file


def main() -> None:
    path = write_sample_file()
    reader = mcapable.Reader.from_path(str(path))

    print("Messages in time range [0, 10]:")
    stream = reader.messages().time_range(0, 10)
    count = 0
    for msg in stream:
        print(f"  time={msg.log_time} channel={msg.channel_id}")
        count += 1
        if count >= 5:
            break

    reader = stream.into_reader()
    print("\nMessages on channels [1]:")
    stream = reader.messages().filter_channel(lambda ch: ch.id == 1)
    count = 0
    for msg in stream:
        print(f"  time={msg.log_time} channel={msg.channel_id}")
        count += 1
        if count >= 5:
            break

    reader = stream.into_reader()
    print("\nMessages on channel 1 in time range [0, 10]:")
    stream = reader.messages().time_range(0, 10).filter_channel(lambda ch: ch.id == 1)
    count = 0
    for msg in stream:
        print(f"  time={msg.log_time} channel={msg.channel_id}")
        count += 1
        if count >= 5:
            break


if __name__ == "__main__":
    main()
