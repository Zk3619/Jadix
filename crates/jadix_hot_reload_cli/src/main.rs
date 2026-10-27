use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use jadix_hot_reload::{TemplateFragment, StyleAttr, TextReplacement};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

mod hmr_log;
mod options;
mod rsx_text_order;
mod supervisor;

const DEFAULT_ADDR: &str = "127.0.0.1:39091";
/// 合并同一轮编辑器/fs 触发的多条 notify。
const DEBOUNCE_MS: u64 = 400;
/// 轮询兜底：两次发送至少间隔这么久，avoid 15s window内反复扫到同一文件。
const POLL_DEBOUNCE_MS: u64 = 2500;
/// 轮询只关心「最近刚动过」的源文件，window过长会与 notify 重复推送。
const POLL_RECENT_MS: u64 = 1200;

fn display_watch_path(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn hmr_update_detail(
    first: &Path,
    root: &Path,
    templates: usize,
    wgsl: usize,
    styles: usize,
) -> String {
    let file = display_watch_path(first, root);
    match (templates, wgsl, styles) {
        (1, 0, 0) => format!("{file} (1 template)"),
        (n, 0, 0) => format!("{file} ({n} templates)"),
        (0, 1, 0) => format!("{file} (1 asset)"),
        (0, w, 0) => format!("{file} ({w} assets)"),
        (0, 0, 1) => format!("{file} (1 stylesheet)"),
        (0, 0, s) => format!("{file} ({s} stylesheets)"),
        (n, w, s) => format!("{file} ({n} templates, {w} assets, {s} stylesheets)"),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = options::Options::parse(std::env::args().skip(1))?;
    let watch_path = &options.root;
    let addr = &options.addr;

    let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
    let mut watcher = RecommendedWatcher::new(
        move |res| {
            let _ = tx.send(res);
        },
        Config::default(),
    )?;
    for path in &options.watch {
        watcher.watch(path, RecursiveMode::Recursive)?;
    }

    let template_cache = build_initial_cache(watch_path);
    hmr_log::watching(&format!("{} -> {}", watch_path.display(), addr));

    let mut supervisor = supervisor::Supervisor::new(&options)?;
    event_loop(rx, &options, template_cache, &mut supervisor);
    Ok(())
}

fn event_loop(
    rx: Receiver<notify::Result<Event>>,
    options: &options::Options,
    mut template_cache: HashMap<String, Vec<TemplateFragment>>,
    supervisor: &mut supervisor::Supervisor,
) {
    let addr = options.addr.as_str();
    let watch_path = options.root.as_path();
    let mut last_poll = Instant::now();
    let mut changed = std::collections::BTreeSet::new();
    let mut deadline = None;
    let mut last_wire_payload = None;
    while !supervisor::stop_requested() {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(event)) => {
                if !event.kind.is_modify() && !event.kind.is_create() && !event.kind.is_remove() {
                    continue;
                }
                let paths: Vec<_> = event
                    .paths
                    .into_iter()
                    .filter(|path| options.tracks(path))
                    .collect();
                if !paths.is_empty() {
                    changed.extend(paths);
                    deadline = Some(Instant::now() + Duration::from_millis(DEBOUNCE_MS));
                }
            }
            Ok(Err(err)) => hmr_log::error(&format!("watch failed: {err}")),
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
        if last_poll.elapsed() >= Duration::from_millis(POLL_DEBOUNCE_MS) {
            for path in &options.watch {
                changed.extend(
                    collect_recent_tracked_files(path, POLL_DEBOUNCE_MS + POLL_RECENT_MS)
                        .into_iter()
                        .filter(|file| options.tracks(file)),
                );
            }
            last_poll = Instant::now();
            if !changed.is_empty() && deadline.is_none() {
                deadline = Some(Instant::now() + Duration::from_millis(DEBOUNCE_MS));
            }
        }
        if deadline.is_none_or(|when| Instant::now() < when) || changed.is_empty() {
            continue;
        }
        let files: Vec<_> = changed.iter().cloned().collect();
        if supervisor.needs_restart(&files) {
            if let Err(error) = supervisor.restart() {
                hmr_log::error(&format!("restart failed: {error}"));
            }
            template_cache = build_initial_cache(watch_path);
            last_wire_payload = None;
            changed.clear();
            deadline = None;
            continue;
        }
        let (rust_files, wgsl_files, style_files) = partition_tracked(&files, watch_path);
        match send_dev_batch(
            addr,
            &rust_files,
            &wgsl_files,
            &style_files,
            &mut template_cache,
            &mut last_wire_payload,
        ) {
            Ok(outcome) => {
                if let SendOutcome::Sent {
                    templates,
                    wgsl,
                    styles,
                } = outcome
                {
                    hmr_log::update(&hmr_update_detail(
                        &files[0], watch_path, templates, wgsl, styles,
                    ));
                }
                changed.clear();
                deadline = None;
            }
            Err(err) => {
                hmr_log::error(&format!("send failed (will retry): {err}"));
                deadline = Some(Instant::now() + Duration::from_secs(1));
            }
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

fn within_workspace(path: &Path, root: &Path) -> bool {
    (path.starts_with(root) || (root == Path::new(".") && path.is_relative()))
        && !path
            .components()
            .any(|part| part == std::path::Component::ParentDir)
        && !is_ignored_dir_path(path)
}

fn is_project_source_file(path: &Path, root: &Path) -> bool {
    is_rust_source(path) && within_workspace(path, root)
}

fn is_tracked_wgsl(path: &Path, root: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("wgsl"))
        .unwrap_or(false)
        && within_workspace(path, root)
}

fn is_stylesheet_name(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        name.as_str(),
        "theme.toml"
            | "theme.json"
            | "styles.toml"
            | "styles.json"
            | "styles.css"
            | "styles.jadix"
            | "globals.css"
            | "globals.jadix"
    ) || ((name.starts_with("theme.") || name.starts_with("styles."))
        && matches!(ext.as_str(), "toml" | "json" | "css" | "jadix"))
}

fn is_tracked_stylesheet(path: &Path, root: &Path) -> bool {
    if !is_stylesheet_name(path) || is_ignored_dir_path(path) {
        return false;
    }
    within_workspace(path, root)
}

fn is_tracked_project_file(path: &Path, root: &Path) -> bool {
    if is_ignored_dir_path(path) {
        return false;
    }
    is_project_source_file(path, root)
        || is_tracked_wgsl(path, root)
        || is_tracked_stylesheet(path, root)
        || (path.starts_with(root)
            && path
                .file_name()
                .is_some_and(|name| name == "Cargo.toml" || name == "Cargo.lock"))
}

fn partition_tracked(paths: &[PathBuf], root: &Path) -> (Vec<PathBuf>, Vec<PathBuf>, Vec<PathBuf>) {
    let mut rust = Vec::new();
    let mut wgsl = Vec::new();
    let mut style = Vec::new();
    for p in paths {
        if is_project_source_file(p, root) {
            rust.push(p.clone());
        } else if is_tracked_wgsl(p, root) {
            wgsl.push(p.clone());
        } else if is_tracked_stylesheet(p, root) {
            style.push(p.clone());
        }
    }
    (rust, wgsl, style)
}

fn is_ignored_dir_path(path: &Path) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str().to_string_lossy().to_ascii_lowercase();
        matches!(
            s.as_str(),
            "target" | "reference" | ".git" | ".tmp" | ".codegraph" | "node_modules"
        )
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
                    if let Some(old) = p_list.get(idx) {
                        // Same content hash, but check if styles or position changed in a way that needs update
                        // Actually if hash is same, the logic usually doesn't need re-injection unless it's a line shift
                        // We always send if it's the first time or if exact ID (line) changed to help runtime track shifts.
                        old.hash != cur.hash
                            || old.id != cur.id
                            || old.style_attrs != cur.style_attrs
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
        }
        // Empty/partial parse: keep the cache and send nothing. Re-broadcasting
        // every cached `ui!` block used to apply unrelated pages onto the live
        // island (line-shift) or, with `templates: []`, force a full rebuild.
    }
    OutgoingTemplatePatch {
        kind: "template_patch",
        file: file_name,
        templates: all_templates,
    }
}

enum SendOutcome {
    Sent {
        templates: usize,
        wgsl: usize,
        styles: usize,
    },
    Duplicate,
    Empty,
}

fn send_dev_batch(
    addr: &str,
    rust_files: &[PathBuf],
    wgsl_files: &[PathBuf],
    style_files: &[PathBuf],
    template_cache: &mut HashMap<String, Vec<TemplateFragment>>,
    last_wire_payload: &mut Option<String>,
) -> std::io::Result<SendOutcome> {
    let mut next_cache = template_cache.clone();
    let mut wire_chunks: Vec<String> = Vec::new();
    let mut wgsl_n = 0usize;
    let mut style_n = 0usize;
    if !wgsl_files.is_empty() {
        let paths: Vec<String> = wgsl_files
            .iter()
            .map(|p| normalize_path_for_id(p))
            .collect();
        let msg = OutgoingAssetReload {
            kind: "asset_reload",
            paths,
        };
        let payload =
            serde_json::to_string(&msg).unwrap_or_else(|_| "{\"kind\":\"reload\"}".into());
        wire_chunks.push(payload);
        wgsl_n = wgsl_files.len();
    }
    if !style_files.is_empty() {
        let paths: Vec<String> = style_files
            .iter()
            .map(|p| normalize_path_for_id(p))
            .collect();
        let msg = OutgoingStylesheetReload {
            kind: "stylesheet_reload",
            paths,
        };
        let payload =
            serde_json::to_string(&msg).unwrap_or_else(|_| "{\"kind\":\"reload\"}".into());
        wire_chunks.push(payload);
        style_n = style_files.len();
    }
    let template_count = if !rust_files.is_empty() {
        let msg = build_template_patch_msg(rust_files, &mut next_cache);
        let n = msg.templates.len();
        if n > 0 {
            let payload =
                serde_json::to_string(&msg).unwrap_or_else(|_| "{\"kind\":\"reload\"}".into());
            wire_chunks.push(payload);
        }
        n
    } else {
        0
    };
    if wire_chunks.is_empty() {
        *template_cache = next_cache;
        return Ok(SendOutcome::Empty);
    }
    let mut wire_fingerprint = String::new();
    for chunk in &wire_chunks {
        wire_fingerprint.push_str(chunk);
        wire_fingerprint.push('\n');
    }
    if last_wire_payload.as_ref() == Some(&wire_fingerprint) {
        return Ok(SendOutcome::Duplicate);
    }
    let mut stream = TcpStream::connect(addr)?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    for chunk in &wire_chunks {
        stream.write_all(chunk.as_bytes())?;
        stream.write_all(b"\n")?;
    }
    stream.flush()?;
    *template_cache = next_cache;
    *last_wire_payload = Some(wire_fingerprint);
    Ok(SendOutcome::Sent {
        templates: template_count,
        wgsl: wgsl_n,
        styles: style_n,
    })
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

#[derive(Serialize)]
struct OutgoingStylesheetReload {
    kind: &'static str,
    paths: Vec<String>,
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
    fn visit(source: &str, stream: proc_macro2::TokenStream, out: &mut Vec<UiTemplateBlock>) {
        use proc_macro2::TokenTree;
        let tokens: Vec<_> = stream.into_iter().collect();
        let mut index = 0;
        while index < tokens.len() {
            if let (
                Some(TokenTree::Ident(name)),
                Some(TokenTree::Punct(bang)),
                Some(TokenTree::Group(body)),
            ) = (
                tokens.get(index),
                tokens.get(index + 1),
                tokens.get(index + 2),
            ) {
                if name == "ui" && bang.as_char() == '!' {
                    let position = name.span().start();
                    let offset = name.span().byte_range().start;
                    let column = source[..offset]
                        .rsplit('\n')
                        .next()
                        .unwrap_or("")
                        .chars()
                        .count()
                        + 1;
                    out.push(UiTemplateBlock {
                        line: position.line,
                        column,
                        body: source[body.span_open().byte_range().end
                            ..body.span_close().byte_range().start]
                            .trim()
                            .to_string(),
                    });
                    index += 3;
                    continue;
                }
            }
            if let TokenTree::Group(group) = &tokens[index] {
                visit(source, group.stream(), out);
            }
            index += 1;
        }
    }
    let mut out = Vec::new();
    if let Ok(stream) = source.parse() {
        visit(source, stream, &mut out);
    }
    out
}

fn normalize_path_for_id(path: &Path) -> String {
    let path_s = normalize_path_text(path);
    let Ok(cwd) = std::env::current_dir() else {
        return path_s;
    };
    let cwd_s = normalize_path_text(&cwd);
    strip_prefix_ci(&path_s, cwd_s.trim_end_matches('/'))
        .map(|rest| rest.trim_start_matches('/').to_string())
        .filter(|rest| !rest.is_empty())
        .unwrap_or(path_s)
}

fn normalize_path_text(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    if let Some(unc) = text.strip_prefix("//?/UNC/") {
        format!("//{unc}")
    } else {
        text.strip_prefix("//?/").unwrap_or(&text).to_string()
    }
}

fn strip_prefix_ci<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    let path_l = path.to_ascii_lowercase();
    let prefix_l = prefix.to_ascii_lowercase();
    let rest_l = path_l.strip_prefix(&prefix_l)?;
    if !rest_l.is_empty() && !rest_l.starts_with('/') {
        return None;
    }
    Some(&path[prefix.len().min(path.len())..])
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
            return templates;
        }
        if attempt + 1 < READ_RETRIES {
            std::thread::sleep(Duration::from_millis(RETRY_SLEEP_MS));
        }
    }
    Vec::new()
}

fn apply_text_replacements_single(prev: &[TemplateFragment], cur: &mut TemplateFragment) {
    // Try to find previous version by index first (most stable for line shifts)
    let old = prev
        .get(cur.index)
        .or_else(|| prev.iter().find(|p| p.id == cur.id));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_sources_are_not_limited_to_crates_and_examples() {
        let root = Path::new("workspace");
        for name in ["src/main.rs", "ui/widgets.rs", "shaders/app.wgsl"] {
            assert!(is_tracked_project_file(&root.join(name), root), "{name}");
        }
        assert!(!is_tracked_project_file(
            &root.join("target/generated.rs"),
            root
        ));
    }
    use std::path::Path;

    #[test]
    fn gallery_button_template_extracts_text_replacement() {
        let file = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/gallery/src/pages/basics.rs");
        let key = normalize_path_for_id(&file);
        let previous = collect_templates_for_file(&file, &key);
        let source = fs::read_to_string(&file).unwrap().replace("\r\n", "\n");
        let changed = source.replacen(
            "\"Secondary\"\n                    </Button>",
            "\"HMR VERIFIED\"\n                    </Button>",
            1,
        );
        assert_ne!(
            changed, source,
            "Gallery Button fixture must contain the target label"
        );
        let templates = extract_ui_templates(&changed);
        let block = templates
            .iter()
            .enumerate()
            .find(|(_, block)| block.body.contains("HMR VERIFIED"))
            .unwrap();
        let mut current = TemplateFragment {
            id: format!("{}:{}:{}", key, block.1.line, block.1.column),
            hash: stable_hash(&block.1.body),
            len: block.1.body.len(),
            index: block.0,
            text_literals: rsx_text_order::extract_text_literals_tree_order(&block.1.body),
            text_replacements: vec![],
            style_attrs: vec![],
        };
        apply_text_replacements_single(&previous, &mut current);
        assert!(
            current
                .text_replacements
                .iter()
                .any(|change| change.from == "Secondary" && change.to == "HMR VERIFIED"),
            "{:?}",
            current.text_literals
        );
    }

    #[test]
    fn strip_prefix_ci_ignores_drive_letter_case() {
        assert_eq!(
            strip_prefix_ci(
                "d:/projects/jadix/examples/gallery/src/main.rs",
                "D:/projects/jadix"
            ),
            Some("/examples/gallery/src/main.rs")
        );
        assert_eq!(
            strip_prefix_ci("d:/projects/jadix-extra/src/main.rs", "d:/projects/jadix"),
            None
        );
    }

    #[test]
    fn extract_ui_templates_records_char_column() {
        let src = "    ui! { \"hi\" }\n";
        let blocks = extract_ui_templates(src);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].line, 1);
        assert_eq!(blocks[0].column, 5);
        assert!(blocks[0].body.contains("hi"));
    }

    #[test]
    fn extraction_ignores_comments_strings_and_literal_braces() {
        let source = r##"// ui! { fake }
fn demo() {
    let ignored = "ui! { fake }";
    let ignored_raw = r#"ui! { fake }"#;
    ui! { <Box><Label> "}" </Label><Label> "ui! { literal }" </Label></Box> }
    ui! { <Box class="p-2" /> }
}"##;
        let blocks = extract_ui_templates(source);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].line, 5);
        assert_eq!(
            blocks[0].body,
            r#"<Box><Label> "}" </Label><Label> "ui! { literal }" </Label></Box>"#
        );
        assert_eq!(blocks[1].line, 6);
        assert_eq!(
            rsx_text_order::extract_text_literals_tree_order(&blocks[0].body),
            vec!["}", "ui! { literal }"]
        );
    }

    #[test]
    fn extraction_preserves_unicode_columns_and_unspaced_tags() {
        let blocks =
            extract_ui_templates("fn demo() { let 中文 = 1; ui! { <Label> \"hi\" </Label> } }");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].column, 25);
        assert!(blocks[0].body.starts_with("<Label>"));
    }

    #[test]
    fn failed_send_keeps_cache_and_fingerprint_for_retry() {
        use std::io::Read;
        use std::net::TcpListener;
        let root = std::env::temp_dir().join(format!("jadix-hmr-retry-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let file = root.join("source.rs");
        fs::write(&file, r#"fn demo() { ui! { <Label> "old" </Label> } }"#).unwrap();
        let files = vec![file.clone()];
        let mut cache = HashMap::new();
        build_template_patch_msg(&files, &mut cache);
        let key = normalize_path_for_id(&file);
        let old_hash = cache[&key][0].hash.clone();
        fs::write(&file, r#"fn demo() { ui! { <Label> "new" </Label> } }"#).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let mut fingerprint = None;
        assert!(send_dev_batch(
            &address.to_string(),
            &files,
            &[],
            &[],
            &mut cache,
            &mut fingerprint
        )
        .is_err());
        assert_eq!(cache[&key][0].hash, old_hash);
        assert!(fingerprint.is_none());
        let listener = TcpListener::bind(address).unwrap();
        assert!(matches!(
            send_dev_batch(
                &address.to_string(),
                &files,
                &[],
                &[],
                &mut cache,
                &mut fingerprint
            ),
            Ok(SendOutcome::Sent { templates: 1, .. })
        ));
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut payload = String::new();
        stream.read_to_string(&mut payload).unwrap();
        assert!(payload.contains("new"));
        assert_ne!(cache[&key][0].hash, old_hash);
        assert!(fingerprint.is_some());
        assert!(matches!(
            send_dev_batch(
                &address.to_string(),
                &files,
                &[],
                &[],
                &mut cache,
                &mut fingerprint
            ),
            Ok(SendOutcome::Empty)
        ));
        fs::remove_file(file).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn tracks_gallery_theme_and_styles_not_cargo_toml() {
        let root = Path::new(".");
        assert!(is_stylesheet_name(Path::new("examples/gallery/theme.toml")));
        assert!(is_stylesheet_name(Path::new("styles.css")));
        assert!(!is_stylesheet_name(Path::new("Cargo.toml")));
        assert!(is_tracked_stylesheet(
            Path::new("examples/gallery/styles.toml"),
            root
        ));
        assert!(!is_tracked_stylesheet(Path::new("Cargo.toml"), root));
    }

    #[test]
    fn normalize_path_for_id_uses_forward_slashes() {
        let p = Path::new("examples")
            .join("gallery")
            .join("src")
            .join("main.rs");
        let key = normalize_path_for_id(&p);
        assert!(!key.contains('\\'));
    }

    #[test]
    fn normalize_path_text_strips_windows_verbatim_prefixes() {
        assert_eq!(
            normalize_path_text(Path::new(r"\\?\C:\workspace\src\main.rs")),
            "C:/workspace/src/main.rs"
        );
        assert_eq!(
            normalize_path_text(Path::new(r"\\?\UNC\server\share\src\main.rs")),
            "//server/share/src/main.rs"
        );
        assert_eq!(
            normalize_path_text(Path::new(r"\\server\share\src\main.rs")),
            "//server/share/src/main.rs"
        );
    }
}
