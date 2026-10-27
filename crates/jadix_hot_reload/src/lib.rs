use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};

#[derive(Debug, Clone)]
pub enum HotReloadCommand {
    Reload,
    TemplatePatch(TemplatePatchMsg),
    AssetReload(Vec<String>),
    StylesheetReload(Vec<String>),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IncomingMessage {
    Reload,
    TemplatePatch {
        file: String,
        #[serde(default)]
        templates: Vec<TemplateFragment>,
    },
    AssetReload {
        #[serde(default)]
        paths: Vec<String>,
    },
    StylesheetReload {
        #[serde(default)]
        paths: Vec<String>,
    },
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct TemplatePatchMsg {
    pub file: String,
    #[serde(default)]
    pub templates: Vec<TemplateFragment>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct TemplateFragment {
    pub id: String,
    pub hash: String,
    pub len: usize,
    #[serde(default)]
    pub index: usize,
    #[serde(default)]
    pub text_literals: Vec<String>,
    #[serde(default)]
    pub text_replacements: Vec<TextReplacement>,
    #[serde(default)]
    pub style_attrs: Vec<StyleAttr>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct StyleAttr {
    #[serde(default)]
    pub element_index: usize,
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct TextReplacement {
    #[serde(default)]
    pub index: usize,
    pub from: String,
    pub to: String,
}

pub struct HotReloadReceiver {
    receiver: mpsc::Receiver<HotReloadCommand>,
    pending: Arc<AtomicUsize>,
}

impl HotReloadReceiver {
    pub fn spawn(
        addr: String,
        wake: impl Fn() + Send + Sync + 'static,
        ready: impl Fn(&str) + Send + Sync + 'static,
        error: impl Fn(&str) + Send + Sync + 'static,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();
        let pending = Arc::new(AtomicUsize::new(0));
        let thread_pending = Arc::clone(&pending);
        std::thread::spawn(move || {
            let listener = match TcpListener::bind(&addr) {
                Ok(listener) => listener,
                Err(reason) => {
                    error(&format!("bind failed on {addr}: {reason}"));
                    return;
                }
            };
            ready(&addr);
            for connection in listener.incoming() {
                let stream = match connection {
                    Ok(stream) => stream,
                    Err(reason) => {
                        error(&format!("accept failed: {reason}"));
                        continue;
                    }
                };
                let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                loop {
                    line.clear();
                    let length = match reader
                        .by_ref()
                        .take(4 * 1024 * 1024 + 1)
                        .read_line(&mut line)
                    {
                        Ok(length) => length,
                        Err(_) => break,
                    };
                    if length == 0 || length > 4 * 1024 * 1024 {
                        break;
                    }
                    if let Some(command) = parse_command(line.trim()) {
                        thread_pending.fetch_add(1, Ordering::Relaxed);
                        if sender.send(command).is_err() {
                            thread_pending.fetch_sub(1, Ordering::Relaxed);
                            return;
                        }
                        wake();
                    }
                }
            }
        });
        Self { receiver, pending }
    }

    pub fn try_recv(&self) -> Result<HotReloadCommand, mpsc::TryRecvError> {
        let command = self.receiver.try_recv()?;
        self.pending.fetch_sub(1, Ordering::Relaxed);
        Ok(command)
    }

    pub fn has_pending(&self) -> bool {
        self.pending.load(Ordering::Relaxed) > 0
    }
}

pub fn parse_command(raw: &str) -> Option<HotReloadCommand> {
    if raw.eq_ignore_ascii_case("reload") {
        return Some(HotReloadCommand::Reload);
    }
    match serde_json::from_str(raw).ok()? {
        IncomingMessage::Reload => Some(HotReloadCommand::Reload),
        IncomingMessage::TemplatePatch { file, templates } => {
            Some(HotReloadCommand::TemplatePatch(TemplatePatchMsg {
                file,
                templates,
            }))
        }
        IncomingMessage::AssetReload { paths } => Some(HotReloadCommand::AssetReload(paths)),
        IncomingMessage::StylesheetReload { paths } => {
            Some(HotReloadCommand::StylesheetReload(paths))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_reload_command() {
        assert!(matches!(
            parse_command("reload"),
            Some(HotReloadCommand::Reload)
        ));
        assert!(matches!(
            parse_command("RELOAD"),
            Some(HotReloadCommand::Reload)
        ));
    }

    #[test]
    fn decodes_template_patch_using_shared_protocol_types() {
        let message = r#"{"kind":"template_patch","file":"src/main.rs","templates":[{"id":"root","hash":"abc","len":1,"index":0,"text_literals":["old"],"text_replacements":[{"index":0,"from":"old","to":"new"}],"style_attrs":[{"element_index":0,"name":"color","value":"red"}]}]}"#;
        let Some(HotReloadCommand::TemplatePatch(patch)) = parse_command(message) else {
            panic!("template patch decoded");
        };
        assert_eq!(patch.file, "src/main.rs");
        assert_eq!(patch.templates[0].id, "root");
        assert_eq!(patch.templates[0].text_replacements[0].to, "new");
        assert_eq!(patch.templates[0].style_attrs[0].name, "color");
    }

    #[test]
    fn rejects_unknown_or_malformed_messages() {
        assert!(parse_command("not-json").is_none());
        assert!(parse_command(r#"{"kind":"unknown"}"#).is_none());
    }
}
