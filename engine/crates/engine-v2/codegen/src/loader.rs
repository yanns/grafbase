use std::{collections::HashMap, path::PathBuf};

use cynic_parser::{
    type_system::{iter::Iter, Definition, Directive, TypeDefinition},
    values::ConstValue,
};
use proc_macro2::{Ident, Span};
use quote::{quote, TokenStreamExt};

use crate::{
    domain::{self, ImportedDomain},
    domain_dir,
};

pub(super) fn load(path: PathBuf) -> anyhow::Result<domain::Domain> {
    let sdl = std::fs::read_to_string(&path)?;
    let document = match cynic_parser::parse_type_system_document(&sdl) {
        Ok(document) => document,
        Err(error) => {
            println!("Error parsing document");
            println!("{}", error.to_report(&sdl));
            return Err(anyhow::anyhow!(""));
        }
    };

    let mut domain: Option<domain::Domain> = None;
    let mut definitions_by_name = HashMap::new();
    definitions_by_name.insert(
        "Boolean".into(),
        domain::Scalar::Value {
            indexed: None,
            name: "bool".into(),
            walker_name: None,
            external_domain_name: None,
            in_prelude: true,
            copy: true,
        }
        .into(),
    );

    for definition in document.definitions() {
        let Definition::Type(ty) = definition else {
            anyhow::bail!("unsupported definition");
        };

        if let Some(ctx) = parse_domain_directive(ty.directives()) {
            assert!(domain.is_none(), "Only one scalar can have the directive @graph");
            let dir = env!("CARGO_MANIFEST_DIR");
            let mut imported_domains = HashMap::new();
            for import in ctx.imports {
                let path = domain_dir().join(&import.domain).with_extension("graphql");
                let imported_domain = load(path)?;
                for (name, mut definition) in imported_domain.definitions_by_name {
                    definition.set_external_domain_name(import.domain.clone());
                    definitions_by_name.entry(name).or_insert(definition);
                }
                imported_domains.insert(
                    import.domain.clone(),
                    ImportedDomain {
                        module: {
                            let parts = import
                                .module
                                .split('/')
                                .map(|s| Ident::new(s, Span::call_site()))
                                .collect::<Vec<_>>();
                            quote! { #(#parts)::* }
                        },
                        name: import.domain,
                    },
                );
            }
            let name = ctx.name.unwrap_or_else(|| {
                path.with_extension("")
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            });
            let context_name = ctx.context_name.unwrap_or_else(|| name.clone());
            domain = Some(domain::Domain {
                name,
                sdl: sdl.clone(),
                source: path.strip_prefix(dir).unwrap().to_path_buf(),
                destination_path: ctx.destination.into(),
                module: {
                    let mut ts = quote! { crate };
                    for module in &ctx.root_module {
                        let module = Ident::new(module, Span::call_site());
                        ts.append_all(quote! { ::#module })
                    }
                    ts
                },
                context_name,
                context_type: ctx
                    .context_type
                    .map(|name| {
                        let name = Ident::new(&name, Span::call_site());
                        quote! { #name<'a> }
                    })
                    .unwrap_or_else(|| {
                        let name = Ident::new(ty.name(), Span::call_site());
                        quote! { &'a #name }
                    }),
                definitions_by_name: Default::default(),
                public_visibility: ctx
                    .visibility
                    .map(|visibility| visibility.parse().unwrap())
                    .unwrap_or_default(),
                imported_domains,
            });
            continue;
        };

        let def: domain::Definition = match ty {
            TypeDefinition::Scalar(scalar) => {
                let in_prelude = scalar.directives().any(|directive| directive.name() == "prelude");
                if is_record(scalar.directives()) {
                    domain::Scalar::Record {
                        indexed: parse_indexed(scalar.name(), scalar.directives()),
                        name: scalar.name().to_string(),
                        record_name: format!("{}Record", scalar.name()),
                        external_domain_name: None,
                        in_prelude,
                        copy: is_copy(scalar.directives()),
                    }
                } else if parse_ref_directive(scalar.directives()).is_some() {
                    continue; // added at the end.
                } else if scalar.directives().any(|directive| directive.name() == "id") {
                    domain::Scalar::Id {
                        name: scalar.name().to_string(),
                        external_domain_name: None,
                        in_prelude,
                    }
                } else {
                    domain::Scalar::Value {
                        indexed: parse_indexed(scalar.name(), scalar.directives()),
                        name: scalar.name().to_string(),
                        walker_name: match scalar.name() {
                            "String" => Some("str".to_string()),
                            _ => None,
                        },
                        external_domain_name: None,
                        in_prelude,
                        copy: is_copy(scalar.directives()),
                    }
                }
            }
            .into(),
            TypeDefinition::Object(object) => domain::Object {
                meta: parse_meta(object.directives()).unwrap_or_default(),
                indexed: parse_indexed(object.name(), object.directives()),
                span: object.span(),
                description: object.description().map(|s| s.to_string()),
                name: object.name().to_string(),
                struct_name: format!("{}Record", object.name()),
                copy: is_copy(object.directives()),
                fields: object
                    .fields()
                    .map(|field| {
                        let directive = parse_field_directive(field.directives()).unwrap_or_default();
                        domain::Field {
                            name: field.name().to_string(),
                            // Add any explicitly defined field name or leave empty to be generated
                            // afterwards.
                            record_field_name: directive.record_field_name.unwrap_or_default(),
                            description: field.description().map(|s| s.to_string()),
                            type_name: field.ty().name().to_string(),
                            wrapping: field.ty().wrappers().collect(),
                        }
                    })
                    .collect(),
                external_domain_name: None,
            }
            .into(),
            TypeDefinition::Union(union) => domain::Union {
                meta: parse_meta(union.directives()).unwrap_or_default(),
                span: union.span(),
                description: union.description().map(|s| s.to_string()),
                kind: parse_union_kind(union.name(), union.directives()),
                variants: {
                    let variant = parse_variants(union.directives()).unwrap_or_default();

                    let mut variants = Vec::new();
                    for (index, member) in union.members().enumerate() {
                        variants.push(domain::Variant {
                            index,
                            name: {
                                if let Some(name) = variant.names.as_ref().and_then(|names| names.get(index)) {
                                    name.to_string()
                                } else {
                                    let name = member.name();
                                    match &variant.remove_suffix {
                                        Ok(false) => name,
                                        Ok(true) => name
                                            .strip_suffix(union.name())
                                            .expect("union name is not a suffix of the variant"),
                                        Err(suffix) => name.strip_suffix(suffix).expect("Suffix not found in variant"),
                                    }
                                    .to_string()
                                }
                            },
                            value_type_name: Some(member.name().to_string()),
                        });
                    }

                    for name in variant.empty {
                        variants.push(domain::Variant {
                            index: variants.len(),
                            name,
                            value_type_name: None,
                        });
                    }
                    variants.sort_by(|a, b| a.name.cmp(&b.name));
                    for (index, variant) in variants.iter_mut().enumerate() {
                        variant.index = index;
                    }
                    variants
                },
                external_domain_name: None,
            }
            .into(),
            _ => anyhow::bail!("unsupported type {}", ty.name()),
        };
        definitions_by_name.insert(def.name().to_string(), def);
    }

    for definition in document.definitions() {
        let Definition::Type(ty) = &definition else {
            continue;
        };
        let TypeDefinition::Scalar(scalar) = ty else {
            continue;
        };
        let Some(RefDirective { target }) = parse_ref_directive(scalar.directives()) else {
            continue;
        };
        let scalar = domain::Scalar::Ref {
            name: scalar.name().to_string(),
            id_struct_name: format!("{}Id", scalar.name()),
            in_prelude: scalar.directives().any(|directive| directive.name() == "prelude"),
            external_domain_name: None,
            target: Box::new(
                definitions_by_name
                    .get(&target)
                    .ok_or_else(|| anyhow::anyhow!("Unknown target: {target}"))?
                    .clone(),
            ),
        };
        definitions_by_name.insert(scalar.name().to_string(), scalar.into());
    }

    let mut domain = domain.expect("Missing scalar with @graph directive");
    domain.definitions_by_name = finalize_field_struct_names(definitions_by_name);

    Ok(domain)
}

fn finalize_field_struct_names(
    mut definitions_by_name: HashMap<String, domain::Definition>,
) -> HashMap<String, domain::Definition> {
    let suffixes = definitions_by_name
        .iter()
        .map(|(name, definition)| {
            let suffix = match definition {
                domain::Definition::Union(union) => match &union.kind {
                    domain::UnionKind::Record(union) => {
                        if union.indexed.is_some() {
                            Some("id")
                        } else {
                            Some("value")
                        }
                    }
                    domain::UnionKind::Id(_) | domain::UnionKind::BitpackedId(_) => Some("id"),
                },
                domain::Definition::Scalar(scalar) => {
                    match scalar {
                        domain::Scalar::Id { .. } => None,
                        domain::Scalar::Ref { .. } => Some("id"),
                        domain::Scalar::Record { indexed, .. } => {
                            if indexed.is_some() {
                                Some("id")
                            } else {
                                Some("record")
                            }
                        }
                        domain::Scalar::Value { indexed, .. } => {
                            if indexed.is_some() {
                                Some("id")
                            } else {
                                // We don't generate a walker for those fields. The Deref & as_ref()
                                // are enough.
                                None
                            }
                        }
                    }
                }
                domain::Definition::Object(object) => {
                    if object.indexed.is_some() {
                        Some("id")
                    } else {
                        Some("record")
                    }
                }
            };
            (name.to_string(), suffix)
        })
        .collect::<HashMap<_, _>>();

    for definition in definitions_by_name.values_mut() {
        let domain::Definition::Object(ref mut object) = definition else {
            continue;
        };
        for field in &mut object.fields {
            if !field.record_field_name.is_empty() {
                continue;
            }
            let Some(suffix) = suffixes.get(&field.type_name).cloned().flatten() else {
                field.record_field_name = field.name.clone();
                continue;
            };
            let record_field_name = if field.has_list_wrapping() {
                format!("{}_{suffix}s", field.name.strip_suffix("s").unwrap_or(&field.name))
            } else {
                format!("{}_{suffix}", field.name)
            };
            field.record_field_name = record_field_name;
        }
    }

    definitions_by_name
}

fn parse_union_kind<'a>(name: &str, directives: Iter<'a, Directive<'a>>) -> domain::UnionKind {
    if let Some(directive) = directives.clone().find(|directive| directive.name() == "id") {
        if let Some(bitpacked_size) = directive
            .arguments()
            .find(|arg| arg.name() == "bitpacked_size")
            .and_then(|arg| arg.value().as_str())
        {
            domain::UnionKind::BitpackedId(domain::BitPackedIdUnion {
                name: name.to_string(),
                size: bitpacked_size.to_string(),
                enum_name: format!("BitPacked{name}Id"),
            })
        } else {
            domain::UnionKind::Id(domain::IdUnion {
                name: name.to_string(),
                enum_name: format!("{name}Id"),
            })
        }
    } else {
        domain::UnionKind::Record(domain::RecordUnion {
            indexed: parse_indexed(name, directives.clone()),
            copy: is_copy(directives),
            name: name.to_string(),
            walker_enum_name: format!("{name}Variant"),
            enum_name: format!("{name}Record"),
        })
    }
}

struct VariantDirective {
    // Result used as a Either
    remove_suffix: Result<bool, String>,
    empty: Vec<String>,
    names: Option<Vec<String>>,
}

impl Default for VariantDirective {
    fn default() -> Self {
        Self {
            remove_suffix: Ok(false),
            empty: Default::default(),
            names: Default::default(),
        }
    }
}

fn parse_variants<'a>(mut directives: Iter<'a, Directive<'a>>) -> Option<VariantDirective> {
    let directive = directives.find(|directive| directive.name() == "variants")?;
    let remove_suffix = directive
        .arguments()
        .find(|arg| arg.name() == "remove_suffix")
        .and_then(|arg| match arg.value() {
            ConstValue::Boolean(value) => Some(Ok(value.value())),
            ConstValue::String(value) => Some(Err(value.to_string())),
            _ => None,
        })
        .unwrap_or(VariantDirective::default().remove_suffix);
    let empty = directive
        .arguments()
        .find(|arg| arg.name() == "empty")
        .and_then(|arg| match arg.value() {
            ConstValue::List(list) => Some(
                list.items()
                    .filter_map(|value| value.as_str())
                    .map(str::to_string)
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_default();
    let names = directive
        .arguments()
        .find(|arg| arg.name() == "names")
        .and_then(|arg| match arg.value() {
            ConstValue::List(list) => Some(
                list.items()
                    .filter_map(|value| value.as_str())
                    .map(str::to_string)
                    .collect(),
            ),
            _ => None,
        });
    Some(VariantDirective {
        remove_suffix,
        empty,
        names,
    })
}

#[derive(Default)]
struct FieldDirective {
    record_field_name: Option<String>,
}

fn parse_field_directive<'a>(mut directives: Iter<'a, Directive<'a>>) -> Option<FieldDirective> {
    let directive = directives.find(|directive| directive.name() == "field")?;
    let record_field_name = directive
        .arguments()
        .find(|arg| arg.name() == "record_field_name")
        .and_then(|arg| arg.value().as_str())
        .map(str::to_string);
    Some(FieldDirective { record_field_name })
}

fn parse_meta<'a>(mut directives: Iter<'a, Directive<'a>>) -> Option<domain::Meta> {
    let directive = directives.find(|directive| directive.name() == "meta")?;
    let derive = directive
        .arguments()
        .find(|arg| arg.name() == "derive")
        .and_then(|arg| match arg.value() {
            ConstValue::List(list) => Some(
                list.items()
                    .filter_map(|value| value.as_str())
                    .map(str::to_string)
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_default();

    let debug = directive
        .arguments()
        .find(|arg| arg.name() == "debug")
        .and_then(|arg| arg.value().as_bool())
        .unwrap_or(true);

    let module_path = directive
        .arguments()
        .find(|arg| arg.name() == "module")
        .and_then(|arg| arg.value().as_str())
        .map(|value| value.split('/').map(str::to_string).collect::<Vec<_>>())
        .unwrap_or_default();

    assert!(!module_path.is_empty(), "Missing or empty module in @meta");

    Some(domain::Meta {
        derive,
        module_path,
        debug,
    })
}

fn parse_indexed<'a>(name: &str, mut directives: Iter<'a, Directive<'a>>) -> Option<domain::Indexed> {
    let directive = directives.find(|directive| directive.name() == "indexed")?;
    let id_size = directive
        .arguments()
        .find(|arg| arg.name() == "id_size")
        .and_then(|arg| arg.value().as_str().map(str::to_string));
    let max_id = directive
        .arguments()
        .find(|arg| arg.name() == "max_id")
        .and_then(|arg| arg.value().as_str().map(str::to_string));
    let deduplicated = directive
        .arguments()
        .find(|arg| arg.name() == "deduplicated")
        .and_then(|arg| arg.value().as_bool())
        .unwrap_or_default();
    Some(domain::Indexed {
        id_struct_name: format!("{name}Id"),
        id_size,
        max_id,
        deduplicated,
    })
}

struct DomainDirective {
    name: Option<String>,
    destination: String,
    root_module: Vec<String>,
    visibility: Option<String>,
    context_name: Option<String>,
    context_type: Option<String>,
    imports: Vec<Import>,
}

struct Import {
    domain: String,
    module: String,
}

fn parse_domain_directive<'a>(mut directives: Iter<'a, Directive<'a>>) -> Option<DomainDirective> {
    let directive = directives.find(|directive| directive.name() == "domain")?;
    let destination = directive
        .arguments()
        .find(|arg| arg.name() == "destination")
        .and_then(|arg| arg.value().as_str())
        .expect("Missing destination in @domain")
        .to_string();
    assert!(!destination.is_empty(), "Missing or empty destination in @domain");
    let root_module = directive
        .arguments()
        .find(|arg| arg.name() == "root_module")
        .and_then(|arg| arg.value().as_str())
        .map(|value| value.split('/').map(str::to_string).collect::<Vec<_>>())
        .unwrap_or_default();
    let imports = directive
        .arguments()
        .find(|arg| arg.name() == "imports")
        .and_then(|arg| match arg.value() {
            ConstValue::List(list) => Some(
                list.items()
                    .filter_map(|value| match value {
                        ConstValue::Object(obj) => Some(obj),
                        _ => None,
                    })
                    .map(|obj| {
                        let domain = obj
                            .fields()
                            .find(|field| field.name() == "domain")
                            .unwrap()
                            .value()
                            .as_str()
                            .unwrap()
                            .to_string();
                        let module = obj
                            .fields()
                            .find(|field| field.name() == "module")
                            .unwrap()
                            .value()
                            .as_str()
                            .unwrap()
                            .to_string();
                        Import { domain, module }
                    })
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .unwrap_or_default();
    let name = directive
        .arguments()
        .find(|arg| arg.name() == "name")
        .and_then(|arg| arg.value().as_str())
        .map(str::to_string);
    let context_name = directive
        .arguments()
        .find(|arg| arg.name() == "context_name")
        .and_then(|arg| arg.value().as_str())
        .map(str::to_string);
    let context_type = directive
        .arguments()
        .find(|arg| arg.name() == "context_type")
        .and_then(|arg| arg.value().as_str())
        .map(str::to_string);
    let visibility = directive
        .arguments()
        .find(|arg| arg.name() == "visibility")
        .and_then(|arg| arg.value().as_str())
        .map(str::to_string);
    Some(DomainDirective {
        name,
        destination,
        root_module,
        visibility,
        context_name,
        context_type,
        imports,
    })
}

fn is_copy<'a>(mut directives: Iter<'a, Directive<'a>>) -> bool {
    directives.any(|directive| directive.name() == "copy")
}

fn is_record<'a>(mut directives: Iter<'a, Directive<'a>>) -> bool {
    directives.any(|directive| directive.name() == "record")
}

struct RefDirective {
    target: String,
}

fn parse_ref_directive<'a>(mut directives: Iter<'a, Directive<'a>>) -> Option<RefDirective> {
    let directive = directives.find(|directive| directive.name() == "ref")?;
    let target = directive
        .arguments()
        .find(|arg| arg.name() == "target")
        .and_then(|arg| arg.value().as_str())
        .expect("Missing target in @ref")
        .to_string();
    Some(RefDirective { target })
}
