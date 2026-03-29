import Foundation

private let sampleBase64 = "iU1DQVAwDQoBGwAAAAAAAAAHAAAAZXhhbXBsZQgAAABtY2FwYWJsZQAAAAADNwAAAAAAAAABAA4AAABleGFtcGxlX3NjaGVtYQoAAABqc29uc2NoZW1hEQAAAHsidHlwZSI6Im9iamVjdCJ9BBwAAAAAAAAAAQABAAgAAAAvZXhhbXBsZQQAAABqc29uAAAAAAUnAAAAAAAAAAEAAQAAAAEAAAAAAAAAAQAAAAAAAAB7ImhlbGxvIjoid29ybGQifQ8EAAAAAAAAAAAAAAADNwAAAAAAAAABAA4AAABleGFtcGxlX3NjaGVtYQoAAABqc29uc2NoZW1hEQAAAHsidHlwZSI6Im9iamVjdCJ9BBwAAAAAAAAAAQABAAgAAAAvZXhhbXBsZQQAAABqc29uAAAAAAs4AAAAAAAAAAEAAAAAAAAAAQABAAAAAAAAAAAAAAAAAAAAAQAAAAAAAAABAAAAAAAAAAoAAAABAAEAAAAAAAAADhEAAAAAAAAAA84AAAAAAAAAQAAAAAAAAAAOEQAAAAAAAAAEDgEAAAAAAAAlAAAAAAAAAA4RAAAAAAAAAAszAQAAAAAAAEEAAAAAAAAAAhQAAAAAAAAAzgAAAAAAAAB0AQAAAAAAAHCmwIyJTUNBUDANCg=="

func decodeSample() -> RustVec<UInt8> {
    let data = Data(base64Encoded: sampleBase64) ?? Data()
    let vec = RustVec<UInt8>()
    for byte in data {
        vec.push(value: byte)
    }
    return vec
}

func assertTrue(_ condition: Bool, _ message: String) {
    if !condition {
        fatalError(message)
    }
}

func testReaderAndStreams() throws {
    let bytes = decodeSample()
    let reader = try reader_from_bytes(bytes)

    let header = try reader_header(reader)
    assertTrue(header.profile.toString() == "example", "unexpected profile")
    assertTrue(header.library.toString() == "mcapable", "unexpected library")

    let messageStream = try reader_messages(reader)
    let first = try message_stream_next(messageStream)
    assertTrue(first.has_value, "expected message")
    let msg = first.value
    assertTrue(msg.data.len() == 17, "expected 17-byte payload")
    let second = try message_stream_next(messageStream)
    assertTrue(!second.has_value, "expected end of stream")

    let reader2 = try message_stream_into_reader(messageStream)
    let parsedSource = try reader_messages(reader2)
    let parsedStream = try message_stream_parsed(parsedSource)
    let parsed = try parsed_message_stream_next(parsedStream)
    assertTrue(parsed.has_value, "expected parsed message")
    if parsed.value.is_json {
        assertTrue(parsed.value.json.toString().contains("hello"), "expected json value")
    } else {
        assertTrue(parsed.value.bytes.len() > 0, "expected parsed bytes")
    }
}

do {
    try testReaderAndStreams()
    print("Swift bindings tests passed")
} catch {
    fatalError("Swift bindings test failed: \(error)")
}
