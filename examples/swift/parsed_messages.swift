import Foundation

@main
struct ParsedMessagesRunner {
    static func main() {
        do {
            try run()
        } catch {
            fatalError("Swift parsed messages example failed: \(error)")
        }
    }

    static func run() throws {
        print("Parsed Message Stream Example")

        let bytes = decodeSample()
        let reader = try reader_from_bytes(bytes)

        let stream = try reader_messages(reader)
        let parsed = try message_stream_parsed(stream)

        var count = 0
        while true {
            let item = try parsed_message_stream_next(parsed)
            if !item.has_value {
                break
            }
            let value = item.value
            if value.is_json {
                print("Message \(count): parsed JSON \(value.json.toString())")
            } else {
                print("Message \(count): raw bytes size=\(value.bytes.len())")
            }
            count += 1
            if count >= 5 {
                break
            }
        }
    }
}
