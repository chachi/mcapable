use mcapable_core::source::BytesSource;
use mcapable_opendal::OpendalBytesSource;
use opendal::Operator;
use opendal::services::Memory;
use std::io::SeekFrom;

#[test]
fn opendal_bytes_source_reads_and_seeks() {
    let op = Operator::new(Memory::default())
        .unwrap()
        .finish()
        .blocking();
    op.write("file.mcap", &b"abcdef"[..]).unwrap();

    let mut source = OpendalBytesSource::new(op, "file.mcap").unwrap();
    assert_eq!(source.len(), 6);

    let out = source.read_exact_bytes(3).unwrap();
    assert_eq!(out.as_ref(), b"abc");

    source.seek(SeekFrom::Start(4)).unwrap();
    let out = source.read_exact_bytes(2).unwrap();
    assert_eq!(out.as_ref(), b"ef");
}

#[test]
fn opendal_bytes_source_read_past_end_is_eof() {
    let op = Operator::new(Memory::default())
        .unwrap()
        .finish()
        .blocking();
    op.write("file.mcap", &b"abc"[..]).unwrap();

    let mut source = OpendalBytesSource::new(op, "file.mcap").unwrap();
    let err = source.read_exact_bytes(4).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::UnexpectedEof);
}

#[test]
fn opendal_bytes_source_seek_past_end_is_error() {
    let op = Operator::new(Memory::default())
        .unwrap()
        .finish()
        .blocking();
    op.write("file.mcap", &b"abc"[..]).unwrap();

    let mut source = OpendalBytesSource::new(op, "file.mcap").unwrap();
    let err = source.seek(SeekFrom::Start(4)).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
}
