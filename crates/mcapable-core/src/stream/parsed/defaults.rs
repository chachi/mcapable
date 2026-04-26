use super::super::{schema_defaults, schema_parser::SchemaParser};
use crate::error::Result;
use crate::support::HashMap;
use crate::types::Schema;
use bytes::Bytes;
use std::collections::hash_map::Entry;

pub(super) struct DefaultParsers<'a, T> {
    json_mapper: Box<dyn Fn(serde_json::Value) -> Result<T> + 'a>,
    bytes_mapper: Box<dyn Fn(Bytes) -> Result<T> + 'a>,
    schema_parsers: HashMap<u16, SchemaParser>,
}

impl<'a, T> DefaultParsers<'a, T> {
    pub(super) fn new<F, G>(json_mapper: F, bytes_mapper: G) -> Self
    where
        F: Fn(serde_json::Value) -> Result<T> + 'a,
        G: Fn(Bytes) -> Result<T> + 'a,
    {
        Self {
            json_mapper: Box::new(json_mapper),
            bytes_mapper: Box::new(bytes_mapper),
            schema_parsers: HashMap::new(),
        }
    }

    pub(super) fn parse(
        &mut self,
        channel: &crate::types::Channel,
        schema: Option<&Schema>,
        data: Bytes,
    ) -> Result<T> {
        if matches!(
            channel.message_encoding.as_ref(),
            "json" | "cbor" | "msgpack"
        ) || matches!(
            schema_defaults::default_schema_parser_for(schema),
            Some(schema_defaults::SchemaDefaultParser::Json)
        ) {
            let value = serde_json::from_slice(data.as_ref())
                .or_else(|_| serde_cbor::from_slice(data.as_ref()))
                .or_else(|_| rmp_serde::from_slice(data.as_ref()))
                .map_err(|err| crate::error::Error::InvalidRecord(err.to_string()))?;
            return (self.json_mapper)(value);
        }

        if let Some(schema) = schema
            && matches!(
                schema.encoding.as_ref(),
                "protobuf" | "flatbuffer" | "ros1msg" | "ros2msg" | "ros2idl" | "omgidl"
            )
        {
            let parser = match self.schema_parsers.entry(schema.id) {
                Entry::Occupied(entry) => entry.into_mut(),
                Entry::Vacant(entry) => {
                    let parser = SchemaParser::from_schema(schema)?;
                    entry.insert(parser)
                }
            };
            let value = parser.parse_json(data)?;
            return (self.json_mapper)(value);
        }

        (self.bytes_mapper)(data)
    }
}
