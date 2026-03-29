import Foundation

@main
struct BytesSourceExample {
    static func main() {
        do {
            try run()
        } catch {
            fatalError("Swift bytes_source example failed: \(error)")
        }
    }

    static func run() throws {
        let bytes = decodeSample()
        let builder = reader_builder_new()
        reader_builder_validate_end_magic(builder, true)
        let reader = try reader_builder_build_from_bytes(builder, bytes)

        let header = try reader_header(reader)
        print("Profile: \(header.profile.toString())")
        print("Library: \(header.library.toString())")

        let stream = try reader_raw_messages(reader)
        var count = 0
        while true {
            let item = try raw_message_stream_next(stream)
            if !item.has_value {
                break
            }
            let msg = item.value
            count += 1
            if count <= 3 {
                print("Message: channel=\(msg.channel_id) time=\(msg.log_time) size=\(msg.data.len())")
            }
        }
        print("Total messages: \(count)")
    }
}
