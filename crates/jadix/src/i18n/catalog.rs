//! Message catalogs: nested TOML/JSON flattened to dotted keys.

use std::collections::HashMap;

use super::locale_id::Locale;

/// One locale's key → ICU template map.
#[derive(Clone, Debug, Default)]
pub struct MessageBundle {
    locale: Locale,
    messages: HashMap<String, String>,
}

impl MessageBundle {
    pub fn new(locale: impl Into<Locale>) -> Self {
        Self {
            locale: locale.into(),
            messages: HashMap::new(),
        }
    }

    pub fn locale(&self) -> &Locale {
        &self.locale
    }

    pub fn insert(&mut self, key: impl Into<String>, template: impl Into<String>) {
        self.messages.insert(key.into(), template.into());
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.messages.get(key).map(String::as_str)
    }

    pub fn extend(&mut self, other: MessageBundle) {
        self.messages.extend(other.messages);
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Parse a TOML table (nested tables flatten with `.`).
    pub fn from_toml(locale: impl Into<Locale>, src: &str) -> Result<Self, String> {
        let value: toml::Value = toml::from_str(src).map_err(|e| e.to_string())?;
        let mut bundle = Self::new(locale);
        flatten_toml("", &value, &mut bundle.messages);
        Ok(bundle)
    }

    /// Parse JSON object (nested objects flatten with `.`).
    pub fn from_json(locale: impl Into<Locale>, src: &str) -> Result<Self, String> {
        let value: serde_json::Value = serde_json::from_str(src).map_err(|e| e.to_string())?;
        let mut bundle = Self::new(locale);
        flatten_json("", &value, &mut bundle.messages);
        Ok(bundle)
    }
}

fn flatten_toml(prefix: &str, value: &toml::Value, out: &mut HashMap<String, String>) {
    match value {
        toml::Value::Table(map) => {
            for (k, v) in map {
                let key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten_toml(&key, v, out);
            }
        }
        toml::Value::String(s) => {
            if !prefix.is_empty() {
                out.insert(prefix.to_string(), s.clone());
            }
        }
        toml::Value::Integer(n) => {
            out.insert(prefix.to_string(), n.to_string());
        }
        toml::Value::Float(n) => {
            out.insert(prefix.to_string(), n.to_string());
        }
        toml::Value::Boolean(b) => {
            out.insert(prefix.to_string(), b.to_string());
        }
        _ => {}
    }
}

fn flatten_json(prefix: &str, value: &serde_json::Value, out: &mut HashMap<String, String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                let key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten_json(&key, v, out);
            }
        }
        serde_json::Value::String(s) => {
            if !prefix.is_empty() {
                out.insert(prefix.to_string(), s.clone());
            }
        }
        serde_json::Value::Number(n) => {
            out.insert(prefix.to_string(), n.to_string());
        }
        serde_json::Value::Bool(b) => {
            out.insert(prefix.to_string(), b.to_string());
        }
        _ => {}
    }
}

/// Ordered set of bundles. Lookup walks the requested locale's fallback chain.
#[derive(Clone, Debug, Default)]
pub struct Catalog {
    bundles: Vec<MessageBundle>,
}

impl Catalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_vacant(
        &mut self,
        locale: impl Into<Locale>,
        key: &str,
        template: impl Into<String>,
    ) {
        let locale = locale.into();
        let template = template.into();
        if let Some(b) = self.bundles.iter_mut().find(|b| b.locale() == &locale) {
            if b.get(key).is_none() {
                b.insert(key, template);
            }
        } else {
            let mut b = MessageBundle::new(locale);
            b.insert(key, template);
            self.bundles.push(b);
        }
    }

    pub fn add(&mut self, bundle: MessageBundle) {
        if let Some(existing) = self
            .bundles
            .iter_mut()
            .find(|b| b.locale() == bundle.locale())
        {
            existing.extend(bundle);
        } else {
            self.bundles.push(bundle);
        }
    }

    pub fn get(&self, locale: &Locale, key: &str) -> Option<&str> {
        for candidate in locale.fallback_chain() {
            for bundle in &self.bundles {
                if bundle.locale() == &candidate {
                    if let Some(msg) = bundle.get(key) {
                        return Some(msg);
                    }
                }
            }
            // Prefix match: bundle `zh` serves `zh-CN`.
            for bundle in &self.bundles {
                if bundle.locale().matches(&candidate) || candidate.matches(bundle.locale()) {
                    if let Some(msg) = bundle.get(key) {
                        return Some(msg);
                    }
                }
            }
        }
        None
    }

    pub fn clear(&mut self) {
        self.bundles.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.bundles.iter().all(|b| b.is_empty())
    }
}

#[cfg(test)]
#[path = "tests/catalog.rs"]
mod tests;
