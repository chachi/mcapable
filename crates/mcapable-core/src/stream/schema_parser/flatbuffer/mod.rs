use crate::error::Result;
use crate::types::Schema;
use bytes::Bytes;
use serde_json::Value;

mod decode;

use decode::parse_flatbuffer_table;

pub(crate) struct FlatbufferSchemaParser {
    schema_bytes: Bytes,
    root_name: String,
}

impl FlatbufferSchemaParser {
    pub(crate) fn from_schema(schema: &Schema) -> Result<Self> {
        Ok(Self {
            schema_bytes: schema.data.clone(),
            root_name: schema.name.as_ref().to_owned(),
        })
    }

    pub(crate) fn parse_json(&self, data: Bytes) -> Result<Value> {
        let schema = flatbuffers_reflection::reflection::root_as_schema(self.schema_bytes.as_ref())
            .map_err(|err| crate::error::Error::InvalidRecord(err.to_string()))?;
        flatbuffers_reflection::SafeBuffer::new(data.as_ref(), &schema)
            .map_err(|err| crate::error::Error::InvalidRecord(err.to_string()))?;

        let root_object = schema
            .objects()
            .lookup_by_key(self.root_name.as_str(), |object, key| {
                object.key_compare_with_value(key)
            })
            .or_else(|| schema.root_table())
            .ok_or_else(|| {
                crate::error::Error::InvalidRecord(format!(
                    "Flatbuffer root object {} not found in schema",
                    self.root_name
                ))
            })?;

        let root_table = unsafe { flatbuffers_reflection::get_any_root(data.as_ref()) };
        parse_flatbuffer_table(&schema, &root_object, &root_table)
    }
}
#[cfg(test)]
mod tests {
    use super::super::typed::SchemaTypedParser;
    use super::*;
    use crate::types::Schema;
    use bytes::Bytes;
    use flatbuffers::FlatBufferBuilder;
    use flatbuffers_reflection::reflection;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct EmptyObject {}

    fn build_schema() -> Schema {
        let mut schema_builder = FlatBufferBuilder::new();
        let name = schema_builder.create_string("TestMessage");
        let fields =
            schema_builder.create_vector::<flatbuffers::ForwardsUOffset<reflection::Field>>(&[]);
        let object = reflection::Object::create(
            &mut schema_builder,
            &reflection::ObjectArgs {
                name: Some(name),
                fields: Some(fields),
                ..Default::default()
            },
        );
        let objects = schema_builder.create_vector(&[object]);
        let enums =
            schema_builder.create_vector::<flatbuffers::ForwardsUOffset<reflection::Enum>>(&[]);
        let schema = reflection::Schema::create(
            &mut schema_builder,
            &reflection::SchemaArgs {
                objects: Some(objects),
                enums: Some(enums),
                root_table: Some(object),
                ..Default::default()
            },
        );
        reflection::finish_schema_buffer(&mut schema_builder, schema);
        let schema_data = Bytes::from(schema_builder.finished_data().to_vec());
        Schema {
            id: 2,
            name: "TestMessage".into(),
            encoding: "flatbuffer".into(),
            data: schema_data,
        }
    }

    fn build_payload() -> Bytes {
        let mut payload_builder = FlatBufferBuilder::new();
        let table_offset = payload_builder.start_table();
        let table = payload_builder.end_table(table_offset);
        payload_builder.finish_minimal(table);
        Bytes::from(payload_builder.finished_data().to_vec())
    }

    #[test]
    fn flatbuffer_dynamic_parser_to_json() {
        let schema = build_schema();
        let parser = FlatbufferSchemaParser::from_schema(&schema).expect("schema parser");
        let payload = build_payload();
        let value = parser.parse_json(payload).expect("parse json");
        assert!(value.as_object().is_some());
    }

    #[test]
    fn flatbuffer_typed_parser_from_schema() {
        let schema = build_schema();
        let parser = SchemaTypedParser::from_schema_where(
            &schema,
            |schema| {
                schema.encoding.as_ref() == "flatbuffer" && schema.name.as_ref() == "TestMessage"
            },
            |value| {
                serde_json::from_value::<EmptyObject>(value)
                    .map_err(|err| crate::error::Error::InvalidRecord(err.to_string()))
            },
        )
        .expect("typed parser");
        let payload = build_payload();
        let value: EmptyObject = parser.parse(payload).expect("parse typed");
        assert_eq!(value, EmptyObject {});
    }

    #[test]
    fn flatbuffer_invalid_schema_bytes() {
        let schema = Schema {
            id: 3,
            name: "MissingRoot".into(),
            encoding: "flatbuffer".into(),
            data: Bytes::from_static(b"not a flatbuffer schema"),
        };
        let parser = FlatbufferSchemaParser::from_schema(&schema).expect("schema parser");
        let payload = Bytes::from_static(b"\x00");
        assert!(parser.parse_json(payload).is_err());
    }
}
