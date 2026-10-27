//! # TreeView
//!
//! 树形结构：两种变体对位 ShadCN 社区常见树实践。
//!
//! ```text
//! TreeView                         // Root：variant + expanded 集合 + exclusive
//! └── TreeViewItem                 // 递归节点（RSX）；或 Builder `.roots([TreeNode])`
//!     └── TreeViewItem …           // 子节点
//! ```
//!
//! | `variant` | 实现 | 适用 |
//! |-----------|------|------|
//! | `flat` | DFS 扁平化 + [`virtual_list`] | 大数据、固定行高 |
//! | `collapsible` | 递归 [`collapsible_scope`] / `CollapsibleTrigger` | 文件树、侧边栏目录 |
//!
//! [`TreeViewChrome::Sidebar`]：侧栏语义色、`height` 默认 0（交给 [`crate::components::SidebarContent`] 滚动）。
//!
//! `collapsible` 默认不互斥（与 [`collapsible_scope`] 一致）。`exclusive={true}` 时同层兄弟目录只展开一项。
//! 旧 `variant="accordion"` / `"sections"` 映射为 `collapsible`（Accordion 变体已废弃）。
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
//!     .roots([tree_node("src", "src").children([
//!         tree_node("main", "main.rs"),
//!         tree_node("lib", "lib.rs"),
//!     ])])
//! ```
//!
//! **Identity (WP-2.8)**: `id` optional; empty string → [`resolve_instance_id`].

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::OnceLock;

use crate::components::base::{BaseComponentBuilder, SlotChrome, SlotStyleFn};
use crate::components::collapsible::{
    collapsible_content, collapsible_scope, CollapsibleTrigger, CollapsibleTriggerProps,
};
use crate::components::scroll_area::scroll_area;
use crate::components::virtual_list::virtual_list;
use crate::core::reactivity::{
    create_context, create_effect, provide_context, use_context, use_signal, with_child_owner,
    Context, Signal,
};
use crate::core::tree::{parse_text_color, Element, LayoutStatus};
use crate::primitives::interactive::{interactive, mark_hover_paint_dirty};
use crate::primitives::r#box::{hstack, vstack};
use crate::style::{
    css_transition_needs_frame, is_variant_style_overridden, parse_color, TransitionEntry,
    TransitionProperty, VariantStyleField,
};
use crate::tokens;
use crate::utils::motion::ensure_motion_transitions;
use crate::utils::resolve_instance_id;
use crate::utils::take_fragment;
use crate::utils::text_node::{
    build_icon_text_node, build_lucide_icon_text_node_or, build_text_node, TextNodeConfig,
};
use crate::utils::tree_model::{self, TreeNodeLike};

const ATTR_SLOT: &str = "data-slot";
const SLOT_ROOT: &str = "tree-view";
const SLOT_ITEM: &str = "tree-view-item";
const SLOT_ROW: &str = "tree-view-row";
const SLOT_CHEVRON: &str = "tree-view-chevron";

const ATTR_CHROME: &str = "data-tree-chrome";
const ATTR_ROW_CHROME: &str = "jadix_tree_row_chrome";
const CHEVRON_PX: f32 = 16.0;
const CHEVRON_OPEN_DEG: f32 = 90.0;
const ROW_WEIGHT: u16 = 500;
const ROW_WEIGHT_SIDEBAR: u16 = 400;
const INDENT_PX: f32 = 16.0;
const ROW_PY: &str = tokens::SPACE_1;
const ROW_PX: &str = tokens::SPACE_2;

// ── Model ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Default)]
pub struct TreeNode {
    pub id: String,
    pub label: String,
    pub children: Vec<TreeNode>,
    pub chrome: SlotChrome,
}

/// Sugar entry: `tree_view("files").roots([tree_node("src", "src").class("ring-2")])`.
pub fn tree_node(id: impl Into<String>, label: impl Into<String>) -> TreeNode {
    TreeNode {
        id: id.into(),
        label: label.into(),
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl TreeNode {
    crate::impl_slot_chrome_methods!();

    pub fn children(mut self, children: impl IntoIterator<Item = TreeNode>) -> Self {
        self.children = children.into_iter().collect();
        self
    }
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
}

impl TreeViewVariant {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "collapsible" | "nested" | "tree" | "accordion" | "sections" => Self::Collapsible,
            "flat" | "virtual" | "list" | "" => Self::Flat,
            _ => Self::Flat,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Flat => "flat",
            Self::Collapsible => "collapsible",
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

/// 行外观。[`Self::Sidebar`] 对齐侧栏菜单语义色，并默认不套内层 ScrollArea。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TreeViewChrome {
    #[default]
    Default,
    Sidebar,
}

impl TreeViewChrome {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "sidebar" | "nav" | "menu" => Self::Sidebar,
            _ => Self::Default,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Sidebar => "sidebar",
        }
    }
}

impl From<&str> for TreeViewChrome {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for TreeViewChrome {
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
    /// Collapsible：同层兄弟目录是否互斥（默认 false）。
    pub exclusive: bool,
    pub chrome: TreeViewChrome,
    pub selected: Option<Signal<String>>,
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
    chrome: SlotChrome,
}

fn flatten(nodes: &[TreeNode], expanded: &HashSet<String>, depth: usize, out: &mut Vec<FlatRow>) {
    *out = tree_model::flatten_expanded(nodes, expanded, depth, |n, d, has| FlatRow {
        id: n.id.clone(),
        label: n.label.clone(),
        depth: d,
        has_children: has,
        chrome: n.chrome.clone(),
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

thread_local! {
    static TREE_ON_SELECT: RefCell<HashMap<String, Rc<dyn Fn(String)>>> = RefCell::new(HashMap::new());
}

fn register_tree_on_select(tree_id: &str, cb: Option<Rc<dyn Fn(String)>>) {
    TREE_ON_SELECT.with(|m| {
        if let Some(cb) = cb {
            m.borrow_mut().insert(tree_id.to_string(), cb);
        } else {
            m.borrow_mut().remove(tree_id);
        }
    });
}

fn invoke_tree_on_select(tree_id: &str, node_id: &str) {
    TREE_ON_SELECT.with(|m| {
        if let Some(cb) = m.borrow().get(tree_id) {
            cb(node_id.to_string());
        }
    });
}

fn emit_select(ctx: &TreeViewCtx, node_id: &str) {
    if let Some(sel) = ctx.selected {
        if sel.is_alive() {
            let _ = sel.try_set(node_id.to_string());
        }
    }
    invoke_tree_on_select(&ctx.id, node_id);
}

fn row_selectable(ctx: &TreeViewCtx) -> bool {
    ctx.selected.is_some() || TREE_ON_SELECT.with(|m| m.borrow().contains_key(&ctx.id))
}

fn rest_label_color(chrome: TreeViewChrome) -> [u8; 4] {
    match chrome {
        TreeViewChrome::Sidebar => parse_text_color("sidebar-foreground"),
        TreeViewChrome::Default => parse_text_color("foreground"),
    }
}

fn rest_label_weight(chrome: TreeViewChrome) -> u16 {
    match chrome {
        TreeViewChrome::Sidebar => ROW_WEIGHT_SIDEBAR,
        TreeViewChrome::Default => ROW_WEIGHT,
    }
}

fn paint_tree_row_fg(el: &mut Element, color: [u8; 4], weight: u16) -> bool {
    let lock_fg = is_variant_style_overridden(&el.attrs, VariantStyleField::Fg);
    let lock_weight = is_variant_style_overridden(&el.attrs, VariantStyleField::Weight);
    let mut changed = false;
    crate::core::tree::walk_elements_mut_pruned(el, |el| {
        if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_CHEVRON) {
            return false;
        }
        if !lock_fg {
            changed |= el.text_style.color != Some(color);
            el.text_style.color = Some(color);
        }
        if !lock_weight {
            changed |= el.text_style.weight != Some(weight);
            el.text_style.weight = Some(weight);
        }
        true
    });
    changed
}

fn row_pointer_id(tree_id: &str, node_id: &str) -> String {
    format!("{tree_id}::row::{node_id}")
}

fn sync_tree_row_chrome(
    el: &mut Element,
    chrome: TreeViewChrome,
    selected: bool,
    hovered: bool,
) -> LayoutStatus {
    let highlighted = selected || hovered;
    let (want_bg, want_fg, want_weight) = if highlighted {
        match chrome {
            TreeViewChrome::Sidebar => (
                parse_color("sidebar-accent"),
                parse_text_color("sidebar-accent-foreground"),
                if selected {
                    ROW_WEIGHT
                } else {
                    ROW_WEIGHT_SIDEBAR
                },
            ),
            TreeViewChrome::Default => (
                parse_color("accent"),
                parse_text_color("accent-foreground"),
                ROW_WEIGHT,
            ),
        }
    } else {
        (
            [0.0, 0.0, 0.0, 0.0],
            rest_label_color(chrome),
            rest_label_weight(chrome),
        )
    };
    let lock_bg = is_variant_style_overridden(&el.attrs, VariantStyleField::Bg);
    let mut changed = !lock_bg && el.style.bg_color != want_bg;
    if changed {
        el.style.bg_color = want_bg;
    }
    changed |= paint_tree_row_fg(el, want_fg, want_weight);
    if changed {
        LayoutStatus::StyleChanged
    } else {
        LayoutStatus::Clean
    }
}

/// Hover + selected share one paint path (no `hover:bg-*` class). Class variants run
/// *after* `reactive_sync` and skip when the hover bool is unchanged — writing rest
/// chrome into `class_set.base_style` previously made unhover/skip leave rows blank.
fn with_row_hover_chrome(ctx: &TreeViewCtx, node_id: &str, visual: Element) -> Element {
    let fid = row_pointer_id(&ctx.id, node_id);
    let hover_key = format!("{fid}_hover");
    let hovered = use_signal(hover_key.as_str(), || false);
    let chrome = ctx.chrome;
    let sel = ctx.selected;
    let nid = node_id.to_string();
    let hover_key_sync = hover_key.clone();
    let mut row: Element = interactive(visual)
        .focus_id(fid)
        .on_hover(move |h| {
            if hovered.try_peek() != Some(h) {
                let _ = hovered.try_set(h);
                mark_hover_paint_dirty();
            }
        })
        .into();
    row.set_attr(ATTR_ROW_CHROME, "1");
    crate::authoring::presentation::attach(
        &mut row,
        crate::authoring::presentation::Phase::Hover,
        |element, layout, _context| {
            let mut impact = LayoutStatus::Clean;
            sync_tree_view_hover_paint_node(element, layout, &mut impact);
            impact
        },
    );
    row.reactive_sync(move |el, _| {
        let selected = sel.and_then(|s| s.try_peek()).is_some_and(|id| id == nid);
        let hovered = use_signal(hover_key_sync.as_str(), || false)
            .try_peek()
            .unwrap_or(false);
        sync_tree_row_chrome(el, chrome, selected, hovered)
    })
}

/// Reconcile row paint after pointer callbacks, including freshly mounted rows.
fn sync_tree_view_hover_paint_node(
    el: &mut Element,
    layout_tree: &mut crate::core::tree::LayoutTree,
    impact: &mut LayoutStatus,
) {
    if !el.attrs.contains_key(ATTR_ROW_CHROME) {
        return;
    }
    if let Some(sync) = el.reactive_sync.clone() {
        let status = sync.run(el, layout_tree);
        if status != LayoutStatus::Clean {
            crate::app::damage::push_frame_paint_element(el, el.paint_scroll_offset());
            layout_tree.note_collect_skip_hash_bust_if_idle(el.collect_cache_live());
            *impact = impact.merge(status);
        }
    }
}

// ── Shared chrome ─────────────────────────────────────────────────────────────

fn chevron_right_icon() -> Element {
    build_lucide_icon_text_node_or("chevron-right", "›")
        .text_size(CHEVRON_PX)
        .w(CHEVRON_PX)
        .h(CHEVRON_PX)
        .text_color(parse_text_color("muted-foreground"))
}

fn chevron_transition_entries() -> [TransitionEntry; 1] {
    [TransitionEntry {
        property: TransitionProperty::Transform,
        duration_ms: crate::utils::collapse_panel::CollapseKeyframes::COLLAPSIBLE.duration_ms,
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
        // ReactiveSync records this icon's stable paint key and visual bounds.
        // A second bounds-hashed dirty overlaps the label and replays its edge twice.
        LayoutStatus::StyleChanged
    } else {
        LayoutStatus::Clean
    }
}

fn indent_pl(depth: usize) -> f32 {
    8.0 + depth as f32 * INDENT_PX
}

fn build_label_text(label: &str, chrome: TreeViewChrome) -> Element {
    build_text_node(
        label,
        TextNodeConfig {
            size_px: 14.0,
            weight: rest_label_weight(chrome),
            color: rest_label_color(chrome),
            leading_scale: Some(1.25),
            ..Default::default()
        },
    )
    .flex_grow(1.0)
    .flex_shrink(1.0)
    .min_w(0.0)
    .text_left()
}

fn leaf_row_visual(ctx: &TreeViewCtx, label: &str, depth: usize) -> Element {
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
        .child(build_label_text(label, ctx.chrome))
        .into();
    row.attrs
        .insert(ATTR_SLOT.to_string(), SLOT_ROW.to_string());
    row
}

fn leaf_row(ctx: &TreeViewCtx, label: &str, depth: usize, node_id: &str) -> Element {
    let visual = with_row_hover_chrome(ctx, node_id, leaf_row_visual(ctx, label, depth));
    if !row_selectable(ctx) {
        return visual;
    }
    let ctx = ctx.clone();
    let nid = node_id.to_string();
    visual.on_click(move || emit_select(&ctx, &nid))
}

fn branch_row_visual(ctx: &TreeViewCtx, label: &str, depth: usize, node_id: &str) -> Element {
    let mut chevron = chevron_right_icon();
    chevron
        .attrs
        .insert(ATTR_SLOT.to_string(), SLOT_CHEVRON.to_string());
    chevron = chevron.flex_shrink(0.0);
    let ek = ctx.expanded_storage_key.clone();
    let nid = node_id.to_string();
    chevron = chevron.reactive_sync(move |el, _| sync_tree_chevron(el, node_expanded(&ek, &nid)));

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
        .child(build_label_text(label, ctx.chrome))
        .into();
    row.attrs
        .insert(ATTR_SLOT.to_string(), SLOT_ROW.to_string());
    row
}

fn branch_row(ctx: &TreeViewCtx, label: &str, depth: usize, node_id: &str) -> Element {
    let ek = ctx.expanded_storage_key.clone();
    let nid = node_id.to_string();
    with_row_hover_chrome(ctx, node_id, branch_row_visual(ctx, label, depth, node_id))
        .on_click(move || toggle_node_expanded_key(&ek, &nid))
}

// ── Expanded sync per node ────────────────────────────────────────────────────

fn node_open_signal(tree_id: &str, node_id: &str, expanded_key: &str) -> Signal<bool> {
    let key = format!("{tree_id}::open::{node_id}");
    let nid = node_id.to_string();
    let exp_key = expanded_key.to_string();
    let sig = use_signal(&key, move || node_expanded(&exp_key, &nid));
    let open_key = key.clone();
    // Registration follows the content owner; the open value survives remounts.
    // A global guard outlives disposed effects and leaves descendants disconnected.
    {
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
                if let Some(groups) = use_signal(
                    sibling_exclusive_groups_key(&exp_key_write).as_str(),
                    Vec::<Vec<String>>::new,
                )
                .try_peek()
                {
                    for g in groups {
                        if g.iter().any(|id| id == &nid_write) {
                            for id in g {
                                if id != nid_write {
                                    set.remove(&id);
                                }
                            }
                        }
                    }
                }
                let _ = expanded.try_set(set);
            } else if !open && contained {
                set.remove(&nid_write);
                let _ = expanded.try_set(set);
            }
        });
    }
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

fn build_flat_tree(ctx: &TreeViewCtx, roots: &[TreeNode], row_height: f32, height: f32) -> Element {
    let roots = roots.to_vec();
    let expanded_key = ctx.expanded_storage_key.clone();
    let list_id = format!("{}__rows", ctx.id);
    let exp_key_count = expanded_key.clone();
    let roots_count = roots.clone();
    let exp_key_render = expanded_key.clone();
    let roots_render = roots.clone();
    let exp_key_data = expanded_key.clone();
    let ctx_render = ctx.clone();
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
                chrome: SlotChrome::default(),
            });
            let nid = row.id.clone();
            let has = row.has_children;
            let el = if has {
                branch_row(&ctx_render, &row.label, row.depth, &nid)
            } else {
                leaf_row(&ctx_render, &row.label, row.depth, &nid)
            };
            row.chrome.apply(el)
        })
        .into()
}

fn build_collapsible_node(
    ctx: &TreeViewCtx,
    node: &TreeNode,
    depth: usize,
    parent_path: &str,
) -> Element {
    if node.children.is_empty() {
        return node
            .chrome
            .apply(leaf_row(ctx, &node.label, depth, &node.id));
    }
    let node_id = node.id.clone();
    let label = node.label.clone();
    let children = node.children.clone();
    let row_chrome = node.chrome.clone();
    let open_sig = node_open_signal(&ctx.id, &node_id, &ctx.expanded_storage_key);
    let ctx_clone = ctx.clone();
    let ctx_trigger = ctx.clone();
    let row_nid = node_id.clone();
    let exclusive = ctx.exclusive;
    let group = exclusive.then(|| collapsible_sibling_group(&ctx.id, parent_path));
    let child_parent = node_id.clone();

    collapsible_scope(
        format!("{}::{}", ctx.id, node_id),
        Some(open_sig),
        open_sig.try_peek().unwrap_or(false),
        false,
        exclusive,
        group,
        move || {
            let trigger = CollapsibleTrigger(CollapsibleTriggerProps {
                disabled: Some(false),
                base: None,
                children: Some(row_chrome.apply(with_row_hover_chrome(
                    &ctx_trigger,
                    &row_nid,
                    branch_row_visual(&ctx_trigger, &label, depth, &row_nid),
                ))),
            });
            let content = collapsible_content(move || {
                let kids: Vec<Element> = children
                    .iter()
                    .map(|c| build_collapsible_node(&ctx_clone, c, depth + 1, &child_parent))
                    .collect();
                vstack().w("100%").min_w(0.0).gap(0.0).children(kids).into()
            });
            let mut frag = Element::new();
            frag.set_attr("jadix_fragment", "1");
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
        .w("100%")
        .h(height)
        .flex_shrink(0.0)
        .child(body)
        .into()
}

fn sibling_exclusive_groups_key(exp_key: &str) -> String {
    format!("{exp_key}::__sib_groups")
}

fn prune_exclusive_siblings(exp_key: &str, siblings: &[String]) {
    if siblings.len() <= 1 {
        return;
    }
    let expanded = expanded_signal_for_key(exp_key);
    let Some(mut set) = expanded.try_peek() else {
        return;
    };
    let Some(keep) = siblings
        .iter()
        .find(|id| set.contains(id.as_str()))
        .cloned()
    else {
        return;
    };
    let mut changed = false;
    for id in siblings {
        if id != &keep && set.remove(id) {
            changed = true;
        }
    }
    if changed {
        let _ = expanded.try_set(set);
    }
}

fn collect_branch_sibling_groups(nodes: &[TreeNode], out: &mut Vec<Vec<String>>) {
    let siblings: Vec<String> = nodes
        .iter()
        .filter(|n| !n.children.is_empty())
        .map(|n| n.id.clone())
        .collect();
    if siblings.len() > 1 {
        out.push(siblings);
    }
    for n in nodes {
        if !n.children.is_empty() {
            collect_branch_sibling_groups(&n.children, out);
        }
    }
}

fn register_collapsible_exclusive_groups(ctx: &TreeViewCtx, nodes: &[TreeNode]) {
    let groups_sig = use_signal(
        sibling_exclusive_groups_key(&ctx.expanded_storage_key).as_str(),
        Vec::<Vec<String>>::new,
    );
    if !ctx.exclusive {
        let _ = groups_sig.try_set(Vec::new());
        return;
    }
    let mut groups = Vec::new();
    collect_branch_sibling_groups(nodes, &mut groups);
    for g in &groups {
        prune_exclusive_siblings(&ctx.expanded_storage_key, g);
    }
    let _ = groups_sig.try_set(groups);
}

fn build_collapsible_tree(ctx: &TreeViewCtx, roots: &[TreeNode], height: f32) -> Element {
    let ctx = ctx.clone();
    let roots = roots.to_vec();
    register_collapsible_exclusive_groups(&ctx, &roots);
    // Per-node `open_sig` + Collapsible presence drive expand/collapse.
    // Do not wrap in `reactive_view` — rebuilding the whole tree on toggle tears down
    // Collapsible mid-exit and leaves triggers stuck (cannot re-open).
    let kids: Vec<Element> = roots
        .iter()
        .map(|n| build_collapsible_node(&ctx, n, 0, ""))
        .collect();
    let tree: Element = vstack().w("100%").min_w(0.0).gap(0.0).children(kids).into();
    wrap_tree_scroll_viewport(&ctx.id, height, tree)
}

/// Sibling exclusive group. Must not use `::sib::root` for both the forest
/// (`parent_path` empty) and a node whose `id` is `"root"`.
fn collapsible_sibling_group(tree_id: &str, parent_path: &str) -> String {
    if parent_path.is_empty() {
        format!("{tree_id}::sib::_top")
    } else {
        format!("{tree_id}::sib::{parent_path}")
    }
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
    }
}

// ── Scope ─────────────────────────────────────────────────────────────────────

/// 提供 TreeView 上下文并构建根壳（`ui!` `<TreeView>` / Builder 共用）。
pub fn tree_view_scope(
    id: impl Into<String>,
    variant: TreeViewVariant,
    expanded: Option<Signal<HashSet<String>>>,
    expanded_storage_key: Option<String>,
    row_height: f32,
    height: Option<f32>,
    exclusive: Option<bool>,
    chrome: TreeViewChrome,
    selected: Option<Signal<String>>,
    on_select: Option<Rc<dyn Fn(String)>>,
    prefab_roots: Vec<TreeNode>,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = resolve_instance_id(id, "tree-view");
    let default_exp_key = format!("{id}__tree_exp");
    let expanded_storage_key = expanded_storage_key
        .filter(|k| !k.trim().is_empty())
        .unwrap_or(default_exp_key);
    let expanded = expanded
        .unwrap_or_else(|| use_signal(expanded_storage_key.as_str(), HashSet::<String>::new));
    let exclusive = exclusive.unwrap_or(false);
    let height = height.unwrap_or(match (chrome, variant) {
        (TreeViewChrome::Sidebar, TreeViewVariant::Collapsible) => 0.0,
        _ => 240.0,
    });
    let ctx = TreeViewCtx {
        id: id.clone(),
        variant,
        expanded,
        expanded_storage_key,
        exclusive,
        chrome,
        selected,
    };
    register_tree_on_select(&id, on_select);
    let kids = with_child_owner(|| {
        provide_context(tree_view_context(), ctx.clone());
        take_fragment(body())
    });
    let nodes = if prefab_roots.is_empty() {
        extract_item_nodes(&kids)
    } else {
        prefab_roots
    };
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
    shell
        .attrs
        .insert(ATTR_SLOT.to_string(), SLOT_ROOT.to_string());
    shell
        .attrs
        .insert(ATTR_CHROME.to_string(), chrome.as_str().to_string());
    shell.template_id = Some(id.clone());
    shell
}

/// 宏 `<TreeViewItem>`：标记节点元数据；由 [`tree_view_scope`] 统一建树。
pub fn tree_view_item_scope(
    node_id: impl Into<String>,
    label: impl Into<String>,
    disabled: bool,
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
        let mut el: Element = vstack().w("100%").min_w(0.0).gap(0.0).children(kids).into();
        el.attrs
            .insert(ATTR_SLOT.to_string(), SLOT_ITEM.to_string());
        el.attrs
            .insert("jadix_tree_item_id".to_string(), node_id.clone());
        el.attrs.insert("jadix_tree_item_label".to_string(), label);
        if disabled {
            el.attrs
                .insert("jadix_tree_item_disabled".to_string(), "1".into());
        }
        el
    })
}

fn extract_item_nodes(kids: &[Element]) -> Vec<TreeNode> {
    kids.iter()
        .filter_map(|el| {
            if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) != Some(SLOT_ITEM) {
                return None;
            }
            let id = el
                .attrs
                .get("jadix_tree_item_id")
                .cloned()
                .unwrap_or_default();
            let label = el
                .attrs
                .get("jadix_tree_item_label")
                .cloned()
                .unwrap_or_else(|| id.clone());
            let children = extract_item_nodes(&el.children);
            let class = el
                .class_set
                .as_ref()
                .map(|cs| cs.base.join(" "))
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_default();
            Some(TreeNode {
                id,
                label,
                children,
                chrome: SlotChrome { class, style: None },
            })
        })
        .collect()
}

// ── RSX parts ─────────────────────────────────────────────────────────────────

crate::__component_contract! {
TreeViewProps [];
/// RSX 备用入口；复合面请走宏 → [`tree_view_scope`]。
#[allow(non_snake_case)]
pub fn TreeView(
identity [] id: String,
plain [] variant: Option<String>,
plain [] expanded: Option<Signal<HashSet<String>>>,
plain [] expanded_storage_key: Option<String>,
plain [] row_height: Option<f32>,
plain [] height: Option<f32>,
plain [] exclusive: Option<bool>,
plain [] chrome: Option<String>,
plain [] selected: Option<Signal<String>>,
plain [] children: Option<Element>,
) -> Element {
    let variant = variant
        .map(|s| TreeViewVariant::parse(&s))
        .unwrap_or_default();
    let chrome = chrome
        .map(|s| TreeViewChrome::parse(&s))
        .unwrap_or_default();
    let row_height = row_height.unwrap_or(32.0);
    tree_view_scope(
        id,
        variant,
        expanded,
        expanded_storage_key,
        row_height,
        height,
        exclusive,
        chrome,
        selected,
        None,
        Vec::new(),
        move || children.unwrap_or_else(Element::new),
    )
}
}

crate::__component_contract! {
TreeViewItemProps [];
/// `ui!` `<TreeViewItem>` 宏展开目标。
#[allow(non_snake_case)]
pub fn TreeViewItem(
identity [] id: String,
plain [] label: Option<String>,
default [] disabled: bool,
plain [] children: Option<Element>,
) -> Element {
    let label = label.unwrap_or_else(|| id.clone());
    let mut el = tree_view_item_scope(id.clone(), label.clone(), disabled, move || {
        children.unwrap_or_else(Element::new)
    });
    el.attrs.insert("jadix_tree_item_label".to_string(), label);
    el
}
}

// ── Builder ───────────────────────────────────────────────────────────────────

pub struct TreeViewBuilder {
    id: String,
    variant: TreeViewVariant,
    roots: Vec<TreeNode>,
    row_height: f32,
    height: Option<f32>,
    expanded: Option<Signal<HashSet<String>>>,
    expanded_storage_key: Option<String>,
    class: Option<String>,
    style: Option<SlotStyleFn>,
    exclusive: Option<bool>,
    chrome: TreeViewChrome,
    selected: Option<Signal<String>>,
    on_select: Option<Rc<dyn Fn(String)>>,
}

pub fn tree_view(id: impl Into<String>) -> TreeViewBuilder {
    TreeViewBuilder {
        id: id.into(),
        variant: TreeViewVariant::Flat,
        roots: Vec::new(),
        row_height: 32.0,
        height: None,
        expanded: None,
        expanded_storage_key: None,
        class: None,
        style: None,
        exclusive: None,
        chrome: TreeViewChrome::Default,
        selected: None,
        on_select: None,
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
        self.height = Some(h);
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

    /// Same [`BaseComponentBuilder`] surface as RSX layout attrs.
    pub fn style(
        mut self,
        f: impl Fn(BaseComponentBuilder) -> BaseComponentBuilder + 'static,
    ) -> Self {
        self.style = Some(std::rc::Rc::new(f));
        self
    }

    /// Collapsible 同层兄弟是否互斥。默认 `false`（与独立 Collapsible 一致）；flat 忽略。
    pub fn exclusive(mut self, v: bool) -> Self {
        self.exclusive = Some(v);
        self
    }

    pub fn chrome(mut self, c: impl Into<TreeViewChrome>) -> Self {
        self.chrome = c.into();
        self
    }

    pub fn selected_signal(mut self, s: Signal<String>) -> Self {
        self.selected = Some(s);
        self
    }

    pub fn on_select(mut self, f: impl Fn(String) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }

    /// 在上下文内构建任意子树（`TreeViewItem` 等）。
    pub fn child(self, body: impl FnOnce() -> Element) -> Element {
        SlotChrome::from_opts(self.class, self.style).apply(tree_view_scope(
            self.id,
            self.variant,
            self.expanded,
            self.expanded_storage_key,
            self.row_height,
            self.height,
            self.exclusive,
            self.chrome,
            self.selected,
            self.on_select,
            Vec::new(),
            body,
        ))
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
            style,
            exclusive,
            chrome,
            selected,
            on_select,
        } = builder;
        SlotChrome::from_opts(class, style).apply(tree_view_scope(
            id,
            variant,
            expanded,
            expanded_storage_key,
            row_height,
            height,
            exclusive,
            chrome,
            selected,
            on_select,
            roots,
            Element::new,
        ))
    }
}

impl crate::core::tree::IntoElementList for TreeViewBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[cfg(all(test, feature = "product-tests"))]
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
                chrome: Default::default(),
            }],
            chrome: Default::default(),
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
    fn sibling_group_ids_do_not_collide_with_node_named_root() {
        assert_ne!(
            collapsible_sibling_group("tv", ""),
            collapsible_sibling_group("tv", "root"),
            "forest exclusive group must not share id with children of a node id=root"
        );
    }

    fn sample_workspace_tree() -> Vec<TreeNode> {
        vec![TreeNode {
            id: "root".into(),
            label: "Workspace".into(),
            children: vec![
                TreeNode {
                    id: "src".into(),
                    label: "src".into(),
                    children: vec![
                        TreeNode {
                            id: "main".into(),
                            label: "main.rs".into(),
                            children: vec![],
                            chrome: Default::default(),
                        },
                        TreeNode {
                            id: "lib".into(),
                            label: "lib.rs".into(),
                            children: vec![],
                            chrome: Default::default(),
                        },
                    ],
                    chrome: Default::default(),
                },
                TreeNode {
                    id: "docs".into(),
                    label: "Docs".into(),
                    children: vec![TreeNode {
                        id: "readme".into(),
                        label: "README.md".into(),
                        children: vec![],
                        chrome: Default::default(),
                    }],
                    chrome: Default::default(),
                },
            ],
            chrome: Default::default(),
        }]
    }

    fn subtree_has_text(el: &Element, needle: &str) -> bool {
        el.text.as_deref() == Some(needle)
            || el.children.iter().any(|c| subtree_has_text(c, needle))
    }

    fn has_slot_containing_text(el: &Element, slot: &str, needle: &str) -> bool {
        let here = el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(slot);
        if here && subtree_has_text(el, needle) {
            return true;
        }
        el.children
            .iter()
            .any(|c| has_slot_containing_text(c, slot, needle))
    }

    #[test]
    fn collapsible_tree_leaves_are_static_rows() {
        let (_o, el) = create_root(|| {
            let expanded_key = "tv_col_leaf::expanded";
            use_signal(expanded_key, HashSet::<String>::new);
            tree_view("tv_col_leaf")
                .variant("collapsible")
                .height(160.0)
                .expanded_storage_key(expanded_key)
                .roots(vec![
                    TreeNode {
                        id: "readme".into(),
                        label: "README.md".into(),
                        children: vec![],
                        chrome: Default::default(),
                    },
                    TreeNode {
                        id: "src".into(),
                        label: "src".into(),
                        children: vec![TreeNode {
                            id: "main".into(),
                            label: "main.rs".into(),
                            children: vec![],
                            chrome: Default::default(),
                        }],
                        chrome: Default::default(),
                    },
                ])
                .into()
        });
        assert!(
            has_slot_containing_text(&el, SLOT_ROW, "README.md"),
            "root-level files must use tree-view-row, not CollapsibleTrigger"
        );
        assert!(
            !has_slot_containing_text(&el, "collapsible-trigger", "README.md"),
            "leaf files must not be expandable collapsibles"
        );
    }

    #[test]
    fn collapsible_tree_sibling_expand_is_exclusive() {
        let (_o, ()) = create_root(|| {
            let expanded_key = "tv_col_excl::expanded";
            use_signal(expanded_key, || {
                let mut s = HashSet::<String>::new();
                s.insert("root".into());
                s.insert("src".into());
                s.insert("docs".into());
                s
            });
            let _: Element = tree_view("tv_col_excl")
                .variant("collapsible")
                .exclusive(true)
                .height(160.0)
                .expanded_storage_key(expanded_key)
                .roots(sample_workspace_tree())
                .into();
            let set = expanded_signal_for_key(expanded_key)
                .try_peek()
                .unwrap_or_default();
            let sibling_open = ["src", "docs"]
                .iter()
                .filter(|id| set.contains(**id))
                .count();
            assert_eq!(
                sibling_open, 1,
                "same-level collapsible must keep only one sibling expanded, set={set:?}"
            );
        });
    }

    #[test]
    fn collapsible_tree_exclusive_false_keeps_sibling_open() {
        let (_o, ()) = create_root(|| {
            let expanded_key = "tv_col_multi::expanded";
            use_signal(expanded_key, || {
                let mut s = HashSet::<String>::new();
                s.insert("root".into());
                s.insert("src".into());
                s.insert("docs".into());
                s
            });
            let _: Element = tree_view("tv_col_multi")
                .variant("collapsible")
                .height(160.0)
                .expanded_storage_key(expanded_key)
                .roots(sample_workspace_tree())
                .into();
            let set = expanded_signal_for_key(expanded_key)
                .try_peek()
                .unwrap_or_default();
            let sibling_open = ["src", "docs"]
                .iter()
                .filter(|id| set.contains(**id))
                .count();
            assert_eq!(
                sibling_open, 2,
                "default collapsible must keep sibling folders open, set={set:?}"
            );
        });
    }

    #[test]
    fn variant_parse_falls_back_to_flat() {
        assert_eq!(
            TreeViewVariant::parse("collapsible"),
            TreeViewVariant::Collapsible
        );
        assert_eq!(
            TreeViewVariant::parse("accordion"),
            TreeViewVariant::Collapsible
        );
        assert_eq!(TreeViewVariant::parse("unknown"), TreeViewVariant::Flat);
    }

    #[test]
    fn distinct_tree_ids_do_not_share_expanded() {
        let roots = vec![TreeNode {
            id: "root".into(),
            label: "Workspace".into(),
            children: vec![TreeNode {
                id: "src".into(),
                label: "src".into(),
                children: vec![],
                chrome: Default::default(),
            }],
            chrome: Default::default(),
        }];
        let (_o, ()) = create_root(|| {
            let a_key = "tv-isol-a__tree_exp";
            let b_key = "tv-isol-b__tree_exp";
            let init = || {
                let mut s = HashSet::<String>::new();
                s.insert("root".into());
                s
            };
            let _: Element = tree_view("tv-isol-a")
                .variant("collapsible")
                .height(80.0)
                .expanded_signal(use_signal(a_key, init))
                .roots(roots.clone())
                .into();
            let _: Element = tree_view("tv-isol-b")
                .variant("flat")
                .height(80.0)
                .expanded_signal(use_signal(b_key, init))
                .roots(roots)
                .into();
            toggle_node_expanded_key(a_key, "root");
            assert!(!expanded_signal_for_key(a_key)
                .try_peek()
                .is_some_and(|s| s.contains("root")));
            assert!(expanded_signal_for_key(b_key)
                .try_peek()
                .is_some_and(|s| s.contains("root")));
        });
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
                    chrome: Default::default(),
                }],
                chrome: Default::default(),
            }],
            chrome: Default::default(),
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
            let roots_m = roots.clone();
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
                let multi: Element = tree_view("tv-shared-multi")
                    .variant("collapsible")
                    .exclusive(false)
                    .height(160.0)
                    .expanded_storage_key(expanded_key)
                    .roots(roots_m.clone())
                    .into();
                let flat: Element = tree_view("tv-shared-flat")
                    .variant("flat")
                    .height(140.0)
                    .expanded_storage_key(expanded_key)
                    .roots(roots_c.clone())
                    .into();
                *h.borrow_mut() = Some(vstack().child(collapsible).child(multi).child(flat).into());
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
                chrome: Default::default(),
            }],
            chrome: Default::default(),
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
    fn node_open_binding_survives_parent_content_remount() {
        let (_owner, ()) = create_root(|| {
            let expanded_key = "tv_remount_binding::expanded";
            let expanded = use_signal(expanded_key, HashSet::<String>::new);
            let revision = crate::core::reactivity::Signal::new(0_u32);
            create_effect(move || {
                let _ = revision.get();
                node_open_signal("tv_remount_binding", "child", expanded_key);
            });
            // CollapsibleContent rebuilds descendants under a fresh effect owner.
            revision.set(1);
            expanded.set(HashSet::from(["child".into()]));
            let open = use_signal("tv_remount_binding::open::child", || false);
            assert_eq!(open.peek(), true, "remounted child must follow expanded");
            open.set(false);
            assert!(
                !expanded.peek().contains("child"),
                "click must update expanded after remount"
            );
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
                    chrome: Default::default(),
                }])
                .into()
        });
        assert_eq!(el.attrs.get(ATTR_SLOT).map(|s| s.as_str()), Some(SLOT_ROOT));
        assert_eq!(el.template_id.as_deref(), Some("tv-test"));
    }

    fn find_tid(el: &Element, id: &str) -> bool {
        el.template_id.as_deref() == Some(id) || el.children.iter().any(|c| find_tid(c, id))
    }

    fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
        if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(slot) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_slot(c, slot))
    }

    fn first_text_color(el: &Element) -> Option<[u8; 4]> {
        if el.text.is_some() {
            return Some(el.text_style.color_resolved());
        }
        el.children.iter().find_map(first_text_color)
    }

    #[test]
    fn sidebar_row_chrome_hover_and_selected() {
        let mut row = crate::primitives::r#box::hstack().w("100%").into();
        sync_tree_row_chrome(&mut row, TreeViewChrome::Sidebar, false, true);
        assert_eq!(row.style.bg_color, parse_color("sidebar-accent"));
        sync_tree_row_chrome(&mut row, TreeViewChrome::Sidebar, true, false);
        assert_eq!(row.style.bg_color, parse_color("sidebar-accent"));
        sync_tree_row_chrome(&mut row, TreeViewChrome::Sidebar, false, false);
        assert_eq!(row.style.bg_color, [0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn sidebar_chrome_skips_inner_scroll_and_uses_sidebar_fg() {
        let (_o, el): (_, Element) = create_root(|| {
            tree_view("tv-sidebar-chrome")
                .variant("collapsible")
                .chrome("sidebar")
                .roots(vec![TreeNode {
                    id: "a".into(),
                    label: "A".into(),
                    children: vec![],
                    chrome: Default::default(),
                }])
                .into()
        });
        assert_eq!(
            el.attrs.get(ATTR_CHROME).map(String::as_str),
            Some("sidebar")
        );
        assert!(!find_tid(&el, "tv-sidebar-chrome__scroll"));
        let row = find_slot(&el, SLOT_ROW).expect("tree row");
        assert_eq!(
            first_text_color(row),
            Some(parse_text_color("sidebar-foreground"))
        );
    }

    #[test]
    fn builder_node_style_reaches_row() {
        let (_o, el): (_, Element) = create_root(|| {
            tree_view("tv-node-chrome")
                .variant("collapsible")
                .roots(vec![tree_node("a", "A").style(|b| b.text_size(20.0))])
                .into()
        });
        fn find_size(el: &Element, want: f32) -> bool {
            el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
                || (el.text_style.size_resolved() - want).abs() < 0.01
                || el.children.iter().any(|c| find_size(c, want))
        }
        assert!(
            find_size(&el, 20.0),
            "tree_node .style must land on the painted row"
        );
    }

    #[test]
    fn emit_select_writes_selected_signal() {
        let (_o, ()) = create_root(|| {
            let sel = use_signal("tv_emit_sel", String::new);
            let ctx = TreeViewCtx {
                id: "tv-emit".into(),
                variant: TreeViewVariant::Collapsible,
                expanded: use_signal("tv_emit_exp", HashSet::<String>::new),
                expanded_storage_key: "tv_emit_exp".into(),
                exclusive: false,
                chrome: TreeViewChrome::Default,
                selected: Some(sel),
            };
            emit_select(&ctx, "leaf");
            assert_eq!(sel.try_peek().as_deref(), Some("leaf"));
        });
    }

    #[test]
    fn dual_surface_compiles_shape() {
        let _b: Element = tree_view("x").variant(TreeViewVariant::Flat).into();
        let el = TreeView(TreeViewProps {
            id: "y".into(),
            variant: Some("collapsible".into()),
            expanded: None,
            expanded_storage_key: None,
            row_height: None,
            height: None,
            exclusive: None,
            chrome: None,
            selected: None,
            children: None,
            base: None,
        });
        let _: Element = el;
    }
}
