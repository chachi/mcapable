import mcapable

from common import write_sample_file


def main() -> None:
    path = write_sample_file()
    reader = mcapable.Reader.from_path(str(path))

    print("Parsed Message Stream Example")
    stream = reader.messages()
    parsed = stream.parsed()
    for i, item in enumerate(parsed):
        if isinstance(item, (dict, list)):
            print(f"Message {i}: parsed JSON {item}")
        else:
            print(f"Message {i}: raw bytes size={len(item)}")
        if i >= 4:
            break

    reader = parsed.into_reader()
    print(f"Total messages: {sum(1 for _ in reader.messages())}")


if __name__ == "__main__":
    main()
