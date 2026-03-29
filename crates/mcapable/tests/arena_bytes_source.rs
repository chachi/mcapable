use mcapable_core::source::ArenaBytesSource;
use mcapable_core::source::BytesSource;

#[test]
fn arena_bytes_source_matches_file_reads() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let data = (0u8..=255).collect::<Vec<_>>();
    std::fs::write(tmp.path(), &data).unwrap();

    let file = std::fs::File::open(tmp.path()).unwrap();
    let mut source = ArenaBytesSource::new(file);

    let mut out = Vec::new();
    let mut offset = 0usize;
    let mut chunk = 17usize;
    while offset < data.len() {
        let len = std::cmp::min(chunk, data.len() - offset);
        let bytes = source.read_exact_bytes(len).unwrap();
        out.extend_from_slice(bytes.as_ref());
        offset += len;
        chunk = (chunk % 64) + 1;
    }

    assert_eq!(out, data);
}
