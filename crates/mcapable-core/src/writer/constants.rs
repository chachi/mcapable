pub(crate) const DEFAULT_CHUNK_MAX_UNCOMPRESSED_BYTES: usize = 4 * 1024 * 1024;

pub(crate) const CHUNK_WRITE_IOV_LIMIT: usize = 256;

pub(crate) const MESSAGE_RECORD_PREFIX_LEN: usize =
    crate::format::RECORD_HEADER_SIZE + crate::format::MESSAGE_HEADER_SIZE;
