import Foundation

@main
struct BasicExample {
    static func main() {
        do {
            try run()
        } catch {
            fatalError("Swift basic example failed: \(error)")
        }
    }

    static func run() throws {
        let reader = try reader_from_bytes(decodeSample())

        let header = try reader_header(reader)
        print("Profile: \(header.profile.toString())")
        print("Library: \(header.library.toString())")

        let stream = try reader_messages(reader)
        var count = 0
        var firstPrinted = false
        while true {
            let item = try message_stream_next(stream)
            if !item.has_value {
                break
            }
            let msg = item.value
            if !firstPrinted {
                print("Message: channel=\(msg.channel_id) time=\(msg.log_time) size=\(msg.data.len())")
                firstPrinted = true
            }
            count += 1
        }
        print("Total messages: \(count)")
    }
}
