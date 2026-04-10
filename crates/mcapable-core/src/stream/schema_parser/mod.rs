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

/// Schema-aware message parser that decodes binary message data to JSON.
///
/// Supports protobuf, flatbuffer, ros1msg, ros2msg, ros2idl, and omgidl encodings.
/// Create one via [`SchemaParser::from_schema`], then call [`SchemaParser::parse_json`]
/// to decode message payloads.
#[allow(private_interfaces)]
#[non_exhaustive]
pub enum SchemaParser {
    /// Protobuf message decoder.
    Protobuf(protobuf::ProtobufSchemaParser),
    /// FlatBuffers message decoder.
    Flatbuffer(flatbuffer::FlatbufferSchemaParser),
    /// ROS1 message decoder.
    Ros1(ros::Ros1SchemaParser),
    /// ROS2 message decoder.
    Ros2(ros::Ros2SchemaParser),
    /// ROS2 IDL message decoder.
    Ros2Idl(idl::IdlSchemaParser),
    /// OMG IDL message decoder.
    OmgIdl(idl::IdlSchemaParser),
}

impl SchemaParser {
    pub fn from_schema(schema: &Schema) -> Result<Self> {
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

    pub fn parse_json(&self, data: Bytes) -> Result<Value> {
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
