use crate::error::Result;
use crate::support::HashMap;
use crate::types::Schema;
use bytes::Bytes;
use serde_json::Value;

pub(crate) struct Ros1SchemaParser {
    root_type: String,
    messages: HashMap<String, RosMessage>,
}

impl Ros1SchemaParser {
    pub(crate) fn from_schema(schema: &Schema) -> Result<Self> {
        let schema_text = std::str::from_utf8(schema.data.as_ref())
            .map_err(|err| crate::error::Error::InvalidRecord(err.to_string()))?;
        let root_type = normalize_type_name(schema.name.as_ref(), schema.name.as_ref());
        let messages = parse_ros1_definitions(schema_text, schema.name.as_ref())?;

        Ok(Self {
            root_type,
            messages,
        })
    }

    pub(crate) fn parse_json(&self, data: Bytes) -> Result<Value> {
        let mut reader = RosReader::new(data.as_ref());
        let value = parse_ros1_message(&self.messages, &self.root_type, &mut reader)?;
        Ok(value)
    }
}

#[derive(Debug, Clone)]
struct RosMessage {
    fields: Vec<RosField>,
}

#[derive(Debug, Clone)]
struct RosField {
    name: String,
    ty: RosType,
    array: RosArray,
}

#[derive(Debug, Clone)]
enum RosArray {
    None,
    Fixed(usize),
    Variable,
}

#[derive(Debug, Clone)]
enum RosType {
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
    Time,
    Duration,
    Named(String),
}

fn parse_ros1_definitions(
    schema_text: &str,
    root_name: &str,
) -> Result<HashMap<String, RosMessage>> {
    let mut messages = HashMap::default();
    let root_pkg = root_name.split('/').next().unwrap_or(root_name);

    for block in schema_text.split(
        "\n================================================================================\n",
    ) {
        let mut lines = block.lines();
        let mut name = root_name.to_owned();
        let mut field_lines = Vec::new();

        if let Some(first) = lines.next() {
            if let Some(msg_name) = first.strip_prefix("MSG: ") {
                name = msg_name.trim().to_owned();
            } else {
                field_lines.push(first);
            }
        }
        field_lines.extend(lines);

        let mut fields = Vec::new();
        for line in field_lines {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() || line.contains('=') {
                continue;
            }
            let mut parts = line.split_whitespace();
            let ty_raw = parts.next().ok_or_else(|| {
                crate::error::Error::InvalidRecord("Missing field type".to_owned())
            })?;
            let name = parts.next().ok_or_else(|| {
                crate::error::Error::InvalidRecord("Missing field name".to_owned())
            })?;

            let (ty_name, array) = parse_ros1_array(ty_raw)?;
            let ty = parse_ros1_type(ty_name, root_pkg);

            fields.push(RosField {
                name: name.to_owned(),
                ty,
                array,
            });
        }

        let full_name = normalize_type_name(&name, root_pkg);
        messages.insert(full_name, RosMessage { fields });
    }

    Ok(messages)
}

fn parse_ros1_array(type_name: &str) -> Result<(&str, RosArray)> {
    if let Some(start) = type_name.find('[') {
        let end = type_name[start..]
            .find(']')
            .map(|offset| start + offset)
            .ok_or_else(|| {
                crate::error::Error::InvalidRecord("Invalid ROS array type".to_owned())
            })?;
        let base = &type_name[..start];
        let inner = &type_name[start + 1..end];
        if inner.is_empty() {
            return Ok((base, RosArray::Variable));
        }
        let len = inner
            .parse::<usize>()
            .map_err(|err| crate::error::Error::InvalidRecord(err.to_string()))?;
        return Ok((base, RosArray::Fixed(len)));
    }
    Ok((type_name, RosArray::None))
}

fn parse_ros1_type(type_name: &str, root_pkg: &str) -> RosType {
    match type_name {
        "bool" => RosType::Bool,
        "int8" | "char" => RosType::I8,
        "uint8" | "byte" => RosType::U8,
        "int16" => RosType::I16,
        "uint16" => RosType::U16,
        "int32" => RosType::I32,
        "uint32" => RosType::U32,
        "int64" => RosType::I64,
        "uint64" => RosType::U64,
        "float32" => RosType::F32,
        "float64" => RosType::F64,
        "string" => RosType::String,
        "time" => RosType::Time,
        "duration" => RosType::Duration,
        _ => RosType::Named(normalize_type_name(type_name, root_pkg)),
    }
}

fn normalize_type_name(type_name: &str, root_pkg: &str) -> String {
    if type_name.contains('/') || root_pkg.is_empty() {
        type_name.to_owned()
    } else {
        format!("{root_pkg}/{type_name}")
    }
}

fn parse_ros1_message(
    messages: &HashMap<String, RosMessage>,
    name: &str,
    reader: &mut RosReader<'_>,
) -> Result<Value> {
    let message = messages.get(name).ok_or_else(|| {
        crate::error::Error::InvalidRecord(format!("ROS1 schema missing message {name}"))
    })?;
    let mut map = serde_json::Map::new();

    for field in &message.fields {
        let value = parse_ros1_field(messages, field, reader)?;
        map.insert(field.name.clone(), value);
    }

    Ok(Value::Object(map))
}

fn parse_ros1_field(
    messages: &HashMap<String, RosMessage>,
    field: &RosField,
    reader: &mut RosReader<'_>,
) -> Result<Value> {
    let parse_item = |reader: &mut RosReader<'_>| match &field.ty {
        RosType::Bool => Ok(Value::Bool(reader.read_bool()?)),
        RosType::I8 => Ok(Value::Number((reader.read_i8()? as i64).into())),
        RosType::U8 => Ok(Value::Number((reader.read_u8()? as u64).into())),
        RosType::I16 => Ok(Value::Number((reader.read_i16()? as i64).into())),
        RosType::U16 => Ok(Value::Number((reader.read_u16()? as u64).into())),
        RosType::I32 => Ok(Value::Number((reader.read_i32()? as i64).into())),
        RosType::U32 => Ok(Value::Number((reader.read_u32()? as u64).into())),
        RosType::I64 => Ok(Value::Number(reader.read_i64()?.into())),
        RosType::U64 => Ok(Value::Number(reader.read_u64()?.into())),
        RosType::F32 => Ok(reader.read_f32()?.map_or(Value::Null, |v| {
            serde_json::Number::from_f64(v as f64)
                .map(Value::Number)
                .unwrap_or(Value::Null)
        })),
        RosType::F64 => Ok(reader.read_f64()?.map_or(Value::Null, |v| {
            serde_json::Number::from_f64(v)
                .map(Value::Number)
                .unwrap_or(Value::Null)
        })),
        RosType::String => Ok(Value::String(reader.read_string()?)),
        RosType::Time | RosType::Duration => {
            let secs = reader.read_u32()?;
            let nsecs = reader.read_u32()?;
            let mut map = serde_json::Map::new();
            map.insert("secs".to_owned(), Value::Number(secs.into()));
            map.insert("nsecs".to_owned(), Value::Number(nsecs.into()));
            Ok(Value::Object(map))
        }
        RosType::Named(name) => parse_ros1_message(messages, name, reader),
    };

    match field.array {
        RosArray::None => parse_item(reader),
        RosArray::Fixed(len) => {
            let mut items = Vec::with_capacity(len);
            for _ in 0..len {
                items.push(parse_item(reader)?);
            }
            Ok(Value::Array(items))
        }
        RosArray::Variable => {
            let len = reader.read_u32()? as usize;
            let mut items = Vec::with_capacity(len);
            for _ in 0..len {
                items.push(parse_item(reader)?);
            }
            Ok(Value::Array(items))
        }
    }
}

struct RosReader<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> RosReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn read_exact<const N: usize>(&mut self) -> Result<[u8; N]> {
        if self.offset + N > self.data.len() {
            return Err(crate::error::Error::InvalidRecord(
                "Unexpected end of ROS message".to_owned(),
            ));
        }
        let mut buf = [0u8; N];
        buf.copy_from_slice(&self.data[self.offset..self.offset + N]);
        self.offset += N;
        Ok(buf)
    }

    fn read_u8(&mut self) -> Result<u8> {
        Ok(self.read_exact::<1>()?[0])
    }

    fn read_bool(&mut self) -> Result<bool> {
        Ok(self.read_u8()? != 0)
    }

    fn read_i8(&mut self) -> Result<i8> {
        Ok(self.read_u8()? as i8)
    }

    fn read_u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.read_exact::<2>()?))
    }

    fn read_i16(&mut self) -> Result<i16> {
        Ok(i16::from_le_bytes(self.read_exact::<2>()?))
    }

    fn read_u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.read_exact::<4>()?))
    }

    fn read_i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.read_exact::<4>()?))
    }

    fn read_u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.read_exact::<8>()?))
    }

    fn read_i64(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(self.read_exact::<8>()?))
    }

    fn read_f32(&mut self) -> Result<Option<f32>> {
        let value = f32::from_le_bytes(self.read_exact::<4>()?);
        if value.is_finite() {
            Ok(Some(value))
        } else {
            Ok(None)
        }
    }

    fn read_f64(&mut self) -> Result<Option<f64>> {
        let value = f64::from_le_bytes(self.read_exact::<8>()?);
        if value.is_finite() {
            Ok(Some(value))
        } else {
            Ok(None)
        }
    }

    fn read_string(&mut self) -> Result<String> {
        let len = self.read_u32()? as usize;
        if self.offset + len > self.data.len() {
            return Err(crate::error::Error::InvalidRecord(
                "Unexpected end of ROS string".to_owned(),
            ));
        }
        let bytes = &self.data[self.offset..self.offset + len];
        self.offset += len;
        std::str::from_utf8(bytes)
            .map(|s| s.to_owned())
            .map_err(|err| crate::error::Error::InvalidRecord(err.to_string()))
    }
}
