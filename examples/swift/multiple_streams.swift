import Foundation

@main
struct MultipleStreamsExample {
    static func main() {
        do {
            try run()
        } catch {
            fatalError("Swift multiple_streams example failed: \(error)")
        }
    }

    static func run() throws {
        var reader = try reader_from_bytes(decodeSample())

        print("Chunk analysis:")
        var chunkStream = try reader_chunks(reader)
        var chunkCount = 0
        var totalUncompressed: UInt64 = 0
        while true {
            let item = try chunk_stream_next(chunkStream)
            if !item.has_value {
                break
            }
            let chunk = item.value
            chunkCount += 1
            totalUncompressed += chunk.uncompressed_size
        }
        print("  \(chunkCount) chunks")
        print("  \(totalUncompressed) bytes uncompressed total")
        reader = try chunk_stream_into_reader(chunkStream)

        print("\nMessage counts per channel:")
        var msgStream = try reader_messages(reader)
        var channelCounts: [UInt16: Int] = [:]
        while true {
            let item = try message_stream_next(msgStream)
            if !item.has_value {
                break
            }
            let msg = item.value
            channelCounts[msg.channel_id, default: 0] += 1
        }
        for (channelId, count) in channelCounts.sorted(by: { $0.key < $1.key }) {
            print("  channel \(channelId): \(count) messages")
        }
        reader = try message_stream_into_reader(msgStream)

        print("\nTime range analysis:")
        var rawStream = try reader_raw_messages(reader)
        var minTime: UInt64? = nil
        var maxTime: UInt64? = nil
        while true {
            let item = try raw_message_stream_next(rawStream)
            if !item.has_value {
                break
            }
            let msg = item.value
            minTime = minTime.map { min($0, msg.log_time) } ?? msg.log_time
            maxTime = maxTime.map { max($0, msg.log_time) } ?? msg.log_time
        }
        if let minTime, let maxTime {
            print("  start: \(minTime)")
            print("  end:   \(maxTime)")
            print("  duration: \(maxTime - minTime)")
        }
        reader = try raw_message_stream_into_reader(rawStream)

        print("\nStream type comparison:")
        let recordStream = try reader_records(reader)
        var recordCount = 0
        while true {
            let item = try record_stream_next(recordStream)
            if !item.has_value {
                break
            }
            recordCount += 1
        }
        print("  Records: \(recordCount)")
        reader = try record_stream_into_reader(recordStream)

        chunkStream = try reader_chunks(reader)
        chunkCount = 0
        while true {
            let item = try chunk_stream_next(chunkStream)
            if !item.has_value {
                break
            }
            chunkCount += 1
        }
        print("  Chunks: \(chunkCount)")
        reader = try chunk_stream_into_reader(chunkStream)

        rawStream = try reader_raw_messages(reader)
        var rawCount = 0
        while true {
            let item = try raw_message_stream_next(rawStream)
            if !item.has_value {
                break
            }
            rawCount += 1
        }
        print("  Raw messages: \(rawCount)")
        reader = try raw_message_stream_into_reader(rawStream)

        msgStream = try reader_messages(reader)
        var msgCount = 0
        while true {
            let item = try message_stream_next(msgStream)
            if !item.has_value {
                break
            }
            msgCount += 1
        }
        print("  Messages: \(msgCount)")
    }
}
