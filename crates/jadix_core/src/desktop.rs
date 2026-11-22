//! Shared desktop request values and URL validation, independent of a renderer.
use crate::presentation::NodeId;
use std::path::PathBuf;

#[derive(Clone, Debug, Default)]
pub struct Share {
    pub subject: Option<String>,
    pub text: Option<String>,
    pub url: Option<String>,
    /// Trusted application paths; script hosts must resolve granted handles.
    pub files: Vec<PathBuf>,
    pub anchor: Option<NodeId>,
}

impl Share {
    pub fn validate(&self) -> Result<(), String> {
        if self.text.as_deref().is_none_or(str::is_empty)
            && self.url.is_none()
            && self.files.is_empty()
        {
            return Err("nothing to share".into());
        }
        if self.subject.as_ref().is_some_and(|v| v.len() > 4096)
            || self.text.as_ref().is_some_and(|v| v.len() > 256 * 1024)
            || self.files.len() > 64
            || self
                .files
                .iter()
                .any(|v| !v.is_absolute() || v.as_os_str().len() > 32768)
        {
            return Err("invalid share payload or file paths".into());
        }
        if let Some(url) = &self.url {
            check_open_url(url)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ShareOutcome {
    Shared { service: String },
    Dismissed,
    Replaced,
}
pub type ShareResult = Result<ShareOutcome, String>;
pub type OpenUrlResult = Result<bool, String>;

/// Absolute external destinations only. In particular, drive letters and
/// executable/data/file URLs never reach an application router or the OS.
pub fn check_open_url(value: &str) -> Result<url::Url, String> {
    if value.len() > 16 * 1024 || value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("invalid external URL length or whitespace".into());
    }
    let url = url::Url::parse(value).map_err(|_| "invalid absolute external URL")?;
    if url.scheme().len() == 1
        || matches!(
            url.scheme(),
            "javascript" | "vbscript" | "data" | "blob" | "file" | "about" | "asset"
        )
        || (matches!(url.scheme(), "http" | "https") && url.host_str().is_none())
    {
        return Err("external URL scheme is not allowed".into());
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_urls_reject_executable_local_and_ambiguous_destinations() {
        for value in [
            "",
            "/relative",
            "C:/secret.txt",
            "c:\\secret.txt",
            "javascript:alert(1)",
            "JaVaScRiPt:alert(1)",
            "data:text/html,test",
            "file:///secret",
            "blob:https://example.com/id",
            "about:blank",
            "asset:/icon",
            "vbscript:run",
            "https://",
            "https://example.com/\nnext",
            "https://example.com/a b",
            "https://example.com/\u{2003}",
        ] {
            assert!(check_open_url(value).is_err(), "accepted {value:?}");
        }
        for value in [
            "https://example.com/中文?q=%20#part",
            "mailto:hello@example.com",
            "my-app:document/1",
        ] {
            assert!(check_open_url(value).is_ok(), "refused {value:?}");
        }
        assert!(check_open_url(&format!("custom:{}", "a".repeat(16 * 1024))).is_err());
    }

    #[test]
    fn share_bounds_are_byte_based_and_subject_alone_is_not_content() {
        assert!(Share::default().validate().is_err());
        assert!(Share {
            subject: Some("subject".into()),
            ..Share::default()
        }
        .validate()
        .is_err());
        let mut share = Share {
            text: Some("🦀".repeat(65536)),
            ..Share::default()
        };
        share.validate().unwrap();
        share.text.as_mut().unwrap().push('a');
        assert!(share.validate().is_err());
        share.text = Some("hello".into());
        share.files.push("relative.txt".into());
        assert!(share.validate().is_err());
        share.files.clear();
        share.url = Some("file:///secret".into());
        assert!(share.validate().is_err());
    }
}
