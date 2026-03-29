import mcapable

from common import write_sample_file


def main() -> None:
    path = write_sample_file()
    reader = mcapable.Reader.from_path(str(path))

    print("Chunk analysis:")
    chunk_count = 0
    total_uncompressed = 0
    stream = reader.chunks()
    for chunk in stream:
        chunk_count += 1
        total_uncompressed += chunk.uncompressed_size
    print(f"  {chunk_count} chunks")
    print(f"  {total_uncompressed} bytes uncompressed total")
    reader = stream.into_reader()

    print("\nMessage counts per channel:")
    counts = {}
    stream = reader.messages()
    for msg in stream:
        counts[msg.channel_id] = counts.get(msg.channel_id, 0) + 1
    for channel_id, count in counts.items():
        print(f"  channel {channel_id}: {count} messages")
    reader = stream.into_reader()

    print("\nTime range analysis:")
    min_time = None
    max_time = None
    stream = reader.raw_messages()
    for msg in stream:
        min_time = msg.log_time if min_time is None else min(min_time, msg.log_time)
        max_time = msg.log_time if max_time is None else max(max_time, msg.log_time)
    if min_time is not None and max_time is not None:
        print(f"  start: {min_time}")
        print(f"  end:   {max_time}")
        print(f"  duration: {max_time - min_time}")
    reader = stream.into_reader()

    print("\nStream type comparison:")
    stream = reader.records()
    record_count = sum(1 for _ in stream)
    print(f"  Records: {record_count}")
    reader = stream.into_reader()

    stream = reader.chunks()
    chunk_count = sum(1 for _ in stream)
    print(f"  Chunks: {chunk_count}")
    reader = stream.into_reader()

    stream = reader.raw_messages()
    raw_count = sum(1 for _ in stream)
    print(f"  Raw messages: {raw_count}")
    reader = stream.into_reader()

    stream = reader.messages()
    msg_count = sum(1 for _ in stream)
    print(f"  Messages: {msg_count}")


if __name__ == "__main__":
    main()
