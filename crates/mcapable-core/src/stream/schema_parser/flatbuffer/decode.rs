use crate::error::Result;
use serde_json::Value;

pub(super) fn parse_flatbuffer_table(
    schema: &flatbuffers_reflection::reflection::Schema,
    object: &flatbuffers_reflection::reflection::Object,
    table: &flatbuffers::Table<'_>,
) -> Result<Value> {
    let mut map = serde_json::Map::new();

    for field in object.fields().iter() {
        let field_value = parse_flatbuffer_field(schema, object, &field, table)?;
        if let Some(value) = field_value {
            map.insert(field.name().to_owned(), value);
        }
    }

    Ok(Value::Object(map))
}

fn parse_flatbuffer_struct(
    schema: &flatbuffers_reflection::reflection::Schema,
    object: &flatbuffers_reflection::reflection::Object,
    st: &flatbuffers_reflection::Struct<'_>,
) -> Result<Value> {
    let mut map = serde_json::Map::new();

    for field in object.fields().iter() {
        let field_value = parse_flatbuffer_struct_field(schema, object, &field, st)?;
        if let Some(value) = field_value {
            map.insert(field.name().to_owned(), value);
        }
    }

    Ok(Value::Object(map))
}

fn flatbuffer_error(err: flatbuffers_reflection::FlatbufferError) -> crate::error::Error {
    crate::error::Error::InvalidRecord(err.to_string())
}

fn parse_flatbuffer_field(
    schema: &flatbuffers_reflection::reflection::Schema,
    _object: &flatbuffers_reflection::reflection::Object,
    field: &flatbuffers_reflection::reflection::Field,
    table: &flatbuffers::Table<'_>,
) -> Result<Option<Value>> {
    use flatbuffers_reflection::reflection::BaseType;

    let base_type = field.type_().base_type();
    let value = match base_type {
        BaseType::None => return Ok(None),
        BaseType::Bool => {
            let value = unsafe { flatbuffers_reflection::get_any_field_integer(table, field) }
                .map_err(flatbuffer_error)?;
            Value::Bool(value != 0)
        }
        BaseType::Byte | BaseType::Short | BaseType::Int | BaseType::Long => {
            let value = unsafe { flatbuffers_reflection::get_any_field_integer(table, field) }
                .map_err(flatbuffer_error)?;
            Value::Number(serde_json::Number::from(value))
        }
        BaseType::UByte | BaseType::UShort | BaseType::UInt | BaseType::ULong | BaseType::UType => {
            let value = unsafe { flatbuffers_reflection::get_any_field_integer(table, field) }
                .map_err(flatbuffer_error)?;
            let unsigned = u64::try_from(value)
                .map_err(|err| crate::error::Error::InvalidRecord(err.to_string()))?;
            Value::Number(serde_json::Number::from(unsigned))
        }
        BaseType::Float | BaseType::Double => {
            let value = unsafe { flatbuffers_reflection::get_any_field_float(table, field) }
                .map_err(flatbuffer_error)?;
            serde_json::Number::from_f64(value)
                .map(Value::Number)
                .unwrap_or(Value::Null)
        }
        BaseType::String => {
            let value =
                unsafe { flatbuffers_reflection::get_any_field_string(table, field, schema) };
            Value::String(value)
        }
        BaseType::Obj => {
            let obj_index = usize::try_from(field.type_().index())
                .map_err(|err| crate::error::Error::InvalidRecord(err.to_string()))?;
            let obj = schema.objects().get(obj_index);
            if obj.is_struct() {
                let st = unsafe { flatbuffers_reflection::get_field_struct(table, field) }
                    .map_err(flatbuffer_error)?;
                match st {
                    Some(st) => parse_flatbuffer_struct(schema, &obj, &st)?,
                    None => return Ok(None),
                }
            } else {
                let nested = unsafe { flatbuffers_reflection::get_field_table(table, field) }
                    .map_err(flatbuffer_error)?;
                match nested {
                    Some(nested) => parse_flatbuffer_table(schema, &obj, &nested)?,
                    None => return Ok(None),
                }
            }
        }
        BaseType::Vector => {
            let element_type = field.type_().element();
            let values = parse_flatbuffer_vector(schema, field, table, element_type)?;
            Value::Array(values)
        }
        _ => {
            return Err(crate::error::Error::InvalidRecord(format!(
                "Unsupported flatbuffer field type {}",
                base_type.variant_name().unwrap_or("unknown")
            )));
        }
    };

    Ok(Some(value))
}

fn parse_flatbuffer_struct_field(
    schema: &flatbuffers_reflection::reflection::Schema,
    _object: &flatbuffers_reflection::reflection::Object,
    field: &flatbuffers_reflection::reflection::Field,
    st: &flatbuffers_reflection::Struct<'_>,
) -> Result<Option<Value>> {
    use flatbuffers_reflection::reflection::BaseType;

    let base_type = field.type_().base_type();
    let value = match base_type {
        BaseType::None => return Ok(None),
        BaseType::Bool => {
            let value =
                unsafe { flatbuffers_reflection::get_any_field_integer_in_struct(st, field) }
                    .map_err(flatbuffer_error)?;
            Value::Bool(value != 0)
        }
        BaseType::Byte | BaseType::Short | BaseType::Int | BaseType::Long => {
            let value =
                unsafe { flatbuffers_reflection::get_any_field_integer_in_struct(st, field) }
                    .map_err(flatbuffer_error)?;
            Value::Number(serde_json::Number::from(value))
        }
        BaseType::UByte | BaseType::UShort | BaseType::UInt | BaseType::ULong | BaseType::UType => {
            let value =
                unsafe { flatbuffers_reflection::get_any_field_integer_in_struct(st, field) }
                    .map_err(flatbuffer_error)?;
            let unsigned = u64::try_from(value)
                .map_err(|err| crate::error::Error::InvalidRecord(err.to_string()))?;
            Value::Number(serde_json::Number::from(unsigned))
        }
        BaseType::Float | BaseType::Double => {
            let value = unsafe { flatbuffers_reflection::get_any_field_float_in_struct(st, field) }
                .map_err(flatbuffer_error)?;
            serde_json::Number::from_f64(value)
                .map(Value::Number)
                .unwrap_or(Value::Null)
        }
        BaseType::String => {
            let value = unsafe {
                flatbuffers_reflection::get_any_field_string_in_struct(st, field, schema)
            };
            Value::String(value)
        }
        BaseType::Obj => {
            let obj_index = usize::try_from(field.type_().index())
                .map_err(|err| crate::error::Error::InvalidRecord(err.to_string()))?;
            let obj = schema.objects().get(obj_index);
            let nested = unsafe { flatbuffers_reflection::get_field_struct_in_struct(st, field) }
                .map_err(flatbuffer_error)?;
            parse_flatbuffer_struct(schema, &obj, &nested)?
        }
        _ => {
            return Err(crate::error::Error::InvalidRecord(format!(
                "Unsupported flatbuffer struct field type {}",
                base_type.variant_name().unwrap_or("unknown")
            )));
        }
    };

    Ok(Some(value))
}

fn parse_flatbuffer_vector(
    schema: &flatbuffers_reflection::reflection::Schema,
    field: &flatbuffers_reflection::reflection::Field,
    table: &flatbuffers::Table<'_>,
    element_type: flatbuffers_reflection::reflection::BaseType,
) -> Result<Vec<Value>> {
    use flatbuffers_reflection::reflection::BaseType;

    let mut values = Vec::new();

    match element_type {
        BaseType::Bool => {
            let vec = unsafe { flatbuffers_reflection::get_field_vector::<bool>(table, field) }
                .map_err(flatbuffer_error)?;
            if let Some(vec) = vec {
                values.extend(vec.iter().map(Value::Bool));
            }
        }
        BaseType::Byte => {
            let vec = unsafe { flatbuffers_reflection::get_field_vector::<i8>(table, field) }
                .map_err(flatbuffer_error)?;
            if let Some(vec) = vec {
                values.extend(vec.iter().map(|v| Value::Number((v as i64).into())));
            }
        }
        BaseType::UByte => {
            let vec = unsafe { flatbuffers_reflection::get_field_vector::<u8>(table, field) }
                .map_err(flatbuffer_error)?;
            if let Some(vec) = vec {
                values.extend(
                    vec.iter()
                        .map(|v| Value::Number(serde_json::Number::from(u64::from(v)))),
                );
            }
        }
        BaseType::Short => {
            let vec = unsafe { flatbuffers_reflection::get_field_vector::<i16>(table, field) }
                .map_err(flatbuffer_error)?;
            if let Some(vec) = vec {
                values.extend(vec.iter().map(|v| Value::Number((v as i64).into())));
            }
        }
        BaseType::UShort => {
            let vec = unsafe { flatbuffers_reflection::get_field_vector::<u16>(table, field) }
                .map_err(flatbuffer_error)?;
            if let Some(vec) = vec {
                values.extend(
                    vec.iter()
                        .map(|v| Value::Number(serde_json::Number::from(u64::from(v)))),
                );
            }
        }
        BaseType::Int => {
            let vec = unsafe { flatbuffers_reflection::get_field_vector::<i32>(table, field) }
                .map_err(flatbuffer_error)?;
            if let Some(vec) = vec {
                values.extend(vec.iter().map(|v| Value::Number((v as i64).into())));
            }
        }
        BaseType::UInt => {
            let vec = unsafe { flatbuffers_reflection::get_field_vector::<u32>(table, field) }
                .map_err(flatbuffer_error)?;
            if let Some(vec) = vec {
                values.extend(
                    vec.iter()
                        .map(|v| Value::Number(serde_json::Number::from(u64::from(v)))),
                );
            }
        }
        BaseType::Long => {
            let vec = unsafe { flatbuffers_reflection::get_field_vector::<i64>(table, field) }
                .map_err(flatbuffer_error)?;
            if let Some(vec) = vec {
                values.extend(vec.iter().map(|v| Value::Number(v.into())));
            }
        }
        BaseType::ULong => {
            let vec = unsafe { flatbuffers_reflection::get_field_vector::<u64>(table, field) }
                .map_err(flatbuffer_error)?;
            if let Some(vec) = vec {
                values.extend(
                    vec.iter()
                        .map(|v| Value::Number(serde_json::Number::from(v))),
                );
            }
        }
        BaseType::Float => {
            let vec = unsafe { flatbuffers_reflection::get_field_vector::<f32>(table, field) }
                .map_err(flatbuffer_error)?;
            if let Some(vec) = vec {
                values.extend(vec.iter().map(|v| {
                    serde_json::Number::from_f64(v as f64)
                        .map(Value::Number)
                        .unwrap_or(Value::Null)
                }));
            }
        }
        BaseType::Double => {
            let vec = unsafe { flatbuffers_reflection::get_field_vector::<f64>(table, field) }
                .map_err(flatbuffer_error)?;
            if let Some(vec) = vec {
                values.extend(vec.iter().map(|v| {
                    serde_json::Number::from_f64(v)
                        .map(Value::Number)
                        .unwrap_or(Value::Null)
                }));
            }
        }
        BaseType::String => {
            let vec = unsafe { flatbuffers_reflection::get_field_vector::<&str>(table, field) }
                .map_err(flatbuffer_error)?;
            if let Some(vec) = vec {
                values.extend(vec.iter().map(|v| Value::String(v.to_owned())));
            }
        }
        BaseType::Obj => {
            let obj_index = usize::try_from(field.type_().index())
                .map_err(|err| crate::error::Error::InvalidRecord(err.to_string()))?;
            let obj = schema.objects().get(obj_index);
            if obj.is_struct() {
                let vec = unsafe {
                    flatbuffers_reflection::get_field_vector::<flatbuffers_reflection::Struct>(
                        table, field,
                    )
                }
                .map_err(flatbuffer_error)?;
                if let Some(vec) = vec {
                    for item in vec.iter() {
                        values.push(parse_flatbuffer_struct(schema, &obj, &item)?);
                    }
                }
            } else {
                let vec = unsafe {
                    flatbuffers_reflection::get_field_vector::<flatbuffers::Table<'_>>(table, field)
                }
                .map_err(flatbuffer_error)?;
                if let Some(vec) = vec {
                    for item in vec.iter() {
                        values.push(parse_flatbuffer_table(schema, &obj, &item)?);
                    }
                }
            }
        }
        _ => {
            return Err(crate::error::Error::InvalidRecord(format!(
                "Unsupported flatbuffer vector element type {}",
                element_type.variant_name().unwrap_or("unknown")
            )));
        }
    }

    Ok(values)
}
