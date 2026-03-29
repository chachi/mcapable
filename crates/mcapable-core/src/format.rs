//! Format-level constants for MCAP files.

/// MCAP magic bytes: `0x89, 'M', 'C', 'A', 'P', 0x30, '\r', '\n'`
pub const MCAP_MAGIC: [u8; 8] = [0x89, b'M', b'C', b'A', b'P', 0x30, b'\r', b'\n'];

/// Size of MCAP magic bytes.
pub const MCAP_MAGIC_SIZE: usize = 8;

/// Size of a record header: 1 byte opcode + 8 bytes length.
pub const RECORD_HEADER_SIZE: usize = 9;

/// Size of a message header (fixed fields before data).
pub const MESSAGE_HEADER_SIZE: usize = 22; // 2 + 4 + 8 + 8

/// Size of footer record data (summary_start + summary_offset_start + summary_crc).
pub const FOOTER_DATA_SIZE: usize = 20; // 8 + 8 + 4

/// Total size from start of footer to end of file.
pub const FOOTER_TOTAL_SIZE: usize = RECORD_HEADER_SIZE + FOOTER_DATA_SIZE + MCAP_MAGIC_SIZE; // 9 + 20 + 8 = 37
