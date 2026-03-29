use mcapable::Opcode;
use mcapable::reader;
use mcapable_core::format::MCAP_MAGIC;
use std::io::Cursor;

fn le_u32_bytes(v: u32) -> [u8; 4] {
    v.to_le_bytes()
}

fn le_u64_bytes(v: u64) -> [u8; 8] {
    v.to_le_bytes()
}

fn length_string_bytes(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + s.len());
    out.extend_from_slice(&le_u32_bytes(s.len() as u32));
    out.extend_from_slice(s.as_bytes());
    out
}

fn append_record(buf: &mut Vec<u8>, opcode: u8, data: &[u8]) {
    buf.push(opcode);
    buf.extend_from_slice(&le_u64_bytes(data.len() as u64));
    buf.extend_from_slice(data);
}

fn channel_record(id: u16, topic: &str, encoding: &str) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&id.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes()); // schema_id
    data.extend_from_slice(&length_string_bytes(topic));
    data.extend_from_slice(&length_string_bytes(encoding));
    data.extend_from_slice(&le_u32_bytes(0)); // empty metadata map
    data
}

fn message_record(channel_id: u16, payload: &[u8]) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&channel_id.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes()); // sequence
    data.extend_from_slice(&10u64.to_le_bytes()); // log_time
    data.extend_from_slice(&10u64.to_le_bytes()); // publish_time
    data.extend_from_slice(payload);
    data
}

fn chunk_record(
    message_start_time: u64,
    message_end_time: u64,
    compression: &str,
    records: &[u8],
) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&le_u64_bytes(message_start_time));
    data.extend_from_slice(&le_u64_bytes(message_end_time));
    data.extend_from_slice(&le_u64_bytes(records.len() as u64)); // uncompressed_size
    data.extend_from_slice(&0u32.to_le_bytes()); // uncompressed_crc
    data.extend_from_slice(&length_string_bytes(compression));
    data.extend_from_slice(&le_u64_bytes(records.len() as u64)); // compressed_length
    data.extend_from_slice(records);
    data
}

fn chunk_index_record(
    message_start_time: u64,
    message_end_time: u64,
    chunk_start_offset: u64,
    chunk_length: u64,
    channel_ids: &[u16],
    compression: &str,
    uncompressed_size: u64,
) -> Vec<u8> {
    let mut map_bytes = Vec::new();
    for channel_id in channel_ids {
        map_bytes.extend_from_slice(&channel_id.to_le_bytes());
        map_bytes.extend_from_slice(&0u64.to_le_bytes()); // message index offset placeholder
    }

    let mut data = Vec::new();
    data.extend_from_slice(&le_u64_bytes(message_start_time));
    data.extend_from_slice(&le_u64_bytes(message_end_time));
    data.extend_from_slice(&le_u64_bytes(chunk_start_offset));
    data.extend_from_slice(&le_u64_bytes(chunk_length));
    data.extend_from_slice(&le_u32_bytes(map_bytes.len() as u32));
    data.extend_from_slice(&map_bytes);
    data.extend_from_slice(&le_u64_bytes(0)); // message_index_length
    data.extend_from_slice(&length_string_bytes(compression));
    data.extend_from_slice(&le_u64_bytes(uncompressed_size)); // compressed_size placeholder
    data.extend_from_slice(&le_u64_bytes(uncompressed_size));
    data
}

fn chunk_index_record_with_offsets(
    message_start_time: u64,
    message_end_time: u64,
    chunk_start_offset: u64,
    chunk_length: u64,
    channel_offsets: &[(u16, u64)],
    compression: &str,
    uncompressed_size: u64,
) -> Vec<u8> {
    let mut map_bytes = Vec::new();
    for (channel_id, offset) in channel_offsets {
        map_bytes.extend_from_slice(&channel_id.to_le_bytes());
        map_bytes.extend_from_slice(&offset.to_le_bytes());
    }

    let mut data = Vec::new();
    data.extend_from_slice(&le_u64_bytes(message_start_time));
    data.extend_from_slice(&le_u64_bytes(message_end_time));
    data.extend_from_slice(&le_u64_bytes(chunk_start_offset));
    data.extend_from_slice(&le_u64_bytes(chunk_length));
    data.extend_from_slice(&le_u32_bytes(map_bytes.len() as u32));
    data.extend_from_slice(&map_bytes);
    data.extend_from_slice(&le_u64_bytes(0)); // message_index_length
    data.extend_from_slice(&length_string_bytes(compression));
    data.extend_from_slice(&le_u64_bytes(uncompressed_size)); // compressed_size placeholder
    data.extend_from_slice(&le_u64_bytes(uncompressed_size));
    data
}

fn message_index_record(channel_id: u16, timestamp: u64) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&timestamp.to_le_bytes());
    body.extend_from_slice(&0u64.to_le_bytes()); // offset

    let mut data = Vec::new();
    data.extend_from_slice(&channel_id.to_le_bytes());
    data.extend_from_slice(&(body.len() as u32).to_le_bytes());
    data.extend_from_slice(&body);
    data
}

#[test]
fn channel_predicate_skips_non_matching_chunks_via_index() {
    // Build minimal MCAP file with two chunks:
    // - Chunk 1: channel 1 only, invalid compression ("bad") so it must be skipped.
    // - Chunk 2: channel 2, valid uncompressed message.
    let mut file = Vec::new();
    file.extend_from_slice(&MCAP_MAGIC);

    // Header (empty strings + empty metadata)
    let mut header_data = Vec::new();
    header_data.extend_from_slice(&le_u32_bytes(0)); // profile
    header_data.extend_from_slice(&le_u32_bytes(0)); // library
    header_data.extend_from_slice(&le_u32_bytes(0)); // metadata map
    append_record(&mut file, Opcode::Header as u8, &header_data);

    // Channel records
    append_record(
        &mut file,
        Opcode::Channel as u8,
        &channel_record(1, "/skip", "raw"),
    );
    append_record(
        &mut file,
        Opcode::Channel as u8,
        &channel_record(2, "/keep", "raw"),
    );

    // Chunk 1: invalid compression, should be skipped by channel predicate + chunk index.
    let chunk1_offset = file.len() as u64;
    let chunk1_records: Vec<u8> = Vec::new();
    let chunk1 = chunk_record(0, 0, "bad", &chunk1_records);
    append_record(&mut file, Opcode::Chunk as u8, &chunk1);
    let chunk1_length = (9 + chunk1.len()) as u64;

    // Chunk 2: valid uncompressed message on channel 2.
    let msg_payload = b"hello";
    let mut chunk2_records = Vec::new();
    append_record(
        &mut chunk2_records,
        Opcode::Message as u8,
        &message_record(2, msg_payload),
    );
    let chunk2_offset = file.len() as u64;
    let chunk2 = chunk_record(0, 0, "", &chunk2_records);
    append_record(&mut file, Opcode::Chunk as u8, &chunk2);
    let chunk2_length = (9 + chunk2.len()) as u64;

    // DataEnd to terminate data section.
    append_record(&mut file, Opcode::DataEnd as u8, &le_u32_bytes(0));

    // Summary section with ChunkIndexes for both chunks.
    let summary_start = file.len() as u64;
    let chunk1_index = chunk_index_record(
        0,
        0,
        chunk1_offset,
        chunk1_length,
        &[1],
        "bad",
        chunk1_records.len() as u64,
    );
    append_record(&mut file, Opcode::ChunkIndex as u8, &chunk1_index);

    let chunk2_index = chunk_index_record(
        0,
        0,
        chunk2_offset,
        chunk2_length,
        &[2],
        "",
        chunk2_records.len() as u64,
    );
    append_record(&mut file, Opcode::ChunkIndex as u8, &chunk2_index);

    // SummaryOffset record to mark end of summary.
    let summary_offset_start = file.len() as u64;
    let mut summary_offset = Vec::new();
    summary_offset.push(0); // group opcode placeholder
    summary_offset.extend_from_slice(&le_u64_bytes(0));
    summary_offset.extend_from_slice(&le_u64_bytes(0));
    append_record(&mut file, Opcode::SummaryOffset as u8, &summary_offset);

    // Footer + trailing magic.
    let mut footer_data = Vec::new();
    footer_data.extend_from_slice(&le_u64_bytes(summary_start));
    footer_data.extend_from_slice(&le_u64_bytes(summary_offset_start));
    footer_data.extend_from_slice(&le_u32_bytes(0)); // summary_crc
    append_record(&mut file, Opcode::Footer as u8, &footer_data);
    file.extend_from_slice(&MCAP_MAGIC);

    let cursor = Cursor::new(file);
    let mut reader = reader::Builder::new()
        .validate_end_magic(false)
        .build(cursor)
        .unwrap();

    let messages: Vec<_> = reader
        .raw_messages()
        .unwrap()
        .filter_channel(|ch| ch.id == 2)
        .collect::<Result<_, _>>()
        .unwrap();

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].channel_id, 2);
    assert_eq!(messages[0].data(), msg_payload);
}

#[test]
fn time_range_skips_chunks_without_decompress_via_message_index() {
    // Build minimal MCAP file with two chunks:
    // - Chunk 1: invalid compression ("bad"), time range overlaps, but MessageIndex has no
    //   timestamps in the filtered range, so it must be skipped without decompression.
    // - Chunk 2: valid uncompressed message in the filtered range.
    let mut file = Vec::new();
    file.extend_from_slice(&MCAP_MAGIC);

    // Header (empty strings + empty metadata)
    let mut header_data = Vec::new();
    header_data.extend_from_slice(&le_u32_bytes(0)); // profile
    header_data.extend_from_slice(&le_u32_bytes(0)); // library
    header_data.extend_from_slice(&le_u32_bytes(0)); // metadata map
    append_record(&mut file, Opcode::Header as u8, &header_data);

    // Channel records
    append_record(
        &mut file,
        Opcode::Channel as u8,
        &channel_record(1, "/skip", "raw"),
    );
    append_record(
        &mut file,
        Opcode::Channel as u8,
        &channel_record(2, "/keep", "raw"),
    );

    // Chunk 1: invalid compression; its (start,end) overlaps the filter range.
    let chunk1_offset = file.len() as u64;
    let chunk1_records: Vec<u8> = Vec::new();
    let chunk1 = chunk_record(0, 100, "bad", &chunk1_records);
    append_record(&mut file, Opcode::Chunk as u8, &chunk1);
    let chunk1_length = (9 + chunk1.len()) as u64;

    // Chunk 2: valid uncompressed message on channel 2 at log_time 55.
    let msg_payload = b"hello";
    let mut chunk2_records = Vec::new();
    let mut msg = message_record(2, msg_payload);
    msg[2 + 4..2 + 4 + 8].copy_from_slice(&55u64.to_le_bytes()); // overwrite log_time
    msg[2 + 4 + 8..2 + 4 + 8 + 8].copy_from_slice(&55u64.to_le_bytes()); // overwrite publish_time
    append_record(&mut chunk2_records, Opcode::Message as u8, &msg);
    let chunk2_offset = file.len() as u64;
    let chunk2 = chunk_record(50, 60, "", &chunk2_records);
    append_record(&mut file, Opcode::Chunk as u8, &chunk2);
    let chunk2_length = (9 + chunk2.len()) as u64;

    // DataEnd to terminate data section.
    append_record(&mut file, Opcode::DataEnd as u8, &le_u32_bytes(0));

    // Summary section with a MessageIndex and ChunkIndexes for both chunks.
    let summary_start = file.len() as u64;
    let msg_index_offset = file.len() as u64;
    let msg_index = message_index_record(1, 10); // outside [50,60]
    append_record(&mut file, Opcode::MessageIndex as u8, &msg_index);

    let chunk1_index = chunk_index_record_with_offsets(
        0,
        100,
        chunk1_offset,
        chunk1_length,
        &[(1, msg_index_offset)],
        "bad",
        chunk1_records.len() as u64,
    );
    append_record(&mut file, Opcode::ChunkIndex as u8, &chunk1_index);

    let chunk2_index = chunk_index_record(
        50,
        60,
        chunk2_offset,
        chunk2_length,
        &[2],
        "",
        chunk2_records.len() as u64,
    );
    append_record(&mut file, Opcode::ChunkIndex as u8, &chunk2_index);

    // SummaryOffset record to mark end of summary.
    let summary_offset_start = file.len() as u64;
    let mut summary_offset = Vec::new();
    summary_offset.push(0); // group opcode placeholder
    summary_offset.extend_from_slice(&le_u64_bytes(0));
    summary_offset.extend_from_slice(&le_u64_bytes(0));
    append_record(&mut file, Opcode::SummaryOffset as u8, &summary_offset);

    // Footer + trailing magic.
    let mut footer_data = Vec::new();
    footer_data.extend_from_slice(&le_u64_bytes(summary_start));
    footer_data.extend_from_slice(&le_u64_bytes(summary_offset_start));
    footer_data.extend_from_slice(&le_u32_bytes(0)); // summary_crc
    append_record(&mut file, Opcode::Footer as u8, &footer_data);
    file.extend_from_slice(&MCAP_MAGIC);

    let cursor = Cursor::new(file);
    let mut reader = reader::Builder::new()
        .validate_end_magic(false)
        .build(cursor)
        .unwrap();

    let messages: Vec<_> = reader
        .raw_messages()
        .unwrap()
        .time_range(50, 60)
        .collect::<Result<_, _>>()
        .unwrap();

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].channel_id, 2);
    assert_eq!(messages[0].data(), msg_payload);
}
