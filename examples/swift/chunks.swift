import Foundation

@main
struct ChunksExample {
    static func main() {
        do {
            try run()
        } catch {
            fatalError("Swift chunks example failed: \(error)")
        }
    }

    static func run() throws {
        let reader = try reader_from_bytes(decodeSample())
        let stream = try reader_chunks(reader)
        var count = 0
        while true {
            let item = try chunk_stream_next(stream)
            if !item.has_value {
                break
            }
            let chunk = item.value
            let ratio: Double
            if chunk.uncompressed_size > 0 {
                ratio = Double(chunk.records.len()) / Double(chunk.uncompressed_size)
            } else {
                ratio = 1.0
            }
            print("Chunk \(count):")
            print("  Time range: \(chunk.message_start_time) - \(chunk.message_end_time)")
            print("  Compression: \(chunk.compression.toString())")
            print("  Compressed size: \(chunk.records.len()) bytes")
            print("  Uncompressed size: \(chunk.uncompressed_size) bytes")
            print(String(format: "  Compression ratio: %.2f%%", ratio * 100.0))
            print(String(format: "  CRC32: 0x%08x", chunk.uncompressed_crc))
            count += 1
            if count >= 5 {
                print("... (showing first 5 chunks)")
                break
            }
        }
        if count == 0 {
            print("(no chunks in sample file)")
        }
    }
}
