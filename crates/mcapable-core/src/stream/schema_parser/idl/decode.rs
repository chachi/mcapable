use super::{IdlRegistry, IdlStruct, IdlType, ResolvedType};
use serde::Deserialize;
use serde::de::{DeserializeSeed, SeqAccess, Visitor};
use serde_json::Value;
use std::fmt;

pub(super) struct IdlStructSeed<'a> {
    pub(super) struct_def: &'a IdlStruct,
    pub(super) registry: &'a IdlRegistry,
}

impl<'de> DeserializeSeed<'de> for IdlStructSeed<'_> {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> std::result::Result<Self::Value, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        deserializer.deserialize_tuple(
            self.struct_def.fields.len(),
            StructVisitor {
                struct_def: self.struct_def,
                registry: self.registry,
            },
        )
    }
}

struct StructVisitor<'a> {
    struct_def: &'a IdlStruct,
    registry: &'a IdlRegistry,
}

impl<'de> Visitor<'de> for StructVisitor<'_> {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("IDL struct")
    }

    fn visit_seq<A>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut map = serde_json::Map::new();
        for field in &self.struct_def.fields {
            let value = seq
                .next_element_seed(IdlTypeSeed {
                    ty: &field.ty,
                    registry: self.registry,
                })?
                .ok_or_else(|| serde::de::Error::custom("missing struct field"))?;
            map.insert(field.name.clone(), value);
        }
        Ok(Value::Object(map))
    }
}

struct IdlTypeSeed<'a> {
    ty: &'a IdlType,
    registry: &'a IdlRegistry,
}

impl<'de> DeserializeSeed<'de> for IdlTypeSeed<'_> {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> std::result::Result<Self::Value, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        match self.ty {
            IdlType::Bool => bool::deserialize(deserializer).map(Value::Bool),
            IdlType::I8 => i8::deserialize(deserializer).map(|v| Value::Number((v as i64).into())),
            IdlType::U8 => u8::deserialize(deserializer).map(|v| Value::Number((v as u64).into())),
            IdlType::I16 => {
                i16::deserialize(deserializer).map(|v| Value::Number((v as i64).into()))
            }
            IdlType::U16 => {
                u16::deserialize(deserializer).map(|v| Value::Number((v as u64).into()))
            }
            IdlType::I32 => {
                i32::deserialize(deserializer).map(|v| Value::Number((v as i64).into()))
            }
            IdlType::U32 => {
                u32::deserialize(deserializer).map(|v| Value::Number((v as u64).into()))
            }
            IdlType::I64 => i64::deserialize(deserializer).map(|v| Value::Number(v.into())),
            IdlType::U64 => u64::deserialize(deserializer).map(|v| Value::Number(v.into())),
            IdlType::F32 => f32::deserialize(deserializer).map(number_from_f32),
            IdlType::F64 => f64::deserialize(deserializer).map(number_from_f64),
            IdlType::String | IdlType::WString => {
                String::deserialize(deserializer).map(Value::String)
            }
            IdlType::Sequence(inner) => IdlSeqSeed {
                element: inner,
                registry: self.registry,
                len: None,
            }
            .deserialize(deserializer),
            IdlType::Array(inner, len) => IdlSeqSeed {
                element: inner,
                registry: self.registry,
                len: Some(*len),
            }
            .deserialize(deserializer),
            IdlType::Struct(name) => match self.registry.resolve_named(name) {
                Some(ResolvedType::Alias(target)) => IdlTypeSeed {
                    ty: target,
                    registry: self.registry,
                }
                .deserialize(deserializer),
                Some(ResolvedType::Struct(struct_def)) => IdlStructSeed {
                    struct_def,
                    registry: self.registry,
                }
                .deserialize(deserializer),
                None => Err(serde::de::Error::custom(format!("Unknown IDL type {name}"))),
            },
        }
    }
}

struct IdlSeqSeed<'a> {
    element: &'a IdlType,
    registry: &'a IdlRegistry,
    len: Option<usize>,
}

impl<'de> DeserializeSeed<'de> for IdlSeqSeed<'_> {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> std::result::Result<Self::Value, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        if let Some(len) = self.len {
            deserializer.deserialize_tuple(
                len,
                SeqVisitor {
                    element: self.element,
                    registry: self.registry,
                    len: Some(len),
                },
            )
        } else {
            deserializer.deserialize_seq(SeqVisitor {
                element: self.element,
                registry: self.registry,
                len: None,
            })
        }
    }
}

struct SeqVisitor<'a> {
    element: &'a IdlType,
    registry: &'a IdlRegistry,
    len: Option<usize>,
}

impl<'de> Visitor<'de> for SeqVisitor<'_> {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("IDL sequence")
    }

    fn visit_seq<A>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut items = Vec::new();
        if let Some(len) = self.len {
            for _ in 0..len {
                let value = seq
                    .next_element_seed(IdlTypeSeed {
                        ty: self.element,
                        registry: self.registry,
                    })?
                    .ok_or_else(|| serde::de::Error::custom("missing array element"))?;
                items.push(value);
            }
            return Ok(Value::Array(items));
        }

        while let Some(value) = seq.next_element_seed(IdlTypeSeed {
            ty: self.element,
            registry: self.registry,
        })? {
            items.push(value);
        }
        Ok(Value::Array(items))
    }
}

fn number_from_f32(value: f32) -> Value {
    serde_json::Number::from_f64(value as f64)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

fn number_from_f64(value: f64) -> Value {
    serde_json::Number::from_f64(value)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}
