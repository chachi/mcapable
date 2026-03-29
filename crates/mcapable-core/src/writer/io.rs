use bytes::Bytes;
use std::io::{IoSlice, Write};
use std::io::{Seek, SeekFrom};

use super::constants::CHUNK_WRITE_IOV_LIMIT;

pub(crate) struct PositionTrackingSink<W: Write + Seek> {
    inner: W,
    pos: u64,
}

impl<W: Write + Seek> PositionTrackingSink<W> {
    pub(crate) fn new(mut inner: W) -> std::io::Result<Self> {
        let pos = inner.stream_position()?;
        Ok(Self { inner, pos })
    }

    #[inline]
    pub(crate) fn position(&self) -> u64 {
        self.pos
    }

    pub(crate) fn into_inner(self) -> W {
        self.inner
    }
}

impl<W: Write + Seek> Write for PositionTrackingSink<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.pos = self.pos.saturating_add(n as u64);
        Ok(n)
    }

    fn write_vectored(&mut self, bufs: &[IoSlice<'_>]) -> std::io::Result<usize> {
        let n = self.inner.write_vectored(bufs)?;
        self.pos = self.pos.saturating_add(n as u64);
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

impl<W: Write + Seek> Seek for PositionTrackingSink<W> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let new = self.inner.seek(pos)?;
        self.pos = new;
        Ok(new)
    }
}

pub(crate) fn write_all_vectored2<W: Write>(
    sink: &mut W,
    first: &[u8],
    second: &[u8],
) -> std::io::Result<()> {
    let mut first_offset = 0usize;
    let mut second_offset = 0usize;

    while first_offset < first.len() || second_offset < second.len() {
        let bufs = [
            IoSlice::new(&first[first_offset..]),
            IoSlice::new(&second[second_offset..]),
        ];
        let wrote = sink.write_vectored(&bufs)?;
        if wrote == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "failed to write buffered data",
            ));
        }

        if first_offset < first.len() {
            let remaining_first = first.len() - first_offset;
            if wrote < remaining_first {
                first_offset += wrote;
                continue;
            }
            first_offset = first.len();
            second_offset = second_offset.saturating_add(wrote - remaining_first);
        } else {
            second_offset = second_offset.saturating_add(wrote);
        }
    }

    Ok(())
}

pub(crate) fn write_all_vectored_chunk_records<W: Write>(
    sink: &mut W,
    message_prefixes: &[u8],
    payloads: &[Bytes],
    prefix_len: usize,
) -> std::io::Result<()> {
    if prefix_len == 0 || payloads.is_empty() {
        return Ok(());
    }
    debug_assert_eq!(message_prefixes.len(), payloads.len() * prefix_len);

    let mut msg_idx = 0usize;
    let mut part_is_payload = false;
    let mut offset = 0usize;
    let mut iovecs: Vec<IoSlice<'_>> = Vec::with_capacity(CHUNK_WRITE_IOV_LIMIT);

    while msg_idx < payloads.len() {
        if part_is_payload && payloads[msg_idx].is_empty() {
            part_is_payload = false;
            msg_idx += 1;
            offset = 0;
            continue;
        }

        iovecs.clear();

        let mut tmp_msg_idx = msg_idx;
        let mut tmp_part_is_payload = part_is_payload;
        let mut tmp_offset = offset;

        while tmp_msg_idx < payloads.len() && iovecs.len() < CHUNK_WRITE_IOV_LIMIT {
            if tmp_part_is_payload {
                let data = payloads[tmp_msg_idx].as_ref();
                if tmp_offset < data.len() {
                    iovecs.push(IoSlice::new(&data[tmp_offset..]));
                }
                tmp_part_is_payload = false;
                tmp_offset = 0;
                tmp_msg_idx += 1;
            } else {
                let start = tmp_msg_idx * prefix_len + tmp_offset;
                let end = (tmp_msg_idx + 1) * prefix_len;
                debug_assert!(start <= end && end <= message_prefixes.len());
                iovecs.push(IoSlice::new(&message_prefixes[start..end]));
                tmp_part_is_payload = true;
                tmp_offset = 0;
            }
        }

        let wrote = sink.write_vectored(&iovecs)?;
        if wrote == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "failed to write buffered data",
            ));
        }

        let mut remaining = wrote;
        while remaining > 0 && msg_idx < payloads.len() {
            if part_is_payload {
                let data_len = payloads[msg_idx].len();
                if offset >= data_len {
                    part_is_payload = false;
                    msg_idx += 1;
                    offset = 0;
                    continue;
                }

                let slice_len = data_len - offset;
                if remaining < slice_len {
                    offset += remaining;
                    remaining = 0;
                } else {
                    remaining -= slice_len;
                    part_is_payload = false;
                    msg_idx += 1;
                    offset = 0;
                }
            } else {
                let slice_len = prefix_len - offset;
                if remaining < slice_len {
                    offset += remaining;
                    remaining = 0;
                } else {
                    remaining -= slice_len;
                    part_is_payload = true;
                    offset = 0;
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_vectored_write_matches_concat() {
        let prefix_len = 9 + 22;
        let payloads = vec![
            Bytes::from_static(b"aaa"),
            Bytes::from_static(b""),
            Bytes::from_static(b"bbbbbbbb"),
        ];
        let mut prefixes = Vec::new();
        for i in 0..payloads.len() {
            prefixes.extend_from_slice(&vec![i as u8; prefix_len]);
        }

        let mut expected = Vec::new();
        for i in 0..payloads.len() {
            expected.extend_from_slice(&prefixes[i * prefix_len..(i + 1) * prefix_len]);
            expected.extend_from_slice(payloads[i].as_ref());
        }

        let mut out = Vec::new();
        write_all_vectored_chunk_records(&mut out, &prefixes, &payloads, prefix_len).unwrap();
        assert_eq!(out, expected);
    }
}
