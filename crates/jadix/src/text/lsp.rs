use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{mpsc, Arc, Mutex},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguageServerSettings {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

type Feedback = Arc<dyn Fn(Value) + Send + Sync>;

enum Outbound {
    Message(Value),
    Ready(Value),
}
static NEXT_REQUEST: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub struct Session {
    sender: mpsc::SyncSender<Outbound>,
    child: Arc<Mutex<Child>>,
}

fn publish(feedback: &Feedback, value: Value) {
    feedback(value);
}

impl Session {
    /// Starts an explicitly configured server; feedback runs on transport threads.
    /// UI consumers must dispatch the callback to their own UI scheduler.
    pub fn start(
        settings: &LanguageServerSettings,
        root: &Path,
        feedback: impl Fn(Value) + Send + Sync + 'static,
    ) -> Result<Self, String> {
        let feedback: Feedback = Arc::new(feedback);
        let reader_feedback = feedback.clone();
        let mut command = Command::new(&settings.command);
        command
            .args(&settings.args)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command
            .spawn()
            .map_err(|error| format!("{}: {error}", settings.command))?;
        let mut input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let child = Arc::new(Mutex::new(child));
        let (sender, requests) = mpsc::sync_channel::<Outbound>(128);
        let control = sender.clone();
        std::thread::spawn(move || {
            let feedback = reader_feedback;
            let mut output = BufReader::new(output);
            loop {
                match read_message(&mut output) {
                    Ok(value) if value.get("id") == Some(&json!(0)) => {
                        if control.send(Outbound::Ready(value)).is_err() {
                            break;
                        }
                    }
                    Ok(value) if value.get("method").is_some() && value.get("id").is_some() => {
                        let result = match value["method"].as_str() {
                            Some("workspace/configuration") => Some(Value::Array(
                                value["params"]["items"]
                                    .as_array()
                                    .map(|items| items.iter().map(|_| json!({})).collect())
                                    .unwrap_or_default(),
                            )),
                            Some(
                                "client/registerCapability"
                                | "client/unregisterCapability"
                                | "window/workDoneProgress/create",
                            ) => Some(Value::Null),
                            _ => None,
                        };
                        let response = if let Some(result) = result {
                            json!({"jsonrpc":"2.0","id":value["id"],"result":result})
                        } else {
                            json!({"jsonrpc":"2.0","id":value["id"],"error":{"code":-32601,"message":"Client request is not supported"}})
                        };
                        if control.send(Outbound::Message(response)).is_err() {
                            break;
                        }
                    }
                    Ok(value) => publish(&feedback, value),
                    Err(error) => {
                        publish(
                            &feedback,
                            json!({"method":"jadix/error","params":{"message":error}}),
                        );
                        break;
                    }
                }
            }
        });
        let root_uri = file_uri(root);
        std::thread::spawn(move || {
            let initialize = json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"processId":std::process::id(),"rootUri":root_uri,"capabilities":{"general":{"positionEncodings":["utf-16"]},"textDocument":{"synchronization":{"didSave":true},"completion":{"completionItem":{"snippetSupport":false}},"hover":{"contentFormat":["plaintext","markdown"]}}}}});
            if let Err(error) = write_message(&mut input, &initialize) {
                publish(
                    &feedback,
                    json!({"method":"jadix/error","params":{"message":error.to_string()}}),
                );
                return;
            }
            let mut ready = false;
            let mut sync_kind = 1;
            let mut pending = Vec::new();
            let mut documents = HashMap::<String, String>::new();
            while let Ok(command) = requests.recv() {
                let mut messages = match command {
                    Outbound::Ready(value) => {
                        if value.get("error").is_some() {
                            publish(
                                &feedback,
                                json!({"method":"jadix/error","params":{"message":value["error"]["message"]}}),
                            );
                            break;
                        }
                        if value["result"]["capabilities"]["positionEncoding"]
                            .as_str()
                            .is_some_and(|encoding| encoding != "utf-16")
                        {
                            publish(
                                &feedback,
                                json!({"method":"jadix/error","params":{"message":"Server selected an unsupported position encoding (UTF-16 required)"}}),
                            );
                            break;
                        }
                        let sync = &value["result"]["capabilities"]["textDocumentSync"];
                        sync_kind = sync
                            .as_u64()
                            .or_else(|| sync["change"].as_u64())
                            .unwrap_or(1);
                        ready = true;
                        let mut messages =
                            vec![json!({"jsonrpc":"2.0","method":"initialized","params":{}})];
                        messages.append(&mut pending);
                        messages
                    }
                    Outbound::Message(value) if !ready => {
                        if pending.len() < 128 {
                            pending.push(value);
                        } else {
                            publish(
                                &feedback,
                                json!({"method":"jadix/error","params":{"message":"LSP initialization backlog exceeded; restart the server"}}),
                            );
                        }
                        continue;
                    }
                    Outbound::Message(value) => vec![value],
                };
                for message in &mut messages {
                    let method = message["method"].as_str().unwrap_or("").to_string();
                    let uri = message["params"]["textDocument"]["uri"]
                        .as_str()
                        .unwrap_or("")
                        .to_string();
                    match method.as_str() {
                        "textDocument/didOpen" => {
                            documents.insert(
                                uri.clone(),
                                message["params"]["textDocument"]["text"]
                                    .as_str()
                                    .unwrap_or("")
                                    .to_string(),
                            );
                        }
                        "textDocument/didChange" => {
                            if sync_kind == 0 {
                                continue;
                            }
                            let text = message["params"]["contentChanges"][0]["text"]
                                .as_str()
                                .unwrap_or("")
                                .to_string();
                            if sync_kind == 2 {
                                let old = documents.get(&uri).map(String::as_str).unwrap_or("");
                                message["params"]["contentChanges"][0]["range"] = json!({"start":{"line":0,"character":0},"end":position(old, old.len())});
                            }
                            documents.insert(uri.clone(), text);
                        }
                        "textDocument/didClose" => {
                            documents.remove(&uri);
                        }
                        _ => {}
                    }
                    if let Err(error) = write_message(&mut input, message) {
                        publish(
                            &feedback,
                            json!({"method":"jadix/error","params":{"message":error.to_string()}}),
                        );
                        return;
                    }
                }
            }
        });
        Ok(Self { sender, child })
    }

    pub fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        self.send(json!({"jsonrpc":"2.0","method":method,"params":params}))
    }
    pub fn request(&self, method: &str, params: Value) -> Result<u64, String> {
        let id = NEXT_REQUEST.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))?;
        Ok(id)
    }
    fn send(&self, message: Value) -> Result<(), String> {
        self.sender
            .try_send(Outbound::Message(message))
            .map_err(|error| error.to_string())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn write_message(writer: &mut impl Write, message: &Value) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(message)?;
    write!(writer, "Content-Length: {}\r\n\r\n", bytes.len())?;
    writer.write_all(&bytes)?;
    writer.flush()
}

fn read_message(reader: &mut impl BufRead) -> Result<Value, String> {
    let mut length = None;
    let mut header_bytes = 0;
    loop {
        let mut line = String::new();
        let count = (&mut *reader)
            .take(8193)
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
        if count == 0 {
            return Err("Language server disconnected".into());
        }
        header_bytes += count;
        if header_bytes > 8192 {
            return Err("LSP header exceeds budget".into());
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = Some(
                    value
                        .trim()
                        .parse::<usize>()
                        .map_err(|error| error.to_string())?,
                );
            }
        }
    }
    let length = length
        .filter(|length| *length <= 16 * 1024 * 1024)
        .ok_or("Invalid LSP content length")?;
    let mut body = vec![0; length];
    reader
        .read_exact(&mut body)
        .map_err(|error| error.to_string())?;
    serde_json::from_slice(&body).map_err(|error| error.to_string())
}

pub fn file_uri(path: &Path) -> String {
    let absolute = std::fs::canonicalize(path).unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        }
    });
    let path = absolute.to_string_lossy().replace('\\', "/");
    let path = path.strip_prefix("//?/").unwrap_or(&path);
    let prefix = if path.starts_with("//") {
        "file:"
    } else if path.starts_with('/') {
        "file://"
    } else {
        "file:///"
    };
    let encoded: String = path
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-._~/:".contains(&byte) {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect();
    format!("{prefix}{encoded}")
}

pub fn position(text: &str, byte: usize) -> Value {
    let mut byte = byte.min(text.len());
    while !text.is_char_boundary(byte) {
        byte -= 1;
    }
    let prefix = &text[..byte];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count();
    let start = prefix.rfind('\n').map_or(0, |offset| offset + 1);
    json!({"line":line,"character":prefix[start..].trim_end_matches('\r').encode_utf16().count()})
}

pub fn byte_at(text: &str, position: &Value) -> Option<usize> {
    let line = position["line"].as_u64()? as usize;
    let character = position["character"].as_u64()? as usize;
    let start = if line == 0 {
        0
    } else {
        text.match_indices('\n').nth(line - 1)?.0 + 1
    };
    let row = text[start..].split('\n').next()?.trim_end_matches('\r');
    let mut units = 0;
    for (offset, value) in row.char_indices() {
        if units == character {
            return Some(start + offset);
        }
        units += value.len_utf16();
        if units > character {
            return None;
        }
    }
    (units == character).then_some(start + row.len())
}

pub fn apply_edits(text: &str, edits: &[Value]) -> Result<String, String> {
    let mut edits: Vec<_> = edits
        .iter()
        .map(|edit| {
            let start = byte_at(text, &edit["range"]["start"]).ok_or("Invalid edit start")?;
            let end = byte_at(text, &edit["range"]["end"]).ok_or("Invalid edit end")?;
            if start > end {
                return Err("Reversed edit range");
            }
            Ok((
                start,
                end,
                edit["newText"].as_str().ok_or("Missing edit text")?,
            ))
        })
        .collect::<Result<_, &str>>()
        .map_err(str::to_string)?;
    edits.sort_by_key(|edit| edit.0);
    if edits.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err("Overlapping LSP edits".into());
    }
    let mut result = text.to_string();
    for (start, end, inserted) in edits.into_iter().rev() {
        result.replace_range(start..end, inserted);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_framing_and_utf16_edits() {
        let message = json!({"text":"中文😀"});
        let mut bytes = Vec::new();
        write_message(&mut bytes, &message).unwrap();
        assert_eq!(
            read_message(&mut std::io::Cursor::new(bytes)).unwrap(),
            message
        );
        let text = "a😀中\r\nz";
        assert_eq!(position(text, 5), json!({"line":0,"character":3}));
        assert_eq!(byte_at(text, &json!({"line":0,"character":2})), None);
        let edit = json!({"range":{"start":{"line":0,"character":1},"end":{"line":0,"character":3}},"newText":"X"});
        assert_eq!(apply_edits(text, &[edit]).unwrap(), "aX中\r\nz");
    }
    #[test]
    fn rejects_oversized_messages_and_overlapping_edits() {
        assert!(read_message(&mut std::io::Cursor::new(
            b"Content-Length: 20000000\r\n\r\n"
        ))
        .is_err());
        let edit = json!({"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":2}},"newText":"X"});
        assert!(apply_edits("abcd", &[edit.clone(), edit]).is_err());
    }
}
