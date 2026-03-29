# MCAP File Format Reference

This document describes the MCAP binary format as implemented in mcapable.

## File Structure Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                      MCAP File Layout                            │
├─────────────────────────────────────────────────────────────────┤
│  Magic (8 bytes): 0x89 'M' 'C' 'A' 'P' 0x30 '\r' '\n'           │
├─────────────────────────────────────────────────────────────────┤
│  Header Record                                                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│                       DATA SECTION                               │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  Schema Records (optional, if not in chunks)            │   │
│  │  Channel Records (optional, if not in chunks)           │   │
│  │  Message Records (if unchunked)                         │   │
│  │  Chunk Records (containing Schema, Channel, Message)     │   │
│  │  MessageIndex Records (after each Chunk)                 │   │
│  │  Metadata Records                                        │   │
│  │  Attachment Records                                      │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                  │
│  DataEnd Record                                                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│                     SUMMARY SECTION                              │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  Schema Records (duplicated for random access)          │   │
│  │  Channel Records (duplicated for random access)         │   │
│  │  ChunkIndex Records                                      │   │
│  │  AttachmentIndex Records                                 │   │
│  │  MetadataIndex Records                                   │   │
│  │  Statistics Record                                       │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                  │
├─────────────────────────────────────────────────────────────────┤
│                  SUMMARY OFFSET SECTION                          │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  SummaryOffset Records (one per opcode group)           │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                  │
├─────────────────────────────────────────────────────────────────┤
│  Footer Record                                                   │
├─────────────────────────────────────────────────────────────────┤
│  Magic (8 bytes): 0x89 'M' 'C' 'A' 'P' 0x30 '\r' '\n'           │
└─────────────────────────────────────────────────────────────────┘
```

## Record Format

Every record follows this structure:

```
┌──────────┬──────────────────┬──────────────────────────────┐
│ 1 byte   │ 8 bytes          │ N bytes                      │
│ Opcode   │ Content Length   │ Record Content               │
│          │ (little-endian)  │                              │
└──────────┴──────────────────┴──────────────────────────────┘
     ^            ^                      ^
     │            │                      │
     │            │                      └─ Payload (length = Content Length)
     │            └─ u64 little-endian length of payload
     └─ Record type identifier
```

**Total record size = 1 + 8 + Content Length = 9 + Content Length bytes**

## Opcodes

| Opcode | Name           | Description                              |
|--------|----------------|------------------------------------------|
| 0x01   | Header         | File metadata                            |
| 0x02   | Footer         | Summary section offsets                  |
| 0x03   | Schema         | Message schema definition                |
| 0x04   | Channel        | Channel/topic definition                 |
| 0x05   | Message        | Actual message data                      |
| 0x06   | Chunk          | Compressed block of records              |
| 0x07   | MessageIndex   | Index of messages within a chunk         |
| 0x08   | ChunkIndex     | Index entry for a chunk                  |
| 0x09   | Attachment     | Binary attachment                        |
| 0x0A   | AttachmentIndex| Index entry for attachment               |
| 0x0B   | Statistics     | File statistics                          |
| 0x0C   | Metadata       | User metadata                            |
| 0x0D   | MetadataIndex  | Index entry for metadata                 |
| 0x0E   | SummaryOffset  | Offset to group of summary records       |
| 0x0F   | DataEnd        | Marks end of data section                |

## Primitive Types (all little-endian)

| Type      | Size    | Description                    |
|-----------|---------|--------------------------------|
| uint8     | 1 byte  | Unsigned 8-bit integer         |
| uint16    | 2 bytes | Unsigned 16-bit integer        |
| uint32    | 4 bytes | Unsigned 32-bit integer        |
| uint64    | 8 bytes | Unsigned 64-bit integer        |
| Timestamp | 8 bytes | Nanoseconds since Unix epoch   |
| String    | 4 + N   | u32 length + UTF-8 bytes       |
| Bytes     | 4 + N   | u32 length + raw bytes         |
| Map       | 4 + N   | u32 byte length + key-value pairs |

## Record Content Layouts

### Header (0x01)

```
┌────────────┬────────────┐
│ String     │ String     │
│ profile    │ library    │
└────────────┴────────────┘
```

### Footer (0x02)

```
┌──────────────────┬────────────────────────┬────────────────┐
│ u64              │ u64                    │ u32            │
│ summary_start    │ summary_offset_start   │ summary_crc    │
│ (8 bytes)        │ (8 bytes)              │ (4 bytes)      │
└──────────────────┴────────────────────────┴────────────────┘
Total: 20 bytes
```

### Schema (0x03)

```
┌────────┬────────────┬──────────────┬───────────────────┐
│ u16    │ String     │ String       │ Bytes             │
│ id     │ name       │ encoding     │ data              │
│(2 bytes)│(4+N bytes)│ (4+N bytes)  │ (4+N bytes)       │
└────────┴────────────┴──────────────┴───────────────────┘
```

### Channel (0x04)

```
┌────────┬───────────┬────────────┬──────────────────┬───────────────┐
│ u16    │ u16       │ String     │ String           │ Map           │
│ id     │ schema_id │ topic      │ message_encoding │ metadata      │
│(2 bytes)│(2 bytes) │(4+N bytes) │ (4+N bytes)      │ (4+N bytes)   │
└────────┴───────────┴────────────┴──────────────────┴───────────────┘
```

### Message (0x05) - CRITICAL: No length prefix on data!

```
┌────────────┬──────────┬──────────────┬────────────────┬───────────┐
│ u16        │ u32      │ u64          │ u64            │ [u8]      │
│ channel_id │ sequence │ log_time     │ publish_time   │ data      │
│ (2 bytes)  │ (4 bytes)│ (8 bytes)    │ (8 bytes)      │ (N bytes) │
└────────────┴──────────┴──────────────┴────────────────┴───────────┘
Fixed header: 2 + 4 + 8 + 8 = 22 bytes
Data length = record_content_length - 22
```

**IMPORTANT**: The message data does NOT have a length prefix. The data length
is implicit: `data_length = record_content_length - 22`

### Chunk (0x06)

```
┌───────────────────┬─────────────────┬───────────────────┬────────────────┐
│ u64               │ u64             │ u64               │ u32            │
│ message_start_time│ message_end_time│ uncompressed_size │ uncompressed_crc│
│ (8 bytes)         │ (8 bytes)       │ (8 bytes)         │ (4 bytes)      │
├───────────────────┴─────────────────┴───────────────────┴────────────────┤
│ String            │ u64             │ [u8]                               │
│ compression       │ compressed_size │ records (compressed data)          │
│ (4+N bytes)       │ (8 bytes)       │ (compressed_size bytes)            │
└───────────────────┴─────────────────┴────────────────────────────────────┘
```

The `records` field contains Schema, Channel, and Message records that have
been (optionally) compressed. When decompressed, the data is `uncompressed_size`
bytes and contains standard record format data.

### ChunkIndex (0x08)

```
┌───────────────────┬─────────────────┬───────────────────┬──────────────┐
│ u64               │ u64             │ u64               │ u64          │
│ message_start_time│ message_end_time│ chunk_start_offset│ chunk_length │
│ (8 bytes)         │ (8 bytes)       │ (8 bytes)         │ (8 bytes)    │
├───────────────────┴─────────────────┼───────────────────┴──────────────┤
│ Map<u16, u64>                       │ u64                              │
│ message_index_offsets               │ message_index_length             │
│ (4+N bytes)                         │ (8 bytes)                        │
├─────────────────────────────────────┼──────────────────────────────────┤
│ String                              │ u64              │ u64           │
│ compression                         │ compressed_size  │ uncompressed  │
│ (4+N bytes)                         │ (8 bytes)        │ (8 bytes)     │
└─────────────────────────────────────┴──────────────────┴───────────────┘
```

### Statistics (0x0B)

```
┌──────────────┬──────────────┬───────────────┬──────────────────┐
│ u64          │ u16          │ u32           │ u32              │
│ message_count│ schema_count │ channel_count │ attachment_count │
│ (8 bytes)    │ (2 bytes)    │ (4 bytes)     │ (4 bytes)        │
├──────────────┼──────────────┼───────────────┼──────────────────┤
│ u32          │ u32          │ u64           │ u64              │
│ metadata_cnt │ chunk_count  │ msg_start_time│ msg_end_time     │
│ (4 bytes)    │ (4 bytes)    │ (8 bytes)     │ (8 bytes)        │
├──────────────┴──────────────┴───────────────┴──────────────────┤
│ Map<u16, u64>                                                  │
│ channel_message_counts (4+N bytes)                             │
└────────────────────────────────────────────────────────────────┘
```

### DataEnd (0x0F)

```
┌──────────────────┐
│ u32              │
│ data_section_crc │
│ (4 bytes)        │
└──────────────────┘
```

## Chunk Data Structure

When you decompress a chunk, you get a stream of records:

```
Decompressed Chunk Data:
┌─────────────────────────────────────────────────────────────┐
│ Record 1 (Schema)                                           │
│ ┌─────────┬──────────┬────────────────────────────────────┐ │
│ │ opcode  │ length   │ content                            │ │
│ │ (1 byte)│ (8 bytes)│ (length bytes)                     │ │
│ └─────────┴──────────┴────────────────────────────────────┘ │
├─────────────────────────────────────────────────────────────┤
│ Record 2 (Channel)                                          │
│ ┌─────────┬──────────┬────────────────────────────────────┐ │
│ │ opcode  │ length   │ content                            │ │
│ └─────────┴──────────┴────────────────────────────────────┘ │
├─────────────────────────────────────────────────────────────┤
│ Record 3 (Message)                                          │
│ ┌─────────┬──────────┬────────────────────────────────────┐ │
│ │ 0x05    │ length   │ channel_id|seq|log|pub|data        │ │
│ └─────────┴──────────┴────────────────────────────────────┘ │
├─────────────────────────────────────────────────────────────┤
│ ... more records ...                                        │
└─────────────────────────────────────────────────────────────┘
```

## Iteration Offsets

When iterating through records in a chunk:

```
Position 0:  opcode (1 byte)
Position 1:  length (8 bytes, little-endian u64)
Position 9:  content start
Position 9+length: next record starts here

To advance: position += 9 + length
```

## Common Bugs to Avoid

1. **Message data length**: Message records do NOT have a length-prefixed data
   field. The data is the remainder of the record after the 22-byte header.

2. **Record length**: The length field in the record header is the content
   length only, not including the 9-byte header (opcode + length).

3. **String/Bytes encoding**: Strings and byte arrays are length-prefixed with
   a u32 (4 bytes), not u64.

4. **Chunk records length**: The `records` field in a Chunk has a u64 length
   prefix (compressed_size), but this is explicit in the format, not implicit.
