use std::borrow::Cow;

use schema::TemplateEscaping;

pub(super) struct JsonContent<'a> {
    pub value: Cow<'a, serde_json::Value>,
    pub escaping: TemplateEscaping,
}

impl<'a> JsonContent<'a> {
    fn get<'s, 'b>(&'s self, name: &str) -> Option<JsonContent<'b>>
    where
        'a: 'b,
        's: 'b,
    {
        if name == "." {
            return Some(JsonContent {
                value: Cow::Borrowed(self.value.as_ref()),
                escaping: self.escaping,
            });
        }
        name.split('.')
            .try_fold(self.value.as_ref(), |parent, key| {
                parent.as_object().and_then(|obj| obj.get(key))
            })
            .map(|value| JsonContent {
                value: Cow::Borrowed(value),
                escaping: self.escaping,
            })
    }
}

fn urlencode(s: &str) -> impl std::fmt::Display + '_ {
    use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};

    // Urlencode char encoding set. Only the characters in the unreserved set don't
    // have any special purpose in any part of a URI and can be safely left
    // unencoded as specified in https://tools.ietf.org/html/rfc3986.html#section-2.3
    const URLENCODE_STRICT_SET: &percent_encoding::AsciiSet =
        &NON_ALPHANUMERIC.remove(b'_').remove(b'.').remove(b'-').remove(b'~');

    utf8_percent_encode(s, URLENCODE_STRICT_SET)
}

impl ramhorns::Content for JsonContent<'_> {
    fn is_truthy(&self) -> bool {
        true // doesn't matter.
    }

    fn capacity_hint(&self, _tpl: &ramhorns::Template<'_>) -> usize {
        match self.value.as_ref() {
            serde_json::Value::Null => 4,
            serde_json::Value::Bool(_) => 5,
            serde_json::Value::Number(n) => {
                let n = n.as_f64().unwrap();
                if n.is_finite() { 24 } else { 64 }
            }
            serde_json::Value::String(s) => s.len(),
            serde_json::Value::Array(v) => v.len() * 2,
            serde_json::Value::Object(o) => o.len() * 2,
        }
    }

    fn render_escaped<E: ramhorns::encoding::Encoder>(&self, encoder: &mut E) -> Result<(), E::Error> {
        match self.value.as_ref() {
            serde_json::Value::Null => encoder.write_unescaped("null"),
            serde_json::Value::Bool(b) => encoder.write_unescaped(if *b { "true" } else { "false" }),
            serde_json::Value::Number(n) => encoder.format_unescaped(n),
            serde_json::Value::String(s) => match self.escaping {
                TemplateEscaping::Json => {
                    let s = serde_json::to_string(s).unwrap();
                    encoder.write_unescaped(&s)
                }
                TemplateEscaping::Url => {
                    encoder.format_unescaped(urlencode(s))?;
                    Ok(())
                }
            },
            serde_json::Value::Array(a) => match self.escaping {
                TemplateEscaping::Url => {
                    let s = serde_json::to_string(a).unwrap();
                    encoder.format_unescaped(urlencode(&s))?;
                    Ok(())
                }
                TemplateEscaping::Json => encoder.write_unescaped(&serde_json::to_string(a).unwrap()),
            },
            serde_json::Value::Object(o) => match self.escaping {
                TemplateEscaping::Url => {
                    let s = serde_json::to_string(o).unwrap();
                    encoder.format_unescaped(urlencode(&s))?;
                    Ok(())
                }
                TemplateEscaping::Json => encoder.write_unescaped(&serde_json::to_string(o).unwrap()),
            },
        }
    }

    fn render_unescaped<E: ramhorns::encoding::Encoder>(&self, encoder: &mut E) -> Result<(), E::Error> {
        match self.value.as_ref() {
            serde_json::Value::Null => encoder.write_unescaped("null"),
            serde_json::Value::Bool(b) => encoder.write_unescaped(if *b { "true" } else { "false" }),
            serde_json::Value::Number(n) => encoder.format_unescaped(n),
            serde_json::Value::String(s) => encoder.write_unescaped(s),
            serde_json::Value::Array(a) => encoder.write_unescaped(&serde_json::to_string(a).unwrap()),
            serde_json::Value::Object(o) => encoder.write_unescaped(&serde_json::to_string(o).unwrap()),
        }
    }

    fn render_section<C, E>(&self, section: ramhorns::Section<'_, C>, encoder: &mut E) -> Result<(), E::Error>
    where
        C: ramhorns::traits::ContentSequence,
        E: ramhorns::encoding::Encoder,
    {
        match self.value.as_ref() {
            serde_json::Value::Array(list) => ramhorns::render_indexed_content_section(
                list.iter().map(|value| JsonContent {
                    value: Cow::Borrowed(value),
                    escaping: self.escaping,
                }),
                section,
                encoder,
            ),
            serde_json::Value::Object(_) => section.with(self).render(encoder),
            _ => section.render(encoder),
        }
    }

    // We don't render the inverse, as it's equivalent to a condition which makes it impossible to
    // determine accurately the dependencies.
    fn render_inverse<C, E>(&self, _section: ramhorns::Section<'_, C>, _encoder: &mut E) -> Result<(), E::Error>
    where
        C: ramhorns::traits::ContentSequence,
        E: ramhorns::encoding::Encoder,
    {
        Ok(())
    }

    fn render_field_escaped<E>(&self, _: u64, name: &str, encoder: &mut E) -> Result<bool, E::Error>
    where
        E: ramhorns::encoding::Encoder,
    {
        match self.get(name) {
            Some(v) => v.render_escaped(encoder).map(|_| true),
            None => Ok(false),
        }
    }

    fn render_field_unescaped<E>(&self, _: u64, name: &str, encoder: &mut E) -> Result<bool, E::Error>
    where
        E: ramhorns::encoding::Encoder,
    {
        match self.get(name) {
            Some(v) => v.render_unescaped(encoder).map(|_| true),
            None => Ok(false),
        }
    }

    fn render_field_section<C, E>(
        &self,
        _: u64,
        name: &str,
        section: ramhorns::Section<'_, C>,
        encoder: &mut E,
    ) -> Result<bool, E::Error>
    where
        C: ramhorns::traits::ContentSequence,
        E: ramhorns::encoding::Encoder,
    {
        match self.get(name) {
            Some(v) => v.render_section(section, encoder).map(|_| true),
            None => Ok(false),
        }
    }

    // We don't render the inverse, as it's equivalent to a condition which makes it impossible to
    // determine accurately the dependencies.
    fn render_field_inverse<C, E>(
        &self,
        _: u64,
        _name: &str,
        _section: ramhorns::Section<'_, C>,
        _encoder: &mut E,
    ) -> Result<bool, E::Error>
    where
        C: ramhorns::traits::ContentSequence,
        E: ramhorns::encoding::Encoder,
    {
        Ok(false)
    }
}
