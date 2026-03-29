import Foundation

@main
struct MetadataExample {
    static func main() {
        do {
            try run()
        } catch {
            fatalError("Swift metadata example failed: \(error)")
        }
    }

    static func run() throws {
        var reader = try reader_from_bytes(decodeSample())

        print("Before streaming:")
        let initialSchemas = try collectSchemas(reader_schemas(reader))
        let initialChannels = try collectChannels(reader_channels(reader))
        print("  Schemas: \(initialSchemas.count)")
        print("  Channels: \(initialChannels.count)")

        let stream = try reader_messages(reader)
        _ = try message_stream_next(stream)
        reader = try message_stream_into_reader(stream)

        let schemas = try collectSchemas(reader_schemas(reader))
        let channels = try collectChannels(reader_channels(reader))

        print("\nMetadata available after caching:")
        print("  Schemas: \(schemas.count)")
        print("  Channels: \(channels.count)")

        print("\nChannels:")
        for channel in channels {
            print("  [\(channel.id)] topic='\(channel.topic.toString())' encoding='\(channel.message_encoding.toString())' schema_id=\(channel.schema_id)")
        }

        print("\nSchemas:")
        for schema in schemas {
            print("  [\(schema.id)] name='\(schema.name.toString())' encoding='\(schema.encoding.toString())' data_len=\(schema.data.len())")
        }

        let msgStream = try reader_messages(reader)
        var msgCount = 0
        while true {
            let item = try message_stream_next(msgStream)
            if !item.has_value {
                break
            }
            let msg = item.value
            msgCount += 1
            if let channel = channels.first(where: { $0.id == msg.channel_id }) {
                if let schema = schemas.first(where: { $0.id == channel.schema_id }) {
                    print("  Message on '\(channel.topic.toString())' using schema '\(schema.name.toString())'")
                }
            }
            if msgCount >= 3 {
                break
            }
        }

        print("\nProcessed \(msgCount) messages")
    }
}
