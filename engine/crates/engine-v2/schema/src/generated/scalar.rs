//! ===================
//! !!! DO NOT EDIT !!!
//! ===================
//! Automatically generated by engine-v2-codegen from domain/schema.graphql
use crate::{
    generated::{TypeSystemDirective, TypeSystemDirectiveId},
    prelude::*,
    ScalarType, StringId,
};
use readable::{Iter, Readable};

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ScalarDefinitionRecord {
    pub name_id: StringId,
    pub ty: ScalarType,
    pub description_id: Option<StringId>,
    pub specified_by_url_id: Option<StringId>,
    pub directive_ids: Vec<TypeSystemDirectiveId>,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash, serde::Serialize, serde::Deserialize, id_derives::Id)]
#[max(MAX_ID)]
pub struct ScalarDefinitionId(std::num::NonZero<u32>);

#[derive(Clone, Copy)]
pub struct ScalarDefinition<'a> {
    schema: &'a Schema,
    id: ScalarDefinitionId,
}

impl<'a> ScalarDefinition<'a> {
    #[allow(clippy::should_implement_trait)]
    pub fn as_ref(&self) -> &'a ScalarDefinitionRecord {
        &self.schema[self.id]
    }
    pub fn id(&self) -> ScalarDefinitionId {
        self.id
    }
    pub fn name(&self) -> &'a str {
        &self.schema[self.as_ref().name_id]
    }
    pub fn ty(&self) -> ScalarType {
        self.as_ref().ty
    }
    pub fn description(&self) -> Option<&'a str> {
        self.as_ref().description_id.map(|id| self.schema[id].as_ref())
    }
    pub fn specified_by_url(&self) -> Option<&'a str> {
        self.as_ref().specified_by_url_id.map(|id| self.schema[id].as_ref())
    }
    pub fn directives(&self) -> impl Iter<Item = TypeSystemDirective<'a>> + 'a {
        self.as_ref().directive_ids.read(self.schema)
    }
}

impl Readable<Schema> for ScalarDefinitionId {
    type Reader<'a> = ScalarDefinition<'a>;
    fn read<'s>(self, schema: &'s Schema) -> Self::Reader<'s>
    where
        Self: 's,
    {
        ScalarDefinition { schema, id: self }
    }
}

impl std::fmt::Debug for ScalarDefinition<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScalarDefinition")
            .field("name", &self.name())
            .field("ty", &self.ty())
            .field("description", &self.description())
            .field("specified_by_url", &self.specified_by_url())
            .field("directives", &self.directives())
            .finish()
    }
}