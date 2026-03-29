use crate::error::Result;
use crate::types::Schema;
use bytes::Bytes;
use serde_json::Value;

mod flatbuffer;
mod idl;
mod protobuf;
mod ros;
mod typed;

pub use typed::SchemaTypedParser;

pub(crate) enum SchemaParser {
    Protobuf(protobuf::ProtobufSchemaParser),
    Flatbuffer(flatbuffer::FlatbufferSchemaParser),
    Ros1(ros::Ros1SchemaParser),
    Ros2(ros::Ros2SchemaParser),
    Ros2Idl(idl::IdlSchemaParser),
    OmgIdl(idl::IdlSchemaParser),
}

impl SchemaParser {
    pub(crate) fn from_schema(schema: &Schema) -> Result<Self> {
        match schema.encoding.as_ref() {
            "protobuf" => Ok(Self::Protobuf(protobuf::ProtobufSchemaParser::from_schema(
                schema,
            )?)),
            "flatbuffer" => Ok(Self::Flatbuffer(
                flatbuffer::FlatbufferSchemaParser::from_schema(schema)?,
            )),
            "ros1msg" => Ok(Self::Ros1(ros::Ros1SchemaParser::from_schema(schema)?)),
            "ros2msg" => Ok(Self::Ros2(ros::Ros2SchemaParser::from_schema(schema)?)),
            "ros2idl" => Ok(Self::Ros2Idl(idl::IdlSchemaParser::from_schema(schema)?)),
            "omgidl" => Ok(Self::OmgIdl(idl::IdlSchemaParser::from_schema(schema)?)),
            _ => Err(crate::error::Error::InvalidRecord(format!(
                "Unsupported schema encoding for parser: {}",
                schema.encoding
            ))),
        }
    }

    pub(crate) fn parse_json(&self, data: Bytes) -> Result<Value> {
        match self {
            SchemaParser::Protobuf(parser) => parser.parse_json(data),
            SchemaParser::Flatbuffer(parser) => parser.parse_json(data),
            SchemaParser::Ros1(parser) => parser.parse_json(data),
            SchemaParser::Ros2(parser) => parser.parse_json(data),
            SchemaParser::Ros2Idl(parser) => parser.parse_json(data),
            SchemaParser::OmgIdl(parser) => parser.parse_json(data),
        }
    }
}
