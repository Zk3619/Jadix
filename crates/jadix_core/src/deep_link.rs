//! Validated deep-link routing shared by native and shell hosts.
use std::collections::BTreeSet;

pub const MAX_DEEP_LINK_BYTES: usize = 32 * 1024;
pub const MAX_DEEP_LINK_QUEUE: usize = 64;
pub const MAX_DEEP_LINK_SCHEMES: usize = 16;

/// A bounded process launch, retained until the application consumes it. Paths
/// and arguments are data; receiving a launch never opens or executes them.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicationLaunch {
    pub arguments: Vec<String>,
    pub working_directory: Option<String>,
}
impl ApplicationLaunch {
    pub fn current() -> Result<Self, &'static str> {
        let mut arguments = Vec::new();
        for argument in std::env::args_os().skip(1).take(65) {
            arguments.push(argument.into_string().map_err(|_| "launch argument is not Unicode")?);
        }
        let launch = Self { arguments, working_directory: std::env::current_dir().ok().and_then(|p| p.into_os_string().into_string().ok()) };
        launch.validate()?;
        Ok(launch)
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.arguments.len() > 64 || self.arguments.iter().any(|s| s.contains('\0'))
            || self.arguments.iter().map(String::len).sum::<usize>() > 64 * 1024
            || self.working_directory.as_ref().is_some_and(|s| s.len() > MAX_DEEP_LINK_BYTES || s.contains('\0')) {
            return Err("invalid application launch");
        }
        Ok(())
    }
}

/// Stable application identity and fixed launcher arguments, shared by protocol
/// and notification activation. Fixed arguments identify an embedded app when
/// several applications use the same executable.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicationActivation {
    pub identifier: String,
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default)]
    pub deep_links: DeepLinkConfig,
    /// Lowercase filename extensions without a leading dot. Files are only
    /// delivered when they exist; registration never changes the user's default.
    #[serde(default)]
    pub file_extensions: BTreeSet<String>,
}
impl ApplicationActivation {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.identifier.is_empty() || self.identifier.len() > 128
            || !self.identifier.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-' | b'_')) {
            return Err("invalid application identifier");
        }
        ApplicationLaunch { arguments: self.arguments.clone(), working_directory: None }.validate()?;
        if self.file_extensions.len() > 64 || self.file_extensions.iter().any(|s| s.is_empty() || s.len() > 32
            || !s.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'+' | b'-' | b'_'))) {
            return Err("invalid application file extensions");
        }
        self.deep_links.validate()
    }
    pub fn launch_links(&self, launch: &ApplicationLaunch, source: DeepLinkSource) -> Result<Vec<DeepLink>, &'static str> {
        self.validate()?; launch.validate()?;
        let mut links = Vec::new();
        for argument in &launch.arguments {
            let link = DeepLink { url: argument.clone(), source };
            let file_url = argument.get(..5).is_some_and(|s| s.eq_ignore_ascii_case("file:"));
            if !file_url && link.validate(&self.deep_links.schemes).is_ok() { links.push(link); continue; }
            if argument.starts_with('-') || argument.contains('\0') || self.file_extensions.is_empty() { continue; }
            let path = if file_url {
                let Ok(url) = url::Url::parse(argument) else { continue; };
                let Ok(path) = url.to_file_path() else { continue; }; path
            } else {
                let path = std::path::PathBuf::from(argument);
                if path.is_absolute() { path } else {
                    let Some(directory) = &launch.working_directory else { continue; };
                    std::path::Path::new(directory).join(path)
                }
            };
            if !path.is_file() || !self.matches_file(&path) { continue; }
            if let Ok(url) = url::Url::from_file_path(&path) {
                let url = String::from(url);
                if url.len() <= MAX_DEEP_LINK_BYTES { links.push(DeepLink { url, source }); }
            }
        }
        Ok(links)
    }
    fn matches_file(&self, path: &std::path::Path) -> bool {
        path.extension().and_then(|s| s.to_str()).is_some_and(|s| self.file_extensions.contains(&s.to_ascii_lowercase()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeepLinkSource { ColdStart, HotStart, Handoff }

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct DeepLink { pub url: String, pub source: DeepLinkSource }

impl DeepLink {
    pub fn validate(&self, schemes: &BTreeSet<String>) -> Result<(), &'static str> {
        if self.url.is_empty() || self.url.len() > MAX_DEEP_LINK_BYTES || self.url.chars().any(|c| c.is_control() || c.is_whitespace()) { return Err("invalid deep link URL"); }
        let (scheme, rest) = self.url.split_once(':').ok_or("deep link must be absolute")?;
        if !valid_scheme(scheme) || rest.is_empty() { return Err("invalid deep link scheme"); }
        url::Url::parse(&self.url).map_err(|_| "invalid deep link URL")?;
        if !schemes.contains(&scheme.to_ascii_lowercase()) { return Err("deep link scheme is not allowed"); }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct DeepLinkConfig { pub schemes: BTreeSet<String>, pub single_instance: bool, pub queue_limit: usize }

impl Default for DeepLinkConfig { fn default() -> Self { Self { schemes: BTreeSet::new(), single_instance: true, queue_limit: MAX_DEEP_LINK_QUEUE } } }

impl DeepLinkConfig {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schemes.len() > MAX_DEEP_LINK_SCHEMES || self.queue_limit == 0 || self.queue_limit > MAX_DEEP_LINK_QUEUE { return Err("invalid deep link configuration"); }
        if self.schemes.iter().any(|s| !valid_scheme(s) || *s != s.to_ascii_lowercase()) { return Err("invalid deep link scheme"); }
        Ok(())
    }
}
fn valid_scheme(s: &str) -> bool { !s.is_empty() && s.len() <= 64 && s.as_bytes()[0].is_ascii_alphabetic() && s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.')) }

/// A root may keep a relative navigation address, but never a credential-bearing
/// or executable URL. Hosts decide whether an absolute address can be advertised.
pub fn validate_app_address(address: &str) -> Result<(), &'static str> {
    if address.is_empty() || address.len() > MAX_DEEP_LINK_BYTES
        || address.chars().any(|c| c.is_control() || c.is_whitespace())
        || address.starts_with("//") || address.contains('\\') {
        return Err("invalid app address");
    }
    if let Some((scheme, _)) = address.split_once(':').filter(|(prefix, _)| !prefix.contains(['/', '?', '#'])) {
        if !valid_scheme(scheme) || matches!(scheme.to_ascii_lowercase().as_str(), "file" | "javascript" | "vbscript" | "data" | "blob" | "about" | "asset") {
            return Err("invalid app address scheme");
        }
        let url = url::Url::parse(address).map_err(|_| "invalid app address URL")?;
        if !url.username().is_empty() || url.password().is_some()
            || (matches!(url.scheme(), "http" | "https") && url.host_str().is_none()) {
            return Err("invalid app address URL");
        }
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct DeepLinkRouter { config: DeepLinkConfig, pending: std::collections::VecDeque<DeepLink> }

impl DeepLinkRouter {
    pub fn new(config: DeepLinkConfig) -> Result<Self, &'static str> { config.validate()?; Ok(Self { config, pending: std::collections::VecDeque::new() }) }
    pub fn config(&self) -> &DeepLinkConfig { &self.config }
    pub fn configure(&mut self, config: DeepLinkConfig) -> Result<(), &'static str> { config.validate()?; self.config = config; self.pending.retain(|link| link.validate(&self.config.schemes).is_ok()); while self.pending.len() > self.config.queue_limit { self.pending.pop_front(); } Ok(()) }
    pub fn accept(&mut self, link: DeepLink) -> Result<(), &'static str> { link.validate(&self.config.schemes)?; if self.pending.len() >= self.config.queue_limit { self.pending.pop_front(); } self.pending.push_back(link); Ok(()) }
    pub fn accept_application_link(&mut self, config: &ApplicationActivation, link: DeepLink) -> Result<(), &'static str> {
        if config.deep_links != self.config { return Err("application link configuration mismatch"); }
        if link.url.starts_with("file:") && !config.file_extensions.is_empty() {
            let launch = ApplicationLaunch { arguments: vec![link.url.clone()], working_directory: None };
            if config.launch_links(&launch, link.source)?.first() != Some(&link) { return Err("application file is not allowed"); }
            if self.pending.len() >= self.config.queue_limit { self.pending.pop_front(); }
            self.pending.push_back(link); Ok(())
        } else { self.accept(link) }
    }
    pub fn drain(&mut self) -> Vec<DeepLink> { self.pending.drain(..).collect() }
    pub fn pending(&self) -> usize { self.pending.len() }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn application_files_use_launch_directory_and_declared_extensions() {
        let directory = std::env::temp_dir().join(format!("jadix-activation-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("消息 #1.JADX"); std::fs::write(&path, b"document").unwrap();
        let mut config = ApplicationActivation { identifier: "dev.jadix.file-test".into(), arguments: vec![], deep_links: DeepLinkConfig { schemes: BTreeSet::from(["jadix".into()]), queue_limit: 2, ..Default::default() }, file_extensions: BTreeSet::from(["jadx".into()]) };
        let launch = ApplicationLaunch { arguments: vec!["消息 #1.JADX".into(), "missing.jadx".into(), "jadix:inbox".into(), "--switch.jadx".into()], working_directory: Some(directory.to_str().unwrap().into()) };
        let links = config.launch_links(&launch, DeepLinkSource::Handoff).unwrap();
        assert_eq!(links.len(), 2); assert_eq!(links[1].url, "jadix:inbox");
        assert_eq!(url::Url::parse(&links[0].url).unwrap().to_file_path().unwrap(), path);
        assert!(links[0].url.contains("%23"));
        let mut router = DeepLinkRouter::new(config.deep_links.clone()).unwrap();
        for link in &links { router.accept_application_link(&config, link.clone()).unwrap(); }
        assert_eq!(router.drain(), links);
        config.file_extensions.clear(); assert!(router.accept_application_link(&config, links[0].clone()).is_err());
        for extension in [".jadx", "JADX", "path/file", "", "nul\0"] { config.file_extensions = BTreeSet::from([extension.into()]); assert!(config.validate().is_err()); }
        std::fs::remove_file(&path).unwrap(); std::fs::remove_dir(&directory).unwrap();
    }
    #[test] fn application_launches_and_activation_are_bounded_data() {
        let mut launch = ApplicationLaunch { arguments: vec!["文件.tsx".into(), "demo://open/消息".into()], working_directory: Some("D:\\my app".into()) };
        assert!(launch.validate().is_ok());
        launch.arguments = vec!["x".repeat(65_537)]; assert!(launch.validate().is_err());
        launch.arguments = vec![String::new(); 65]; assert!(launch.validate().is_err());
        launch.arguments = vec!["bad\0argument".into()]; assert!(launch.validate().is_err());
        let mut config = ApplicationActivation { identifier: "dev.jadix.app-1".into(), arguments: vec!["entry.tsx".into()], deep_links: Default::default(), file_extensions: Default::default() };
        assert!(config.validate().is_ok());
        for invalid in ["", "../other", "space name", "key\\child", "emoji🦀"] { config.identifier = invalid.into(); assert!(config.validate().is_err()); }
        assert!(serde_json::from_str::<ApplicationLaunch>(r#"{"arguments":[],"working_directory":null,"execute":true}"#).is_err());
    }
    #[test] fn app_addresses_allow_local_routes_and_safe_handoff_urls() {
        for address in ["/inbox/消息", "inbox?time=12:30#item", "demo://inbox/item", "https://example.com/inbox"] { assert!(validate_app_address(address).is_ok(), "{address}"); }
        for address in ["", "//outside/path", "https://user:secret@example.com", "JaVaScRiPt:alert(1)", "file:///private/item", "data:text/html,a", "bad scheme://item", "route\nitem", "\\\\outside\\item"] { assert!(validate_app_address(address).is_err(), "{address}"); }
        assert!(validate_app_address(&"a".repeat(MAX_DEEP_LINK_BYTES + 1)).is_err());
    }
    #[test] fn validates_and_bounds_queue() {
        let mut router = DeepLinkRouter::new(DeepLinkConfig { schemes: BTreeSet::from(["jadix".into()]), queue_limit: 2, ..Default::default() }).unwrap();
        for source in [DeepLinkSource::ColdStart, DeepLinkSource::HotStart, DeepLinkSource::Handoff] { router.accept(DeepLink { url: "jadix://open/item".into(), source }).unwrap(); }
        assert_eq!(router.pending(), 2); assert_eq!(router.drain()[0].source, DeepLinkSource::HotStart);
        assert!(router.accept(DeepLink { url: "https://outside".into(), source: DeepLinkSource::HotStart }).is_err());
    }
}
