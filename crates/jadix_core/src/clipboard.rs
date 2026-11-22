//! Bounded MIME clipboard interchange. Several representations of one selection
//! can be published atomically (for example text/plain and text/html).
pub const MAX_BYTES: usize = 16 * 1024 * 1024;
pub mod html;
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipboardItem {
    pub mime: String,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClipboardRequest {
    Read { mime_types: Vec<String> },
    Write { items: Vec<ClipboardItem> },
    Clear,
    Formats,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ClipboardResponse {
    Item(Option<ClipboardItem>),
    Formats(Vec<String>),
    Complete,
}
pub type ClipboardResult = Result<ClipboardResponse, String>;
fn valid_mime(s: &str) -> bool {
    s.len() <= 256 && s.contains('/') && s.is_ascii() && !s.bytes().any(|c| c.is_ascii_control())
}
impl ClipboardRequest {
    pub fn validate(&self) -> Result<(), String> {
        let mimes: Vec<_> = match self {
            Self::Read { mime_types } => mime_types.iter().map(String::as_str).collect(),
            Self::Write { items } => {
                if items
                    .iter()
                    .try_fold(0usize, |n, i| n.checked_add(i.bytes.len()))
                    .is_none_or(|n| n > MAX_BYTES)
                {
                    return Err("clipboard payload too large".into());
                }
                items.iter().map(|i| i.mime.as_str()).collect()
            }
            _ => return Ok(()),
        };
        let distinct: std::collections::BTreeSet<_> = mimes.iter().collect();
        if mimes.is_empty()
            || mimes.len() > 16
            || distinct.len() != mimes.len()
            || mimes.iter().any(|s| !valid_mime(s))
        {
            return Err("invalid clipboard MIME representations".into());
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn representations_are_unique_and_bounded() {
        assert!(ClipboardRequest::Read {
            mime_types: vec!["text/plain".into(), "text/html".into()]
        }
        .validate()
        .is_ok());
        assert!(ClipboardRequest::Read {
            mime_types: vec!["text/plain\0bad".into()]
        }
        .validate()
        .is_err());
        assert!(ClipboardRequest::Read {
            mime_types: vec!["text/plain".into(); 2]
        }
        .validate()
        .is_err());
        assert!(ClipboardRequest::Write {
            items: vec![ClipboardItem {
                mime: "image/png".into(),
                bytes: vec![0; MAX_BYTES + 1]
            }]
        }
        .validate()
        .is_err());
    }
}
