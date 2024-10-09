use std::borrow::Cow;

use itertools::Itertools;

pub(super) struct Attrs<'a> {
    label: Cow<'a, str>,
    others: Vec<Cow<'a, str>>,
}

impl<'a> Attrs<'a> {
    pub fn new(label: impl Into<Cow<'a, str>>) -> Self {
        Self {
            label: label.into(),
            others: vec![],
        }
    }

    #[must_use]
    pub fn bold(mut self) -> Self {
        self.label = Cow::Owned(format!("<<b>{}</b>>", self.label));
        self
    }

    #[must_use]
    pub fn with(mut self, attr: impl Into<Cow<'a, str>>) -> Self {
        self.others.push(attr.into());
        self
    }
}

impl std::fmt::Display for Attrs<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.label.starts_with("<") {
            write!(f, "label = {}", self.label,)?;
        } else {
            write!(f, "label = \"{}\"", self.label,)?;
        }
        if !self.others.is_empty() {
            write!(f, ", ")?;
        }

        write!(f, "{} ", self.others.iter().join(", "))
    }
}
