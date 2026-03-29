use mcapable::reader;
use std::io::Cursor;

#[path = "helpers/mod.rs"]
mod helpers;
use helpers::mcap_builder::{McapBuilder, TestChannel};

#[test]
fn channel_filter_by_topic_prefix() {
    let mcap = McapBuilder::new()
        .chunked(true)
        .chunk_size(Some(1))
        .add_simple_channel(0, "/camera/front")
        .add_simple_channel(1, "/imu")
        .add_simple_message(0, 1, 10, b"a".to_vec())
        .add_simple_message(1, 1, 11, b"b".to_vec())
        .build();

    let mut reader = reader::Builder::new().build(Cursor::new(mcap)).unwrap();
    let msgs: Vec<_> = reader
        .raw_messages()
        .unwrap()
        .filter_channel(|ch| ch.topic.starts_with("/camera/"))
        .collect::<mcapable::Result<Vec<_>>>()
        .unwrap();

    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].channel_id, 0);
}

#[test]
fn channel_filter_by_schema_id_and_combination() {
    let mcap = McapBuilder::new()
        .chunked(true)
        .chunk_size(Some(1))
        .add_channel(TestChannel {
            id: 0,
            topic: "/t1".to_string(),
            message_encoding: "raw".to_string(),
            schema_id: 7,
            schema_name: Some("s1".to_string()),
            schema_encoding: Some("msgpack".to_string()),
            schema_data: Some(vec![1, 2, 3]),
        })
        .add_channel(TestChannel {
            id: 1,
            topic: "/t2".to_string(),
            message_encoding: "raw".to_string(),
            schema_id: 0,
            schema_name: None,
            schema_encoding: None,
            schema_data: None,
        })
        .add_simple_message(0, 1, 10, b"a".to_vec())
        .add_simple_message(1, 1, 11, b"b".to_vec())
        .build();

    let mut reader = reader::Builder::new().build(Cursor::new(mcap)).unwrap();
    let msgs: Vec<_> = reader
        .raw_messages()
        .unwrap()
        .filter_channel(|ch| ch.schema_id != 0 && ch.topic == "/t1")
        .collect::<mcapable::Result<Vec<_>>>()
        .unwrap();

    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].channel_id, 0);
}
