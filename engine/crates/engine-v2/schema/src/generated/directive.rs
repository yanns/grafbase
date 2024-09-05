//! ===================
//! !!! DO NOT EDIT !!!
//! ===================
//! Automatically generated by engine-v2-codegen from domain/schema.graphql
mod authorized;
mod deprecated;

use crate::{prelude::*, RequiresScopesDirective, RequiresScopesDirectiveId};
pub use authorized::*;
pub use deprecated::*;
use readable::Readable;

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum TypeSystemDirectiveId {
    Authenticated,
    Authorized(AuthorizedDirectiveId),
    Deprecated(DeprecatedDirectiveRecord),
    RequiresScopes(RequiresScopesDirectiveId),
}

impl From<AuthorizedDirectiveId> for TypeSystemDirectiveId {
    fn from(value: AuthorizedDirectiveId) -> Self {
        TypeSystemDirectiveId::Authorized(value)
    }
}
impl From<DeprecatedDirectiveRecord> for TypeSystemDirectiveId {
    fn from(value: DeprecatedDirectiveRecord) -> Self {
        TypeSystemDirectiveId::Deprecated(value)
    }
}
impl From<RequiresScopesDirectiveId> for TypeSystemDirectiveId {
    fn from(value: RequiresScopesDirectiveId) -> Self {
        TypeSystemDirectiveId::RequiresScopes(value)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum TypeSystemDirective<'a> {
    Authenticated,
    Authorized(AuthorizedDirective<'a>),
    Deprecated(DeprecatedDirective<'a>),
    RequiresScopes(RequiresScopesDirective<'a>),
}

impl Readable<Schema> for TypeSystemDirectiveId {
    type Reader<'a> = TypeSystemDirective<'a>;
    fn read<'s>(self, schema: &'s Schema) -> Self::Reader<'s>
    where
        Self: 's,
    {
        match self {
            TypeSystemDirectiveId::Authenticated => TypeSystemDirective::Authenticated,
            TypeSystemDirectiveId::Authorized(id) => TypeSystemDirective::Authorized(id.read(schema)),
            TypeSystemDirectiveId::Deprecated(item) => TypeSystemDirective::Deprecated(item.read(schema)),
            TypeSystemDirectiveId::RequiresScopes(id) => TypeSystemDirective::RequiresScopes(id.read(schema)),
        }
    }
}