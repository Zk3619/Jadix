use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AutosaveSettings {
    pub enabled: bool,
    pub delay_ms: u64,
}
impl Default for AutosaveSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            delay_ms: 1500,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguageServerSettings {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorSettings {
    pub autosave: AutosaveSettings,
    pub language_servers: BTreeMap<String, LanguageServerSettings>,
}
impl EditorSettings {
    pub fn load() -> Result<Self, String> {
        let path = std::env::var_os("JADIX_EDITOR_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("jadix-editor.json"));
        let mut settings = match std::fs::read_to_string(&path) {
            Ok(source) => serde_json::from_str::<Self>(&source)
                .map_err(|error| format!("{}: {error}", path.display()))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(error) => return Err(format!("{}: {error}", path.display())),
        };
        if let Ok(delay) = std::env::var("JADIX_AUTOSAVE_MS") {
            let delay = delay.parse::<u64>().map_err(|error| error.to_string())?;
            settings.autosave.enabled = delay != 0;
            settings.autosave.delay_ms = delay.max(250);
        }
        settings.autosave.delay_ms = settings.autosave.delay_ms.clamp(250, 3_600_000);
        Ok(settings)
    }
}
