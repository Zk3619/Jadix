//! BCP 47 locale tags (`language[-script][-region]`).

use std::fmt;
use std::hash::{Hash, Hasher};

use super::direction::WritingDirection;

/// Parsed BCP 47 locale (language required; script / region optional).
#[derive(Clone, Debug, Eq)]
pub struct Locale {
    language: String,
    script: Option<String>,
    region: Option<String>,
}

impl PartialEq for Locale {
    fn eq(&self, other: &Self) -> bool {
        self.language == other.language
            && self.script == other.script
            && self.region == other.region
    }
}

impl Hash for Locale {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.language.hash(state);
        self.script.hash(state);
        self.region.hash(state);
    }
}

impl Locale {
    /// English (`en`).
    pub fn en() -> Self {
        Self {
            language: "en".into(),
            script: None,
            region: None,
        }
    }

    /// Parse `zh-CN`, `zh_Hans_CN`, `pt-BR`, `en`. Empty / invalid → [`None`].
    pub fn parse(tag: &str) -> Option<Self> {
        let raw = tag.trim();
        if raw.is_empty() || raw == "*" || raw == "C" || raw.eq_ignore_ascii_case("posix") {
            return None;
        }
        let stripped = raw.split(['.', '@']).next().unwrap_or(raw).trim();
        if stripped.is_empty() {
            return None;
        }
        let mut parts = stripped
            .split(['-', '_'])
            .filter(|p| !p.is_empty())
            .map(|p| p.to_ascii_lowercase());
        let mut language = parts.next()?;
        if language.len() < 2
            || language.len() > 8
            || !language.chars().all(|c| c.is_ascii_alphabetic())
        {
            return None;
        }
        language = alias_language(&language).to_string();
        let mut script = None;
        let mut region = None;
        for part in parts {
            if part.len() == 4 && part.chars().all(|c| c.is_ascii_alphabetic()) && script.is_none()
            {
                let mut chars = part.chars();
                let first = chars.next()?.to_ascii_uppercase();
                script = Some(format!("{first}{}", chars.as_str()));
            } else if (part.len() == 2 && part.chars().all(|c| c.is_ascii_alphabetic()))
                || (part.len() == 3 && part.chars().all(|c| c.is_ascii_digit()))
            {
                if region.is_none() {
                    region = Some(part.to_ascii_uppercase());
                }
            }
        }
        Some(Self {
            language,
            script,
            region,
        })
    }

    /// CLDR / HTML pseudo-locales (`en-XA` accented, `en-XB` bidi).
    pub fn is_pseudo(&self) -> bool {
        matches!(self.region.as_deref(), Some("XA") | Some("XB"))
    }

    /// `zh-Hans-CN` / `en` canonical form.
    pub fn tag(&self) -> String {
        let mut out = self.language.clone();
        if let Some(ref s) = self.script {
            out.push('-');
            out.push_str(s);
        }
        if let Some(ref r) = self.region {
            out.push('-');
            out.push_str(r);
        }
        out
    }

    pub fn language(&self) -> &str {
        &self.language
    }

    pub fn script(&self) -> Option<&str> {
        self.script.as_deref()
    }

    pub fn region(&self) -> Option<&str> {
        self.region.as_deref()
    }

    /// Fallback lookup order: full tag → language-region → language-script → language → `en`.
    pub fn fallback_chain(&self) -> Vec<Locale> {
        let mut out = Vec::new();
        let push = |out: &mut Vec<Locale>, loc: Locale| {
            if !out.contains(&loc) {
                out.push(loc);
            }
        };
        push(&mut out, self.clone());
        if self.script.is_some() && self.region.is_some() {
            push(
                &mut out,
                Locale {
                    language: self.language.clone(),
                    script: None,
                    region: self.region.clone(),
                },
            );
            push(
                &mut out,
                Locale {
                    language: self.language.clone(),
                    script: self.script.clone(),
                    region: None,
                },
            );
        }
        if self.script.is_some() || self.region.is_some() {
            push(
                &mut out,
                Locale {
                    language: self.language.clone(),
                    script: None,
                    region: None,
                },
            );
        }
        // zh-CN also looks at Hans; zh-TW at Hant.
        if self.language == "zh" && self.script.is_none() {
            let inferred = match self.region.as_deref() {
                Some("TW") | Some("HK") | Some("MO") => "Hant",
                _ => "Hans",
            };
            push(
                &mut out,
                Locale {
                    language: "zh".into(),
                    script: Some(inferred.into()),
                    region: self.region.clone(),
                },
            );
            push(
                &mut out,
                Locale {
                    language: "zh".into(),
                    script: Some(inferred.into()),
                    region: None,
                },
            );
        }
        push(&mut out, Locale::en());
        out
    }

    pub fn direction(&self) -> WritingDirection {
        WritingDirection::for_language(&self.language)
    }

    /// `true` when `self` is a prefix match of `other` (same language, optional script/region).
    pub fn matches(&self, other: &Locale) -> bool {
        self.language == other.language
            && (self.script.is_none() || self.script == other.script)
            && (self.region.is_none() || self.region == other.region)
    }
}

impl Default for Locale {
    fn default() -> Self {
        Self::en()
    }
}

impl fmt::Display for Locale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.tag())
    }
}

impl From<&str> for Locale {
    fn from(s: &str) -> Self {
        Self::parse(s).unwrap_or_else(Self::en)
    }
}

impl From<String> for Locale {
    fn from(s: String) -> Self {
        Locale::from(s.as_str())
    }
}

/// Deprecated / informal language codes → ISO 639-1.
fn alias_language(lang: &str) -> &str {
    match lang {
        "jp" | "jpn" => "ja",
        "kr" | "kor" => "ko",
        "iw" => "he",
        "in" => "id",
        "ji" => "yi",
        "no" => "nb",
        "eng" => "en",
        "chi" | "zho" => "zh",
        other => other,
    }
}

/// OS / env locale (`LANG` / `LC_ALL` / Windows `GetUserDefaultLocaleName`).
pub fn system_locale() -> Option<Locale> {
    system_locale_raw().and_then(|s| Locale::parse(&s))
}

fn system_locale_raw() -> Option<String> {
    for key in ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
        if let Ok(v) = std::env::var(key) {
            let first = v.split(':').next().unwrap_or(&v).trim();
            if !first.is_empty() && first != "C" {
                return Some(first.to_string());
            }
        }
    }
    system_locale_os()
}

#[cfg(windows)]
fn system_locale_os() -> Option<String> {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetUserDefaultLocaleName(lp_locale_name: *mut u16, cch_locale_name: i32) -> i32;
    }
    let mut buf = [0u16; 85];
    let n = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
    if n > 1 {
        String::from_utf16(&buf[..(n as usize - 1)]).ok()
    } else {
        None
    }
}

#[cfg(not(windows))]
fn system_locale_os() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_common_tags() {
        let zh = Locale::parse("zh-CN").unwrap();
        assert_eq!(zh.language(), "zh");
        assert_eq!(zh.region(), Some("CN"));
        assert!(zh.script().is_none());
        assert_eq!(zh.tag(), "zh-CN");

        let hans = Locale::parse("zh_Hans_CN").unwrap();
        assert_eq!(hans.script(), Some("Hans"));
        assert_eq!(hans.region(), Some("CN"));

        let en = Locale::parse("en").unwrap();
        assert_eq!(en, Locale::en());
        assert_eq!(Locale::parse("jp").unwrap().language(), "ja");
        assert_eq!(Locale::parse("kr").unwrap().language(), "ko");
        assert_eq!(Locale::parse("iw").unwrap().language(), "he");
        assert_eq!(Locale::parse("in").unwrap().language(), "id");
        assert_eq!(Locale::parse("ja-JP").unwrap().language(), "ja");
        assert!(Locale::parse("en-XA").unwrap().is_pseudo());
        assert!(Locale::parse("").is_none());
        assert!(Locale::parse("C").is_none());
    }

    #[test]
    fn fallback_includes_language_and_en() {
        let chain = Locale::parse("zh-CN").unwrap().fallback_chain();
        let tags: Vec<String> = chain.iter().map(|l| l.tag()).collect();
        assert_eq!(tags[0], "zh-CN");
        assert!(tags.iter().any(|t| t == "zh"));
        assert!(tags.iter().any(|t| t == "zh-Hans" || t == "zh-Hans-CN"));
        assert_eq!(tags.last().map(String::as_str), Some("en"));
    }

    #[test]
    fn unix_env_style_tags() {
        let loc = Locale::parse("pt_BR.UTF-8").unwrap();
        assert_eq!(loc.language(), "pt");
        assert_eq!(loc.region(), Some("BR"));
    }
}
