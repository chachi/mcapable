use crate::error::Result;
use crate::support::HashMap;
use crate::types::Schema;
use serde::de::DeserializeSeed;
use serde_json::Value;

mod decode;
mod parse;

use decode::IdlStructSeed;
use parse::{collect_definitions, normalize_root_name};

pub(crate) struct IdlSchemaParser {
    root: String,
    registry: IdlRegistry,
}

impl IdlSchemaParser {
    pub(crate) fn from_schema(schema: &Schema) -> Result<Self> {
        let schema_text = std::str::from_utf8(schema.data.as_ref())
            .map_err(|err| crate::error::Error::InvalidRecord(err.to_string()))?;
        let defs = t4_idl_parser::parse(schema_text)
            .map_err(|err| crate::error::Error::InvalidRecord(err.to_string()))?;

        let mut registry = IdlRegistry::default();
        collect_definitions(&defs, &mut Vec::new(), &mut registry)?;

        let root = normalize_root_name(schema.name.as_ref());
        let root = registry.find_struct_name(&root).cloned().unwrap_or(root);

        Ok(Self { root, registry })
    }

    pub(crate) fn parse_json(&self, data: bytes::Bytes) -> Result<Value> {
        use byteorder::LittleEndian;
        let mut deserializer = cdr_encoding::CdrDeserializer::<LittleEndian>::new(data.as_ref());
        let struct_def = self.registry.find_struct(&self.root).ok_or_else(|| {
            crate::error::Error::InvalidRecord(format!(
                "IDL root struct {} not found in schema",
                self.root
            ))
        })?;
        let value = IdlStructSeed {
            struct_def,
            registry: &self.registry,
        }
        .deserialize(&mut deserializer)
        .map_err(|err| crate::error::Error::InvalidRecord(err.to_string()))?;
        Ok(value)
    }
}

#[derive(Default)]
struct IdlRegistry {
    structs: HashMap<String, IdlStruct>,
    aliases: HashMap<String, IdlType>,
}

impl IdlRegistry {
    fn find_struct(&self, name: &str) -> Option<&IdlStruct> {
        self.structs.get(name)
    }

    fn find_struct_name(&self, name: &str) -> Option<&String> {
        self.structs.keys().find(|key| {
            **key == name || key.replace("::", "/") == name || key.replace("/", "::") == name
        })
    }

    fn resolve_named(&self, name: &str) -> Option<ResolvedType<'_>> {
        if let Some(alias) = self.aliases.get(name) {
            return Some(ResolvedType::Alias(alias));
        }
        if let Some(struct_def) = self.structs.get(name) {
            return Some(ResolvedType::Struct(struct_def));
        }
        self.find_struct_name(name)
            .and_then(|name| self.structs.get(name))
            .map(ResolvedType::Struct)
    }
}

enum ResolvedType<'a> {
    Alias(&'a IdlType),
    Struct(&'a IdlStruct),
}

#[derive(Clone, Debug)]
struct IdlStruct {
    fields: Vec<IdlField>,
}

#[derive(Clone, Debug)]
struct IdlField {
    name: String,
    ty: IdlType,
}

#[derive(Clone, Debug)]
enum IdlType {
    Bool,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    F32,
    F64,
    String,
    WString,
    Struct(String),
    Sequence(Box<IdlType>),
    Array(Box<IdlType>, usize),
}
#[cfg(test)]
mod tests {
    use super::super::typed::SchemaTypedParser;
    use super::*;
    use crate::types::Schema;
    use bytes::Bytes;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct TestMsg {
        value: i32,
    }

    #[test]
    fn ros2idl_dynamic_parser_to_json() {
        let schema = Schema {
            id: 1,
            name: "test_pkg/msg/TestMsg".into(),
            encoding: "ros2idl".into(),
            data: Bytes::from_static(
                b"module test_pkg { module msg { struct TestMsg { long value; }; }; };",
            ),
        };
        let parser = IdlSchemaParser::from_schema(&schema).expect("schema parser");
        let payload = Bytes::from(42i32.to_le_bytes().to_vec());
        let value = parser.parse_json(payload).expect("parse json");
        assert_eq!(value.get("value").and_then(|v| v.as_i64()), Some(42));
    }

    #[test]
    fn ros2idl_typed_parser_from_schema() {
        let schema = Schema {
            id: 2,
            name: "test_pkg/msg/TestMsg".into(),
            encoding: "ros2idl".into(),
            data: Bytes::from_static(
                b"module test_pkg { module msg { struct TestMsg { long value; }; }; };",
            ),
        };
        let parser = SchemaTypedParser::from_schema_where(
            &schema,
            |schema| schema.encoding.as_ref() == "ros2idl",
            |value| {
                serde_json::from_value::<TestMsg>(value)
                    .map_err(|err| crate::error::Error::InvalidRecord(err.to_string()))
            },
        )
        .expect("typed parser");
        let payload = Bytes::from(42i32.to_le_bytes().to_vec());
        let value = parser.parse(payload).expect("parse typed");
        assert_eq!(value, TestMsg { value: 42 });
    }

    #[test]
    fn omgidl_dynamic_parser_to_json() {
        let schema = Schema {
            id: 3,
            name: "TestMsg".into(),
            encoding: "omgidl".into(),
            data: Bytes::from_static(b"struct TestMsg { long value; };"),
        };
        let parser = IdlSchemaParser::from_schema(&schema).expect("schema parser");
        let payload = Bytes::from(7i32.to_le_bytes().to_vec());
        let value = parser.parse_json(payload).expect("parse json");
        assert_eq!(value.get("value").and_then(|v| v.as_i64()), Some(7));
    }

    #[test]
    fn omgidl_typed_parser_from_schema() {
        let schema = Schema {
            id: 4,
            name: "TestMsg".into(),
            encoding: "omgidl".into(),
            data: Bytes::from_static(b"struct TestMsg { long value; };"),
        };
        let parser = SchemaTypedParser::from_schema_where(
            &schema,
            |schema| schema.encoding.as_ref() == "omgidl",
            |value| {
                serde_json::from_value::<TestMsg>(value)
                    .map_err(|err| crate::error::Error::InvalidRecord(err.to_string()))
            },
        )
        .expect("typed parser");
        let payload = Bytes::from(7i32.to_le_bytes().to_vec());
        let value = parser.parse(payload).expect("parse typed");
        assert_eq!(value, TestMsg { value: 7 });
    }

    #[test]
    fn idl_invalid_schema() {
        let schema = Schema {
            id: 5,
            name: "TestMsg".into(),
            encoding: "omgidl".into(),
            data: Bytes::from_static(b"struct TestMsg {"),
        };
        assert!(IdlSchemaParser::from_schema(&schema).is_err());
    }

    #[test]
    fn idl_missing_root_struct() {
        let schema = Schema {
            id: 6,
            name: "MissingMsg".into(),
            encoding: "omgidl".into(),
            data: Bytes::from_static(b"struct TestMsg { long value; };"),
        };
        let parser = IdlSchemaParser::from_schema(&schema).expect("schema parser");
        let payload = Bytes::from(1i32.to_le_bytes().to_vec());
        assert!(parser.parse_json(payload).is_err());
    }
}
