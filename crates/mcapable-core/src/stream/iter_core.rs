use super::{
    Stream,
    filters::{
        cache_schema_or_channel, chunk_might_have_messages_in_range_via_index, chunk_passes_filters,
    },
    io::read_record_block,
};
use crate::error::Result;
use crate::types::{Chunk, Opcode, RawMessage};
use bytes::Bytes;

/// State for iterating through messages within a decompressed chunk.
pub(crate) struct ChunkMessageState {
    pub(super) data: Bytes,
    pub(super) position: usize,
    pub(super) chunk_offset: u64,
}

/// State for decompressing and iterating through a chunk.
pub(crate) struct ChunkState {
    /// Decompressed chunk data.
    pub(super) data: Bytes,
    /// Current position within the chunk.
    pub(super) position: usize,
}

/// Process messages from a decompressed chunk.
///
/// Returns Some(item) if a message was found, None if chunk is exhausted.
fn process_chunk_messages<'a, T, F, G>(
    stream: &mut Stream<'a, T>,
    mut chunk_state: ChunkState,
    map_raw: &mut F,
    include: &mut G,
) -> Option<Result<T>>
where
    F: FnMut(RawMessage) -> T,
    G: FnMut(&mut Stream<'a, T>, &T) -> bool,
{
    use crate::records::RecordIterator;

    let iter = RecordIterator::new(chunk_state.data.as_ref(), chunk_state.position);

    for next in iter {
        let loc = match next {
            Ok(loc) => loc,
            Err(_) => break,
        };

        chunk_state.position = loc.next_record_start;

        let opcode = match loc.header.opcode_typed() {
            Some(op) => op,
            None => continue,
        };
        if opcode != Opcode::Message {
            continue;
        }

        let (header, payload) = match crate::parser::parse_message_from_backing_range(
            &chunk_state.data,
            loc.content_start,
            loc.next_record_start,
        ) {
            Ok(parsed) => parsed,
            Err(_) => break,
        };

        if let Some(filter) = &stream.message_filter
            && !filter(&header)
        {
            continue;
        }

        let raw_msg = RawMessage::new(
            header.channel_id,
            header.sequence,
            header.log_time,
            header.publish_time,
            payload,
        );

        let item = map_raw(raw_msg);
        if !include(stream, &item) {
            continue;
        }

        stream.chunk_state = Some(chunk_state);
        return Some(Ok(item));
    }

    None
}

/// Process a top-level Message record.
fn process_top_level_message<'a, T, F, G>(
    stream: &mut Stream<'a, T>,
    data: Bytes,
    map_raw: &mut F,
    include: &mut G,
) -> Option<Result<T>>
where
    F: FnMut(RawMessage) -> T,
    G: FnMut(&mut Stream<'a, T>, &T) -> bool,
{
    use crate::parser::{parse_message_header_from_content, parse_message_record};

    if let Some(filter) = &stream.message_filter {
        let header = match parse_message_header_from_content(data.as_ref()) {
            Ok(header) => header,
            Err(e) => {
                stream.done = true;
                return Some(Err(e));
            }
        };
        if !filter(&header) {
            return None;
        }
    }

    let raw = match parse_message_record(data) {
        Ok(raw) => raw,
        Err(e) => {
            stream.done = true;
            return Some(Err(e));
        }
    };

    let item = map_raw(raw);
    if !include(stream, &item) {
        return None;
    }
    Some(Ok(item))
}

/// Decompress and setup chunk state for message iteration.
fn setup_chunk_state<T>(stream: &mut Stream<'_, T>, chunk: Chunk) -> Result<()> {
    use crate::compression::decompress;

    let compression = crate::compression::parse_compression(&chunk.compression)?;
    let decompressed = decompress(
        compression.as_ref(),
        chunk.records.clone(),
        chunk.uncompressed_size,
    )?;

    stream.chunk_state = Some(ChunkState {
        data: decompressed,
        position: 0,
    });
    Ok(())
}

pub(crate) fn next_message_common<'a, T, F, G>(
    stream: &mut Stream<'a, T>,
    mut map_raw: F,
    mut include: G,
) -> Option<Result<T>>
where
    F: FnMut(RawMessage) -> T,
    G: FnMut(&mut Stream<'a, T>, &T) -> bool,
{
    use crate::parser::parse_chunk_record;

    if stream.done {
        return None;
    }

    loop {
        // Process messages from decompressed chunk if available.
        if let Some(chunk_state) = stream.chunk_state.take() {
            if let Some(result) =
                process_chunk_messages(stream, chunk_state, &mut map_raw, &mut include)
            {
                return Some(result);
            }
            continue;
        }

        // Read next record from file.
        let block = match read_record_block(stream.reader) {
            Ok(Some(block)) => block,
            Ok(None) => {
                stream.done = true;
                return None;
            }
            Err(e) => {
                stream.done = true;
                return Some(Err(e));
            }
        };

        // Cache schemas and channels as we encounter them.
        if block.opcode == Opcode::Schema || block.opcode == Opcode::Channel {
            cache_schema_or_channel(stream.reader, block.opcode, block.data);
            continue;
        }

        // End of data section.
        if block.opcode == Opcode::DataEnd {
            stream.done = true;
            return None;
        }

        // Handle top-level Message records.
        if block.opcode == Opcode::Message {
            if let Some(result) =
                process_top_level_message(stream, block.data, &mut map_raw, &mut include)
            {
                return Some(result);
            }
            continue;
        }

        // Only process Chunk records from here.
        if block.opcode != Opcode::Chunk {
            continue;
        }

        let chunk = match parse_chunk_record(block.data) {
            Ok(chunk) => chunk,
            Err(e) => return Some(Err(e)),
        };

        // Apply chunk filters.
        if !chunk_passes_filters(
            stream.reader,
            block.offset,
            &chunk,
            stream.time_range,
            &stream.channel_predicate,
        ) {
            continue;
        }

        // Check message index for time range optimization.
        if let Some((start, end)) = stream.time_range
            && !chunk_might_have_messages_in_range_via_index(
                stream.reader,
                block.offset,
                start,
                end,
                &stream.channel_predicate,
            )
        {
            continue;
        }

        // Decompress and setup for iteration.
        if let Err(e) = setup_chunk_state(stream, chunk) {
            return Some(Err(e));
        }
    }
}
