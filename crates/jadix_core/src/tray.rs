//! Status item content shared by native hosts and script applications.
use serde::{Deserialize, Serialize};
use crate::hosted::{HostedContent, HostedMenuItem};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrayTitleRun { pub text: String, pub color: Option<[u8; 4]> }
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrayTitlePlacement { Left, #[default] Right }
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tray {
    #[serde(default)] pub title: Vec<TrayTitleRun>,
    #[serde(default)] pub placement: TrayTitlePlacement,
    #[serde(default)] pub tooltip: String,
    /// SF Symbol on Apple hosts. An omitted symbol uses the application icon.
    pub symbol: Option<String>,
    #[serde(default)] pub menu: Vec<HostedMenuItem>,
}
impl Tray {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.title.len() > 128 || self.title.iter().map(|run| run.text.len()).sum::<usize>() > 4096
            || self.title.iter().any(|run| run.text.contains('\0')) || self.tooltip.len() > 4096
            || self.tooltip.contains('\0') || self.symbol.as_ref().is_some_and(|s| s.is_empty() || s.len() > 128 || s.contains('\0')) {
            return Err("invalid tray content");
        }
        HostedContent::MenuButton { label: String::new(), enabled: true, items: self.menu.clone() }.validate()
    }
    /// Hosts without attributed title support retain the complete text.
    pub fn plain_title(&self) -> String { self.title.iter().map(|run| run.text.as_str()).collect() }
    pub fn accepts_command(&self, id: &str) -> bool {
        HostedContent::MenuButton { label: String::new(), enabled: true, items: self.menu.clone() }
            .accepts_event("select", &serde_json::Value::String(id.into()))
    }
}
