import Foundation

@main
struct ChannelFilterExample {
    static func main() {
        do {
            try run()
        } catch {
            fatalError("Swift channel_filter example failed: \(error)")
        }
    }

    static func run() throws {
        var reader = try reader_from_bytes(decodeSample())
        let cacheStream = try reader_messages(reader)
        _ = try message_stream_next(cacheStream)
        reader = try message_stream_into_reader(cacheStream)

        let channels = try collectChannels(reader_channels(reader))
        let exampleChannel = channels.first { $0.topic.toString() == "/example" }
        guard let channelId = exampleChannel?.id else {
            print("No /example channel found")
            return
        }

        print("Messages on channel \(channelId):")
        let stream = try reader_messages(reader)
        var count = 0
        while true {
            let item = try message_stream_next(stream)
            if !item.has_value {
                break
            }
            let msg = item.value
            if msg.channel_id == channelId {
                print("  time=\(msg.log_time) channel=\(msg.channel_id)")
                count += 1
                if count >= 5 {
                    break
                }
            }
        }
    }
}
