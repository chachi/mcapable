use crate::error::Result;
use crate::types::Schema;
use bytes::Bytes;
use serde_json::Value;

pub(crate) struct ProtobufSchemaParser {
    message: prost_reflect::MessageDescriptor,
}

impl ProtobufSchemaParser {
    pub(crate) fn from_schema(schema: &Schema) -> Result<Self> {
        let pool = prost_reflect::DescriptorPool::decode(schema.data.clone()).map_err(|err| {
            crate::error::Error::InvalidRecord(format!(
                "Invalid protobuf descriptor set for schema {}: {err}",
                schema.name
            ))
        })?;
        let message = pool
            .get_message_by_name(schema.name.as_ref())
            .ok_or_else(|| {
                crate::error::Error::InvalidRecord(format!(
                    "Protobuf message {} not found in descriptor set",
                    schema.name
                ))
            })?;
        Ok(Self { message })
    }

    pub(crate) fn parse_json(&self, data: Bytes) -> Result<Value> {
        let message = prost_reflect::DynamicMessage::decode(self.message.clone(), data)
            .map_err(|err| crate::error::Error::InvalidRecord(err.to_string()))?;
        serde_json::to_value(&message)
            .map_err(|err| crate::error::Error::InvalidRecord(err.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::super::typed::SchemaTypedParser;
    use super::*;
    use bytes::Bytes;
    use prost_reflect::prost::Message as _;
    use prost_reflect::prost_types::{
        DescriptorProto, FieldDescriptorProto, FileDescriptorProto, FileDescriptorSet,
        field_descriptor_proto,
    };
    use prost_reflect::{DynamicMessage, Value as ProstValue};
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct TestMessage {
        value: i32,
    }

    fn build_schema() -> Schema {
        let file = FileDescriptorProto {
            name: Some("test.proto".to_string()),
            package: Some("test".to_string()),
            message_type: vec![DescriptorProto {
                name: Some("TestMessage".to_string()),
                field: vec![FieldDescriptorProto {
                    name: Some("value".to_string()),
                    number: Some(1),
                    label: Some(field_descriptor_proto::Label::Optional as i32),
                    r#type: Some(field_descriptor_proto::Type::Int32 as i32),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        let set = FileDescriptorSet { file: vec![file] };
        let schema_data = Bytes::from(set.encode_to_vec());
        Schema {
            id: 1,
            name: "test.TestMessage".into(),
            encoding: "protobuf".into(),
            data: schema_data,
        }
    }

    fn build_payload() -> Bytes {
        let pool = prost_reflect::DescriptorPool::decode(build_schema().data.clone())
            .expect("descriptor pool");
        let message = pool
            .get_message_by_name("test.TestMessage")
            .expect("message");
        let mut dynamic = DynamicMessage::new(message);
        dynamic
            .try_set_field_by_name("value", ProstValue::I32(42))
            .expect("set field");
        Bytes::from(dynamic.encode_to_vec())
    }

    #[test]
    fn protobuf_dynamic_parser_to_json() {
        let schema = build_schema();
        let parser = ProtobufSchemaParser::from_schema(&schema).expect("schema parser");
        let payload = build_payload();
        let value = parser.parse_json(payload).expect("parse json");
        assert_eq!(value.get("value").and_then(|v| v.as_i64()), Some(42));
    }

    #[test]
    fn protobuf_typed_parser_from_schema() {
        let schema = build_schema();
        let parser = SchemaTypedParser::from_schema_where(
            &schema,
            |schema| {
                schema.encoding.as_ref() == "protobuf" && schema.name.as_ref() == "test.TestMessage"
            },
            |value| {
                serde_json::from_value::<TestMessage>(value)
                    .map_err(|err| crate::error::Error::InvalidRecord(err.to_string()))
            },
        )
        .expect("typed parser");
        let payload = build_payload();
        let message = parser.parse(payload).expect("parse typed");
        assert_eq!(message, TestMessage { value: 42 });
    }

    #[test]
    fn protobuf_schema_missing_message() {
        let mut schema = build_schema();
        schema.name = "test.Missing".into();
        assert!(ProtobufSchemaParser::from_schema(&schema).is_err());
    }

    #[test]
    fn protobuf_schema_invalid_descriptor() {
        let schema = Schema {
            id: 2,
            name: "test.TestMessage".into(),
            encoding: "protobuf".into(),
            data: Bytes::from_static(b"not a descriptor set"),
        };
        assert!(ProtobufSchemaParser::from_schema(&schema).is_err());
    }

    #[test]
    fn protobuf_invalid_payload() {
        let schema = build_schema();
        let parser = ProtobufSchemaParser::from_schema(&schema).expect("schema parser");
        let payload = Bytes::from_static(b"\xff\xff\xff");
        assert!(parser.parse_json(payload).is_err());
    }
}
