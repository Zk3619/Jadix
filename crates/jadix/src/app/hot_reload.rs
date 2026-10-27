#[cfg(all(debug_assertions, feature = "hot-reload"))]
use super::JadixUserEvent;
#[cfg(all(debug_assertions, feature = "hot-reload"))]
use jadix_hot_reload::HotReloadCommand;
#[cfg(any(all(debug_assertions, feature = "hot-reload"), test))]
use jadix_hot_reload::TemplatePatchMsg;
pub use jadix_hot_reload::{StyleAttr, TemplateFragment, TextReplacement};
#[cfg(any(all(debug_assertions, feature = "hot-reload"), test))]
use std::collections::HashMap;
#[cfg(all(debug_assertions, feature = "hot-reload"))]
const DEFAULT_ADDR: &str = "127.0.0.1:39091";
#[cfg(all(debug_assertions, feature = "hot-reload"))]
pub(super) struct HotReloadState {
    receiver: jadix_hot_reload::HotReloadReceiver,
}
#[cfg(all(debug_assertions, feature = "hot-reload"))]
impl HotReloadState {
    pub(super) fn new() -> Self {
        let addr =
            std::env::var("JADIX_HOT_RELOAD_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_string());
        Self {
            receiver: jadix_hot_reload::HotReloadReceiver::spawn(
                addr,
                || {
                    let _ = super::ui_bridge::send_user_event(JadixUserEvent::HotReloadWake);
                },
                super::hmr_log::ready,
                super::hmr_log::error,
            ),
        }
    }
    pub(super) fn drain_pending(&self) -> HotReloadBatch {
        let mut batch = HotReloadBatch::default();
        let mut by_id = HashMap::new();
        let mut assets = Vec::new();
        while let Ok(command) = self.receiver.try_recv() {
            batch.count += 1;
            match command {
                HotReloadCommand::Reload => batch.force_reload = true,
                HotReloadCommand::TemplatePatch(patch) => {
                    ingest_template_patch(&mut batch, patch, &mut by_id)
                }
                HotReloadCommand::AssetReload(paths) => {
                    for path in paths {
                        if !assets.contains(&path) {
                            assets.push(path);
                        }
                    }
                }
                HotReloadCommand::StylesheetReload(paths) => {
                    for path in paths {
                        if !batch.stylesheet_paths.contains(&path) {
                            batch.stylesheet_paths.push(path);
                        }
                    }
                }
            }
        }
        batch.merged_templates = by_id.into_values().collect();
        batch
            .merged_templates
            .sort_by(|left, right| left.id.cmp(&right.id));
        batch.asset_paths = assets;
        batch
    }
    pub(super) fn has_pending(&self) -> bool {
        self.receiver.has_pending()
    }
}
#[cfg(all(debug_assertions, not(feature = "hot-reload")))]
pub(super) struct HotReloadState;
#[cfg(all(debug_assertions, not(feature = "hot-reload")))]
impl HotReloadState {
    pub(super) fn new() -> Self {
        Self
    }
    pub(super) fn drain_pending(&self) -> HotReloadBatch {
        HotReloadBatch::default()
    }
    pub(super) fn has_pending(&self) -> bool {
        false
    }
}
#[cfg(any(debug_assertions, test))]
#[derive(Debug, Clone, Default)]
pub(super) struct HotReloadBatch {
    pub(super) count: usize,
    pub(super) last_patch_file: String,
    pub(super) force_reload: bool,
    pub(super) merged_templates: Vec<TemplateFragment>,
    pub(super) asset_paths: Vec<String>,
    pub(super) stylesheet_paths: Vec<String>,
}
#[cfg(any(all(debug_assertions, feature = "hot-reload"), test))]
fn ingest_template_patch(
    batch: &mut HotReloadBatch,
    patch: TemplatePatchMsg,
    by_id: &mut HashMap<String, TemplateFragment>,
) {
    if batch.last_patch_file.is_empty() {
        batch.last_patch_file = patch.file.clone();
    }
    if patch.templates.is_empty() {
        return;
    }
    for template in patch.templates {
        by_id.insert(template.id.clone(), template);
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
fn has_any_template_id_inner(el: &crate::core::tree::Element, expected_ids: &[String]) -> bool {
    crate::core::tree::find_in_tree(el, |node| {
        element_matches_any_template_id(node, expected_ids)
    })
    .is_some()
}

#[cfg(debug_assertions)]
fn element_matches_any_template_id(
    el: &crate::core::tree::Element,
    expected_ids: &[String],
) -> bool {
    let mut ids = Vec::new();
    collect_element_template_keys(el, &mut ids);
    expected_ids.iter().any(|want| {
        ids.iter()
            .any(|have| template_id_rank(want, have).is_some())
    })
}

#[cfg(any(debug_assertions, test))]
fn collect_element_template_keys(el: &crate::core::tree::Element, out: &mut Vec<String>) {
    if let Some(id) = el.template_id.as_ref() {
        out.push(id.clone());
    }
    if let Some(id) = el.get_attr(crate::app::reactive::ATTR_SOURCE_TEMPLATE_ID) {
        out.push(id.clone());
    }
}

/// Same `ui!` call-site (`file:line` / `file:line:column`) after path-suffix normalize.
/// Does **not** treat column as a sequence index — that made every same-indent `ui!`
/// in a file collide, so DFS applied the patch to the first one.
#[cfg(any(debug_assertions, test))]
pub(super) fn template_id_matches(incoming: &str, runtime: &str) -> bool {
    matches!(template_id_rank(incoming, runtime), Some(score) if score >= STRICT_MATCH_SCORE)
}

/// Rank a CLI / runtime `template_id` pair. `None` = not the same slot.
///
/// - exact normalized string → 10000
/// - same path suffix + same line (+ same column when both present) → 9000
/// - same path suffix + same column + line drifted by at most [`LINE_SHIFT_MAX`] → 5000−dist
#[cfg(any(debug_assertions, test))]
pub(super) fn template_id_rank(incoming: &str, runtime: &str) -> Option<i32> {
    let a = normalize_template_id(incoming);
    let b = normalize_template_id(runtime);
    if a == b {
        return Some(EXACT_MATCH_SCORE);
    }
    let pa = parse_template_id_parts(&a);
    let pb = parse_template_id_parts(&b);
    if pa.line == 0 || pb.line == 0 {
        return None;
    }
    if !path_suffix_component_match(&pa.path, &pb.path) {
        return None;
    }
    let col_ok = pa.column == 0 || pb.column == 0 || pa.column == pb.column;
    if pa.line == pb.line && col_ok {
        return Some(STRICT_MATCH_SCORE);
    }
    if pa.column != 0 && pa.column == pb.column {
        let dist = (pa.line as isize - pb.line as isize).abs();
        if dist > 0 && dist <= LINE_SHIFT_MAX {
            return Some(LINE_SHIFT_BASE_SCORE - dist as i32);
        }
    }
    None
}

#[cfg(any(debug_assertions, test))]
const EXACT_MATCH_SCORE: i32 = 10_000;
#[cfg(any(debug_assertions, test))]
const STRICT_MATCH_SCORE: i32 = 9_000;
#[cfg(any(debug_assertions, test))]
const LINE_SHIFT_BASE_SCORE: i32 = 5_000;
#[cfg(any(debug_assertions, test))]
const LINE_SHIFT_MAX: isize = 20;

#[cfg(any(debug_assertions, test))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ParsedTemplateId {
    pub path: String,
    pub line: usize,
    /// 0 = absent (`file:line` legacy / opaque ids).
    pub column: usize,
}

#[cfg(any(debug_assertions, test))]
pub(super) fn parse_template_id(id: &str) -> ParsedTemplateId {
    parse_template_id_parts(&normalize_template_id(id))
}

#[cfg(any(debug_assertions, test))]
fn normalize_template_id(id: &str) -> String {
    id.replace('\\', "/").to_ascii_lowercase()
}

/// `file:line:column` from `ui!` / CLI. Last two numeric segments are line and column —
/// never a source-order block index (that lives on `TemplateFragment.index`).
#[cfg(any(debug_assertions, test))]
fn parse_template_id_parts(id: &str) -> ParsedTemplateId {
    let mut parts: Vec<String> = id.split(':').map(|s| s.to_string()).collect();
    // Windows paths: `d:/projects/.../main.rs:188` splits as ["d", "/projects/.../main.rs", "188"].
    if parts.len() >= 3 {
        let head = &parts[0];
        if head.len() == 1
            && head.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            && parts[1].starts_with('/')
        {
            let mut tail = parts.split_off(2);
            let drive = parts.remove(0);
            let path_rest = parts.remove(0);
            parts.push(format!("{drive}:{path_rest}"));
            parts.append(&mut tail);
        }
    }
    if parts.len() >= 3 {
        if let (Ok(line), Ok(column)) = (
            parts[parts.len() - 2].parse::<usize>(),
            parts[parts.len() - 1].parse::<usize>(),
        ) {
            if line > 0 {
                return ParsedTemplateId {
                    path: parts[..parts.len() - 2].join(":"),
                    line,
                    column,
                };
            }
        }
    }
    if parts.len() >= 2 {
        if let Ok(line) = parts[parts.len() - 1].parse::<usize>() {
            if line > 0 {
                return ParsedTemplateId {
                    path: parts[..parts.len() - 1].join(":"),
                    line,
                    column: 0,
                };
            }
        }
    }
    ParsedTemplateId {
        path: id.to_string(),
        line: 0,
        column: 0,
    }
}

/// Child-index path to the unique best `template_id` for a hot-reload patch.
/// Line-shift candidates that tie are rejected (do not guess).
#[cfg(any(debug_assertions, test))]
pub(super) fn find_best_template_path(
    root: &crate::core::tree::Element,
    incoming_id: &str,
) -> Option<Vec<usize>> {
    let mut best_path: Option<Vec<usize>> = None;
    let mut best_score = i32::MIN;
    let mut best_count = 0u32;
    score_template_paths(
        root,
        incoming_id,
        &mut Vec::new(),
        &mut best_path,
        &mut best_score,
        &mut best_count,
    );
    if best_count == 0 {
        return None;
    }
    if best_count > 1 && best_score < STRICT_MATCH_SCORE {
        return None;
    }
    best_path
}

#[cfg(any(debug_assertions, test))]
fn score_template_paths(
    el: &crate::core::tree::Element,
    incoming_id: &str,
    path: &mut Vec<usize>,
    best_path: &mut Option<Vec<usize>>,
    best_score: &mut i32,
    best_count: &mut u32,
) {
    struct Frame<'a> {
        el: &'a crate::core::tree::Element,
        next_child: usize,
    }
    fn consider(
        el: &crate::core::tree::Element,
        incoming_id: &str,
        path: &[usize],
        best_path: &mut Option<Vec<usize>>,
        best_score: &mut i32,
        best_count: &mut u32,
    ) {
        let mut keys = Vec::new();
        collect_element_template_keys(el, &mut keys);
        for id in &keys {
            if let Some(score) = template_id_rank(incoming_id, id) {
                if score > *best_score {
                    *best_score = score;
                    *best_path = Some(path.to_vec());
                    *best_count = 1;
                } else if score == *best_score {
                    *best_count += 1;
                }
            }
        }
    }

    consider(el, incoming_id, path, best_path, best_score, best_count);
    let mut stack = vec![Frame { el, next_child: 0 }];
    loop {
        let next = {
            let Some(frame) = stack.last_mut() else {
                break;
            };
            if frame.next_child >= frame.el.children.len() {
                None
            } else {
                let i = frame.next_child;
                frame.next_child += 1;
                Some((
                    i,
                    &frame.el.children[i] as *const crate::core::tree::Element,
                ))
            }
        };
        let Some((i, child_ptr)) = next else {
            stack.pop();
            if !stack.is_empty() {
                path.pop();
            }
            continue;
        };
        path.push(i);
        // SAFETY: child lives in the caller's tree; Frames only store that borrow.
        let child = unsafe { &*child_ptr };
        consider(child, incoming_id, path, best_path, best_score, best_count);
        stack.push(Frame {
            el: child,
            next_child: 0,
        });
    }
}

#[cfg(any(debug_assertions, test))]
pub(super) fn follow_template_path_mut<'a>(
    root: &'a mut crate::core::tree::Element,
    path: &[usize],
) -> Option<&'a mut crate::core::tree::Element> {
    let mut cursor = root;
    for &index in path {
        cursor = cursor.children.get_mut(index)?;
    }
    Some(cursor)
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
    // (e.g. `examples/gallery/src/main.rs`).
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
#[allow(dead_code)]
pub(super) fn collect_template_ids(root: &crate::core::tree::Element, out: &mut Vec<String>) {
    collect_element_template_keys(root, out);
    for child in &root.children {
        collect_template_ids(child, out);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ingest_template_patch, template_id_matches, HotReloadBatch, TemplateFragment,
        TemplatePatchMsg,
    };

    #[test]
    fn template_id_match_accepts_same_tail_components_with_same_line() {
        assert!(template_id_matches(
            "d:/projects/jadix/examples/gallery/src/main.rs:188",
            "examples/gallery/src/main.rs:188",
        ));
    }

    #[test]
    fn template_id_match_rejects_partial_segment_suffix_collisions() {
        assert!(!template_id_matches(
            "d:/projects/jadix/examples/gallery/src/main.rs:188",
            "gallery/src/domain.rs:188",
        ));
        assert!(!template_id_matches(
            "d:/projects/jadix/examples/gallery/src/main.rs:188",
            "examples/gallery/src/main.rs.bak:188",
        ));
        assert!(!template_id_matches(
            "d:/projects/jadix/examples/gallery/src/main.rs:188",
            "src/main.rs:188",
        ));
    }

    #[test]
    fn template_id_match_requires_same_line() {
        assert!(!template_id_matches(
            "examples/gallery/src/main.rs:188",
            "examples/gallery/src/main.rs:189",
        ));
    }

    #[test]
    fn template_id_match_does_not_treat_column_as_block_index() {
        // Same indent, different ui! blocks must not collide.
        assert!(!template_id_matches(
            "examples/gallery/src/main.rs:188:9",
            "examples/gallery/src/main.rs:400:9",
        ));
        assert!(template_id_matches(
            "d:/projects/jadix/examples/gallery/src/main.rs:400:9",
            "examples/gallery/src/main.rs:400:9",
        ));
    }

    #[test]
    fn template_id_rank_picks_nearby_line_shift_same_column() {
        let near = super::template_id_rank(
            "d:/projects/jadix/examples/gallery/src/main.rs:402:9",
            "examples/gallery/src/main.rs:400:9",
        );
        let far = super::template_id_rank(
            "d:/projects/jadix/examples/gallery/src/main.rs:402:9",
            "examples/gallery/src/main.rs:188:9",
        );
        assert!(near.is_some_and(|s| s < 9_000));
        assert!(far.is_none());
    }

    #[test]
    fn find_best_template_path_prefers_matching_line_not_first_same_column() {
        use crate::core::tree::Element;
        let root = Element::new().children(vec![
            Element::new()
                .template_id("examples/gallery/src/main.rs:188:9")
                .text("first"),
            Element::new()
                .template_id("examples/gallery/src/main.rs:400:9")
                .text("second"),
        ]);
        let path = super::find_best_template_path(
            &root,
            "d:/projects/jadix/examples/gallery/src/main.rs:400:9",
        )
        .expect("second block");
        assert_eq!(path, vec![1]);
        let shifted = super::find_best_template_path(
            &root,
            "d:/projects/jadix/examples/gallery/src/main.rs:401:9",
        )
        .expect("line-shifted second block");
        assert_eq!(shifted, vec![1]);
    }

    #[test]
    fn find_best_template_path_uses_preserved_source_id_on_reactive_island() {
        use crate::core::tree::Element;
        let mut island = Element::new()
            .template_id("examples/gallery/App:section_body")
            .text("导航");
        island.set_attr(
            crate::app::reactive::ATTR_SOURCE_TEMPLATE_ID,
            "examples/gallery/src/main.rs:4334:21",
        );
        let root = Element::new().children(vec![island]);
        let path = super::find_best_template_path(
            &root,
            "d:/projects/jadix/examples/gallery/src/main.rs:4334:21",
        )
        .expect("source template id on island");
        assert_eq!(path, vec![0]);
    }

    #[test]
    fn ingest_template_patch_empty_templates_is_noop() {
        let mut batch = HotReloadBatch::default();
        let mut by_template_id = std::collections::HashMap::new();
        ingest_template_patch(
            &mut batch,
            TemplatePatchMsg {
                file: "examples/gallery/src/main.rs".to_string(),
                templates: vec![],
            },
            &mut by_template_id,
        );
        assert!(
            !batch.force_reload,
            "empty templates must not rebuild from app_builder placeholders"
        );
        assert_eq!(batch.last_patch_file, "examples/gallery/src/main.rs");
        assert!(by_template_id.is_empty());
    }

    #[test]
    fn ingest_template_patch_merges_by_template_id() {
        let mut batch = HotReloadBatch::default();
        let mut by_template_id = std::collections::HashMap::new();
        ingest_template_patch(
            &mut batch,
            TemplatePatchMsg {
                file: "a.rs".to_string(),
                templates: vec![TemplateFragment {
                    id: "slot:a".to_string(),
                    hash: "h1".to_string(),
                    len: 1,
                    index: 0,
                    text_literals: vec!["A".to_string()],
                    text_replacements: vec![],
                    style_attrs: vec![],
                }],
            },
            &mut by_template_id,
        );
        ingest_template_patch(
            &mut batch,
            TemplatePatchMsg {
                file: "b.rs".to_string(),
                templates: vec![TemplateFragment {
                    id: "slot:a".to_string(),
                    hash: "h2".to_string(),
                    len: 2,
                    index: 1,
                    text_literals: vec!["B".to_string()],
                    text_replacements: vec![],
                    style_attrs: vec![],
                }],
            },
            &mut by_template_id,
        );
        assert!(!batch.force_reload);
        assert_eq!(batch.last_patch_file, "a.rs");
        assert_eq!(by_template_id.len(), 1);
        let merged = by_template_id.get("slot:a").expect("slot:a present");
        assert_eq!(merged.hash, "h2");
        assert_eq!(merged.text_literals, vec!["B".to_string()]);
    }

    /// WP-R.6: hot-reload inject may fuzzy-match path suffix; `reconcile_subtree`
    /// must **not** — it only equals after slash/case normalize.
    #[test]
    fn reconcile_anchor_id_is_exact_not_fuzzy() {
        let incoming = "d:/projects/jadix/examples/gallery/src/main.rs:188";
        let runtime = "examples/gallery/src/main.rs:188";
        assert!(
            template_id_matches(incoming, runtime),
            "dev patches still use fuzzy path-suffix matching"
        );
        let a = incoming.replace('\\', "/").to_ascii_lowercase();
        let b = runtime.replace('\\', "/").to_ascii_lowercase();
        assert_ne!(
            a, b,
            "structure-island reconcile must not treat these as the same slot"
        );
    }
}
