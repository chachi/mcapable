import Foundation

@main
struct FilteringExample {
    static func main() {
        do {
            try run()
        } catch {
            fatalError("Swift filtering example failed: \(error)")
        }
    }

    static func run() throws {
        var reader = try reader_from_bytes(decodeSample())

        print("Messages in time range [0, 10]:")
        var stream = try reader_messages(reader)
        try message_stream_time_range(stream, 0, 10)
        var count = 0
        while true {
            let item = try message_stream_next(stream)
            if !item.has_value {
                break
            }
            let msg = item.value
            print("  time=\(msg.log_time) channel=\(msg.channel_id)")
            count += 1
            if count >= 5 {
                break
            }
        }
        reader = try message_stream_into_reader(stream)

        print("\nMessages on channels [1]:")
        stream = try reader_messages(reader)
        count = 0
        while true {
            let item = try message_stream_next(stream)
            if !item.has_value {
                break
            }
            let msg = item.value
            if msg.channel_id != 1 {
                continue
            }
            print("  time=\(msg.log_time) channel=\(msg.channel_id)")
            count += 1
            if count >= 5 {
                break
            }
        }
        reader = try message_stream_into_reader(stream)

        print("\nMessages on channel 1 in time range [0, 10]:")
        stream = try reader_messages(reader)
        count = 0
        while true {
            let item = try message_stream_next(stream)
            if !item.has_value {
                break
            }
            let msg = item.value
            if msg.channel_id != 1 || msg.log_time > 10 {
                continue
            }
            print("  time=\(msg.log_time) channel=\(msg.channel_id)")
            count += 1
            if count >= 5 {
                break
            }
        }
    }
}
