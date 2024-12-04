use schema::EntityDefinition;
use serde::ser::{Error, SerializeMap};
use walker::Walk;

use super::{ResponseObjectView, ResponseObjectViewWithExtraFields, ResponseValueView};
use crate::response::ResponseValue;

impl serde::Serialize for ResponseObjectViewWithExtraFields<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.selection_set.len() + self.extra_constant_fields.len()))?;

        for (name, value) in self.extra_constant_fields {
            map.serialize_entry(name, value)?
        }

        for selection in self.selection_set {
            let key = self.ctx.schema[selection.alias_id].as_str();
            let value = ResponseValueView {
                ctx: self.ctx,
                value: self.response_object.find_required_field(selection.id).ok_or_else(|| {
                    S::Error::custom(format!(
                        "Could not retrieve field {}",
                        selection.id.walk(self.ctx.schema).definition().name()
                    ))
                })?,
                selection_set: &selection.subselection_record,
            };
            map.serialize_entry(key, &value)?;
        }

        map.end()
    }
}

impl serde::Serialize for ResponseObjectView<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.selection_set.len()))?;
        if let Some(definition_id) = self.response_object.definition_id {
            tracing::debug!("{}", definition_id.walk(self.ctx.schema).name());
        }
        for selection in self.selection_set {
            let key = self.ctx.schema[selection.alias_id].as_str();
            let value = match self.response_object.find_required_field(selection.id) {
                Some(value) => value,
                None => {
                    // If this field doesn't match the actual response object, meaning this field
                    // was in a fragment that doesn't apply to this object, we can safely skip it.
                    let field_definition = selection.id.walk(self.ctx.schema).definition();
                    if let Some(definition_id) = self.response_object.definition_id {
                        match field_definition.parent_entity() {
                            EntityDefinition::Interface(inf) => {
                                if inf.possible_type_ids.binary_search(&definition_id).is_err() {
                                    continue;
                                }
                            }
                            EntityDefinition::Object(obj) => {
                                if obj.id != definition_id {
                                    continue;
                                }
                            }
                        }
                    }

                    return Err(S::Error::custom(format_args!(
                        "Could not retrieve field {}.{}",
                        field_definition.parent_entity().name(),
                        field_definition.name()
                    )));
                }
            };
            let value = ResponseValueView {
                ctx: self.ctx,
                value,
                selection_set: &selection.subselection_record,
            };
            map.serialize_entry(key, &value)?;
        }

        map.end()
    }
}

impl serde::Serialize for ResponseValueView<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self.value {
            ResponseValue::Null => serializer.serialize_none(),
            ResponseValue::Inaccessible { id } => ResponseValueView {
                ctx: self.ctx,
                value: &self.ctx.response.data_parts[*id],
                selection_set: self.selection_set,
            }
            .serialize(serializer),
            ResponseValue::Boolean { value, .. } => value.serialize(serializer),
            ResponseValue::Int { value, .. } => value.serialize(serializer),
            ResponseValue::Float { value, .. } => value.serialize(serializer),
            ResponseValue::String { value, .. } => value.serialize(serializer),
            ResponseValue::StringId { id, .. } => self.ctx.schema[*id].serialize(serializer),
            ResponseValue::BigInt { value, .. } => value.serialize(serializer),
            &ResponseValue::List { id, .. } => {
                let values = &self.ctx.response.data_parts[id];
                serializer.collect_seq(values.iter().map(|value| ResponseValueView {
                    ctx: self.ctx,
                    value,
                    selection_set: self.selection_set,
                }))
            }
            &ResponseValue::Object { id, .. } => ResponseObjectView {
                ctx: self.ctx,
                response_object: &self.ctx.response.data_parts[id],
                selection_set: self.selection_set,
            }
            .serialize(serializer),
            ResponseValue::Unexpected => Err(S::Error::custom("Unexpected value")),
            ResponseValue::U64 { value } => value.serialize(serializer),
            ResponseValue::Map { id } => {
                serializer.collect_map(self.ctx.response.data_parts[*id].iter().map(|(key, value)| {
                    (
                        key.as_str(),
                        ResponseValueView {
                            ctx: self.ctx,
                            value,
                            selection_set: self.selection_set,
                        },
                    )
                }))
            }
        }
    }
}
