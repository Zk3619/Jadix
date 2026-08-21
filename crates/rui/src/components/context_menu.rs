//! # ContextMenu
//!
//! 对齐 solid-ui / ShadCN **复合 ContextMenu**：
//!
//! ```text
//! ContextMenu
//! ├── ContextMenuTrigger      // 右键打开；子节点为触发区域
//! └── ContextMenuContent      // Portal + floating；bg-popover / border / shadow
//!     ├── ContextMenuLabel
//!     ├── ContextMenuItem     // hover/focus:bg-accent；可选 inset / destructive
//!     ├── ContextMenuShortcut // ml-auto text-xs tracking-widest
//!     ├── ContextMenuSeparator
//!     ├── ContextMenuGroup
//!     └── ContextMenuSub
//!         ├── ContextMenuSubTrigger  // chevron-right；悬停打开
//!         └── ContextMenuSubContent  // 右侧嵌套菜单
//! ```
//!
//! `ui!` 中 `<ContextMenu>` / `<ContextMenuContent>` 由宏展开。Builder
//! [`context_menu`] / [`ContextMenuBuilder::trigger`] 仍可用（内部接到同一套实现）。
//!
//! ## 用法（双写法）
//! ```ignore
//! // A. 复合 RSX（推荐；宏走 context_menu_scope）
//! ui! {
//!     <ContextMenu id="ctx">
//!         <ContextMenuTrigger>
//!             <Rect p="0.75rem" bg="muted" class="rounded-md">
//!                 <Label>"右键打开"</Label>
//!             </Rect>
//!         </ContextMenuTrigger>
//!         <ContextMenuContent>
//!             <ContextMenuLabel>"Actions"</ContextMenuLabel>
//!             <ContextMenuItem on_click={|| toast_push("Copy", 1500)}>
//!                 "Copy"
//!                 <ContextMenuShortcut>"⌘C"</ContextMenuShortcut>
//!             </ContextMenuItem>
//!             <ContextMenuSeparator />
//!             <ContextMenuItem variant="destructive" on_click={|| {}}>
//!                 "Delete"
//!             </ContextMenuItem>
//!         </ContextMenuContent>
//!     </ContextMenu>
//! }
//!
//! // B. Builder（便捷入口，内建 Trigger + Content + Items）
//! context_menu("ctx")
//!     .item("Copy", || toast_push("Copy", 1500))
//!     .item("Paste", || toast_push("Paste", 1500))
//!     .trigger(|| { /* 触发区域 Element */ })
//! ```
//!
//! ## 注意
//! - 右键打开时以**鼠标位置**为原点向右下展开；空间不足时先换向，再视口内平移，最后夹高度并内部滚动。
//! - 打开后 ↑↓ / Home / End 走 roving；Enter / Space 激活高亮项；Esc 经 overlay 栈关闭。
//! - 未传 `open` 时用内部信号 `{id}__ctx_open`；多实例请保证 `id` 唯一。

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::OnceLock;

use rui_macros::component;
use winit::keyboard::{Key, NamedKey};

use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode};
use crate::app::reactive::{reactive_anchor, reactive_placeholder_sized};
use crate::components::label::label;
use crate::components::scroll_area::scroll_area;
use crate::components::separator::separator;
use crate::core::reactivity::{
    create_context, isolate_reads, provide_context, untrack, use_context, use_signal,
    with_child_owner, Context, Signal,
};
use crate::core::tree::{CursorKind, Element, Handler, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::interactive::{interactive, mark_hover_paint_dirty};
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::primitives::text::text;
use crate::style::{parse_color, parse_text_color, with_class_list, WhiteSpace};
use crate::tokens;
use crate::utils::lucide_glyph;
use crate::utils::popup::{
    apply_stored_floating_place, mark_floating, mark_floating_at_point, mark_floating_with_gap,
    mount_anchored_overlay, PopupPlacement, ATTR_FLOATING_MATCH_TRIGGER, ATTR_FLOATING_MIN_W,
    ATTR_FLOATING_TRIGGER, FLOATING_TRIGGER_GAP,
};
use crate::utils::resolve_instance_id;
use crate::utils::roving::{roving_edge, roving_move};
use crate::utils::text_node::build_icon_text_node;

const ATTR_SLOT: &str = "data-slot";
const SLOT_TRIGGER: &str = "context-menu-trigger";
const SLOT_CONTENT: &str = "context-menu-content";
const SLOT_ITEM: &str = "context-menu-item";
const SLOT_LABEL: &str = "context-menu-label";
const SLOT_SEPARATOR: &str = "context-menu-separator";
const SLOT_SHORTCUT: &str = "context-menu-shortcut";
const SLOT_GROUP: &str = "context-menu-group";
const SLOT_SUB_TRIGGER: &str = "context-menu-sub-trigger";
const SLOT_SUB_CONTENT: &str = "context-menu-sub-content";

/// Visual gap between SubTrigger and SubContent (same as default floating gap).
/// Hit-testing uses transparent side pads so the pointer cannot fall through the seam.
const SUBMENU_VISUAL_GAP_PX: f32 = FLOATING_TRIGGER_GAP;

/// Roving / hover highlight: no item selected.
const ACTIVE_NONE: usize = usize::MAX;

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

/// Root 上下文：`open` + 稳定实例 `id` + 触发器锚点 + roving 高亮。
#[derive(Clone)]
pub struct ContextMenuCtx {
    pub open: Signal<bool>,
    pub id: String,
    /// 触发器 focus_id（键盘 / outside-press）；面板几何优先用 [`Self::cursor`]。
    pub trigger_fid: Signal<String>,
    /// 右键打开时的视口坐标；`[-1,-1]` 表示未记录（回退触发器锚点）。
    pub cursor: Signal<[f32; 2]>,
    pub active: Signal<usize>,
    pub placement: PopupPlacement,
}

fn context_menu_context() -> Context<ContextMenuCtx> {
    static CTX: OnceLock<Context<ContextMenuCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<ContextMenuCtx>)
}

thread_local! {
    static CTX_ACTIVATES: RefCell<HashMap<String, Vec<Rc<dyn Fn()>>>> = RefCell::new(HashMap::new());
    static SUB_SEQ: RefCell<HashMap<String, usize>> = RefCell::new(HashMap::new());
}

fn reset_activates(id: &str) {
    CTX_ACTIVATES.with(|m| {
        m.borrow_mut().insert(id.to_string(), Vec::new());
    });
}

fn push_activate(id: &str, f: Rc<dyn Fn()>) {
    CTX_ACTIVATES.with(|m| {
        m.borrow_mut().entry(id.to_string()).or_default().push(f);
    });
}

fn activate_count(id: &str) -> usize {
    CTX_ACTIVATES.with(|m| m.borrow().get(id).map(|v| v.len()).unwrap_or(0))
}

fn take_activate(id: &str, ix: usize) -> Option<Rc<dyn Fn()>> {
    CTX_ACTIVATES.with(|m| m.borrow().get(id).and_then(|v| v.get(ix).cloned()))
}

fn reset_sub_seq(parent_id: &str) {
    SUB_SEQ.with(|m| {
        m.borrow_mut().insert(parent_id.to_string(), 0);
    });
}

fn next_sub_seq(parent_id: &str) -> usize {
    SUB_SEQ.with(|m| {
        let mut map = m.borrow_mut();
        let e = map.entry(parent_id.to_string()).or_insert(0);
        let i = *e;
        *e += 1;
        i
    })
}

fn set_ctx_active(active: Signal<usize>, next: usize) {
    if active.peek() != next {
        active.set_silent(next);
        mark_hover_paint_dirty();
    }
}

fn clear_ctx_active_if(active: Signal<usize>, ix: usize) {
    if active.peek() == ix {
        set_ctx_active(active, ACTIVE_NONE);
    }
}

fn close_open_subs(parent_id: &str) {
    close_open_subs_except(parent_id, None);
}

fn close_open_subs_except(parent_id: &str, keep_sub_id: Option<&str>) {
    // Best-effort: close a small range of auto-sequenced sub menus.
    for i in 0..16 {
        let sid = format!("{parent_id}__sub{i}");
        if keep_sub_id.is_some_and(|k| k == sid) {
            continue;
        }
        let key = format!("{sid}__open");
        let sig = use_signal(key.as_str(), || false);
        if sig.peek() {
            sig.set(false);
        }
    }
}

/// 读取最近的 [`ContextMenu`] 上下文。
pub fn use_context_menu() -> Option<ContextMenuCtx> {
    use_context(context_menu_context())
}

fn require_context_menu(part: &str) -> ContextMenuCtx {
    use_context_menu().unwrap_or_else(|| {
        panic!("{part} must be used within <ContextMenu> / context_menu_scope");
    })
}

/// 提供 ContextMenu 上下文并构建子树（`ui!` `<ContextMenu>` / Builder 共用）。
pub fn context_menu_scope(
    id: impl Into<String>,
    open: Option<Signal<bool>>,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = resolve_instance_id(id, "context_menu");
    let open = open.unwrap_or_else(|| use_signal(format!("{id}__ctx_open").as_str(), || false));
    let active = use_signal(format!("{id}__ctx_active").as_str(), || ACTIVE_NONE);
    let trigger_fid = use_signal(format!("{id}__ctx_trigger_fid").as_str(), || {
        format!("{id}__ctx_trigger")
    });
    let cursor = use_signal(format!("{id}__ctx_cursor").as_str(), || [-1.0_f32, -1.0]);
    provide_context(
        context_menu_context(),
        ContextMenuCtx {
            open,
            id: id.clone(),
            trigger_fid,
            cursor,
            active,
            placement: PopupPlacement::BottomStart,
        },
    );
    let mut root = body();
    if root.template_id.is_none() {
        root.template_id = Some(format!("{id}__context_menu"));
    }
    root
}

fn take_packed(mut el: Element) -> Vec<Element> {
    if el.attrs.get("rui_fragment").map(|s| s.as_str()) == Some("1") {
        std::mem::take(&mut el.children)
    } else if !el.children.is_empty()
        && el.text.is_none()
        && el.on_click.is_none()
        && el.focus_id.is_none()
    {
        std::mem::take(&mut el.children)
    } else {
        vec![el]
    }
}

fn extract_text_leaf(el: &Element) -> String {
    if let Some(t) = &el.text {
        return t.clone();
    }
    for c in &el.children {
        let t = extract_text_leaf(c);
        if !t.is_empty() {
            return t;
        }
    }
    String::new()
}

fn is_text_leaf(el: &Element) -> bool {
    el.children.is_empty() && el.text.as_ref().is_some_and(|t| !t.is_empty())
}

fn has_shortcut_slot(el: &Element) -> bool {
    el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_SHORTCUT)
        || el.children.iter().any(has_shortcut_slot)
}

fn style_item_text(el: &mut Element, color: [u8; 4]) {
    if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_SHORTCUT) {
        return;
    }
    if el.text.is_some() {
        el.text_style.size = 14.0;
        el.text_style.line_height_px = Some(20.0);
        el.text_style.weight = 600;
        el.text_style.color = color;
        // Mark explicit so `cascade_inheritable_text_style` does not wipe to parent 400
        // (that flash is the open-time weight jump).
        el.attrs.insert("font_weight".to_string(), "1".to_string());
        el.attrs.insert("text_size".to_string(), "1".to_string());
        el.attrs.insert("text_color".to_string(), "1".to_string());
        el.attrs.insert("line_height".to_string(), "1".to_string());
        // Prevent shrink-wrap death spiral (min_w:0 + wrap → 1ch menu width).
        el.text_style.white_space = WhiteSpace::Nowrap;
        el.attrs
            .insert("white_space".to_string(), "nowrap".to_string());
        el.style.flex_shrink = 0.0;
    }
    for c in &mut el.children {
        style_item_text(c, color);
    }
}

/// Shared item chrome. Do **not** use `min_w(0)` — that collapses wrapping labels to 1ch.
fn item_row_chrome(inset: bool) -> crate::primitives::r#box::BoxBuilder {
    hstack()
        .w("auto")
        .min_w("100%")
        .items_center()
        .gap("0.5rem")
        .h("auto")
        .min_h("1.75rem")
        .flex_shrink(0.0)
        .rounded("0.25rem")
        .py("0.25rem")
        .px(if inset { "2rem" } else { "0.5rem" })
        .justify("start")
}

fn attach_trigger_keys(mut trigger: Element, ctx: &ContextMenuCtx) -> Element {
    let open_keys = ctx.open;
    let active_keys = ctx.active;
    let menu_id = ctx.id.clone();
    trigger = trigger.on_key_down(move |ev| {
        if !open_keys.peek() {
            return false;
        }
        let n = activate_count(&menu_id);
        if n == 0 {
            return false;
        }
        match &ev.logical_key {
            Key::Named(NamedKey::ArrowDown) => {
                let cur = active_keys.peek();
                let next = if cur == ACTIVE_NONE || cur >= n {
                    0
                } else if let Some(i) = roving_move(n, cur, 1) {
                    i
                } else {
                    return false;
                };
                set_ctx_active(active_keys, next);
                true
            }
            Key::Named(NamedKey::ArrowUp) => {
                let cur = active_keys.peek();
                let next = if cur == ACTIVE_NONE || cur >= n {
                    n - 1
                } else if let Some(i) = roving_move(n, cur, -1) {
                    i
                } else {
                    return false;
                };
                set_ctx_active(active_keys, next);
                true
            }
            Key::Named(NamedKey::Home) => {
                if let Some(i) = roving_edge(n, false) {
                    set_ctx_active(active_keys, i);
                }
                true
            }
            Key::Named(NamedKey::End) => {
                if let Some(i) = roving_edge(n, true) {
                    set_ctx_active(active_keys, i);
                }
                true
            }
            Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                let ix = active_keys.peek();
                if ix == ACTIVE_NONE {
                    return false;
                }
                if let Some(cb) = take_activate(&menu_id, ix) {
                    cb();
                    open_keys.set(false);
                    return true;
                }
                false
            }
            _ => false,
        }
    });
    trigger
}

// ---------------------------------------------------------------------------
// Trigger / Content
// ---------------------------------------------------------------------------

/// solid-ui / shadcn `ContextMenuTrigger`：右键打开；子节点为触发区域。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ContextMenuTrigger(id: String, children: Option<Element>) -> Element {
    let ctx = require_context_menu("ContextMenuTrigger");
    let open = ctx.open;
    let active = ctx.active;
    let trigger_id = if id.is_empty() {
        format!("{}__ctx_trigger", ctx.id)
    } else {
        id
    };
    ctx.trigger_fid.set(trigger_id.clone());

    let mut trigger = match children {
        None => {
            let mut el: Element = r#box().px("0.75rem").py("0.5rem").into();
            el = el.child(text("Right-click me"));
            el.focus_id(trigger_id.as_str())
        }
        Some(c) if is_text_leaf(&c) => {
            let t = c.text.clone().unwrap_or_default();
            let mut el: Element = r#box().px("0.75rem").py("0.5rem").into();
            el = el.child(text(t.as_str()));
            el.focus_id(trigger_id.as_str())
        }
        Some(mut c) => {
            if c.focus_id.is_none() {
                c = c.focus_id(trigger_id.as_str());
            }
            let fid = c.focus_id.clone().unwrap_or(trigger_id.clone());
            ctx.trigger_fid.set(fid);
            c
        }
    };

    // Custom (non-Button) triggers need an explicit focus-visible ring.
    if trigger.button_hover_paint.is_none() && trigger.input_paint_sync.is_none() {
        trigger.focusable = true;
        trigger.focus_ring = true;
    }
    let fid_focus = trigger
        .focus_id
        .clone()
        .unwrap_or_else(|| trigger_id.clone());
    ctx.trigger_fid.set(fid_focus.clone());

    let open_ctx = open;
    let active_open = active;
    let cursor = ctx.cursor;
    let menu_id_open = ctx.id.clone();
    trigger = trigger.on_context_menu(move |ev| {
        set_ctx_active(active_open, ACTIVE_NONE);
        close_open_subs(&menu_id_open);
        cursor.set([ev.x, ev.y]);
        // Mouse open: focus trigger for arrow-key roving, but keep focus-visible false (no ring).
        use_signal("__rui_button_focus_id", String::new).set(fid_focus.clone());
        open_ctx.set(true);
    });
    trigger = attach_trigger_keys(trigger, &ctx);
    trigger.set_attr(ATTR_SLOT, SLOT_TRIGGER);
    trigger
}

/// `ContextMenuContent` 选项（`ui!` / [`context_menu_content`] 共用）。
#[derive(Clone, Debug)]
pub struct ContextMenuContentOpts {
    /// `bottom-start` / `right` / …（默认 `bottom-start`）。
    pub placement: String,
    /// 额外 class（utility）。
    pub class: Option<String>,
}

impl Default for ContextMenuContentOpts {
    fn default() -> Self {
        Self {
            placement: "bottom-start".into(),
            class: None,
        }
    }
}

/// solid-ui / shadcn `ContextMenuContent`：Portal + floating 面板。
/// `ui!` 中由宏编成此函数（子树在打开时惰性重建）。
pub fn context_menu_content(
    opts: ContextMenuContentOpts,
    body: impl Fn() -> Element + 'static,
) -> Element {
    let ctx = require_context_menu("ContextMenuContent");
    let open = ctx.open;
    let id = ctx.id.clone();
    let trigger_fid = ctx.trigger_fid;
    let cursor = ctx.cursor;
    let menu_id = id.clone();
    let _ = ctx.active;
    let _placement = if opts.placement.trim().is_empty() {
        ctx.placement
    } else {
        PopupPlacement::from_attr(&opts.placement)
    };
    let class = opts.class.filter(|s| !s.trim().is_empty());
    let view_id = format!("{id}__ctx_view");

    reactive_anchor(view_id.clone(), move || {
        let is_open = open.get();
        // Do not subscribe to `active` — highlight is paint-synced (hover must not rebuild).
        let _ = cursor.get();
        let Some(oz) = portal_sync(&id, open, OverlayKind::Menu, OverlayMode::Stacked) else {
            return Element::new().w(0.0).h(0.0);
        };
        if !is_open {
            close_open_subs(&menu_id);
            return Element::new().w(0.0).h(0.0);
        }

        reset_activates(&menu_id);
        reset_sub_seq(&menu_id);
        // isolate_reads：嵌套 SubContent 的 reactive_anchor 必须能注册；勿订阅 sub open
        //（否则整棵 Content 重建会拆掉悬停目标，子菜单 hover 不跟手）。
        let rows = isolate_reads(|| body());
        let scroll_id = format!("{menu_id}__ctx_scroll");
        // 外层 chrome 承载 floating max_h；内层 ScrollArea 负责溢出滚动。
        let viewport: Element = untrack(|| {
            scroll_area(scroll_id.as_str())
                .overflow_x("hidden")
                .overflow_y("auto")
                .scrollbar_visibility("auto")
                .w("100%")
                .h("auto")
                .min_h(0.0)
                .flex_grow(1.0)
                .flex_shrink(1.0)
                .child(rows)
                .into()
        });
        let mut panel: Element = vstack()
            .w("auto")
            .h("auto")
            .min_w("12rem")
            .min_h(0.0)
            .p("0.25rem")
            .gap("0")
            .overflow_x("hidden")
            .overflow_y("hidden")
            .child(viewport)
            .into();
        // solid-ui: rounded-md border bg-popover p-1 text-popover-foreground shadow-md
        panel = panel
            .bg(tokens::popover())
            .rounded("0.375rem")
            .border_style("solid")
            .border_width(1.0)
            .border_color(tokens::border())
            .shadow([0.0, 0.0, 0.0, 0.14], 12.0)
            .absolute();
        if let Some(ref c) = class {
            panel = with_class_list(panel, c);
        }
        panel.set_attr(ATTR_SLOT, SLOT_CONTENT);
        panel = panel.accesskit_role("menu");
        let anchor = trigger_fid.peek();
        let pt = cursor.peek();
        if pt[0] >= 0.0 && pt[1] >= 0.0 {
            // 贴鼠标右下展开；gap≈2 避免压住光标热点。
            mark_floating_at_point(&mut panel, &id, &anchor, pt[0], pt[1], 2.0);
            // Cursor-anchored: clicking the original trigger must still dismiss (not
            // treated as "on floating trigger").
            panel.attrs.remove(ATTR_FLOATING_TRIGGER);
        } else {
            mark_floating(&mut panel, &id, &anchor, _placement);
        }
        panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
        panel.set_attr(ATTR_FLOATING_MIN_W, "12rem");
        apply_stored_floating_place(&mut panel, &id);
        mount_anchored_overlay(panel, oz.panel)
    });
    reactive_placeholder_sized(view_id, 0.0, 0.0).pointer_events_box_none()
}

// ---------------------------------------------------------------------------
// Item / Label / Separator / Shortcut / Group
// ---------------------------------------------------------------------------

/// solid-ui / shadcn `ContextMenuItem`：`px-2 py-1.5 text-sm`；高亮 `bg-accent`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ContextMenuItem(
    disabled: bool,
    inset: bool,
    variant: Option<String>,
    class: Option<String>,
    #[prop(default = "Handler::default()")] mut on_click: Handler,
    children: Option<Element>,
) -> Element {
    let ctx = require_context_menu("ContextMenuItem");
    let open = ctx.open;
    let active = ctx.active;
    let menu_id = ctx.id.clone();
    let destructive = variant
        .as_deref()
        .map(|s| s.trim().eq_ignore_ascii_case("destructive"))
        .unwrap_or(false);

    let on_click = Rc::new(RefCell::new(on_click));
    let activate = {
        let on_click = on_click.clone();
        Rc::new(move || {
            if !disabled {
                on_click.borrow_mut().call(());
            }
        }) as Rc<dyn Fn()>
    };
    let activate_click = activate.clone();
    let ix = activate_count(&ctx.id);
    push_activate(&ctx.id, activate);

    let mut kids = children
        .map(take_packed)
        .unwrap_or_else(|| vec![Element::new().text("Item")]);

    let fg = if destructive {
        parse_text_color("destructive")
    } else {
        parse_text_color("popover-foreground")
    };
    for k in &mut kids {
        if !has_shortcut_slot(k) {
            style_item_text(k, fg);
        }
    }

    let mut row = item_row_chrome(inset)
        .bg([0.0, 0.0, 0.0, 0.0])
        .text_color(fg);
    for k in kids {
        row = row.child(k);
    }
    let mut row: Element = row.into();
    if disabled {
        row = row.opacity(0.5);
    }
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        row = with_class_list(row, &c);
    }

    let mut hit: Element = interactive(row)
        .on_hover(move |h| {
            if disabled {
                return;
            }
            if !h {
                clear_ctx_active_if(active, ix);
                return;
            }
            close_open_subs(&menu_id);
            set_ctx_active(active, ix);
        })
        .on_click(move || {
            if disabled {
                return;
            }
            set_ctx_active(active, ix);
            activate_click();
            open.set(false);
        })
        .cursor_kind(if disabled {
            CursorKind::NotAllowed
        } else {
            CursorKind::Default
        })
        .into();
    // Hover paint targets this node — keep rounded here so accent fill matches chrome.
    hit = hit.rounded("0.25rem");
    hit.set_attr(ATTR_SLOT, SLOT_ITEM);
    hit.set_attr("rui_ctx_id", &ctx.id);
    hit.set_attr("rui_ctx_ix", ix.to_string());
    hit.set_attr("data-inset", if inset { "true" } else { "false" });
    if destructive {
        hit.set_attr("data-variant", "destructive");
        hit.set_attr("rui_ctx_destructive", "1");
    }
    if disabled {
        hit.set_attr("data-disabled", "true");
        hit.set_attr("rui_ctx_disabled", "1");
    }
    hit.accesskit_role("menuitem")
}

// ---------------------------------------------------------------------------
// Submenu (Sub / SubTrigger / SubContent)
// ---------------------------------------------------------------------------

/// Nested submenu context (under a root [`ContextMenuCtx`]).
#[derive(Clone)]
pub struct ContextMenuSubCtx {
    pub open: Signal<bool>,
    pub id: String,
    pub trigger_fid: Signal<String>,
}

fn context_menu_sub_context() -> Context<ContextMenuSubCtx> {
    static CTX: OnceLock<Context<ContextMenuSubCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<ContextMenuSubCtx>)
}

fn use_context_menu_sub() -> Option<ContextMenuSubCtx> {
    use_context(context_menu_sub_context())
}

fn require_context_menu_sub(part: &str) -> ContextMenuSubCtx {
    use_context_menu_sub().unwrap_or_else(|| {
        panic!("{part} must be used within <ContextMenuSub>");
    })
}

/// solid-ui / shadcn `ContextMenuSub`：包裹 SubTrigger + SubContent。
/// `ui!` 中由宏编成此函数（子树在 `provide_context` **之后**求值）。
pub fn context_menu_sub(id: Option<String>, body: impl FnOnce() -> Element) -> Element {
    let parent = require_context_menu("ContextMenuSub");
    let seq = next_sub_seq(&parent.id);
    let sid = match id {
        Some(s) if !s.trim().is_empty() => s,
        _ => format!("{}__sub{seq}", parent.id),
    };
    let open = use_signal(format!("{sid}__open").as_str(), || false);
    let trigger_fid = use_signal(format!("{sid}__tfid").as_str(), || format!("{sid}__trigger"));
    let kids = with_child_owner(|| {
        provide_context(
            context_menu_sub_context(),
            ContextMenuSubCtx {
                open,
                id: sid,
                trigger_fid,
            },
        );
        take_packed(body())
    });
    let mut frag = Element::new();
    frag.set_attr("rui_fragment", "1");
    frag.set_attr("data-slot", "context-menu-sub");
    frag.children(kids)
}

/// solid-ui / shadcn `ContextMenuSubTrigger`：右侧 chevron；悬停打开子菜单。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ContextMenuSubTrigger(
    disabled: bool,
    inset: bool,
    class: Option<String>,
    children: Option<Element>,
) -> Element {
    let parent = require_context_menu("ContextMenuSubTrigger");
    let sub = require_context_menu_sub("ContextMenuSubTrigger");
    let active = parent.active;
    let parent_id = parent.id.clone();
    let sub_id = sub.id.clone();
    let parent_id_click = parent_id.clone();
    let sub_id_click = sub_id.clone();
    let sub_open = sub.open;
    let sub_open_click = sub_open;
    let trigger_id = format!("{}__trigger", sub.id);
    sub.trigger_fid.set(trigger_id.clone());

    let activate = {
        let sub_open = sub_open;
        Rc::new(move || {
            if !disabled {
                sub_open.set(true);
            }
        }) as Rc<dyn Fn()>
    };
    let ix = activate_count(&parent.id);
    push_activate(&parent.id, activate);

    let mut kids = children
        .map(take_packed)
        .unwrap_or_else(|| vec![Element::new().text("More")]);
    let fg = parse_text_color("popover-foreground");
    for k in &mut kids {
        if !has_shortcut_slot(k) {
            style_item_text(k, fg);
        }
    }
    let chevron = lucide_glyph("chevron-right").unwrap_or_else(|| "›".into());
    let mut icon = build_icon_text_node(chevron)
        .text_size(14.0)
        .text_color(parse_text_color("muted-foreground"));
    icon = icon.ml("auto").flex_shrink(0.0);

    let mut row = item_row_chrome(inset)
        .bg([0.0, 0.0, 0.0, 0.0])
        .text_color(fg);
    for k in kids {
        row = row.child(k);
    }
    row = row.child(icon);
    let mut row: Element = row.into();
    if disabled {
        row = row.opacity(0.5);
    }
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        row = with_class_list(row, &c);
    }

    let mut hit: Element = interactive(row)
        .on_hover(move |h| {
            if disabled {
                return;
            }
            if !h {
                // Keep SubTrigger highlighted while its submenu is open.
                if !sub_open.peek() {
                    clear_ctx_active_if(active, ix);
                }
                return;
            }
            close_open_subs_except(&parent_id, Some(sub_id.as_str()));
            set_ctx_active(active, ix);
            if !sub_open.peek() {
                sub_open.set(true);
                crate::app::tree_hit::invalidate_hover_handlers();
                crate::app::runtime::request_redraw();
            }
        })
        .on_click(move || {
            if disabled {
                return;
            }
            close_open_subs_except(&parent_id_click, Some(sub_id_click.as_str()));
            if !sub_open_click.peek() {
                sub_open_click.set(true);
                crate::app::tree_hit::invalidate_hover_handlers();
                crate::app::runtime::request_redraw();
            }
            set_ctx_active(active, ix);
        })
        .cursor_kind(if disabled {
            CursorKind::NotAllowed
        } else {
            CursorKind::Default
        })
        .into();
    hit = hit.focus_id(trigger_id.as_str()).rounded("0.25rem");
    hit.set_attr(ATTR_SLOT, SLOT_SUB_TRIGGER);
    hit.set_attr("rui_ctx_id", &parent.id);
    hit.set_attr("rui_ctx_ix", ix.to_string());
    hit.set_attr("data-inset", if inset { "true" } else { "false" });
    if disabled {
        hit.set_attr("data-disabled", "true");
        hit.set_attr("rui_ctx_disabled", "1");
    }
    hit.accesskit_role("menuitem")
}

/// solid-ui / shadcn `ContextMenuSubContent`：相对 SubTrigger 向右弹出。
///
/// 独立 [`reactive_anchor`]（在父 Content 的 `isolate_reads` 中注册）。父菜单**不要**
/// 订阅 sub open，否则整树重建会打断悬停。
pub fn context_menu_sub_content(
    class: Option<String>,
    body: impl Fn() -> Element + 'static,
) -> Element {
    let parent = require_context_menu("ContextMenuSubContent");
    let sub = require_context_menu_sub("ContextMenuSubContent");
    let parent_open = parent.open;
    let sub_open = sub.open;
    let sid = sub.id.clone();
    let trigger_fid = sub.trigger_fid;
    let cursor = parent.cursor;
    let class = class.filter(|s| !s.trim().is_empty());
    let view_id = format!("{sid}__view");

    reactive_anchor(view_id.clone(), move || {
        let parent_is_open = parent_open.get();
        let is_open = sub_open.get();
        if !parent_is_open {
            if sub_open.peek() {
                sub_open.set(false);
            }
            // Clear sticky submenu highlight when the root menu closes.
            use_signal(format!("{sid}__ctx_active").as_str(), || ACTIVE_NONE)
                .set_silent(ACTIVE_NONE);
            return Element::new().w(0.0).h(0.0);
        }
        if !is_open {
            use_signal(format!("{sid}__ctx_active").as_str(), || ACTIVE_NONE)
                .set_silent(ACTIVE_NONE);
            let _ = portal_sync(&sid, sub_open, OverlayKind::Menu, OverlayMode::Stacked);
            return Element::new().w(0.0).h(0.0);
        }
        let Some(oz) = portal_sync(&sid, sub_open, OverlayKind::Menu, OverlayMode::Stacked) else {
            return Element::new().w(0.0).h(0.0);
        };

        let kids = with_child_owner(|| {
            let sub_active = use_signal(format!("{sid}__ctx_active").as_str(), || ACTIVE_NONE);
            provide_context(
                context_menu_context(),
                ContextMenuCtx {
                    open: parent_open,
                    id: sid.clone(),
                    trigger_fid,
                    cursor,
                    active: sub_active,
                    placement: PopupPlacement::RightStart,
                },
            );
            reset_activates(&sid);
            take_packed(body())
        });
        // Match root Content chrome: outer `p-1` + overflow clip, inner `w-full` column so
        // `menu_bleed` separators cancel the same padding on both sides as the main menu.
        let mut rows = vstack().w("100%").gap("0");
        for k in kids {
            rows = rows.child(k);
        }
        let mut chrome: Element = vstack()
            .w("auto")
            .h("auto")
            .min_w("12rem")
            .p("0.25rem")
            .gap("0")
            .overflow_x("hidden")
            .overflow_y("hidden")
            .child(rows)
            .into();
        chrome = chrome
            .bg(tokens::popover())
            .rounded("0.375rem")
            .border_style("solid")
            .border_width(1.0)
            .border_color(tokens::border())
            .shadow([0.0, 0.0, 0.0, 0.14], 12.0)
            .min_w("12rem")
            .w("auto")
            .h("auto")
            .flex_shrink(0.0);
        if let Some(ref c) = class {
            chrome = with_class_list(chrome, c);
        }
        chrome.set_attr(ATTR_SLOT, SLOT_SUB_CONTENT);
        chrome = chrome.accesskit_role("menu");

        // Transparent pads on both sides: keep the visual gap, but absorb pointer hits in the
        // seam. Dual pads cover both RightStart and flipped LeftStart without reading placement.
        let seam_pad = || {
            r#box()
                .w(SUBMENU_VISUAL_GAP_PX)
                .h("100%")
                .min_h(1.0)
                .flex_shrink(0.0)
                .flex_grow(0.0)
                .bg([0.0, 0.0, 0.0, 0.0])
        };
        let mut shell: Element = hstack()
            .w("auto")
            .h("auto")
            .items("stretch")
            .gap("0")
            .child(seam_pad())
            .child(chrome)
            .child(seam_pad())
            .into();
        shell = shell.absolute();
        let fid = trigger_fid.peek();
        // Gap 0: shell abuts the trigger; the near-side pad is the visible spacing.
        mark_floating_with_gap(
            &mut shell,
            &sid,
            fid.as_str(),
            PopupPlacement::RightStart,
            0.0,
        );
        shell.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
        // Shell includes seam pads; keep the chrome itself ≥ 12rem.
        shell.set_attr(
            ATTR_FLOATING_MIN_W,
            format!("{}px", 12.0 * 16.0 + 2.0 * SUBMENU_VISUAL_GAP_PX),
        );
        apply_stored_floating_place(&mut shell, &sid);
        mount_anchored_overlay(shell, oz.panel)
    });
    reactive_placeholder_sized(view_id, 0.0, 0.0).pointer_events_box_none()
}

/// 每帧按 `{id}__ctx_active` 写回高亮（`peek`，不订阅根 effect）。
pub(crate) fn sync_context_menu_item_paint_subtree(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
) {
    let slot = el.attrs.get(ATTR_SLOT).map(String::as_str);
    if matches!(slot, Some(SLOT_ITEM) | Some(SLOT_SUB_TRIGGER)) {
        let disabled = el.attrs.get("rui_ctx_disabled").map(String::as_str) == Some("1");
        let destructive = el.attrs.get("rui_ctx_destructive").map(String::as_str) == Some("1");
        let ix = el
            .attrs
            .get("rui_ctx_ix")
            .and_then(|s| s.parse::<usize>().ok());
        let id = el.attrs.get("rui_ctx_id").cloned();
        if let (Some(id), Some(ix)) = (id, ix) {
            let active = use_signal(format!("{id}__ctx_active").as_str(), || ACTIVE_NONE).peek();
            let on = !disabled && active == ix;
            apply_ctx_item_highlight(el, layout_tree, impact, on, destructive);
        }
    }
    for child in &mut el.children {
        sync_context_menu_item_paint_subtree(child, layout_tree, impact);
    }
}

fn apply_ctx_item_highlight(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
    on: bool,
    destructive: bool,
) {
    let want_bg = if on {
        parse_color("accent")
    } else {
        [0.0, 0.0, 0.0, 0.0]
    };
    let want_fg = if destructive {
        parse_text_color("destructive")
    } else if on {
        parse_text_color("accent-foreground")
    } else {
        parse_text_color("popover-foreground")
    };
    let bg_changed = el.style.bg_color != want_bg;
    if bg_changed {
        el.style.bg_color = want_bg;
        layout_tree.update_taffy_node_style(el);
        *impact = impact.merge(LayoutStatus::StyleChanged);
    }
    paint_ctx_item_fg(el, want_fg);
}

fn paint_ctx_item_fg(el: &mut Element, fg: [u8; 4]) {
    if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_SHORTCUT) {
        return;
    }
    if el.text.is_some() && el.text_style.font_family.as_deref() != Some("lucide") {
        el.text_style.color = fg;
        el.text_style.weight = 600;
        el.text_style.white_space = WhiteSpace::Nowrap;
        el.attrs
            .entry("font_weight".to_string())
            .or_insert_with(|| "1".to_string());
    }
    for child in &mut el.children {
        paint_ctx_item_fg(child, fg);
    }
}

/// solid-ui / shadcn `ContextMenuLabel`：`px-2 py-1.5 text-sm font-semibold`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ContextMenuLabel(inset: bool, children: Option<Element>) -> Element {
    let _ = require_context_menu("ContextMenuLabel");
    let t = match &children {
        Some(c) => c
            .text
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| extract_text_leaf(c)),
        None => String::new(),
    };
    let mut el: Element = label(format!("ctx-label:{t}"))
        .text(t)
        .size(14.0)
        .line_height(20.0)
        .font_weight(600.0)
        .color(parse_text_color("muted-foreground"))
        .into();
    el = el
        .px(if inset { "2rem" } else { "0.5rem" })
        .py("0.25rem")
        .w("100%")
        .pointer_events_none();
    el.set_attr(ATTR_SLOT, SLOT_LABEL);
    el.set_attr("data-inset", if inset { "true" } else { "false" });
    el
}

/// solid-ui / shadcn `ContextMenuSeparator`：`-mx-1 my-1 h-px bg-border`。
///
/// 经 [`Element::menu_bleed`] 抵消 Content `p-1`；勿手写 `w-full` + 负 `mx`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ContextMenuSeparator(children: Option<Element>) -> Element {
    let _ = children;
    let _ = require_context_menu("ContextMenuSeparator");
    let mut el = separator().menu_bleed().pointer_events_none();
    el.set_attr(ATTR_SLOT, SLOT_SEPARATOR);
    el
}

/// solid-ui / shadcn `ContextMenuShortcut`：`ml-auto text-xs tracking-widest opacity-60`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ContextMenuShortcut(children: Option<Element>) -> Element {
    let _ = require_context_menu("ContextMenuShortcut");
    let t = match &children {
        Some(c) => c
            .text
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| extract_text_leaf(c)),
        None => String::new(),
    };
    let mut kbd: Element = label(format!("ctx-sc:{t}"))
        .text(t)
        .size(12.0)
        .line_height(16.0)
        .color(parse_text_color("muted-foreground"))
        .into();
    kbd = kbd.whitespace_nowrap().letter_spacing(0.8).opacity(0.6);
    let mut el: Element = hstack()
        .flex_shrink(0.0)
        .ml("auto")
        .items_center()
        .child(kbd)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_SHORTCUT);
    el
}

/// solid-ui / shadcn `ContextMenuGroup`：分组容器。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ContextMenuGroup(children: Option<Element>) -> Element {
    let _ = require_context_menu("ContextMenuGroup");
    let kids = children.map(take_packed).unwrap_or_default();
    let mut col = vstack().w("100%").gap("0");
    for k in kids {
        col = col.child(k);
    }
    let mut el: Element = col.into();
    el.set_attr(ATTR_SLOT, SLOT_GROUP);
    el
}

// ---------------------------------------------------------------------------
// Builder（兼容旧 API）
// ---------------------------------------------------------------------------

/// Builder / 便捷 API 用的数据项（RSX 请用组件 [`ContextMenuItem`]）。
#[derive(Clone)]
pub struct ContextMenuAction {
    pub label: String,
    pub on_click: Rc<dyn Fn()>,
    pub disabled: bool,
    pub destructive: bool,
    pub shortcut: Option<String>,
}

impl ContextMenuAction {
    pub fn new(label: impl Into<String>, on_click: impl Fn() + 'static) -> Self {
        Self {
            label: label.into(),
            on_click: Rc::new(on_click),
            disabled: false,
            destructive: false,
            shortcut: None,
        }
    }

    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }

    pub fn destructive(mut self, d: bool) -> Self {
        self.destructive = d;
        self
    }

    pub fn shortcut(mut self, s: impl Into<String>) -> Self {
        self.shortcut = Some(s.into());
        self
    }
}

/// 旧名别名（[`ContextMenuAction`]）。
#[deprecated(note = "use ContextMenuAction; ContextMenuItem is now the RSX component")]
pub type ContextMenuItemData = ContextMenuAction;

pub struct ContextMenuBuilder {
    id: String,
    items: Vec<ContextMenuAction>,
    open: Option<Signal<bool>>,
    placement: PopupPlacement,
}

/// Builder 入口：便捷 Trigger + Content + Items（内部接到复合实现）。
pub fn context_menu(id: impl Into<String>) -> ContextMenuBuilder {
    ContextMenuBuilder {
        id: id.into(),
        items: Vec::new(),
        open: None,
        placement: PopupPlacement::BottomStart,
    }
}

impl ContextMenuBuilder {
    pub fn open_signal(mut self, s: Signal<bool>) -> Self {
        self.open = Some(s);
        self
    }

    pub fn placement(mut self, p: PopupPlacement) -> Self {
        self.placement = p;
        self
    }

    pub fn item(mut self, label: impl Into<String>, on_click: impl Fn() + 'static) -> Self {
        self.items
            .push(ContextMenuAction::new(label, on_click));
        self
    }

    pub fn action(mut self, action: ContextMenuAction) -> Self {
        self.items.push(action);
        self
    }

    /// 旧 API：`trigger(|| …)`；内部接到 [`context_menu_scope`] + Content。
    pub fn trigger(self, build: impl FnOnce() -> Element) -> Element {
        let id = self.id.clone();
        let open = self.open;
        let placement = self.placement;
        let items = self.items;
        let side = placement.as_attr().to_string();
        context_menu_scope(id, open, || {
            let trigger = ContextMenuTrigger(ContextMenuTriggerProps {
                id: String::new(),
                children: Some(build()),
                base: None,
            });
            let tip = context_menu_content(
                ContextMenuContentOpts {
                    placement: side,
                    class: None,
                },
                move || {
                    let mut col = vstack().w("100%").gap("0");
                    for it in &items {
                        let cb = it.on_click.clone();
                        let mut kids = vec![Element::new().text(it.label.clone())];
                        if let Some(ref sc) = it.shortcut {
                            kids.push(ContextMenuShortcut(ContextMenuShortcutProps {
                                children: Some(Element::new().text(sc.clone())),
                                base: None,
                            }));
                        }
                        let mut frag = Element::new();
                        frag.set_attr("rui_fragment", "1");
                        frag = frag.children(kids);
                        let row = ContextMenuItem(ContextMenuItemProps {
                            disabled: Some(it.disabled),
                            inset: Some(false),
                            variant: if it.destructive {
                                Some("destructive".into())
                            } else {
                                None
                            },
                            class: None,
                            on_click: Some(Handler::new(move |_| cb())),
                            children: Some(frag),
                            base: None,
                        });
                        col = col.child(row);
                    }
                    col.into()
                },
            );
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            frag.children(vec![trigger, tip])
        })
    }
}

impl From<ContextMenuBuilder> for Element {
    fn from(builder: ContextMenuBuilder) -> Self {
        builder.trigger(|| {
            let mut el: Element = r#box().px("0.75rem").py("0.5rem").into();
            el = el.child(text("Right-click me"));
            el
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn context_menu_records_items() {
        let fired = Rc::new(Cell::new(false));
        let f = fired.clone();
        let _el: Element = context_menu("cm1")
            .item("Copy", move || f.set(true))
            .into();
        assert!(!fired.get());
    }

    #[test]
    fn compound_scope_exposes_template() {
        let el = context_menu_scope("cm-scope", None, || {
            let trigger = ContextMenuTrigger(ContextMenuTriggerProps {
                id: String::new(),
                children: Some(Element::new().text("area")),
                base: None,
            });
            let content = context_menu_content(ContextMenuContentOpts::default(), || {
                ContextMenuItem(ContextMenuItemProps {
                    disabled: Some(false),
                    inset: Some(false),
                    variant: None,
                    class: None,
                    on_click: None,
                    children: Some(Element::new().text("One")),
                    base: None,
                })
            });
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            frag.children(vec![trigger, content])
        });
        assert_eq!(
            el.template_id.as_deref(),
            Some("cm-scope__context_menu"),
            "scope must expose a stable template id"
        );
    }

    #[test]
    fn compound_sub_provides_context_before_children() {
        let _el = context_menu_scope("cm-sub", None, || {
            context_menu_content(ContextMenuContentOpts::default(), || {
                context_menu_sub(None, || {
                    let st = ContextMenuSubTrigger(ContextMenuSubTriggerProps {
                        disabled: Some(false),
                        inset: Some(false),
                        class: None,
                        children: Some(Element::new().text("More")),
                        base: None,
                    });
                    let sc = context_menu_sub_content(None, || {
                        ContextMenuItem(ContextMenuItemProps {
                            disabled: Some(false),
                            inset: Some(false),
                            variant: None,
                            class: None,
                            on_click: None,
                            children: Some(Element::new().text("Nested")),
                            base: None,
                        })
                    });
                    let mut frag = Element::new();
                    frag.set_attr("rui_fragment", "1");
                    frag.children(vec![st, sc])
                })
            })
        });
    }
}
