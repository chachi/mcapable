use crate::error::Result;
use num_traits::ToPrimitive;

use super::{IdlField, IdlRegistry, IdlStruct, IdlType};

pub(super) fn collect_definitions(
    defs: &[t4_idl_parser::expr::AnnotationAndDef],
    scope: &mut Vec<String>,
    registry: &mut IdlRegistry,
) -> Result<()> {
    use t4_idl_parser::expr::{ConstrTypeDcl, Definition, StructDcl, TypeDcl, TypedefType};

    for def in defs {
        match &def.definition {
            Definition::Module(module) => {
                scope.push(module.id.clone());
                collect_definitions(&module.definitions, scope, registry)?;
                scope.pop();
            }
            Definition::Type(TypeDcl::ConstrType(ConstrTypeDcl::Struct(StructDcl::Def(def)))) => {
                let name = scoped_name(scope, &def.id);
                let fields = parse_members(&def.members, scope)?;
                registry.structs.insert(name, IdlStruct { fields });
            }
            Definition::Type(TypeDcl::Typedef(typedef)) => {
                let base = match &typedef.type_dcl {
                    TypedefType::Simple(spec) => parse_type_spec(spec, scope)?,
                    TypedefType::Template(spec) => parse_template(spec, scope)?,
                    TypedefType::Constr(ConstrTypeDcl::Struct(StructDcl::Def(def))) => {
                        let name = scoped_name(scope, &def.id);
                        let fields = parse_members(&def.members, scope)?;
                        registry.structs.insert(name.clone(), IdlStruct { fields });
                        IdlType::Struct(name)
                    }
                    _ => {
                        return Err(crate::error::Error::InvalidRecord(
                            "Unsupported IDL typedef".to_owned(),
                        ));
                    }
                };

                for declarator in &typedef.declarators {
                    let (name, sizes) = parse_declarator(declarator)?;
                    let mut ty = base.clone();
                    for size in sizes.into_iter().rev() {
                        ty = IdlType::Array(Box::new(ty), size);
                    }
                    registry.aliases.insert(scoped_name(scope, &name), ty);
                }
            }
            _ => {}
        }
    }

    Ok(())
}

fn parse_members(
    members: &[t4_idl_parser::expr::Member],
    scope: &[String],
) -> Result<Vec<IdlField>> {
    let mut fields = Vec::new();
    for member in members {
        let base = parse_type_spec(&member.type_spec, scope)?;
        for declarator in &member.declarators {
            let (name, sizes) = parse_declarator(declarator)?;
            let mut ty = base.clone();
            for size in sizes.into_iter().rev() {
                ty = IdlType::Array(Box::new(ty), size);
            }
            fields.push(IdlField { name, ty });
        }
    }
    Ok(fields)
}

fn parse_declarator(
    declarator: &t4_idl_parser::expr::AnyDeclarator,
) -> Result<(String, Vec<usize>)> {
    use t4_idl_parser::expr::AnyDeclarator;

    match declarator {
        AnyDeclarator::Simple(id) => Ok((id.clone(), Vec::new())),
        AnyDeclarator::Array(array) => {
            let mut sizes = Vec::new();
            for size in &array.array_size {
                sizes.push(const_expr_to_usize(size)?);
            }
            Ok((array.id.clone(), sizes))
        }
    }
}

fn parse_type_spec(spec: &t4_idl_parser::expr::TypeSpec, scope: &[String]) -> Result<IdlType> {
    use t4_idl_parser::expr::TypeSpec;

    match spec {
        TypeSpec::PrimitiveType(primitive) => Ok(parse_primitive(primitive)),
        TypeSpec::ScopedName(scoped) => Ok(IdlType::Struct(scoped_name_from(scoped, scope))),
        TypeSpec::Template(spec) => parse_template(spec, scope),
    }
}

fn parse_template(
    spec: &t4_idl_parser::expr::TemplateTypeSpec,
    scope: &[String],
) -> Result<IdlType> {
    use t4_idl_parser::expr::{SequenceType, TemplateTypeSpec};

    match spec {
        TemplateTypeSpec::Sequence(seq) => match seq {
            SequenceType::Unlimited(inner) | SequenceType::Limited(inner, _) => {
                Ok(IdlType::Sequence(Box::new(parse_type_spec(inner, scope)?)))
            }
        },
        TemplateTypeSpec::String(_) => Ok(IdlType::String),
        TemplateTypeSpec::WString(_) => Ok(IdlType::WString),
        _ => Err(crate::error::Error::InvalidRecord(
            "Unsupported IDL template type".to_owned(),
        )),
    }
}

fn parse_primitive(primitive: &t4_idl_parser::expr::PrimitiveType) -> IdlType {
    use t4_idl_parser::expr::PrimitiveType;

    match primitive {
        PrimitiveType::Boolean => IdlType::Bool,
        PrimitiveType::Short | PrimitiveType::Int16 => IdlType::I16,
        PrimitiveType::Long | PrimitiveType::Int32 => IdlType::I32,
        PrimitiveType::LongLong | PrimitiveType::Int64 => IdlType::I64,
        PrimitiveType::UnsignedShort | PrimitiveType::Uint16 => IdlType::U16,
        PrimitiveType::UnsignedLong | PrimitiveType::Uint32 => IdlType::U32,
        PrimitiveType::UnsignedLongLong | PrimitiveType::Uint64 => IdlType::U64,
        PrimitiveType::Float => IdlType::F32,
        PrimitiveType::Double => IdlType::F64,
        PrimitiveType::Char | PrimitiveType::Octet | PrimitiveType::Uint8 => IdlType::U8,
        PrimitiveType::Int8 => IdlType::I8,
        PrimitiveType::WChar => IdlType::U16,
        _ => IdlType::String,
    }
}

fn scoped_name(scope: &[String], name: &str) -> String {
    if scope.is_empty() {
        name.to_owned()
    } else {
        format!("{}::{}", scope.join("::"), name)
    }
}

fn scoped_name_from(scoped: &t4_idl_parser::expr::ScopedName, scope: &[String]) -> String {
    match scoped {
        t4_idl_parser::expr::ScopedName::Absolute(parts) => parts.join("::"),
        t4_idl_parser::expr::ScopedName::Relative(parts) => {
            let mut merged = scope.to_vec();
            merged.extend(parts.iter().cloned());
            merged.join("::")
        }
    }
}

pub(super) fn normalize_root_name(name: &str) -> String {
    name.replace('/', "::").trim_start_matches("::").to_owned()
}

fn const_expr_to_usize(expr: &t4_idl_parser::expr::ConstExpr) -> Result<usize> {
    use t4_idl_parser::expr::{ConstExpr, Literal};

    match expr {
        ConstExpr::Literal(Literal::Integer(value)) => value
            .to_usize()
            .ok_or_else(|| crate::error::Error::InvalidRecord("Invalid array size".to_owned())),
        _ => Err(crate::error::Error::InvalidRecord(
            "Unsupported array size expression".to_owned(),
        )),
    }
}
