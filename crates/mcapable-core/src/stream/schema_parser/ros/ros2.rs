use crate::error::Result;
use crate::types::Schema;
use bytes::Bytes;
use serde_json::Value;

pub(crate) struct Ros2SchemaParser {
    message: ros2_message::dynamic::DynamicMsg,
}

impl Ros2SchemaParser {
    pub(crate) fn from_schema(schema: &Schema) -> Result<Self> {
        let schema_text = std::str::from_utf8(schema.data.as_ref())
            .map_err(|err| crate::error::Error::InvalidRecord(err.to_string()))?;
        let message = ros2_message::dynamic::DynamicMsg::new(schema.name.as_ref(), schema_text)
            .map_err(|err| crate::error::Error::InvalidRecord(err.to_string()))?;
        Ok(Self { message })
    }

    pub(crate) fn parse_json(&self, data: Bytes) -> Result<Value> {
        let cursor = std::io::Cursor::new(data.as_ref());
        let decoded = self
            .message
            .decode(cursor)
            .map_err(|err| crate::error::Error::InvalidRecord(err.to_string()))?;
        Ok(ros2_map_to_json(&decoded))
    }
}

fn ros2_map_to_json(map: &ros2_message::MessageValue) -> Value {
    let mut json = serde_json::Map::new();
    for (key, value) in map {
        json.insert(key.to_owned(), ros2_value_to_json(value));
    }
    Value::Object(json)
}

fn ros2_value_to_json(value: &ros2_message::Value) -> Value {
    match value {
        ros2_message::Value::Bool(value) => Value::Bool(*value),
        ros2_message::Value::I8(value) => Value::Number((*value as i64).into()),
        ros2_message::Value::I16(value) => Value::Number((*value as i64).into()),
        ros2_message::Value::I32(value) => Value::Number((*value as i64).into()),
        ros2_message::Value::I64(value) => Value::Number((*value).into()),
        ros2_message::Value::U8(value) => Value::Number((*value as u64).into()),
        ros2_message::Value::U16(value) => Value::Number((*value as u64).into()),
        ros2_message::Value::U32(value) => Value::Number((*value as u64).into()),
        ros2_message::Value::U64(value) => Value::Number((*value).into()),
        ros2_message::Value::F32(value) => serde_json::Number::from_f64(*value as f64)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        ros2_message::Value::F64(value) => serde_json::Number::from_f64(*value)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        ros2_message::Value::String(value) => Value::String(value.clone()),
        ros2_message::Value::Time(value) => {
            let mut map = serde_json::Map::new();
            map.insert("sec".to_owned(), Value::Number(value.sec.into()));
            map.insert("nsec".to_owned(), Value::Number(value.nsec.into()));
            Value::Object(map)
        }
        ros2_message::Value::Duration(value) => {
            let mut map = serde_json::Map::new();
            map.insert("sec".to_owned(), Value::Number((value.sec as i64).into()));
            map.insert("nsec".to_owned(), Value::Number((value.nsec as i64).into()));
            Value::Object(map)
        }
        ros2_message::Value::Array(items) => {
            Value::Array(items.iter().map(ros2_value_to_json).collect())
        }
        ros2_message::Value::Message(message) => ros2_map_to_json(message),
    }
}
