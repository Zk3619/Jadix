//! TOML stylesheet files (`[[rule]]` + optional `[theme]`).

use serde::Deserialize;

use super::error::{parse_selector_list, StylesheetError};
use super::ir::{StyleRule, Stylesheet};
use crate::style::ThemeOverrides;

#[derive(Debug, Deserialize)]
struct StylesheetFile {
    #[serde(default, alias = "rules")]
    rule: Vec<RawRule>,
    #[serde(default)]
    theme: Option<ThemeOverrides>,
}

#[derive(Debug, Deserialize)]
struct RawRule {
    selectors: Vec<String>,
    classes: String,
    #[serde(default)]
    priority: u16,
}

pub fn parse_stylesheet(src: &str) -> Result<Stylesheet, StylesheetError> {
    let file: StylesheetFile =
        toml::from_str(src).map_err(|e| StylesheetError::Toml(e.to_string()))?;
    file_to_stylesheet(file)
}

fn file_to_stylesheet(file: StylesheetFile) -> Result<Stylesheet, StylesheetError> {
    let mut rules = Vec::with_capacity(file.rule.len());
    for raw in file.rule {
        let selectors = parse_selector_list(&raw.selectors, None)?;
        rules.push(StyleRule {
            selectors,
            classes: raw.classes,
            priority: raw.priority,
        });
    }
    Ok(Stylesheet {
        theme: file.theme,
        rules,
        source: None,
    })
}
