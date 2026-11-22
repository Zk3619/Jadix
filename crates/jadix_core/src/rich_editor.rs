//! Shared contract for native rich editors. Payloads are bounded before any platform view is touched.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RichEditorSpec { pub html: String, #[serde(default)] pub editable: bool, #[serde(default)] pub selection: Option<[usize; 2]>, #[serde(default)] pub revision: u64 }
impl RichEditorSpec {
    pub const MAX_HTML: usize = 1_048_576;
    /// Selection is a half-open UTF-16 range in the rendered document, not HTML
    /// bytes. Native editors clamp it after parsing, at character boundaries.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.html.len() > Self::MAX_HTML || self.html.contains('\0')
            || self.selection.is_some_and(|r| r[0] > r[1] || r[1] > Self::MAX_HTML * 2)
        { return Err("invalid rich editor specification"); }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag="type", rename_all="snake_case", deny_unknown_fields)]
pub enum RichEditorEvent { Change { html: String, revision: u64 }, Selection { range: [usize; 2], revision: u64 }, Focus { focused: bool, revision: u64 } }
impl RichEditorEvent { pub fn validate(&self)->Result<(), &'static str> { match self { Self::Change{html,..} if html.len()<=RichEditorSpec::MAX_HTML && !html.contains('\0')=>Ok(()), Self::Selection{range,..} if range[0]<=range[1]=>Ok(()), Self::Focus{..}=>Ok(()), _=>Err("invalid rich editor event") } } }
