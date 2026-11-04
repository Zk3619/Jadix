use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Schema {
    Null,
    Bool,
    Number,
    String {
        max_bytes: usize,
    },
    Array {
        items: Box<Schema>,
        max_items: usize,
    },
    Object {
        fields: BTreeMap<String, Field>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub schema: Schema,
    pub required: bool,
}

impl Value {
    pub fn is_valid(&self) -> bool {
        let mut pending = vec![(self, 0)];
        let mut count = 0;
        let mut bytes = 0usize;
        while let Some((value, depth)) = pending.pop() {
            count += 1;
            if depth > 32 || count > 4096 || pending.len() > 4096 {
                return false;
            }
            match value {
                Self::Number(value) if !value.is_finite() => return false,
                Self::String(value) => bytes = bytes.saturating_add(value.len()),
                Self::Array(values) => {
                    pending.extend(values.iter().map(|value| (value, depth + 1)))
                }
                Self::Object(values) => {
                    bytes = bytes.saturating_add(values.keys().map(String::len).sum::<usize>());
                    pending.extend(values.values().map(|value| (value, depth + 1)));
                }
                _ => {}
            }
            if bytes > 1024 * 1024 {
                return false;
            }
        }
        true
    }
}

impl Serialize for Value {
    fn serialize<Serializer: serde::Serializer>(
        &self,
        serializer: Serializer,
    ) -> Result<Serializer::Ok, Serializer::Error> {
        match self {
            Self::Null => serializer.serialize_unit(),
            Self::Bool(value) => serializer.serialize_bool(*value),
            Self::Number(value) if value.is_finite() => serializer.serialize_f64(*value),
            Self::Number(_) => Err(serde::ser::Error::custom("nonfinite number")),
            Self::String(value) => serializer.serialize_str(value),
            Self::Array(value) => value.serialize(serializer),
            Self::Object(value) => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<Deserializer: serde::Deserializer<'de>>(
        deserializer: Deserializer,
    ) -> Result<Self, Deserializer::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Value;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("bounded JSON value")
            }
            fn visit_unit<Error: serde::de::Error>(self) -> Result<Value, Error> {
                Ok(Value::Null)
            }
            fn visit_bool<Error: serde::de::Error>(self, value: bool) -> Result<Value, Error> {
                Ok(Value::Bool(value))
            }
            fn visit_i64<Error: serde::de::Error>(self, value: i64) -> Result<Value, Error> {
                self.visit_f64(value as f64)
            }
            fn visit_u64<Error: serde::de::Error>(self, value: u64) -> Result<Value, Error> {
                self.visit_f64(value as f64)
            }
            fn visit_f64<Error: serde::de::Error>(self, value: f64) -> Result<Value, Error> {
                if value.is_finite() {
                    Ok(Value::Number(value))
                } else {
                    Err(Error::custom("nonfinite number"))
                }
            }
            fn visit_str<Error: serde::de::Error>(self, value: &str) -> Result<Value, Error> {
                self.visit_string(value.to_string())
            }
            fn visit_string<Error: serde::de::Error>(self, value: String) -> Result<Value, Error> {
                if value.len() > 1024 * 1024 {
                    return Err(Error::custom("string limit"));
                }
                Ok(Value::String(value))
            }
            fn visit_seq<Access: serde::de::SeqAccess<'de>>(
                self,
                mut access: Access,
            ) -> Result<Value, Access::Error> {
                let mut values = Vec::new();
                while let Some(value) = access.next_element()? {
                    if values.len() >= 4096 {
                        return Err(serde::de::Error::custom("array limit"));
                    }
                    values.push(value);
                }
                Ok(Value::Array(values))
            }
            fn visit_map<Access: serde::de::MapAccess<'de>>(
                self,
                mut access: Access,
            ) -> Result<Value, Access::Error> {
                let mut values = BTreeMap::new();
                while let Some((key, value)) = access.next_entry::<String, Value>()? {
                    if key.len() > 1024
                        || values.len() >= 4096
                        || values.insert(key, value).is_some()
                    {
                        return Err(serde::de::Error::custom("duplicate key or object limit"));
                    }
                }
                Ok(Value::Object(values))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

impl Schema {
    pub fn is_valid(&self) -> bool {
        let mut pending = vec![(self, 0)];
        let mut visited = 0;
        while let Some((schema, depth)) = pending.pop() {
            visited += 1;
            if depth > 32 || visited > 4096 {
                return false;
            }
            match schema {
                Self::Array { items, .. } => pending.push((items, depth + 1)),
                Self::Object { fields } => {
                    if fields.len() > 4096
                        || fields
                            .keys()
                            .any(|name| name.is_empty() || name.len() > 1024)
                    {
                        return false;
                    }
                    pending.extend(fields.values().map(|field| (&field.schema, depth + 1)));
                }
                _ => {}
            }
        }
        true
    }

    pub fn accepts(&self, value: &Value) -> bool {
        let mut pending = vec![(self, value, 0)];
        let mut visited = 0;
        let mut bytes = 0usize;
        while let Some((schema, value, depth)) = pending.pop() {
            visited += 1;
            if depth > 32 || visited > 4096 {
                return false;
            }
            match (schema, value) {
                (Self::Null, Value::Null) | (Self::Bool, Value::Bool(_)) => {}
                (Self::Number, Value::Number(number)) if number.is_finite() => {}
                (Self::String { max_bytes }, Value::String(value)) if value.len() <= *max_bytes => {
                    bytes = bytes.saturating_add(value.len());
                }
                (Self::Array { items, max_items }, Value::Array(values))
                    if values.len() <= *max_items && values.len() <= 4096 =>
                {
                    pending.extend(
                        values
                            .iter()
                            .map(|value| (items.as_ref(), value, depth + 1)),
                    );
                }
                (Self::Object { fields }, Value::Object(values))
                    if values.len() <= 4096 && fields.len() <= 4096 =>
                {
                    if fields
                        .iter()
                        .any(|(name, field)| field.required && !values.contains_key(name))
                    {
                        return false;
                    }
                    for (name, value) in values {
                        let Some(field) = fields.get(name) else {
                            return false;
                        };
                        bytes = bytes.saturating_add(name.len());
                        pending.push((&field.schema, value, depth + 1));
                    }
                }
                _ => return false,
            }
            if bytes > 1024 * 1024 || pending.len() > 4096 {
                return false;
            }
        }
        true
    }
}
