use super::{lsp, CodeFile, Workspace};
use jadix::prelude::*;
use serde_json::{json, Value};
use std::{collections::HashMap, rc::Rc};

#[derive(Default)]
pub struct Clients {
    sessions: HashMap<String, lsp::Session>,
    opened: HashMap<usize, Opened>,
    pending: HashMap<u64, Pending>,
    failed: std::collections::HashSet<String>,
}
struct Opened {
    language: String,
    uri: String,
    text: String,
    saved: String,
    version: i64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Completion {
    pub label: String,
    pub edits: Vec<Value>,
    pub file: usize,
    pub snapshot: String,
}
struct Pending {
    file: CodeFile,
    text: String,
    byte: usize,
    method: String,
}

fn language(file: &CodeFile) -> Option<String> {
    let path = file.path.peek()?;
    Some(
        match path.extension()?.to_str()? {
            "rs" => "rust",
            "py" => "python",
            "js" | "jsx" => "javascript",
            "ts" | "tsx" => "typescript",
            "md" => "markdown",
            extension => extension,
        }
        .into(),
    )
}

pub fn install(workspace: &Rc<Workspace>) {
    use jadix::core::reactivity::{create_effect, dispose_effect, on_cleanup};
    let feedback = Signal::new(None::<Value>);
    let source = workspace.clone();
    let sync = create_effect(move || {
        let files = source.files.get();
        let mut clients = source.language.borrow_mut();
        for file in files {
            let path = file.path.get();
            let text = file.value.get();
            let saved = file.saved.get();
            let Some(path) = path else {
                continue;
            };
            let Some(language) = language(&file) else {
                continue;
            };
            let Some(settings) = source.settings.language_servers.get(&language).or_else(|| {
                path.extension()
                    .and_then(|extension| extension.to_str())
                    .and_then(|extension| source.settings.language_servers.get(extension))
            }) else {
                continue;
            };
            if !clients.sessions.contains_key(&language) && !clients.failed.contains(&language) {
                let root = std::env::current_dir().unwrap_or_else(|_| {
                    path.parent()
                        .unwrap_or(std::path::Path::new("."))
                        .to_path_buf()
                });
                match lsp::Session::start(settings, &root, feedback) {
                    Ok(session) => {
                        clients.sessions.insert(language.clone(), session);
                    }
                    Err(error) => {
                        clients.failed.insert(language.clone());
                        source.status.set(format!("LSP 启动失败：{error}"));
                        continue;
                    }
                }
            }
            let uri = lsp::file_uri(&path);
            let previous = clients.opened.remove(&file.id);
            let notify = |session: &lsp::Session, method: &str, params: Value| {
                if let Err(error) = session.notify(method, params) {
                    source.status.set(format!("LSP：{error}"));
                }
            };
            if let Some(previous) = previous
                .as_ref()
                .filter(|previous| previous.uri != uri || previous.language != language)
            {
                if let Some(session) = clients.sessions.get(&previous.language) {
                    notify(
                        session,
                        "textDocument/didClose",
                        json!({"textDocument":{"uri":previous.uri}}),
                    );
                }
            }
            let Some(session) = clients.sessions.get(&language) else {
                continue;
            };
            let mut opened = match previous
                .filter(|previous| previous.uri == uri && previous.language == language)
            {
                Some(opened) => opened,
                None => {
                    notify(
                        session,
                        "textDocument/didOpen",
                        json!({"textDocument":{"uri":uri,"languageId":language,"version":1,"text":text}}),
                    );
                    Opened {
                        language: language.clone(),
                        uri: uri.clone(),
                        text: text.clone(),
                        saved: saved.clone(),
                        version: 1,
                    }
                }
            };
            if opened.text != text {
                opened.version += 1;
                notify(
                    session,
                    "textDocument/didChange",
                    json!({"textDocument":{"uri":uri,"version":opened.version},"contentChanges":[{"text":text}]}),
                );
                opened.text = text;
            }
            if opened.saved != saved {
                notify(
                    session,
                    "textDocument/didSave",
                    json!({"textDocument":{"uri":uri},"text":saved}),
                );
                opened.saved = saved;
            }
            clients.opened.insert(file.id, opened);
        }
    });
    let target = workspace.clone();
    let events = create_effect(move || {
        if let Some(event) = feedback.get() {
            receive(&target, event);
        }
    });
    let cleanup = workspace.clone();
    on_cleanup(move || {
        dispose_effect(sync);
        dispose_effect(events);
        *cleanup.language.borrow_mut() = Clients::default();
    });
}

pub fn request(workspace: &Workspace, method: &str) {
    let file = workspace.current();
    let text = file.value.peek();
    let field = jadix::utils::document_registry::document_field_id(&format!(
        "jadix-ide://buffer/{}",
        file.id
    ));
    let byte = use_signal(
        &jadix::utils::text_field::input_caret_signal_key(&field),
        || None::<usize>,
    )
    .peek()
    .unwrap_or(0);
    let mut clients = workspace.language.borrow_mut();
    let Some(opened) = clients.opened.get(&file.id) else {
        workspace
            .status
            .set("请先打开文件，并在 jadix-editor.json 配置对应 language_servers".into());
        return;
    };
    let Some(session) = clients.sessions.get(&opened.language) else {
        return;
    };
    let mut params =
        json!({"textDocument":{"uri":opened.uri},"position":lsp::position(&text, byte)});
    if method == "textDocument/formatting" {
        params =
            json!({"textDocument":{"uri":opened.uri},"options":{"tabSize":4,"insertSpaces":true}});
    }
    match session.request(method, params) {
        Ok(id) => {
            clients.pending.insert(
                id,
                Pending {
                    file,
                    text,
                    byte,
                    method: method.into(),
                },
            );
            workspace.status.set(format!("LSP 请求：{method}"));
        }
        Err(error) => workspace.status.set(format!("LSP：{error}")),
    }
}

fn receive(workspace: &Workspace, event: Value) {
    if event["method"] == "jadix/error" {
        workspace.status.set(format!(
            "LSP：{}",
            event["params"]["message"].as_str().unwrap_or("连接失败")
        ));
        return;
    }
    if event["method"] == "textDocument/publishDiagnostics" {
        let current = workspace.current();
        if current.path.peek().is_some_and(|path| {
            lsp::file_uri(&path) == event["params"]["uri"].as_str().unwrap_or("")
        }) {
            let diagnostics = event["params"]["diagnostics"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let text = diagnostics
                .iter()
                .take(20)
                .map(|diagnostic| {
                    format!(
                        "{}:{}  {}",
                        diagnostic["range"]["start"]["line"].as_u64().unwrap_or(0) + 1,
                        diagnostic["range"]["start"]["character"]
                            .as_u64()
                            .unwrap_or(0)
                            + 1,
                        diagnostic["message"].as_str().unwrap_or("")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            workspace.lsp_panel.set(if text.is_empty() {
                "LSP：当前文件无诊断".into()
            } else {
                text
            });
        }
        return;
    }
    let Some(id) = event["id"].as_u64() else {
        return;
    };
    let Some(pending) = workspace.language.borrow_mut().pending.remove(&id) else {
        return;
    };
    if event.get("error").is_some() {
        workspace.status.set(format!(
            "LSP：{}",
            event["error"]["message"].as_str().unwrap_or("请求失败")
        ));
        return;
    }
    if pending.file.value.peek() != pending.text {
        workspace.status.set("文档已变化，忽略过期 LSP 响应".into());
        return;
    }
    let result = &event["result"];
    match pending.method.as_str() {
        "textDocument/completion" => {
            let items = result
                .as_array()
                .or_else(|| result["items"].as_array())
                .cloned()
                .unwrap_or_default();
            let completions = items.iter().filter(|item| item["insertTextFormat"].as_u64() != Some(2)).take(24).filter_map(|item| {
                let inserted = item["textEdit"]["newText"].as_str().or_else(|| item["insertText"].as_str()).or_else(|| item["label"].as_str())?;
                let mut edit = item["textEdit"].clone();
                if edit.is_null() { let mut start = pending.byte.min(pending.text.len()); while start > 0 { let previous = pending.text[..start].char_indices().last()?; if previous.1 != '_' && !previous.1.is_alphanumeric() { break; } start = previous.0; } edit = json!({"range":{"start":lsp::position(&pending.text, start),"end":lsp::position(&pending.text, pending.byte)},"newText":inserted}); }
                else if edit.get("range").is_none() { edit["range"] = edit["replace"].clone(); }
                let mut edits = vec![edit]; if let Some(additional) = item["additionalTextEdits"].as_array() { edits.extend_from_slice(additional); }
                Some(Completion { label: item["label"].as_str().unwrap_or(inserted).into(), edits, file: pending.file.id, snapshot: pending.text.clone() })
            }).collect::<Vec<_>>();
            workspace
                .lsp_panel
                .set(format!("{} 个补全候选（点击应用）", completions.len()));
            workspace.completions.set(completions);
        }
        "textDocument/formatting" => {
            if let Some(edits) = result.as_array() {
                apply(workspace, &pending.file, &pending.text, edits);
            }
        }
        "textDocument/hover" => {
            let contents = &result["contents"];
            workspace.lsp_panel.set(
                contents
                    .as_str()
                    .or_else(|| contents["value"].as_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| contents.to_string()),
            );
        }
        "textDocument/definition" => {
            let location = result
                .as_array()
                .and_then(|locations| locations.first())
                .unwrap_or(result);
            let uri = location["uri"]
                .as_str()
                .or_else(|| location["targetUri"].as_str())
                .unwrap_or("");
            workspace
                .lsp_panel
                .set(format!("定义：{uri}\n{}", location["range"]));
        }
        _ => {}
    }
}

pub fn complete(workspace: &Workspace, completion: &Completion) {
    if let Some(file) = workspace
        .files
        .peek()
        .into_iter()
        .find(|file| file.id == completion.file)
    {
        apply(workspace, &file, &completion.snapshot, &completion.edits);
    }
    workspace.completions.set(Vec::new());
}

fn apply(workspace: &Workspace, file: &CodeFile, snapshot: &str, edits: &[Value]) {
    if file.value.peek() != snapshot {
        workspace.status.set("文档已变化，请重新请求 LSP".into());
        return;
    }
    match lsp::apply_edits(snapshot, edits) {
        Ok(text) => {
            let field = jadix::utils::document_registry::document_field_id(&format!(
                "jadix-ide://buffer/{}",
                file.id
            ));
            jadix::utils::text_field::commit_value_with_policy(
                &field,
                text.clone(),
                jadix::utils::text_edit_history::SetTextUndoPolicy::Undoable,
            );
            file.value.set(text);
            workspace.status.set("已应用 LSP 编辑；可撤销".into());
        }
        Err(error) => workspace.status.set(format!("LSP 编辑失败：{error}")),
    }
}
