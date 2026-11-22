//! Bounded native terminal sessions. IDs never alias a released session.
use jadix_core::terminal::{TerminalScreen, TerminalSpec};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "camelCase", deny_unknown_fields)]
enum Request {
    Create { specification: TerminalSpec },
    Configure { id: String, specification: TerminalSpec },
    Feed { id: String, data: Vec<u8> },
    Close { id: String },
}
#[derive(Default)]
pub(crate) struct Sessions { next: u64, screens: BTreeMap<u64, TerminalScreen> }
impl Sessions {
    pub(crate) fn dispatch(&mut self, input: &str) -> Result<String, String> {
        if input.len() > 512 * 1024 { return Err("terminal request exceeds budget".into()); }
        let request: Request = serde_json::from_str(input).map_err(|e| e.to_string())?;
        let result: Value = match request {
            Request::Create { specification } => {
                if self.screens.len() >= 128 { return Err("terminal session budget exceeded".into()); }
                let mut screen = TerminalScreen::new(specification).map_err(str::to_owned)?;
                let id = self.next.checked_add(1).ok_or("terminal session identity exhausted")?;
                self.next = id;
                let frame = screen.frame(); self.screens.insert(id, screen);
                json!({"id": id.to_string(), "frame": frame})
            }
            Request::Configure { id, specification } => {
                let screen = self.screens.get_mut(&id.parse::<u64>().map_err(|e| e.to_string())?).ok_or("terminal session disposed")?;
                screen.configure(specification).map_err(str::to_owned)?;
                json!(screen.frame())
            }
            Request::Feed { id, data } => {
                let screen = self.screens.get_mut(&id.parse::<u64>().map_err(|e| e.to_string())?).ok_or("terminal session disposed")?;
                screen.feed(&data).map_err(str::to_owned)?;
                json!(screen.frame())
            }
            Request::Close { id } => { self.screens.remove(&id.parse::<u64>().map_err(|e| e.to_string())?); Value::Null }
        };
        serde_json::to_string(&result).map_err(|e| e.to_string())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sessions_preserve_split_utf8_and_reject_released_identity() {
        let mut sessions = Sessions::default();
        let create: Value = serde_json::from_str(&sessions.dispatch(r#"{"operation":"create","specification":{"columns":8,"rows":2,"scrollback":0}}"#).unwrap()).unwrap();
        assert_eq!(create["id"], "1");
        sessions.dispatch(r#"{"operation":"feed","id":"1","data":[228,184]}"#).unwrap();
        let frame = sessions.dispatch(r#"{"operation":"feed","id":"1","data":[173]}"#).unwrap();
        assert!(frame.contains('中'));
        sessions.dispatch(r#"{"operation":"close","id":"1"}"#).unwrap();
        assert!(sessions.dispatch(r#"{"operation":"feed","id":"1","data":[]}"#).is_err());
        assert!(sessions.dispatch(r#"{"operation":"create","specification":{"columns":512,"rows":256,"scrollback":10000}}"#).is_err());
    }
}
