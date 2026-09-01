//! # TreeView
//!
//! 树形结构：三种变体对位 ShadCN 社区常见树 / 手风琴文件树实践。
//!
//! ```text
//! TreeView                         // Root：variant + expanded 集合
//! └── TreeViewItem                 // 递归节点（RSX）；或 Builder `.roots([TreeNode])`
//!     └── TreeViewItem …           // 子节点
//! ```
//!
//! | `variant` | 实现 | 适用 |
//! |-----------|------|------|
//! | `flat` | DFS 扁平化 + [`virtual_list`] | 大数据、固定行高 |
//! | `collapsible` | 递归 [`collapsible_scope`] / `CollapsibleTrigger` | 文件树、侧边栏目录 |
//! | `accordion` | 每层兄弟 [`accordion_scope`] + `multiple` | 分组章节、可多项展开 |
//!
//! `<TreeView>` in `ui!` / `<TreeViewItem>` 由宏展开（[`tree_view_scope`] /
//! [`tree_view_item_scope`]）。Builder [`tree_view`] 与 RSX 共用实现。
//!
//! ## Usage (dual style)
//! ```ignore
//! // A · RSX（camelCase：expandedSignal / rowHeight）
//! ui! {
//!     <TreeView id="files" variant="collapsible" expandedSignal={expanded}>
//!         <TreeViewItem id="src" label="src">
//!             <TreeViewItem id="main" label="main.rs" />
//!             <TreeViewItem id="lib" label="lib.rs" />
//!         </TreeViewItem>
//!     </TreeView>
//! }
//!
//! // B · Builder（snake_case only）
//! tree_view("files")
//!     .variant("collapsible")
//!     .expanded_signal(expanded)
//!     .roots([TreeNode { id: "src".into(), label: "src".into(), children: vec![…] }])
//! ```
//!
//! **Identity (WP-2.8)**: `id` optional; empty string → [`resolve_instance_id`].

use std::collections::HashSet;
use std::sync::OnceLock;

use rui_macros::component;
use crate::components::accordion::{
    accordion_content, accordion_item_scope, accordion_scope, AccordionTrigger, AccordionTriggerProps,
};
use crate::components::collapsible::{
    collapsible_content, collapsible_scope, CollapsibleTrigger, CollapsibleTriggerProps,
};
use crate::components::scroll_area::scroll_area;
use crate::components::virtual_list::virtual_list;
use crate::core::reactivity::{
    create_context, create_effect, provide_context, use_context, use_scoped_signal, use_signal,
    with_child_owner, Context, Signal,
};
use crate::core::tree::{parse_text_color, Element, LayoutStatus};
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, vstack};
use crate::style::{
    css_transition_needs_frame, with_class_list, TransitionEntry, TransitionProperty,
};
use crate::tokens;
use crate::utils::lucide::lucide_glyph;
use crate::utils::motion::ensure_motion_transitions;
use crate::utils::resolve_instance_id;
use crate::utils::take_fragment;
use crate::utils::text_node::{build_icon_text_node, build_text_node, TextNodeConfig};
use crate::utils::tree_model::{self, TreeNodeLike};

const ATTR_SLOT: &str = "data-slot";
const SLOT_ROOT: &str = "tree-view";
const SLOT_ITEM: &str = "tree-view-item";
const SLOT_ROW: &str = "tree-view-row";
const SLOT_CHEVRON: &str = "tree-view-chevron";

const CHEVRON_PX: f32 = 16.0;
const CHEVRON_OPEN_DEG: f32 = 90.0;
const ROW_WEIGHT: u16 = 500;
const INDENT_PX: f32 = 16.0;
const ROW_PY: &str = tokens::SPACE_1;
const ROW_PX: &str = tokens::SPACE_2;

// ── Model ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct TreeNode {
    pub id: String,
    pub label: String,
    pub children: Vec<TreeNode>,
}

impl TreeNodeLike for TreeNode {
    fn id(&self) -> &str {
        &self.id
    }
    fn children(&self) -> &[Self] {
        &self.children
    }
}

/// 树展示变体（`variant` 轴）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TreeViewVariant {
    /// 虚拟列表扁平化（默认）。
    #[default]
    Flat,
    /// 递归 Collapsible 真树。
    Collapsible,
    /// 每层兄弟 Accordion（`multiple`）。
    Accordion,
}

impl TreeViewVariant {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "collapsible" | "nested" | "tree" => Self::Collapsible,
            "accordion" | "sections" => Self::Accordion,
            "flat" | "virtual" | "list" | "" => Self::Flat,
            _ => Self::Flat,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Flat => "flat",
            Self::Collapsible => "collapsible",
            Self::Accordion => "accordion",
        }
    }
}

impl From<&str> for TreeViewVariant {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for TreeViewVariant {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

// ── Context ───────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct TreeViewCtx {
    pub id: String,
    pub variant: TreeViewVariant,
    pub expanded: Signal<HashSet<String>>,
    /// `use_signal` storage key for [`expanded`] — re-resolve in long-lived closures.
    pub expanded_storage_key: String,
}

#[derive(Clone)]
pub struct TreeViewItemCtx {
    pub depth: usize,
}

fn tree_view_context() -> Context<TreeViewCtx> {
    static CTX: OnceLock<Context<TreeViewCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<TreeViewCtx>)
}

fn tree_view_item_context() -> Context<TreeViewItemCtx> {
    static CTX: OnceLock<Context<TreeViewItemCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<TreeViewItemCtx>)
}

pub fn use_tree_view() -> Option<TreeViewCtx> {
    use_context(tree_view_context())
}

pub fn use_tree_view_item() -> Option<TreeViewItemCtx> {
    use_context(tree_view_item_context())
}

fn require_tree_view(part: &str) -> TreeViewCtx {
    use_tree_view().unwrap_or_else(|| {
        panic!("{part} must be used within <TreeView> / tree_view_scope");
    })
}

// ── Flat row model ────────────────────────────────────────────────────────────

#[derive(Clone)]
struct FlatRow {
    id: String,
    label: String,
    depth: usize,
    has_children: bool,
}

fn flatten(nodes: &[TreeNode], expanded: &HashSet<String>, depth: usize, out: &mut Vec<FlatRow>) {
    *out = tree_model::flatten_expanded(nodes, expanded, depth, |n, d, has| FlatRow {
        id: n.id.clone(),
        label: n.label.clone(),
        depth: d,
        has_children: has,
    });
}

fn expanded_signal_for_key(key: &str) -> Signal<HashSet<String>> {
    use_signal(key, HashSet::<String>::new)
}

fn node_expanded(key: &str, node_id: &str) -> bool {
    expanded_signal_for_key(key)
        .try_peek()
        .is_some_and(|set| set.contains(node_id))
}

fn toggle_node_expanded_key(key: &str, node_id: &str) {
    let sig = expanded_signal_for_key(key);
    if !sig.is_alive() {
        return;
    }
    toggle_node_expanded(sig, node_id);
}

// ── Shared chrome ─────────────────────────────────────────────────────────────

fn chevron_right_icon() -> Element {
    let glyph = lucide_glyph("chevron-right").unwrap_or_else(|| "›".into());
    build_icon_text_node(glyph)
        .text_size(CHEVRON_PX)
        .text_color(parse_text_color("muted-foreground"))
}

fn chevron_transition_entries() -> [TransitionEntry; 1] {
    [TransitionEntry {
        property: TransitionProperty::Transform,
        duration_ms: 200.0,
        delay_ms: 0.0,
        timing: crate::style::TimingFunction::EaseOut,
    }]
}

fn sync_tree_chevron(el: &mut Element, open: bool) -> LayoutStatus {
    ensure_motion_transitions(&mut el.style, &chevron_transition_entries());
    let want_rotate = if open { CHEVRON_OPEN_DEG } else { 0.0 };
    let mut changed = false;
    if (el.style.rotate_deg - want_rotate).abs() > 0.01 {
        el.style.rotate_deg = want_rotate;
        changed = true;
    }
    let animating = css_transition_needs_frame(&el.style, &el.transition_snapshot)
        || el.transition_snapshot.has_active_tweens();
    if changed || animating {
        crate::app::damage::push_frame_paint_damage(el.layout_bounds);
        LayoutStatus::StyleChanged
    } else {
        LayoutStatus::Clean
    }
}

fn indent_pl(depth: usize) -> f32 {
    8.0 + depth as f32 * INDENT_PX
}

fn build_label_text(label: &str) -> Element {
    build_text_node(
        label,
        TextNodeConfig {
            size_px: 14.0,
            weight: ROW_WEIGHT,
            color: parse_text_color("foreground"),
            leading_scale: Some(1.25),
            ..Default::default()
        },
    )
    .flex_grow(1.0)
    .flex_shrink(1.0)
    .min_w(0.0)
    .text_left()
}

fn leaf_row(label: &str, depth: usize) -> Element {
    let spacer = build_icon_text_node(" ")
        .text_size(CHEVRON_PX)
        .flex_shrink(0.0);
    let mut row: Element = hstack()
        .w("100%")
        .min_w(0.0)
        .pl(indent_pl(depth))
        .pr(ROW_PX)
        .py(ROW_PY)
        .items("center")
        .gap(tokens::SPACE_1)
        .rounded(tokens::RADIUS_SM)
        .text_size(14.0)
        .child(spacer)
        .child(build_label_text(label))
        .into();
    row = with_class_list(row, "hover:bg-accent");
    row.attrs.insert(ATTR_SLOT.to_string(), SLOT_ROW.to_string());
    row
}

fn branch_row_visual(label: &str, depth: usize, expanded_key: &str, node_id: &str) -> Element {
    let mut chevron = chevron_right_icon();
    chevron
        .attrs
        .insert(ATTR_SLOT.to_string(), SLOT_CHEVRON.to_string());
    chevron = chevron.flex_shrink(0.0);
    let ek = expanded_key.to_string();
    let nid = node_id.to_string();
    chevron = chevron.reactive_sync(move |el, _| {
        sync_tree_chevron(el, node_expanded(&ek, &nid))
    });

    let mut row: Element = hstack()
        .w("100%")
        .min_w(0.0)
        .pl(indent_pl(depth))
        .pr(ROW_PX)
        .py(ROW_PY)
        .items("center")
        .gap(tokens::SPACE_1)
        .rounded(tokens::RADIUS_SM)
        .text_size(14.0)
        .cursor("pointer")
        .child(chevron)
        .child(build_label_text(label))
        .into();
    row = with_class_list(row, "hover:bg-accent");
    row.attrs.insert(ATTR_SLOT.to_string(), SLOT_ROW.to_string());
    row
}

fn branch_row(label: &str, depth: usize, expanded_key: &str, node_id: &str) -> Element {
    let ek = expanded_key.to_string();
    let nid = node_id.to_string();
    interactive(branch_row_visual(label, depth, &ek, &nid))
        .on_click(move || toggle_node_expanded_key(&ek, &nid))
        .into()
}

// ── Expanded sync per node ────────────────────────────────────────────────────

fn node_open_signal(tree_id: &str, node_id: &str, expanded_key: &str) -> Signal<bool> {
    let key = format!("{tree_id}::open::{node_id}");
    let nid = node_id.to_string();
    let exp_key = expanded_key.to_string();
    let sig = use_signal(&key, move || node_expanded(&exp_key, &nid));
    let sync_reg_key = format!("{tree_id}::open_sync_reg::{node_id}");
    let open_key = key.clone();
    use_signal(sync_reg_key.as_str(), move || {
        let nid_read = node_id.to_string();
        let nid_write = node_id.to_string();
        let exp_key_read = expanded_key.to_string();
        let exp_key_write = expanded_key.to_string();
        let open_key_read = open_key.clone();
        let open_key_write = open_key.clone();
        create_effect(move || {
            let Some(set) = expanded_signal_for_key(&exp_key_read).try_get() else {
                return;
            };
            let want = set.contains(&nid_read);
            let sig = use_signal(&open_key_read, || false);
            if let Some(cur) = sig.try_peek() {
                if cur != want {
                    let _ = sig.try_set(want);
                }
            }
        });
        create_effect(move || {
            let sig = use_signal(&open_key_write, || false);
            let Some(open) = sig.try_get() else {
                return;
            };
            let expanded = expanded_signal_for_key(&exp_key_write);
            let Some(mut set) = expanded.try_peek() else {
                return;
            };
            let contained = set.contains(&nid_write);
            if open && !contained {
                set.insert(nid_write.clone());
                let _ = expanded.try_set(set);
            } else if !open && contained {
                set.remove(&nid_write);
                let _ = expanded.try_set(set);
            }
        });
    });
    sig
}

fn toggle_node_expanded(expanded: Signal<HashSet<String>>, node_id: &str) {
    let Some(mut set) = expanded.try_get() else {
        return;
    };
    tree_model::toggle_expanded(&mut set, node_id);
    let _ = expanded.try_set(set);
}

// ── Variant builders ──────────────────────────────────────────────────────────

fn expanded_storage_fingerprint(key: &str) -> String {
    expanded_signal_for_key(key)
        .try_peek()
        .map(|set| {
            let mut ids: Vec<_> = set.into_iter().collect();
            ids.sort();
            ids.join("|")
        })
        .unwrap_or_default()
}

fn build_flat_tree(
    ctx: &TreeViewCtx,
    roots: &[TreeNode],
    row_height: f32,
    height: f32,
) -> Element {
    let roots = roots.to_vec();
    let expanded_key = ctx.expanded_storage_key.clone();
    let list_id = format!("{}__rows", ctx.id);
    let exp_key_count = expanded_key.clone();
    let roots_count = roots.clone();
    let exp_key_render = expanded_key.clone();
    let roots_render = roots.clone();
    let exp_key_data = expanded_key.clone();
    let flat_redraw_key = format!("{}__flat_exp_redraw", ctx.id);
    let exp_key_redraw = expanded_key.clone();
    use_signal(flat_redraw_key.as_str(), move || {
        create_effect(move || {
            let _ = expanded_signal_for_key(&exp_key_redraw).try_get();
            crate::app::runtime::request_redraw();
        });
    });

    let expanded = expanded_signal_for_key(&expanded_key);
    let exp = expanded.try_peek().unwrap_or_default();
    let mut flat = Vec::new();
    flatten(&roots, &exp, 0, &mut flat);
    let initial_count = flat.len();

    virtual_list(list_id)
        .item_count(initial_count)
        .item_count_fn(move || {
            let Some(exp) = expanded_signal_for_key(&exp_key_count).try_peek() else {
                return 0;
            };
            let mut flat = Vec::new();
            flatten(&roots_count, &exp, 0, &mut flat);
            flat.len()
        })
        .data_key_fn(move || expanded_storage_fingerprint(&exp_key_data))
        .item_height(row_height)
        .height(height)
        .render_item(move |i| {
            let exp = expanded_signal_for_key(&exp_key_render)
                .try_peek()
                .unwrap_or_default();
            let mut flat = Vec::new();
            flatten(&roots_render, &exp, 0, &mut flat);
            let row = flat.get(i).cloned().unwrap_or(FlatRow {
                id: String::new(),
                label: String::new(),
                depth: 0,
                has_children: false,
            });
            let nid = row.id.clone();
            let has = row.has_children;
            if has {
                branch_row(&row.label, row.depth, &exp_key_render, &nid)
            } else {
                leaf_row(&row.label, row.depth)
            }
        })
        .into()
}

fn build_collapsible_node(ctx: &TreeViewCtx, node: &TreeNode, depth: usize) -> Element {
    if node.children.is_empty() {
        return leaf_row(&node.label, depth);
    }
    let node_id = node.id.clone();
    let label = node.label.clone();
    let children = node.children.clone();
    let open_sig = node_open_signal(&ctx.id, &node_id, &ctx.expanded_storage_key);
    let ctx_clone = ctx.clone();
    let exp_key = ctx.expanded_storage_key.clone();
    let row_nid = node_id.clone();

    collapsible_scope(
        format!("{}::{}", ctx.id, node_id),
        Some(open_sig),
        open_sig.try_peek().unwrap_or(false),
        false,
        None,
        move || {
            let trigger = CollapsibleTrigger(CollapsibleTriggerProps {
                disabled: Some(false),
                class: None,
                base: None,
                children: Some(branch_row_visual(&label, depth, &exp_key, &row_nid)),
            });
            let content = collapsible_content(move || {
                let kids: Vec<Element> = children
                    .iter()
                    .map(|c| build_collapsible_node(&ctx_clone, c, depth + 1))
                    .collect();
                vstack()
                    .w("100%")
                    .min_w(0.0)
                    .gap(0.0)
                    .children(kids)
                    .into()
            });
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            frag.children(vec![trigger, content])
        },
    )
}

/// Fixed-height nested ScrollArea so inner overflow does not leak into the page
/// `content_h` (outer thumb jump) and clipped rows stay reachable.
fn wrap_tree_scroll_viewport(tree_id: &str, height: f32, body: Element) -> Element {
    if height <= 0.0 {
        return body;
    }
    scroll_area(&format!("{tree_id}__scroll"))
        .overflow_y("auto")
        .overflow_x("hidden")
        .scroll_behavior("jump")
        .w("100%")
        .h(height)
        .flex_shrink(0.0)
        .child(body)
        .into()
}

fn build_collapsible_tree(ctx: &TreeViewCtx, roots: &[TreeNode], height: f32) -> Element {
    let ctx = ctx.clone();
    let roots = roots.to_vec();
    // Per-node `open_sig` + Collapsible presence drive expand/collapse.
    // Do not wrap in `reactive_view` — rebuilding the whole tree on toggle tears down
    // Collapsible mid-exit and leaves triggers stuck (cannot re-open).
    let kids: Vec<Element> = roots
        .iter()
        .map(|n| build_collapsible_node(&ctx, n, 0))
        .collect();
    let tree: Element = vstack()
        .w("100%")
        .min_w(0.0)
        .gap(0.0)
        .children(kids)
        .into();
    wrap_tree_scroll_viewport(&ctx.id, height, tree)
}

fn build_accordion_group(
    ctx: &TreeViewCtx,
    nodes: &[TreeNode],
    depth: usize,
    parent_path: &str,
) -> Element {
    if nodes.is_empty() {
        return Element::new();
    }
    let acc_id = if parent_path.is_empty() {
        format!("{}::acc::root", ctx.id)
    } else {
        format!("{}::acc::{}", ctx.id, parent_path)
    };
    let sibling_ids: Vec<String> = nodes.iter().map(|n| n.id.clone()).collect();
    let exp_key = ctx.expanded_storage_key.clone();
    let open_default: Vec<String> = sibling_ids
        .iter()
        .filter(|id| node_expanded(&exp_key, id.as_str()))
        .cloned()
        .collect();
    let value_sig = use_scoped_signal(&format!("{acc_id}__value"), || open_default.clone());
    let siblings_read = sibling_ids.clone();
    let siblings_write = sibling_ids.clone();
    let exp_key_write = exp_key.clone();
    let exp_key_read = exp_key.clone();

    create_effect(move || {
        let Some(open) = value_sig.try_get() else {
            return;
        };
        let expanded = expanded_signal_for_key(&exp_key_write);
        let Some(mut set) = expanded.try_peek() else {
            return;
        };
        let mut changed = false;
        for id in &siblings_write {
            if open.iter().any(|v| v == id) && !set.contains(id) {
                set.insert(id.clone());
                changed = true;
            } else if !open.iter().any(|v| v == id) && set.contains(id) {
                set.remove(id);
                changed = true;
            }
        }
        if changed {
            let _ = expanded.try_set(set);
        }
    });
    create_effect(move || {
        let Some(set) = expanded_signal_for_key(&exp_key_read).try_get() else {
            return;
        };
        let want: Vec<String> = siblings_read
            .iter()
            .filter(|id| set.contains(id.as_str()))
            .cloned()
            .collect();
        if let Some(cur) = value_sig.try_peek() {
            if cur != want {
                let _ = value_sig.try_set(want);
            }
        }
    });

    let ctx = ctx.clone();
    let nodes = nodes.to_vec();
    accordion_scope(
        acc_id,
        Some(value_sig),
        open_default,
        true,
        false,
        None,
        move || {
            let mut items = Vec::with_capacity(nodes.len());
            for node in nodes {
                let node_id = node.id.clone();
                let label = node.label.clone();
                let has_children = !node.children.is_empty();
                let children = node.children.clone();
                let ctx_item = ctx.clone();
                let depth_item = depth;
                items.push(accordion_item_scope(
                    node_id.clone(),
                    false,
                    None,
                    move || {
                        let trigger = AccordionTrigger(AccordionTriggerProps {
                            disabled: None,
                            class: None,
                            base: None,
                            children: Some(build_label_text(&label)),
                        });
                        let mut kids = vec![trigger];
                        if has_children {
                            kids.push(accordion_content(move || {
                                build_accordion_group(
                                    &ctx_item,
                                    &children,
                                    depth_item + 1,
                                    &node_id,
                                )
                            }));
                        }
                        let mut frag = Element::new();
                        frag.set_attr("rui_fragment", "1");
                        frag.children(kids)
                    },
                ));
            }
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            frag.children(items)
        },
    )
}

fn build_accordion_tree(ctx: &TreeViewCtx, roots: &[TreeNode], height: f32) -> Element {
    let ctx = ctx.clone();
    let roots = roots.to_vec();
    // Accordion open state is synced via scoped `value_sig` effects, not a full rebuild.
    let tree: Element = build_accordion_group(&ctx, &roots, 0, "");
    wrap_tree_scroll_viewport(&ctx.id, height, tree)
}

fn build_from_nodes(
    ctx: &TreeViewCtx,
    roots: &[TreeNode],
    row_height: f32,
    height: f32,
) -> Element {
    match ctx.variant {
        TreeViewVariant::Flat => build_flat_tree(ctx, roots, row_height, height),
        TreeViewVariant::Collapsible => build_collapsible_tree(ctx, roots, height),
        TreeViewVariant::Accordion => build_accordion_tree(ctx, roots, height),
    }
}

// ── Scope ─────────────────────────────────────────────────────────────────────

/// 提供 TreeView 上下文并构建根壳（`ui!` `<TreeView>` / Builder 共用）。
pub fn tree_view_scope(
    id: impl Into<String>,
    variant: TreeViewVariant,
    expanded: Option<Signal<HashSet<String>>>,
    expanded_storage_key: Option<String>,
    class: Option<String>,
    row_height: f32,
    height: f32,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = resolve_instance_id(id, "tree-view");
    let default_exp_key = format!("{id}__tree_exp");
    let expanded_storage_key = expanded_storage_key
        .filter(|k| !k.trim().is_empty())
        .unwrap_or(default_exp_key);
    let expanded = expanded.unwrap_or_else(|| {
        use_signal(expanded_storage_key.as_str(), HashSet::<String>::new)
    });
    let ctx = TreeViewCtx {
        id: id.clone(),
        variant,
        expanded,
        expanded_storage_key,
    };
    let kids = with_child_owner(|| {
        provide_context(tree_view_context(), ctx.clone());
        take_fragment(body())
    });
    let nodes = extract_item_nodes(&kids);
    let tree_body = if nodes.is_empty() {
        Element::new()
    } else {
        build_from_nodes(&ctx, &nodes, row_height, height)
    };
    let mut shell: Element = vstack()
        .w("100%")
        .min_w(0.0)
        .gap(0.0)
        .child(tree_body)
        .into();
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        shell = with_class_list(shell, &c);
    }
    shell.attrs.insert(ATTR_SLOT.to_string(), SLOT_ROOT.to_string());
    shell.template_id = Some(id.clone());
    shell
}

/// 宏 `<TreeViewItem>`：标记节点元数据；由 [`tree_view_scope`] 统一建树。
pub fn tree_view_item_scope(
    node_id: impl Into<String>,
    label: impl Into<String>,
    disabled: bool,
    class: Option<String>,
    body: impl FnOnce() -> Element,
) -> Element {
    let _ctx = require_tree_view("TreeViewItem");
    let node_id = node_id.into();
    let label = label.into();
    let parent_depth = use_tree_view_item().map(|c| c.depth).unwrap_or(0);

    with_child_owner(|| {
        provide_context(
            tree_view_item_context(),
            TreeViewItemCtx {
                depth: parent_depth + 1,
            },
        );
        let kids = take_fragment(body());
        let mut el: Element = vstack()
            .w("100%")
            .min_w(0.0)
            .gap(0.0)
            .children(kids)
            .into();
        el.attrs.insert(ATTR_SLOT.to_string(), SLOT_ITEM.to_string());
        el.attrs.insert("rui_tree_item_id".to_string(), node_id.clone());
        el.attrs.insert("rui_tree_item_label".to_string(), label);
        if disabled {
            el.attrs.insert("rui_tree_item_disabled".to_string(), "1".into());
        }
        if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
            el = with_class_list(el, &c);
        }
        el
    })
}

fn tree_node_marker(node: TreeNode) -> Element {
    let kids: Vec<Element> = node
        .children
        .into_iter()
        .map(tree_node_marker)
        .collect();
    let mut el: Element = vstack()
        .w("100%")
        .min_w(0.0)
        .gap(0.0)
        .children(kids)
        .into();
    el.attrs.insert(ATTR_SLOT.to_string(), SLOT_ITEM.to_string());
    el.attrs.insert("rui_tree_item_id".to_string(), node.id);
    el.attrs.insert("rui_tree_item_label".to_string(), node.label);
    el
}

fn extract_item_nodes(kids: &[Element]) -> Vec<TreeNode> {
    kids
        .iter()
        .filter_map(|el| {
            if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) != Some(SLOT_ITEM) {
                return None;
            }
            let id = el
                .attrs
                .get("rui_tree_item_id")
                .cloned()
                .unwrap_or_default();
            let label = el
                .attrs
                .get("rui_tree_item_label")
                .cloned()
                .unwrap_or_else(|| id.clone());
            let children = extract_item_nodes(&el.children);
            Some(TreeNode {
                id,
                label,
                children,
            })
        })
        .collect()
}

// ── RSX parts ─────────────────────────────────────────────────────────────────

/// RSX 备用入口；复合面请走宏 → [`tree_view_scope`]。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn TreeView(
    id: String,
    variant: Option<String>,
    expanded: Option<Signal<HashSet<String>>>,
    expanded_storage_key: Option<String>,
    class: Option<String>,
    row_height: Option<f32>,
    height: Option<f32>,
    children: Option<Element>,
) -> Element {
    let variant = variant
        .map(|s| TreeViewVariant::parse(&s))
        .unwrap_or_default();
    let row_height = row_height.unwrap_or(32.0);
    let height = height.unwrap_or(240.0);
    tree_view_scope(
        id,
        variant,
        expanded,
        expanded_storage_key,
        class,
        row_height,
        height,
        move || children.unwrap_or_else(Element::new),
    )
}

/// `ui!` `<TreeViewItem>` 宏展开目标。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn TreeViewItem(
    id: String,
    label: Option<String>,
    disabled: bool,
    class: Option<String>,
    children: Option<Element>,
) -> Element {
    let label = label.unwrap_or_else(|| id.clone());
    let mut el = tree_view_item_scope(id.clone(), label.clone(), disabled, class, move || {
        children.unwrap_or_else(Element::new)
    });
    el.attrs.insert("rui_tree_item_label".to_string(), label);
    el
}

// ── Builder ───────────────────────────────────────────────────────────────────

pub struct TreeViewBuilder {
    id: String,
    variant: TreeViewVariant,
    roots: Vec<TreeNode>,
    row_height: f32,
    height: f32,
    expanded: Option<Signal<HashSet<String>>>,
    expanded_storage_key: Option<String>,
    class: Option<String>,
}

pub fn tree_view(id: impl Into<String>) -> TreeViewBuilder {
    TreeViewBuilder {
        id: id.into(),
        variant: TreeViewVariant::Flat,
        roots: Vec::new(),
        row_height: 32.0,
        height: 240.0,
        expanded: None,
        expanded_storage_key: None,
        class: None,
    }
}

impl TreeViewBuilder {
    pub fn variant(mut self, v: impl Into<TreeViewVariant>) -> Self {
        self.variant = v.into();
        self
    }

    pub fn roots(mut self, roots: impl IntoIterator<Item = TreeNode>) -> Self {
        self.roots = roots.into_iter().collect();
        self
    }

    pub fn row_height(mut self, h: f32) -> Self {
        self.row_height = h;
        self
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    pub fn expanded_signal(mut self, s: Signal<HashSet<String>>) -> Self {
        self.expanded = Some(s);
        self
    }

    /// `use_signal` key for [`Self::expanded_signal`] when it is not the default `{id}__tree_exp`.
    pub fn expanded_storage_key(mut self, key: impl Into<String>) -> Self {
        self.expanded_storage_key = Some(key.into());
        self
    }

    pub fn class(mut self, c: impl Into<String>) -> Self {
        self.class = Some(c.into());
        self
    }

    /// 在上下文内构建任意子树（`TreeViewItem` 等）。
    pub fn child(self, body: impl FnOnce() -> Element) -> Element {
        tree_view_scope(
            self.id,
            self.variant,
            self.expanded,
            self.expanded_storage_key,
            self.class,
            self.row_height,
            self.height,
            body,
        )
    }
}

impl From<TreeViewBuilder> for Element {
    fn from(builder: TreeViewBuilder) -> Self {
        let TreeViewBuilder {
            id,
            variant,
            roots,
            row_height,
            height,
            expanded,
            expanded_storage_key,
            class,
        } = builder;
        tree_view_scope(
            id,
            variant,
            expanded,
            expanded_storage_key,
            class,
            row_height,
            height,
            move || {
            if roots.is_empty() {
                Element::new()
            } else {
                let mut frag = Element::new();
                frag.set_attr("rui_fragment", "1");
                frag.children(roots.into_iter().map(tree_node_marker))
            }
        })
    }
}

impl crate::core::tree::IntoElementList for TreeViewBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::create_root;

    #[test]
    fn flatten_respects_expanded() {
        let roots = vec![TreeNode {
            id: "a".into(),
            label: "A".into(),
            children: vec![TreeNode {
                id: "b".into(),
                label: "B".into(),
                children: vec![],
            }],
        }];
        let mut out = Vec::new();
        flatten(&roots, &HashSet::new(), 0, &mut out);
        assert_eq!(out.len(), 1);
        let mut exp = HashSet::new();
        exp.insert("a".into());
        out.clear();
        flatten(&roots, &exp, 0, &mut out);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn variant_parse_falls_back_to_flat() {
        assert_eq!(
            TreeViewVariant::parse("collapsible"),
            TreeViewVariant::Collapsible
        );
        assert_eq!(
            TreeViewVariant::parse("accordion"),
            TreeViewVariant::Accordion
        );
        assert_eq!(TreeViewVariant::parse("unknown"), TreeViewVariant::Flat);
    }

    #[test]
    fn shared_trees_collapse_root_no_disposed_peek() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let roots = vec![TreeNode {
            id: "root".into(),
            label: "Workspace".into(),
            children: vec![TreeNode {
                id: "src".into(),
                label: "src".into(),
                children: vec![TreeNode {
                    id: "main".into(),
                    label: "main.rs".into(),
                    children: vec![],
                }],
            }],
        }];
        let (_o, ()) = create_root(|| {
            let expanded_key = "tv_shared_collapse::expanded";
            use_signal(expanded_key, || {
                let mut s = HashSet::<String>::new();
                s.insert("root".into());
                s.insert("src".into());
                s
            });
            let holder: Rc<RefCell<Option<Element>>> = Rc::new(RefCell::new(None));
            let h = Rc::clone(&holder);
            let roots_c = roots.clone();
            let roots_a = roots.clone();
            create_effect(move || {
                if h.borrow().is_some() {
                    return;
                }
                let collapsible: Element = tree_view("tv-shared-collapsible")
                    .variant("collapsible")
                    .height(160.0)
                    .expanded_storage_key(expanded_key)
                    .roots(roots_c.clone())
                    .into();
                let accordion: Element = tree_view("tv-shared-accordion")
                    .variant("accordion")
                    .height(160.0)
                    .expanded_storage_key(expanded_key)
                    .roots(roots_a.clone())
                    .into();
                let flat: Element = tree_view("tv-shared-flat")
                    .variant("flat")
                    .height(140.0)
                    .expanded_storage_key(expanded_key)
                    .roots(roots_c.clone())
                    .into();
                *h.borrow_mut() = Some(
                    vstack()
                        .child(collapsible)
                        .child(accordion)
                        .child(flat)
                        .into(),
                );
            });
            toggle_node_expanded_key(expanded_key, "root");
            let _keep = holder.borrow().is_some();
        });
    }

    #[test]
    fn flat_tree_collapse_root_no_disposed_peek() {
        let roots = vec![TreeNode {
            id: "root".into(),
            label: "Root".into(),
            children: vec![TreeNode {
                id: "a".into(),
                label: "A".into(),
                children: vec![],
            }],
        }];
        let (_o, ()) = create_root(|| {
            let expanded_key = "tv_flat_collapse::expanded";
            use_signal(expanded_key, || {
                let mut s = HashSet::<String>::new();
                s.insert("root".into());
                s
            });
            let _: Element = tree_view("tv_flat_collapse")
                .variant("flat")
                .height(80.0)
                .expanded_storage_key(expanded_key)
                .roots(roots)
                .into();
            toggle_node_expanded_key(expanded_key, "root");
            assert!(expanded_signal_for_key(expanded_key).try_peek().is_some());
        });
    }

    #[test]
    fn node_open_signal_peek_after_expanded_toggle() {
        use crate::core::reactivity::create_effect;
        let (_o, ()) = create_root(|| {
            let expanded_key = "tv_node_open_regression::expanded";
            let expanded = use_signal(expanded_key, HashSet::<String>::new);
            expanded.set_silent({
                let mut s = HashSet::new();
                s.insert("node_a".into());
                s
            });
            let _sig = node_open_signal("tv_node_open_regression", "node_a", expanded_key);
            create_effect(move || {
                let _ = expanded.try_get();
                let _ = node_open_signal("tv_node_open_regression", "node_a", expanded_key);
                let sig = use_signal("tv_node_open_regression::open::node_a", || false);
                assert!(sig.try_peek().is_some());
            });
            expanded.set(HashSet::new());
            let sig = use_signal("tv_node_open_regression::open::node_a", || false);
            assert!(sig.try_peek().is_some());
        });
    }

    #[test]
    fn builder_sets_slot() {
        let (_o, el): (_, Element) = create_root(|| {
            tree_view("tv-test")
                .variant("collapsible")
                .roots(vec![TreeNode {
                    id: "a".into(),
                    label: "A".into(),
                    children: vec![],
                }])
                .into()
        });
        assert_eq!(
            el.attrs.get(ATTR_SLOT).map(|s| s.as_str()),
            Some(SLOT_ROOT)
        );
        assert_eq!(el.template_id.as_deref(), Some("tv-test"));
    }

    #[test]
    fn dual_surface_compiles_shape() {
        let _b: Element = tree_view("x").variant(TreeViewVariant::Flat).into();
        let el = TreeView(TreeViewProps {
            id: "y".into(),
            variant: Some("collapsible".into()),
            expanded: None,
            class: None,
            row_height: None,
            height: None,
            children: None,
            base: None,
            ..Default::default()
        });
        let _: Element = el;
    }
}
