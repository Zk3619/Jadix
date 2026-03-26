use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

mod rsx_text_order;

const DEFAULT_ADDR: &str = "127.0.0.1:39091";
/// 合并同一轮编辑器/fs 触发的多条 notify。
const DEBOUNCE_MS: u64 = 400;
/// 轮询兜底：两次发送至少间隔这么久，避免 15s 窗口内反复扫到同一文件。
const POLL_DEBOUNCE_MS: u64 = 2500;
/// 轮询只关心「最近刚动过」的源文件，窗口过长会与 notify 重复推送。
const POLL_RECENT_MS: u64 = 1200;
/// notify 路径为空时的兜底扫描窗口（略大于一次保存的抖动）。
const NOTIFY_EMPTY_FALLBACK_MS: u64 = 800;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let watch_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or(std::env::current_dir()?);
    let addr = std::env::args().nth(2).unwrap_or_else(|| DEFAULT_ADDR.to_string());

    let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
    let mut watcher = RecommendedWatcher::new(
        move |res| {
            let _ = tx.send(res);
        },
        Config::default(),
    )?;
    watcher.watch(&watch_path, RecursiveMode::Recursive)?;

    println!(
        "[rui-hot-reload-cli] watching {} -> {}",
        watch_path.display(),
        addr
    );

    event_loop(rx, &addr, &watch_path);
    Ok(())
}

fn event_loop(rx: Receiver<notify::Result<Event>>, addr: &str, watch_path: &Path) {
    let mut last_sent = Instant::now() - Duration::from_millis(DEBOUNCE_MS);
    let mut last_poll_sent = Instant::now() - Duration::from_millis(POLL_DEBOUNCE_MS);
    let mut last_wire_payload: Option<String> = None;
    let mut template_cache: HashMap<String, Vec<TemplateFragment>> = build_initial_cache(watch_path);
    loop {
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(Ok(event)) => {
                let kind = event.kind;
                let mut changed: Vec<PathBuf> = event
                    .paths
                    .iter()
                    .filter(|p| is_tracked_project_file(p, watch_path))
                    .cloned()
                    .collect();
                while let Ok(Ok(ev)) = rx.try_recv() {
                    for p in ev.paths {
                        if is_tracked_project_file(&p, watch_path) {
                            changed.push(p);
                        }
                    }
                }
                changed.sort();
                changed.dedup();
                if changed.is_empty() {
                    changed = collect_recent_tracked_files(watch_path, NOTIFY_EMPTY_FALLBACK_MS);
                }
                if changed.is_empty() {
                    continue;
                }
                let now = Instant::now();
                if now.duration_since(last_sent) < Duration::from_millis(DEBOUNCE_MS) {
                    continue;
                }
                let (rust_files, wgsl_files) = partition_rust_and_wgsl(&changed, watch_path);
                if rust_files.is_empty() && wgsl_files.is_empty() {
                    continue;
                }
                match send_dev_batch(
                    addr,
                    &rust_files,
                    &wgsl_files,
                    &mut template_cache,
                    &mut last_wire_payload,
                ) {
                    Ok(Some((template_count, wgsl_count))) => {
                        println!(
                            "[rui-hot-reload-cli] reload: {:?}, files={}, templates={}, wgsl={}, first={}",
                            kind,
                            changed.len(),
                            template_count,
                            wgsl_count,
                            changed[0].display()
                        );
                        last_sent = now;
                    }
                    Ok(None) => {}
                    Err(err) => {
                        eprintln!("[rui-hot-reload-cli] send error: {err}");
                    }
                }
            }
            Ok(Err(err)) => {
                eprintln!("[rui-hot-reload-cli] watch error: {err}");
            }
            Err(RecvTimeoutError::Timeout) => {
                // Fallback polling: some editors/filesystems may miss notify events.
                if last_poll_sent.elapsed() < Duration::from_millis(POLL_DEBOUNCE_MS) {
                    continue;
                }
                let changed = collect_recent_tracked_files(watch_path, POLL_RECENT_MS);
                if changed.is_empty() {
                    continue;
                }
                let (rust_files, wgsl_files) = partition_rust_and_wgsl(&changed, watch_path);
                if rust_files.is_empty() && wgsl_files.is_empty() {
                    continue;
                }
                match send_dev_batch(
                    addr,
                    &rust_files,
                    &wgsl_files,
                    &mut template_cache,
                    &mut last_wire_payload,
                ) {
                    Ok(Some((template_count, wgsl_count))) => {
                        println!(
                            "[rui-hot-reload-cli] poll reload: files={}, templates={}, wgsl={}, first={}",
                            changed.len(),
                            template_count,
                            wgsl_count,
                            changed[0].display()
                        );
                        last_poll_sent = Instant::now();
                        last_sent = Instant::now();
                    }
                    Ok(None) => {
                        last_poll_sent = Instant::now();
                    }
                    Err(err) => {
                        eprintln!("[rui-hot-reload-cli] poll send error: {err}");
                    }
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn collect_recent_tracked_files(root: &Path, within_ms: u64) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let now = std::time::SystemTime::now();
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            let Ok(entries) = fs::read_dir(&path) else {
                continue;
            };
            for entry in entries.flatten() {
                let p = entry.path();
                if is_ignored_dir_path(&p) {
                    continue;
                }
                stack.push(p);
            }
            continue;
        }
        if !is_tracked_project_file(&path, root) {
            continue;
        }
        let Ok(modified) = meta.modified() else {
            continue;
        };
        let Ok(delta) = now.duration_since(modified) else {
            continue;
        };
        if delta <= Duration::from_millis(within_ms) {
            out.push(path);
        }
    }
    out
}

fn build_initial_cache(root: &Path) -> HashMap<String, Vec<TemplateFragment>> {
    let mut cache = HashMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            let Ok(entries) = fs::read_dir(&path) else {
                continue;
            };
            for entry in entries.flatten() {
                let p = entry.path();
                if is_ignored_dir_path(&p) {
                    continue;
                }
                stack.push(p);
            }
            continue;
        }
        if !is_tracked_project_file(&path, root) {
            continue;
        }
        if !is_rust_source(&path) {
            continue;
        }
        let key = normalize_path_for_id(&path);
        let parsed = collect_templates_for_file(&path, &key);
        if !parsed.is_empty() {
            cache.insert(key, parsed);
        }
    }
    cache
}

fn is_rust_source(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("rs"))
        .unwrap_or(false)
}

fn under_crates_or_examples(path: &Path, root: &Path) -> bool {
    let rel = path.strip_prefix(root).ok().unwrap_or(path);
    let mut comps = rel.components();
    let head = comps
        .next()
        .map(|c| c.as_os_str().to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    head == "crates" || head == "examples"
}

fn is_project_source_file(path: &Path, root: &Path) -> bool {
    is_rust_source(path) && under_crates_or_examples(path, root)
}

fn is_tracked_wgsl(path: &Path, root: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("wgsl"))
        .unwrap_or(false)
        && under_crates_or_examples(path, root)
}

fn is_tracked_project_file(path: &Path, root: &Path) -> bool {
    is_project_source_file(path, root) || is_tracked_wgsl(path, root)
}

fn partition_rust_and_wgsl(paths: &[PathBuf], root: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut rust = Vec::new();
    let mut wgsl = Vec::new();
    for p in paths {
        if is_project_source_file(p, root) {
            rust.push(p.clone());
        } else if is_tracked_wgsl(p, root) {
            wgsl.push(p.clone());
        }
    }
    (rust, wgsl)
}

fn is_ignored_dir_path(path: &Path) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str().to_string_lossy().to_ascii_lowercase();
        s == "target" || s == "reference" || s == ".git"
    })
}

fn build_template_patch_msg(
    rust_files: &[PathBuf],
    template_cache: &mut HashMap<String, Vec<TemplateFragment>>,
) -> OutgoingTemplatePatch {
    let mut all_templates = Vec::new();
    let mut file_name = "unknown.rs".to_string();
    for file in rust_files {
        file_name = file.display().to_string();
        let key = normalize_path_for_id(file);
        let extracted = collect_templates_for_file(file, &key);
        if !extracted.is_empty() {
            let prev = template_cache.get(&key);
            let mut patched = Vec::new();

            for (idx, mut cur) in extracted.iter().cloned().enumerate() {
                cur.index = idx; // Assign stable sequence index
                // Note: CLI `id` already contains key:line:column from collect_templates_for_file
                
                let is_changed = if let Some(p_list) = prev {
                    if let Some(old) = p_list.iter().find(|p| p.hash == cur.hash) {
                        // Same content hash, but check if styles or position changed in a way that needs update
                        // Actually if hash is same, the logic usually doesn't need re-injection unless it's a line shift
                        // We always send if it's the first time or if exact ID (line) changed to help runtime track shifts.
                        old.id != cur.id || old.style_attrs != cur.style_attrs
                    } else {
                        true // Different hash
                    }
                } else {
                    true // New file
                };

                if is_changed {
                    if let Some(p_list) = prev {
                        apply_text_replacements_single(p_list, &mut cur);
                    }
                    patched.push(cur);
                }
            }
            
            template_cache.insert(key.clone(), extracted);
            all_templates.extend(patched);
        } else if let Some(cached) = template_cache.get(&key) {
            // Editor atomic-save may briefly expose an empty/partial file.
            // Reuse last good parse result to avoid dropping hot-reload updates.
            all_templates.extend(cached.clone());
        }
    }
    OutgoingTemplatePatch {
        kind: "template_patch",
        file: file_name,
        templates: all_templates,
    }
}

fn send_dev_batch(
    addr: &str,
    rust_files: &[PathBuf],
    wgsl_files: &[PathBuf],
    template_cache: &mut HashMap<String, Vec<TemplateFragment>>,
    last_wire_payload: &mut Option<String>,
) -> std::io::Result<Option<(usize, usize)>> {
    let mut wire_chunks: Vec<String> = Vec::new();
    let mut wgsl_n = 0usize;
    if !wgsl_files.is_empty() {
        let paths: Vec<String> = wgsl_files.iter().map(|p| normalize_path_for_id(p)).collect();
        let msg = OutgoingAssetReload {
            kind: "asset_reload",
            paths,
        };
        let payload = serde_json::to_string(&msg).unwrap_or_else(|_| "{\"kind\":\"reload\"}".into());
        wire_chunks.push(payload);
        wgsl_n = wgsl_files.len();
    }
    let template_count = if !rust_files.is_empty() {
        let msg = build_template_patch_msg(rust_files, template_cache);
        let n = msg.templates.len();
        let payload = serde_json::to_string(&msg).unwrap_or_else(|_| "{\"kind\":\"reload\"}".into());
        wire_chunks.push(payload);
        n
    } else {
        0
    };
    if wire_chunks.is_empty() {
        return Ok(Some((0, 0)));
    }
    let mut wire_fingerprint = String::new();
    for chunk in &wire_chunks {
        wire_fingerprint.push_str(chunk);
        wire_fingerprint.push('\n');
    }
    if last_wire_payload.as_ref() == Some(&wire_fingerprint) {
        return Ok(None);
    }
    *last_wire_payload = Some(wire_fingerprint);

    let mut stream = TcpStream::connect(addr)?;
    for chunk in &wire_chunks {
        stream.write_all(chunk.as_bytes())?;
        stream.write_all(b"\n")?;
    }
    stream.flush()?;
    Ok(Some((template_count, wgsl_n)))
}

#[derive(Serialize)]
struct OutgoingTemplatePatch {
    kind: &'static str,
    file: String,
    templates: Vec<TemplateFragment>,
}

#[derive(Serialize)]
struct OutgoingAssetReload {
    kind: &'static str,
    paths: Vec<String>,
}

#[derive(Serialize, Clone)]
struct TemplateFragment {
    id: String,
    hash: String,
    len: usize,
    #[serde(default)]
    index: usize,
    #[serde(default)]
    text_literals: Vec<String>,
    #[serde(default)]
    text_replacements: Vec<TextReplacement>,
    #[serde(default)]
    style_attrs: Vec<StyleAttr>,
}

#[derive(Serialize, Clone, PartialEq, Eq)]
struct StyleAttr {
    #[serde(default)]
    element_index: usize,
    name: String,
    value: String,
}

#[derive(Serialize, Clone)]
struct TextReplacement {
    index: usize,
    from: String,
    to: String,
}

fn stable_hash(text: &str) -> String {
    let normalized = normalize_template_source(text);
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in normalized.as_bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn normalize_template_source(source: &str) -> String {
    if let Ok(ts) = source.parse::<proc_macro2::TokenStream>() {
        return ts.to_string();
    }
    let mut out = String::with_capacity(source.len());
    let mut prev_ws = false;
    for ch in source.chars() {
        if ch.is_whitespace() {
            if !prev_ws {
                out.push(' ');
                prev_ws = true;
            }
        } else {
            out.push(ch);
            prev_ws = false;
        }
    }
    out.trim().to_string()
}

struct UiTemplateBlock {
    line: usize,
    column: usize,
    body: String,
}

fn extract_ui_templates(source: &str) -> Vec<UiTemplateBlock> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    let mut line_no = 1usize;
    let mut line_start = 0usize;
    let mut i = 0usize;
    while i + 2 < bytes.len() {
        if bytes[i] == b'\n' {
            line_no += 1;
            line_start = i + 1;
            i += 1;
            continue;
        }
        if bytes[i] == b'u' && bytes[i + 1] == b'i' && bytes[i + 2] == b'!' {
            let macro_line = line_no;
            let macro_column = i - line_start + 1;
            let mut j = i + 3;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j >= bytes.len() {
                break;
            }
            let open = bytes[j];
            let close = match open {
                b'{' => b'}',
                b'(' => b')',
                b'[' => b']',
                _ => {
                    i += 1;
                    continue;
                }
            };
            let mut depth = 0usize;
            let start = j + 1;
            let mut k = j;
            while k < bytes.len() {
                let ch = bytes[k];
                if ch == open {
                    depth += 1;
                } else if ch == close {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        if let Some(body) = source.get(start..k) {
                            out.push(UiTemplateBlock {
                                line: macro_line,
                                column: macro_column,
                                body: body.trim().to_string(),
                            });
                        }
                        i = k + 1;
                        break;
                    }
                }
                if ch == b'\n' {
                    line_no += 1;
                    line_start = k + 1;
                }
                k += 1;
            }
            if k >= bytes.len() {
                break;
            }
            continue;
        }
        i += 1;
    }
    out
}

fn normalize_path_for_id(path: &Path) -> String {
    let normalized = if let Ok(cwd) = std::env::current_dir() {
        if let Ok(rel) = path.strip_prefix(cwd) {
            rel.to_path_buf()
        } else {
            path.to_path_buf()
        }
    } else {
        path.to_path_buf()
    };
    normalized.to_string_lossy().replace('\\', "/")
}


fn collect_templates_for_file(file: &Path, key: &str) -> Vec<TemplateFragment> {
    const READ_RETRIES: usize = 3;
    const RETRY_SLEEP_MS: u64 = 25;
    for attempt in 0..READ_RETRIES {
        if let Ok(source) = fs::read_to_string(file) {
            let templates: Vec<TemplateFragment> = extract_ui_templates(&source)
                .into_iter()
                .map(|tpl| TemplateFragment {
                    id: format!("{}:{}:{}", key, tpl.line, tpl.column),
                    hash: stable_hash(&tpl.body),
                    len: tpl.body.len(),
                    index: 0, // Assigned in build_template_patch_msg
                    text_literals: rsx_text_order::extract_text_literals_tree_order(&tpl.body),
                    text_replacements: Vec::new(),
                    style_attrs: rsx_text_order::extract_style_attributes_tree_order(&tpl.body),
                })
                .collect();
            if !templates.is_empty() {
                return templates;
            }
        }
        if attempt + 1 < READ_RETRIES {
            std::thread::sleep(Duration::from_millis(RETRY_SLEEP_MS));
        }
    }
    Vec::new()
}

fn apply_text_replacements_single(prev: &[TemplateFragment], cur: &mut TemplateFragment) {
    // Try to find previous version by index first (most stable for line shifts)
    let old = prev.get(cur.index).or_else(|| prev.iter().find(|p| p.id == cur.id));
    let Some(old) = old else {
        return;
    };
    let n = old.text_literals.len().min(cur.text_literals.len());
    let mut replacements = Vec::new();
    for i in 0..n {
        if old.text_literals[i] != cur.text_literals[i] {
            replacements.push(TextReplacement {
                index: i,
                from: old.text_literals[i].clone(),
                to: cur.text_literals[i].clone(),
            });
        }
    }
    cur.text_replacements = replacements;
}

#[allow(dead_code)]
fn apply_text_replacements(prev: &[TemplateFragment], next: &mut [TemplateFragment]) {
    for cur in next.iter_mut() {
        apply_text_replacements_single(prev, cur);
    }
}
