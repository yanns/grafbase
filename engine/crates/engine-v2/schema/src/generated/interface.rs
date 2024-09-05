//! ===================
//! !!! DO NOT EDIT !!!
//! ===================
//! Automatically generated by engine-v2-codegen from domain/schema.graphql
use crate::{
    generated::{
        FieldDefinition, FieldDefinitionId, ObjectDefinition, ObjectDefinitionId, TypeSystemDirective,
        TypeSystemDirectiveId,
    },
    prelude::*,
    StringId,
};
use readable::{Iter, Readable};

#[derive(serde::Serialize, serde::Deserialize)]
pub struct InterfaceDefinitionRecord {
    pub name_id: StringId,
    pub description_id: Option<StringId>,
    pub field_ids: IdRange<FieldDefinitionId>,
    pub interface_ids: Vec<InterfaceDefinitionId>,
    /// sorted by ObjectId
    pub possible_type_ids: Vec<ObjectDefinitionId>,
    pub possible_types_ordered_by_typename_ids: Vec<ObjectDefinitionId>,
    pub directive_ids: Vec<TypeSystemDirectiveId>,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash, serde::Serialize, serde::Deserialize, id_derives::Id)]
#[max(MAX_ID)]
pub struct InterfaceDefinitionId(std::num::NonZero<u32>);

#[derive(Clone, Copy)]
pub struct InterfaceDefinition<'a> {
    schema: &'a Schema,
    id: InterfaceDefinitionId,
}

impl<'a> InterfaceDefinition<'a> {
    #[allow(clippy::should_implement_trait)]
    pub fn as_ref(&self) -> &'a InterfaceDefinitionRecord {
        &self.schema[self.id]
    }
    pub fn id(&self) -> InterfaceDefinitionId {
        self.id
    }
    pub fn name(&self) -> &'a str {
        &self.schema[self.as_ref().name_id]
    }
    pub fn description(&self) -> Option<&'a str> {
        self.as_ref().description_id.map(|id| self.schema[id].as_ref())
    }
    pub fn fields(&self) -> impl Iter<Item = FieldDefinition<'a>> + 'a {
        self.as_ref().field_ids.read(self.schema)
    }
    pub fn interfaces(&self) -> impl Iter<Item = InterfaceDefinition<'a>> + 'a {
        self.as_ref().interface_ids.read(self.schema)
    }
    pub fn possible_types(&self) -> impl Iter<Item = ObjectDefinition<'a>> + 'a {
        self.as_ref().possible_type_ids.read(self.schema)
    }
    pub fn possible_types_ordered_by_typename(&self) -> impl Iter<Item = ObjectDefinition<'a>> + 'a {
        self.as_ref().possible_types_ordered_by_typename_ids.read(self.schema)
    }
    pub fn directives(&self) -> impl Iter<Item = TypeSystemDirective<'a>> + 'a {
        self.as_ref().directive_ids.read(self.schema)
    }
}

impl Readable<Schema> for InterfaceDefinitionId {
    type Reader<'a> = InterfaceDefinition<'a>;
    fn read<'s>(self, schema: &'s Schema) -> Self::Reader<'s>
    where
        Self: 's,
    {
        InterfaceDefinition { schema, id: self }
    }
}