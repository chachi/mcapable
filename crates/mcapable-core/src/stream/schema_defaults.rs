use crate::types::Schema;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SchemaDefaultParser {
    Json,
    Bytes,
}

pub(crate) const REGISTRY_SCHEMA_ENCODINGS: [&str; 8] = [
    "",
    "protobuf",
    "flatbuffer",
    "ros1msg",
    "ros2msg",
    "ros2idl",
    "omgidl",
    "jsonschema",
];

pub(crate) fn is_registry_schema_encoding(encoding: &str) -> bool {
    REGISTRY_SCHEMA_ENCODINGS.contains(&encoding)
}

pub(crate) fn default_schema_parser(encoding: &str) -> Option<SchemaDefaultParser> {
    if !is_registry_schema_encoding(encoding) {
        return None;
    }
    if encoding == "jsonschema" {
        Some(SchemaDefaultParser::Json)
    } else {
        Some(SchemaDefaultParser::Bytes)
    }
}

pub(crate) fn default_schema_parser_for(schema: Option<&Schema>) -> Option<SchemaDefaultParser> {
    schema.and_then(|schema| default_schema_parser(schema.encoding.as_ref()))
}
