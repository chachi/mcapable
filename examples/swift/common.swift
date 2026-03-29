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

func collectSchemas(_ list: SchemaList) throws -> [Schema] {
    let count = schema_list_len(list)
    var schemas: [Schema] = []
    for index in 0..<Int(count) {
        schemas.append(try schema_list_get(list, UInt(index)))
    }
    return schemas
}

func collectChannels(_ list: ChannelList) throws -> [Channel] {
    let count = channel_list_len(list)
    var channels: [Channel] = []
    for index in 0..<Int(count) {
        channels.append(try channel_list_get(list, UInt(index)))
    }
    return channels
}

func collectMetadata(_ list: MetadataList) throws -> [Metadata] {
    let count = metadata_list_len(list)
    var items: [Metadata] = []
    for index in 0..<Int(count) {
        items.append(try metadata_list_get(list, UInt(index)))
    }
    return items
}

func collectAttachments(_ list: AttachmentList) throws -> [Attachment] {
    let count = attachment_list_len(list)
    var items: [Attachment] = []
    for index in 0..<Int(count) {
        items.append(try attachment_list_get(list, UInt(index)))
    }
    return items
}
