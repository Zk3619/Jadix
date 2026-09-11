//! Errors and selector-string parsing for TOML stylesheets.

use std::fmt;
use std::io;
use std::path::PathBuf;

use super::ir::Selector;
use crate::style::ThemeOverrideError;

#[derive(Debug)]
pub enum StylesheetError {
    Io(io::Error),
    Toml(String),
    UnsupportedExtension(String),
    UnsupportedSelector {
        selector: String,
        line: Option<usize>,
    },
    Theme(ThemeOverrideError),
    Path {
        path: PathBuf,
        source: Box<StylesheetError>,
    },
}

impl fmt::Display for StylesheetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "read stylesheet: {e}"),
            Self::Toml(e) => write!(f, "parse stylesheet TOML: {e}"),
            Self::UnsupportedExtension(ext) => {
                write!(f, "unsupported stylesheet extension: {ext}")
            }
            Self::UnsupportedSelector { selector, line } => match line {
                Some(n) => write!(f, "unsupported selector at line {n}: {selector}"),
                None => write!(f, "unsupported selector: {selector}"),
            },
            Self::Theme(e) => write!(f, "stylesheet theme: {e}"),
            Self::Path { path, source } => write!(f, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for StylesheetError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Toml(_) => None,
            Self::Theme(e) => Some(e),
            Self::Path { source, .. } => Some(source.as_ref()),
            _ => None,
        }
    }
}

impl From<ThemeOverrideError> for StylesheetError {
    fn from(e: ThemeOverrideError) -> Self {
        Self::Theme(e)
    }
}

pub fn with_path(path: PathBuf, err: StylesheetError) -> StylesheetError {
    StylesheetError::Path {
        path,
        source: Box::new(err),
    }
}

/// Parse a single selector string (`[data-slot=bubble-content]`, `.chat`, `#root`,
/// or a chain of those with no combinator).
pub fn parse_selector(raw: &str) -> Result<Selector, StylesheetError> {
    parse_selector_at(raw, None)
}

pub fn parse_selector_at(raw: &str, line: Option<usize>) -> Result<Selector, StylesheetError> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(StylesheetError::UnsupportedSelector {
            selector: raw.to_string(),
            line,
        });
    }
    if has_combinator(s) {
        return Err(StylesheetError::UnsupportedSelector {
            selector: s.to_string(),
            line,
        });
    }
    if s.contains('!')
        || s.contains(":nth")
        || s.contains(":not(")
        || s.contains('*')
        || s.contains('^')
        || s.contains('$')
        || s.contains('|')
    {
        return Err(StylesheetError::UnsupportedSelector {
            selector: s.to_string(),
            line,
        });
    }

    let parts = tokenize_simple_selector(s).map_err(|_| StylesheetError::UnsupportedSelector {
        selector: s.to_string(),
        line,
    })?;
    if parts.is_empty() {
        return Err(StylesheetError::UnsupportedSelector {
            selector: s.to_string(),
            line,
        });
    }
    if parts.len() == 1 {
        Ok(parts.into_iter().next().unwrap())
    } else {
        Ok(Selector::And(parts))
    }
}

fn has_combinator(s: &str) -> bool {
    let mut in_attr = false;
    let mut prev_ws = false;
    for ch in s.chars() {
        match ch {
            '[' => in_attr = true,
            ']' => in_attr = false,
            '>' | '+' | '~' if !in_attr => return true,
            c if c.is_whitespace() && !in_attr => prev_ws = true,
            _ if prev_ws && !in_attr => return true,
            _ => prev_ws = false,
        }
    }
    false
}

fn tokenize_simple_selector(s: &str) -> Result<Vec<Selector>, ()> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'[' => {
                let end = s[i..].find(']').ok_or(())?;
                let inner = &s[i + 1..i + end];
                out.push(parse_attr_selector(inner)?);
                i += end + 1;
            }
            b'.' => {
                i += 1;
                let ident = take_ident(&s[i..])?;
                if ident.is_empty() {
                    return Err(());
                }
                out.push(Selector::Class(ident.to_string()));
                i += ident.len();
            }
            b'#' => {
                i += 1;
                let ident = take_ident(&s[i..])?;
                if ident.is_empty() {
                    return Err(());
                }
                out.push(Selector::Id(ident.to_string()));
                i += ident.len();
            }
            _ => return Err(()),
        }
    }
    Ok(out)
}

fn take_ident(s: &str) -> Result<&str, ()> {
    let end = s
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .unwrap_or(s.len());
    Ok(&s[..end])
}

fn parse_attr_selector(inner: &str) -> Result<Selector, ()> {
    let inner = inner.trim();
    let eq = inner.find('=').ok_or(())?;
    let key = inner[..eq].trim();
    if key.is_empty() {
        return Err(());
    }
    let mut value = inner[eq + 1..].trim();
    if (value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\''))
    {
        value = &value[1..value.len() - 1];
    }
    if key == "data-slot" {
        Ok(Selector::DataSlot(value.to_string()))
    } else {
        Ok(Selector::DataEq {
            key: key.to_string(),
            value: value.to_string(),
        })
    }
}

pub fn parse_selector_list(
    raw: &[String],
    line: Option<usize>,
) -> Result<Vec<Selector>, StylesheetError> {
    raw.iter().map(|s| parse_selector_at(s, line)).collect()
}
