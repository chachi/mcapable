use bytes::Bytes;
use mcapable::reader;

#[path = "helpers/mod.rs"]
mod helpers;
use helpers::mcap_builder::*;

fn assert_ptr_within(outer: &Bytes, inner: &Bytes) {
    let outer_base = outer.as_ref().as_ptr() as usize;
    let outer_end = outer_base + outer.len();
    let inner_ptr = inner.as_ref().as_ptr() as usize;
    assert!(inner_ptr >= outer_base && inner_ptr < outer_end);
}

#[test]
fn build_bytes_header_profile_is_zero_copy_slice() {
    let mcap = McapBuilder::new().build();
    let root = Bytes::from(mcap);

    let mut reader = reader::Builder::new().build_bytes(root.clone()).unwrap();
    let header = reader.header().unwrap();

    assert_ptr_within(&root, header.profile.bytes());
    assert_ptr_within(&root, header.library.bytes());
}

#[test]
fn build_bytes_unchunked_message_payload_is_zero_copy_slice() {
    let mcap = create_unchunked_mcap();
    let root = Bytes::from(mcap);

    let mut reader = reader::Builder::new().build_bytes(root.clone()).unwrap();
    let mut stream = reader.records();

    let msg = loop {
        match stream.next().unwrap().unwrap() {
            mcapable::Record::Message(msg) => break msg,
            mcapable::Record::DataEnd => panic!("no message records found"),
            _ => continue,
        }
    };

    assert_ptr_within(&root, &msg.data_bytes());
}
