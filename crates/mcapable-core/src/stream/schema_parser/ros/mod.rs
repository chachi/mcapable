mod ros1;
mod ros2;

pub(crate) use ros1::Ros1SchemaParser;
pub(crate) use ros2::Ros2SchemaParser;
#[cfg(test)]
mod tests {
    use super::super::typed::SchemaTypedParser;
    use super::*;
    use crate::types::Schema;
    use bytes::Bytes;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Ros1Value {
        value: i32,
    }

    #[test]
    fn ros1_dynamic_parser_to_json() {
        let schema = Schema {
            id: 1,
            name: "test_msgs/Value".into(),
            encoding: "ros1msg".into(),
            data: Bytes::from_static(b"int32 value\n"),
        };
        let parser = Ros1SchemaParser::from_schema(&schema).expect("schema parser");
        let payload = Bytes::from(42i32.to_le_bytes().to_vec());
        let value = parser.parse_json(payload).expect("parse json");
        assert_eq!(value.get("value").and_then(|v| v.as_i64()), Some(42));
    }

    #[test]
    fn ros1_typed_parser_from_schema() {
        let schema = Schema {
            id: 2,
            name: "test_msgs/Value".into(),
            encoding: "ros1msg".into(),
            data: Bytes::from_static(b"int32 value\n"),
        };
        let parser = SchemaTypedParser::from_schema_where(
            &schema,
            |schema| schema.encoding.as_ref() == "ros1msg",
            |value| {
                serde_json::from_value::<Ros1Value>(value)
                    .map_err(|err| crate::error::Error::InvalidRecord(err.to_string()))
            },
        )
        .expect("typed parser");
        let payload = Bytes::from(42i32.to_le_bytes().to_vec());
        let value = parser.parse(payload).expect("parse typed");
        assert_eq!(value, Ros1Value { value: 42 });
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct Ros2Stamp {
        sec: i32,
        nanosec: u32,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct Ros2SmallMsg {
        stamp: Ros2Stamp,
        value: f32,
    }

    fn ros2_schema() -> Schema {
        Schema {
            id: 3,
            name: "package/msg/SmallMsg".into(),
            encoding: "ros2msg".into(),
            data: Bytes::from_static(
                b"builtin_interfaces/Time stamp\nfloat32 value\n\n================================================================================\nMSG: builtin_interfaces/Time\n\nint32 sec\nuint32 nanosec\n",
            ),
        }
    }

    fn ros2_payload() -> Bytes {
        Bytes::from_static(&[
            0x00, 0x01, 0x00, 0x00, 0x9d, 0x2f, 0x88, 0x66, 0x2a, 0x00, 0x00, 0x00, 0xdb, 0x0f,
            0x49, 0x40,
        ])
    }

    #[test]
    fn ros2_dynamic_parser_to_json() {
        let schema = ros2_schema();
        let parser = Ros2SchemaParser::from_schema(&schema).expect("schema parser");
        let value = parser.parse_json(ros2_payload()).expect("parse json");
        let parsed = value.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0);
        assert!((parsed - std::f64::consts::PI).abs() < 1e-6);
    }

    #[test]
    fn ros2_typed_parser_from_schema() {
        let schema = ros2_schema();
        let parser = SchemaTypedParser::from_schema_where(
            &schema,
            |schema| schema.encoding.as_ref() == "ros2msg",
            |value| {
                serde_json::from_value::<Ros2SmallMsg>(value)
                    .map_err(|err| crate::error::Error::InvalidRecord(err.to_string()))
            },
        )
        .expect("typed parser");
        let value = parser.parse(ros2_payload()).expect("parse typed");
        assert_eq!(
            value,
            Ros2SmallMsg {
                stamp: Ros2Stamp {
                    sec: 1720201117,
                    nanosec: 42,
                },
                value: std::f32::consts::PI,
            }
        );
    }

    #[test]
    fn ros1_invalid_schema() {
        let schema = Schema {
            id: 5,
            name: "test_msgs/BadMsg".into(),
            encoding: "ros1msg".into(),
            data: Bytes::from_static(b"int32\n"),
        };
        assert!(Ros1SchemaParser::from_schema(&schema).is_err());
    }

    #[test]
    fn ros2_invalid_schema() {
        let schema = Schema {
            id: 6,
            name: "package/msg/BadMsg".into(),
            encoding: "ros2msg".into(),
            data: Bytes::from_static(b"float32\n"),
        };
        assert!(Ros2SchemaParser::from_schema(&schema).is_err());
    }
}
