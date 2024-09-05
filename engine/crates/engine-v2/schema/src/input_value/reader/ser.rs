use readable::Readable;

use crate::SchemaInputValueRecord;

use super::SchemaInputValue;

impl<'a> serde::Serialize for SchemaInputValue<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let SchemaInputValue { schema, value } = *self;
        match value {
            SchemaInputValueRecord::Null => serializer.serialize_none(),
            SchemaInputValueRecord::String(id) => schema[*id].serialize(serializer),
            SchemaInputValueRecord::EnumValue(id) => id.read(schema).name().serialize(serializer),
            SchemaInputValueRecord::Int(n) => n.serialize(serializer),
            SchemaInputValueRecord::BigInt(n) => n.serialize(serializer),
            SchemaInputValueRecord::Float(f) => f.serialize(serializer),
            SchemaInputValueRecord::U64(n) => n.serialize(serializer),
            SchemaInputValueRecord::Boolean(b) => b.serialize(serializer),
            &SchemaInputValueRecord::InputObject(ids) => serializer.collect_map(
                ids.read(schema)
                    .map(|(input_value_definition, value)| (input_value_definition.name(), value)),
            ),
            &SchemaInputValueRecord::List(ids) => serializer.collect_seq(ids.read(schema)),
            &SchemaInputValueRecord::Map(ids) => serializer.collect_map(ids.read(schema)),
        }
    }
}
