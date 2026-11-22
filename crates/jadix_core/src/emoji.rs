//! Renderer-independent Unicode emoji data and localized, incremental search.
use serde::{Deserialize, Serialize};
use std::{collections::{BTreeMap, BTreeSet}, sync::OnceLock};
mod grid;
pub use grid::{EmojiGrid, EmojiRow};

#[derive(Debug, Serialize)]
pub struct Emoji {
    pub text: &'static str,
    pub name: &'static str,
    pub group: u8,
    pub version: &'static str,
    #[serde(skip)] light: &'static str,
    #[serde(skip)] keywords: String,
    pub keyword_labels: Vec<&'static str>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EmojiMatch {
    pub text: String,
    pub base: &'static str,
    pub name: String,
    pub group: u8,
    /// Display section; 10 is the caller's recent section.
    pub section: u8,
    pub version: &'static str,
    pub has_skin_tones: bool,
    /// Applied tone, or None when the emoji has no tone variants.
    pub skin_tone: Option<u8>,
    pub keywords: Vec<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmojiAnnotation {
    pub emoji: String,
    pub label: String,
    #[serde(default)] pub keywords: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EmojiQuery {
    pub query: String,
    pub group: Option<u8>,
    pub tone: Option<u8>,
    pub offset: usize,
    pub limit: usize,
    /// Ordered sections, including 10 (recent). None uses Unicode order.
    pub groups: Option<Vec<u8>>,
    pub recent: Vec<String>,
    pub annotations: Vec<EmojiAnnotation>,
    pub emoji_version: Option<String>,
}
impl Default for EmojiQuery {
    fn default() -> Self {
        Self { query: String::new(), group: None, tone: None, offset: 0, limit: 128,
            groups: None, recent: Vec::new(), annotations: Vec::new(), emoji_version: None }
    }
}
fn version(text: &str) -> Option<(u16, u16)> {
    let mut parts = text.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().map_or(Some(0), |part| part.parse().ok())?;
    if parts.next().is_some() { return None; }
    Some((major, minor))
}
fn identity(text: &str) -> String {
    text.chars().filter(|c| *c != '\u{fe0f}' && !('\u{1f3fb}'..='\u{1f3ff}').contains(c)).collect()
}
impl Emoji {
    fn as_match(&self, tone: Option<u8>, section: u8, annotation: Option<&EmojiAnnotation>) -> EmojiMatch {
        let mut keywords: Vec<String> = self.keyword_labels.iter().map(|s| (*s).into()).collect();
        if let Some(annotation) = annotation { keywords.extend(annotation.keywords.clone()); }
        EmojiMatch { text: self.projected(tone), base: self.text,
            name: annotation.map_or(self.name, |a| a.label.as_str()).into(),
            group: self.group, section, version: self.version, has_skin_tones: !self.light.is_empty(),
            skin_tone: tone.filter(|_| !self.light.is_empty()), keywords }
    }
    fn projected(&self, tone: Option<u8>) -> String {
        match tone {
            Some(tone) if !self.light.is_empty() => self.light.replace('🏻', &char::from_u32(0x1f3fa + tone as u32).unwrap().to_string()),
            _ => self.text.into(),
        }
    }
}
impl EmojiQuery {
    pub fn validate(&self) -> Result<(), &'static str> {
        let bounded = |s: &str, n: usize| s.len() <= n * 4 && s.encode_utf16().count() <= n && !s.contains('\0');
        if !bounded(&self.query, 256) || self.group.is_some_and(|g| g > 10)
            || self.tone.is_some_and(|t| !(1..=5).contains(&t)) || self.offset > 8192
            || !(1..=256).contains(&self.limit) || self.recent.len() > 256
            || self.recent.iter().any(|s| !bounded(s, 64)) || self.annotations.len() > 4096
            || self.annotations.iter().any(|a| !bounded(&a.emoji, 64) || !bounded(&a.label, 256)
                || a.keywords.len() > 32 || a.keywords.iter().any(|s| !bounded(s, 128)))
            || self.emoji_version.as_ref().is_some_and(|v| v.len() > 16 || version(v).is_none())
            || self.groups.as_ref().is_some_and(|groups| groups.len() > 11 || groups.iter().any(|g| *g > 10)
                || groups.iter().copied().collect::<BTreeSet<_>>().len() != groups.len()) {
            return Err("invalid emoji query");
        }
        Ok(())
    }
    pub fn search(&self) -> Result<EmojiPage, &'static str> { EmojiSearch::default().search(self) }
    /// Exact normalized identity lookup, independent of the active search and sections.
    pub fn entry(&self, text: &str) -> Result<Option<EmojiMatch>, &'static str> {
        self.validate()?;
        Ok(entry(text).filter(|emoji| self.emoji_version.as_deref().and_then(version)
            .is_none_or(|cap| version(emoji.version).is_some_and(|v| v <= cap)))
            .map(|emoji| {
                let annotation = self.annotations.iter().rev().find(|a| identity(&a.emoji) == identity(emoji.text));
                emoji.as_match(self.tone, emoji.group, annotation)
            }))
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct EmojiPage { pub total: usize, pub items: Vec<EmojiMatch> }

/// Per-picker cache. Extending a query filters the previous matches; changing
/// annotations, ordering or font coverage rebuilds the index.
#[derive(Default)]
pub struct EmojiSearch {
    configuration: Option<EmojiQuery>,
    entries: Vec<(EmojiMatch, String, String)>,
    query: String,
    matched: Vec<usize>,
}
impl EmojiSearch {
    pub fn search(&mut self, request: &EmojiQuery) -> Result<EmojiPage, &'static str> {
        request.validate()?;
        let mut configuration = request.clone();
        configuration.query.clear(); configuration.offset = 0; configuration.limit = 128;
        let rebuilt = self.configuration.as_ref() != Some(&configuration);
        if rebuilt {
            self.entries.clear();
            let annotations: BTreeMap<_, _> = request.annotations.iter().map(|a| (identity(&a.emoji), a)).collect();
            let cap = request.emoji_version.as_deref().and_then(version);
            let groups = request.groups.clone().unwrap_or_else(|| {
                if request.group == Some(2) { vec![2] }
                else { vec![10, 0, 1, 3, 4, 5, 6, 7, 8, 9] }
            });
            for section in groups.into_iter().filter(|g| request.group.is_none_or(|selected| selected == *g)) {
                let mut seen = BTreeSet::new();
                let entries: Vec<_> = if section == 10 {
                    request.recent.iter().filter_map(|text| entry(text)).filter(|e| seen.insert(e.text)).collect()
                } else { catalog().iter().filter(|e| e.group == section).collect() };
                for emoji in entries.into_iter().filter(|e| cap.is_none_or(|cap| version(e.version).is_some_and(|v| v <= cap))) {
                    let annotation = annotations.get(&identity(emoji.text));
                    let mut folded = emoji.keywords.clone();
                    if let Some(a) = annotation { folded.push_str(&format!(" {} {}", a.label, a.keywords.join(" ")).to_lowercase()); }
                    let text = emoji.projected(request.tone);
                    folded.push_str(&format!(" {} {}", emoji.text, text));
                    let names = format!("{} {}", emoji.name, annotation.map_or("", |a| a.label.as_str())).to_lowercase();
                    self.entries.push((emoji.as_match(request.tone, section, annotation.copied()), folded, names));
                }
            }
            self.configuration = Some(configuration);
        }
        let query = request.query.to_lowercase();
        if rebuilt || query != self.query {
            let terms: Vec<_> = query.split_whitespace().collect();
            let candidates = if !rebuilt && query.starts_with(&self.query) { std::mem::take(&mut self.matched) }
                else { (0..self.entries.len()).collect() };
            self.matched = candidates.into_iter().filter(|i| terms.iter().all(|term| self.entries[*i].1.contains(term))).collect();
            if !terms.is_empty() {
                let prefixes = |text: &str| terms.iter().all(|term|
                    text.split(|c: char| !c.is_alphanumeric()).any(|word| word.starts_with(term)));
                self.matched.sort_by_cached_key(|i| {
                    let (entry, _, names) = &self.entries[*i];
                    let rank = if prefixes(names) { 0 } else if prefixes(&entry.keywords.join(" ").to_lowercase()) { 1 } else { 2 };
                    (rank, *i)
                });
            }
            self.query = query;
        }
        Ok(EmojiPage { total: self.matched.len(), items: self.matched.iter().skip(request.offset).take(request.limit).map(|i| self.entries[*i].0.clone()).collect() })
    }
    /// Complete filtered data for virtual viewports; this allocates no UI nodes.
    pub fn matches(&self) -> impl ExactSizeIterator<Item = &EmojiMatch> {
        self.matched.iter().map(|i| &self.entries[*i].0)
    }
}
pub fn catalog() -> &'static [Emoji] {
    static CATALOG: OnceLock<Vec<Emoji>> = OnceLock::new();
    CATALOG.get_or_init(|| include_str!("emoji/emoji.tsv").lines().filter(|line| !line.starts_with('#')).filter_map(|line| {
        let fields: Vec<_> = line.split('\t').collect();
        Some(Emoji { text: *fields.first()?, group: fields.get(1)?.parse().ok()?, version: *fields.get(2)?,
            light: *fields.get(3)?, name: *fields.get(4)?, keywords: fields[4..].join(" ").to_lowercase(), keyword_labels: fields[5..].to_vec() })
    }).collect())
}
pub fn entry(text: &str) -> Option<&'static Emoji> {
    static IDENTITIES: OnceLock<BTreeMap<String, usize>> = OnceLock::new();
    let index = IDENTITIES.get_or_init(|| catalog().iter().enumerate().map(|(i, e)| (identity(e.text), i)).collect());
    index.get(&identity(text)).map(|i| &catalog()[*i])
}
pub fn contains(text: &str) -> bool { entry(text).is_some() }

#[cfg(test)] mod tests {
    use super::*;
    #[test]
    fn search_ranks_name_then_keyword_prefix_then_substring_and_reports_applied_tone() {
        let query = EmojiQuery { query: "zzneedle".into(), tone: Some(5), annotations: vec![
            EmojiAnnotation { emoji: "😀".into(), label: "azzneedle".into(), keywords: vec![] },
            EmojiAnnotation { emoji: "😃".into(), label: "other".into(), keywords: vec!["zzneedle".into()] },
            EmojiAnnotation { emoji: "👍".into(), label: "zzneedle".into(), keywords: vec![] },
        ], ..Default::default() };
        let page = query.search().unwrap();
        assert_eq!(page.items.iter().map(|item| item.base).collect::<Vec<_>>(), vec!["👍", "😃", "😀"]);
        assert_eq!(page.items[0].skin_tone, Some(5));
        assert_eq!(page.items[1].skin_tone, None);
    }
    #[test] fn catalog_localization_recent_versions_and_incremental_search() {
        assert!(catalog().len() > 1800);
        let mut search = EmojiSearch::default();
        let mut query = EmojiQuery { query: "thumbs up".into(), group: Some(1), tone: Some(5), ..Default::default() };
        assert!(search.search(&query).unwrap().items.iter().any(|e| e.text == "👍🏿"));
        query.query = "👍🏿".into();
        assert_eq!(search.search(&query).unwrap().items[0].base, "👍");
        query = EmojiQuery { recent: vec!["👍🏿".into(), "👍".into(), "invalid".into()], annotations: vec![EmojiAnnotation {
            emoji: "👍".into(), label: "赞".into(), keywords: vec!["支持".into()] }], ..Default::default() };
        let page = search.search(&query).unwrap();
        assert_eq!(page.items[0].section, 10); assert_eq!(page.items[0].name, "赞");
        assert_eq!(page.items[1].section, 0);
        for text in ["支", "支持", "thumb", "thumbs up", "", "不存在"] {
            query.query = text.into();
            assert_eq!(search.search(&query).unwrap().items, query.search().unwrap().items);
        }
        query.emoji_version = Some("0.6".into()); query.query.clear();
        assert!(search.search(&query).unwrap().items.iter().all(|e| version(e.version).unwrap() <= (0, 6)));
        query.tone = Some(0); assert!(search.search(&query).is_err());
        assert!(contains("👍🏿")); assert!(!contains("invalid"));
    }
}
