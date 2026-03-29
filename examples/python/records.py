import mcapable

from common import write_sample_file


def main() -> None:
    path = write_sample_file()
    reader = mcapable.Reader.from_path(str(path))

    counts = {
        "Header": 0,
        "Footer": 0,
        "Schema": 0,
        "Channel": 0,
        "Message": 0,
        "Chunk": 0,
        "Other": 0,
    }

    stream = reader.records()
    for record in stream:
        kind = record.kind
        if kind in counts:
            counts[kind] += 1
        else:
            counts["Other"] += 1

    print("Record counts:")
    for key in ["Header", "Footer", "Schema", "Channel", "Message", "Chunk", "Other"]:
        print(f"  {key}:   {counts[key]}")


if __name__ == "__main__":
    main()
