import mcapable

from common import write_sample_file


def main() -> None:
    path = write_sample_file()
    reader = mcapable.Reader.from_path(str(path))

    print("Chunk details:")
    count = 0
    stream = reader.chunks()
    for chunk in stream:
        count += 1
        compression_ratio = (
            len(chunk.records) / chunk.uncompressed_size
            if chunk.uncompressed_size
            else 1.0
        )
        print(f"Chunk {count - 1}:")
        print(f"  Time range: {chunk.message_start_time} - {chunk.message_end_time}")
        print(f"  Compression: {chunk.compression}")
        print(f"  Compressed size: {len(chunk.records)} bytes")
        print(f"  Uncompressed size: {chunk.uncompressed_size} bytes")
        print(f"  Compression ratio: {compression_ratio * 100.0:.2f}%")
        print(f"  CRC32: 0x{chunk.uncompressed_crc:08x}")
        if count >= 5:
            print("... (showing first 5 chunks)")
            break
    if count == 0:
        print("  (no chunks in sample file)")


if __name__ == "__main__":
    main()
