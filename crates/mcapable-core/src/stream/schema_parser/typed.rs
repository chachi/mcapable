use super::SchemaParser;
use crate::error::Result;
use crate::types::Schema;
use bytes::Bytes;

/// Schema-aware parser that maps decoded JSON into a typed output.
pub struct SchemaTypedParser<T> {
    parser: SchemaParser,
    map: Box<dyn Fn(serde_json::Value) -> Result<T>>,
}

impl<T> SchemaTypedParser<T> {
    /// Build a typed parser using the provided schema.
    pub fn from_schema<F>(schema: &Schema, map: F) -> Result<Self>
    where
        F: Fn(serde_json::Value) -> Result<T> + 'static,
    {
        Ok(Self {
            parser: SchemaParser::from_schema(schema)?,
            map: Box::new(map),
        })
    }

    /// Build a typed parser when `predicate` returns true for the schema.
    pub fn from_schema_where<F, P>(schema: &Schema, predicate: P, map: F) -> Result<Self>
    where
        F: Fn(serde_json::Value) -> Result<T> + 'static,
        P: Fn(&Schema) -> bool,
    {
        if !predicate(schema) {
            return Err(crate::error::Error::InvalidRecord(
                "Schema predicate rejected schema".to_owned(),
            ));
        }
        Self::from_schema(schema, map)
    }

    /// Parse raw bytes into a typed value using the compiled schema parser.
    pub fn parse(&self, data: Bytes) -> Result<T> {
        let value = self.parser.parse_json(data)?;
        (self.map)(value)
    }
}

#[cfg(test)]
mod tests {
    use super::SchemaTypedParser;
    use bytes::Bytes;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct TestMessage {
        value: i32,
    }

    #[test]
    fn typed_parser_rejects_mismatched_schema() {
        let schema = crate::types::Schema {
            id: 10,
            name: "Test".into(),
            encoding: "jsonschema".into(),
            data: Bytes::from_static(b"{}"),
        };
        let result = SchemaTypedParser::from_schema_where(
            &schema,
            |schema| schema.encoding.as_ref() == "protobuf",
            |_| Ok(TestMessage { value: 0 }),
        );
        assert!(result.is_err());
    }
}
