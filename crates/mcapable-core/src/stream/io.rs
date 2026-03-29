//! Low-level I/O operations for reading MCAP records.

use super::ReaderAccess;
use crate::error::Result;
use crate::format::RECORD_HEADER_SIZE;
use crate::records::decode_record_header;
use crate::types::{Chunk, Opcode};
use bytes::Bytes;
use std::io::SeekFrom;

/// A parsed record block with header and data.
pub(super) struct RecordBlock {
    pub(super) opcode: Opcode,
    pub(super) data: Bytes,
    pub(super) offset: u64,
}

/// A record header that has been read but data not yet loaded.
pub(super) struct RecordHeaderBlock {
    pub(super) opcode: Opcode,
    pub(super) length: u64,
    pub(super) offset: u64,
    pub(super) data_start: u64,
}

/// Read just the record header (9 bytes) without loading payload.
///
/// Returns `Ok(None)` at EOF, allowing for graceful stream termination.
pub(super) fn read_record_header_block(
    reader: &mut dyn ReaderAccess,
) -> Result<Option<RecordHeaderBlock>> {
    let offset = reader.source().stream_position()?;
    let header_buf = match reader.source().read_exact_bytes(RECORD_HEADER_SIZE) {
        Ok(buf) => buf,
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    };

    let (opcode, length) = decode_record_header(header_buf.as_ref())?;
    let data_start = offset.saturating_add(RECORD_HEADER_SIZE as u64);

    Ok(Some(RecordHeaderBlock {
        opcode,
        length,
        offset,
        data_start,
    }))
}

/// Skip over a record's payload data by seeking forward.
///
/// Used when filtering records we don't need to parse.
pub(super) fn skip_record_payload(reader: &mut dyn ReaderAccess, len: u64) -> Result<()> {
    let start = reader.source().stream_position()?;
    let end = start.saturating_add(len);
    reader.source().seek(SeekFrom::Start(end))?;
    Ok(())
}

/// Read a complete record (header + payload data).
///
/// Returns `Ok(None)` at EOF.
pub(super) fn read_record_block(reader: &mut dyn ReaderAccess) -> Result<Option<RecordBlock>> {
    let Some(header) = read_record_header_block(reader)? else {
        return Ok(None);
    };

    let len: usize = header
        .length
        .try_into()
        .map_err(|_| crate::Error::UnexpectedEof(header.offset))?;
    let data = reader.source().read_exact_bytes(len)?;

    Ok(Some(RecordBlock {
        opcode: header.opcode,
        data,
        offset: header.offset,
    }))
}

/// Read a chunk record at a specific file offset.
///
/// Used for random access via ChunkIndex. Seeks to the offset, reads the
/// record header, verifies it's a Chunk, then parses the chunk data.
///
/// Returns `Ok(None)` if the record at that offset isn't a Chunk.
pub(super) fn read_chunk_at(reader: &mut dyn ReaderAccess, offset: u64) -> Result<Option<Chunk>> {
    reader.source().seek(SeekFrom::Start(offset))?;
    let header_buf = match reader.source().read_exact_bytes(RECORD_HEADER_SIZE) {
        Ok(buf) => buf,
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    };

    let (opcode, length) = decode_record_header(header_buf.as_ref())?;
    if opcode != Opcode::Chunk {
        return Ok(None);
    }

    let len: usize = length
        .try_into()
        .map_err(|_| crate::Error::UnexpectedEof(offset))?;
    let data = reader.source().read_exact_bytes(len)?;
    let chunk = crate::parser::parse_chunk_record(data)?;

    Ok(Some(chunk))
}
