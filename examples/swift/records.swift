import Foundation

@main
struct RecordsExample {
    static func main() {
        do {
            try run()
        } catch {
            fatalError("Swift records example failed: \(error)")
        }
    }

    static func run() throws {
        let reader = try reader_from_bytes(decodeSample())
        let stream = try reader_records(reader)

        var counts: [String: Int] = [
            "Header": 0,
            "Footer": 0,
            "Schema": 0,
            "Channel": 0,
            "Message": 0,
            "Chunk": 0,
            "Other": 0,
        ]

        while true {
            let item = try record_stream_next(stream)
            if !item.has_value {
                break
            }
            let record = item.value
            let kind = record.kind.toString()
            let key = counts.keys.contains(kind) ? kind : "Other"
            counts[key, default: 0] += 1
        }

        print("Record counts:")
        for key in ["Header", "Footer", "Schema", "Channel", "Message", "Chunk", "Other"] {
            print("  \(key): \(counts[key, default: 0])")
        }
    }
}
