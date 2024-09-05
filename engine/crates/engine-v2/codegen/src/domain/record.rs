use cynic_parser::common::WrappingType;

use super::{Definition, Indexed, Meta};

#[derive(Debug)]
pub struct Object {
    pub meta: Meta,
    pub indexed: Option<Indexed>,
    pub name: String,
    pub struct_name: String,
    pub copy: bool,
    pub fields: Vec<Field>,
}

impl From<Object> for Definition {
    fn from(object: Object) -> Self {
        Definition::Object(object)
    }
}

impl Object {
    pub fn reader_name(&self) -> &str {
        &self.name
    }
}

#[derive(Debug)]
pub struct Field {
    pub name: String,
    pub description: Option<String>,
    pub type_name: String,
    /// The wrapper types from the outermost to innermost
    pub wrapping: Vec<WrappingType>,
}

impl Field {
    pub fn has_list_wrapping(&self) -> bool {
        self.wrapping.iter().any(|w| matches!(w, WrappingType::List))
    }
}
