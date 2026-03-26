#[cfg(debug_assertions)]
use std::collections::HashMap;
#[cfg(debug_assertions)]
use std::io::{BufRead, BufReader};
#[cfg(debug_assertions)]
use std::net::TcpListener;
#[cfg(debug_assertions)]
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(debug_assertions)]
use std::sync::mpsc::{self, Receiver};
#[cfg(debug_assertions)]
use std::sync::Arc;
#[cfg(debug_assertions)]
use serde::Deserialize;
#[cfg(debug_assertions)]
use winit::event_loop::EventLoopProxy;

#[cfg(debug_assertions)]
const DEFAULT_ADDR: &str = "127.0.0.1:39091";

#[cfg(debug_assertions)]
pub(super) struct HotReloadState {
    rx: Receiver<HotReloadCommand>,
    pending_count: Arc<AtomicUsize>,
}

#[cfg(debug_assertions)]
impl HotReloadState {
    /// `wake`：TCP 线程收到补丁后唤醒主线程事件循环（`ControlFlow::Wait` 下否则可能一直阻塞到用户输入）。
    pub(super) fn new(wake: EventLoopProxy<()>) -> Self {
        let (tx, rx) = mpsc::channel();
        let pending_count = Arc::new(AtomicUsize::new(0));
        let pending_count_ref = Arc::clone(&pending_count);
        let wake_thread = wake.clone();
        let addr = std::env::var("RUI_HOT_RELOAD_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_string());
        std::thread::spawn(move || {
            let listener = match TcpListener::bind(&addr) {
                Ok(v) => v,
                Err(err) => {
                    log::warn!("hot-reload listener bind failed on {}: {}", addr, err);
                    return;
                }
            };
            log::warn!("hot-reload listener started at {}", addr);
            for conn in listener.incoming() {
                let stream = match conn {
                    Ok(v) => v,
                    Err(err) => {
                        log::warn!("hot-reload listener accept failed: {}", err);
                        continue;
                    }
                };
                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                loop {
                    line.clear();
                    let n = match reader.read_line(&mut line) {
                        Ok(v) => v,
                        Err(_) => break,
                    };
                    if n == 0 {
                        break;
                    }
                    if let Some(cmd) = parse_command(line.trim()) {
                        pending_count_ref.fetch_add(1, Ordering::Relaxed);
                        let _ = tx.send(cmd);
                        let _ = wake_thread.send_event(());
                    }
                }
            }
        });
        Self { rx, pending_count }
    }

    pub(super) fn drain_pending(&self) -> HotReloadBatch {
        let mut batch = HotReloadBatch::default();
        let mut by_template_id: HashMap<String, TemplateFragment> = HashMap::new();
        let mut asset_paths: Vec<String> = Vec::new();
        while let Ok(cmd) = self.rx.try_recv() {
            batch.count += 1;
            self.pending_count.fetch_sub(1, Ordering::Relaxed);
            match cmd {
                HotReloadCommand::Reload => batch.force_reload = true,
                HotReloadCommand::TemplatePatch(patch) => {
                    if batch.last_patch_file.is_empty() {
                        batch.last_patch_file = patch.file.clone();
                    }
                    if patch.templates.is_empty() {
                        // 解析到文件但无 ui! 块：退化为整树重建（与旧版「有 patch 但 templates 为空」行为一致）。
                        batch.force_reload = true;
                    } else {
                        for tpl in patch.templates {
                            by_template_id.insert(tpl.id.clone(), tpl);
                        }
                    }
                }
                HotReloadCommand::AssetReload(paths) => {
                    for p in paths {
                        if !asset_paths.iter().any(|x| x == &p) {
                            asset_paths.push(p);
                        }
                    }
                }
            }
        }
        batch.merged_templates = by_template_id.into_values().collect();
        batch.asset_paths = asset_paths;
        batch
    }

    pub(super) fn has_pending(&self) -> bool {
        self.pending_count.load(Ordering::Relaxed) > 0
    }
}

#[cfg(debug_assertions)]
#[derive(Debug, Clone, Default)]
pub(super) struct HotReloadBatch {
    pub(super) count: usize,
    /// 若本批含 `TemplatePatch`，记录其中一个源文件路径（仅用于日志）。
    pub(super) last_patch_file: String,
    pub(super) force_reload: bool,
    pub(super) merged_templates: Vec<TemplateFragment>,
    pub(super) asset_paths: Vec<String>,
}

#[cfg(debug_assertions)]
#[derive(Debug, Clone)]
pub(super) enum HotReloadCommand {
    Reload,
    TemplatePatch(TemplatePatchMsg),
    AssetReload(Vec<String>),
}

#[cfg(debug_assertions)]
#[derive(Debug, Clone, Deserialize)]
pub(super) struct TemplatePatchMsg {
    pub(super) file: String,
    #[serde(default)]
    pub(super) templates: Vec<TemplateFragment>,
}

#[cfg(debug_assertions)]
#[derive(Debug, Clone, Deserialize)]
pub(super) struct TemplateFragment {
    pub(super) id: String,
    pub(super) hash: String,
    pub(super) len: usize,
    #[serde(default)]
    pub(super) index: usize,
    #[serde(default)]
    pub(super) text_literals: Vec<String>,
    #[serde(default)]
    pub(super) text_replacements: Vec<TextReplacement>,
    #[serde(default)]
    pub(super) style_attrs: Vec<StyleAttr>,
}

#[cfg(debug_assertions)]
#[derive(Debug, Clone, Deserialize)]
pub(super) struct StyleAttr {
    #[serde(default)]
    pub(super) element_index: usize,
    pub(super) name: String,
    pub(super) value: String,
}

#[cfg(debug_assertions)]
#[derive(Debug, Clone, Deserialize)]
pub(super) struct TextReplacement {
    #[serde(default)]
    pub(super) index: usize,
    pub(super) from: String,
    pub(super) to: String,
}

#[cfg(debug_assertions)]
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum IncomingMessage {
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
}

#[cfg(debug_assertions)]
fn parse_command(raw: &str) -> Option<HotReloadCommand> {
    if raw.eq_ignore_ascii_case("reload") {
        return Some(HotReloadCommand::Reload);
    }
    let msg: IncomingMessage = serde_json::from_str(raw).ok()?;
    match msg {
        IncomingMessage::Reload => Some(HotReloadCommand::Reload),
        IncomingMessage::TemplatePatch { file, templates } => {
            Some(HotReloadCommand::TemplatePatch(TemplatePatchMsg { file, templates }))
        }
        IncomingMessage::AssetReload { paths } => Some(HotReloadCommand::AssetReload(paths)),
    }
}

#[cfg(debug_assertions)]
pub(super) fn has_any_template_id(
    root: &crate::core::tree::Element,
    expected_ids: &[String],
) -> bool {
    if expected_ids.is_empty() {
        return false;
    }
    has_any_template_id_inner(root, expected_ids)
}

#[cfg(debug_assertions)]
fn has_any_template_id_inner(
    el: &crate::core::tree::Element,
    expected_ids: &[String],
) -> bool {
    if let Some(id) = el.template_id.as_ref() {
        if expected_ids.iter().any(|v| template_id_matches(v, id)) {
            return true;
        }
    }
    for child in &el.children {
        if has_any_template_id_inner(child, expected_ids) {
            return true;
        }
    }
    false
}

/// `hot_reload_inject` 与单元测试（含 `cargo test --release`）会用到。
#[cfg(any(debug_assertions, test))]
pub(super) fn template_id_matches(incoming: &str, runtime: &str) -> bool {
    let a = normalize_template_id(incoming);
    let b = normalize_template_id(runtime);
    if a == b {
        return true;
    }
    let (ap, al, ai) = split_template_id_triple(&a);
    let (bp, bl, bi) = split_template_id_triple(&b);
    
    // If indices match, it's a very strong signal.
    if ai != usize::MAX && ai == bi {
        if path_suffix_component_match(&ap, &bp) {
            return true;
        }
    }

    // Fallback to line matching if indices aren't available or don't match (legacy/compatibility)
    if al != 0 && al == bl {
        if path_suffix_component_match(&ap, &bp) {
            return true;
        }
    }
    false
}

#[cfg(any(debug_assertions, test))]
fn normalize_template_id(id: &str) -> String {
    id.replace('\\', "/").to_ascii_lowercase()
}

#[cfg(any(debug_assertions, test))]
fn split_template_id_triple(id: &str) -> (String, usize, usize) {
    let parts: Vec<&str> = id.split(':').collect();
    if parts.len() >= 3 {
        let p = parts[..parts.len() - 2].join(":");
        let l = parts[parts.len() - 2].parse().unwrap_or(0);
        let i = parts[parts.len() - 1].parse().unwrap_or(usize::MAX);
        (p, l, i)
    } else if parts.len() == 2 {
        (parts[0].to_string(), parts[1].parse().unwrap_or(0), usize::MAX)
    } else {
        (id.to_string(), 0, usize::MAX)
    }
}


#[cfg(any(debug_assertions, test))]
fn path_suffix_component_match(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let ac: Vec<&str> = a.split('/').filter(|seg| !seg.is_empty()).collect();
    let bc: Vec<&str> = b.split('/').filter(|seg| !seg.is_empty()).collect();
    if ac.is_empty() || bc.is_empty() {
        return false;
    }
    // Avoid ambiguous matches like `src/main.rs` that can appear in many files.
    // Real template ids from this project include at least 3 path components
    // (e.g. `examples/hello_world/src/main.rs`).
    if ac.len().min(bc.len()) < 3 {
        return false;
    }
    let min_len = ac.len().min(bc.len());
    for i in 1..=min_len {
        if ac[ac.len() - i] != bc[bc.len() - i] {
            return false;
        }
    }
    true
}

#[cfg(debug_assertions)]
pub(super) fn collect_template_ids(root: &crate::core::tree::Element, out: &mut Vec<String>) {
    if let Some(id) = root.template_id.as_ref() {
        out.push(id.clone());
    }
    for child in &root.children {
        collect_template_ids(child, out);
    }
}

#[cfg(test)]
mod tests {
    use super::template_id_matches;

    #[test]
    fn template_id_match_accepts_same_tail_components_with_same_line() {
        assert!(template_id_matches(
            "d:/projects/rust-native-ui/examples/hello_world/src/main.rs:188",
            "examples/hello_world/src/main.rs:188",
        ));
    }

    #[test]
    fn template_id_match_rejects_partial_segment_suffix_collisions() {
        assert!(!template_id_matches(
            "d:/projects/rust-native-ui/examples/hello_world/src/main.rs:188",
            "hello_world/src/domain.rs:188",
        ));
        assert!(!template_id_matches(
            "d:/projects/rust-native-ui/examples/hello_world/src/main.rs:188",
            "examples/hello_world/src/main.rs.bak:188",
        ));
        assert!(!template_id_matches(
            "d:/projects/rust-native-ui/examples/hello_world/src/main.rs:188",
            "src/main.rs:188",
        ));
    }

    #[test]
    fn template_id_match_requires_same_line() {
        assert!(!template_id_matches(
            "examples/hello_world/src/main.rs:188",
            "examples/hello_world/src/main.rs:189",
        ));
    }
}
