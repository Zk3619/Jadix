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
//!     ├── ContextMenuCheckboxItem
//!     ├── ContextMenuRadioGroup / ContextMenuRadioItem
//!     ├── ContextMenuShortcut // ml-auto text-xs tracking-widest
//!     ├── ContextMenuSeparator
//!     ├── ContextMenuGroup
//!     └── ContextMenuSub
//!         ├── ContextMenuSubTrigger  // chevron-right；悬停打开
//! └── ContextMenuSubContent // Nested menu on the right
//! ```
//!
//! `<ContextMenu>` / `<ContextMenuContent>` / `<ContextMenuRadioGroup>` in `ui!` are expanded by macros. Builder
//! [`context_menu`] / [`ContextMenuBuilder::trigger`] 仍可用（内部接到同一套实现）。
//!
//! ## Usage (dual style)
//! ```ignore
//! // A. Composite RSX (recommended; macro context_menu_scope)
//! ui! {
//!     <ContextMenu id="ctx">
//!         <ContextMenuTrigger>
//!             <Box p="0.75rem" bg="muted" class="rounded-md">
//!                 <Label>"右键打开"</Label>
//!             </Box>
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
//! // B. Builder (convenient entrance, built-in Trigger + Content + Items)
//! context_menu("ctx")
//!     .item("Copy", || toast_push("Copy", 1500))
//!     .item("Paste", || toast_push("Paste", 1500))
//!     .trigger(|| { /* 触发区域 Element */ })
//! ```
//!
//! ## Notes
//! - 右键打开时以**鼠标位置**为原点向右下展开；空间不足时先换向，再视口内平移，最后夹高度并内部滚动。
//! - When open, ↑↓ / Home / End use roving; Enter/Space activate the highlighted item; Esc closes via the overlay stack.
//! - 未传 `open` 时用内部信号 `{id}__ctx_open`；多实例请保证 `id` 唯一。

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::OnceLock;

use jadix_macros::component;
use winit::keyboard::{Key, NamedKey};

use crate::app::portal_overlay::{OverlayKind, OverlayMode};
use crate::components::base::SlotMap;
use crate::components::internal::menu::checkable::{menu_checkbox_control, menu_radio_control};
use crate::components::internal::menu::{item_chrome, subtree};
use crate::components::label::label;
use crate::components::scroll_area::scroll_area;
use crate::components::separator::separator;
use crate::core::reactivity::{
    create_context, isolate_reads, provide_context, untrack, use_context, use_signal,
    with_child_owner, Context, Signal,
};
use crate::core::tree::{CursorKind, Element, Handler, LayoutStatus};
use crate::impl_simple_slot_builder;
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::primitives::text::text;
use crate::style::parse_text_color;
use crate::tokens;
use crate::utils::activate_registry;
use crate::utils::anchored_overlay::{
    anchored_overlay_content, provide_anchored_overlay_context, AnchoredOverlayContentOpts,
    FloatingMotionKind, FloatingStrategy, Placement,
};
use crate::utils::lucide_glyph;
use crate::utils::popup::{PopupPlacement, FLOATING_TRIGGER_GAP};
use crate::utils::resolve_instance_id;
use crate::utils::roving::{roving_edge, roving_move};
use crate::utils::take_packed;
use crate::utils::text_node::build_icon_text_node;
use crate::utils::toggle_state::write_bool_toggle;

const ATTR_SLOT: &str = "data-slot";
const SLOT_TRIGGER: &str = "context-menu-trigger";
const SLOT_CONTENT: &str = "context-menu-content";
const SLOT_ITEM: &str = "context-menu-item";
const SLOT_CHECKBOX_ITEM: &str = "context-menu-checkbox-item";
const SLOT_RADIO_ITEM: &str = "context-menu-radio-item";
const SLOT_LABEL: &str = "context-menu-label";
const SLOT_SEPARATOR: &str = "context-menu-separator";
const SLOT_SHORTCUT: &str = "context-menu-shortcut";
const SLOT_GROUP: &str = "context-menu-group";
const SLOT_RADIO_GROUP: &str = "context-menu-radio-group";
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

/// Root context: `open` + stable instance `id` + trigger anchor + roving highlight.
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
    static CTX_SUB_AT_IX: RefCell<HashMap<String, HashMap<usize, String>>> =
        RefCell::new(HashMap::new());
    static RADIO_SEQ: RefCell<HashMap<String, usize>> = RefCell::new(HashMap::new());
}

fn reset_activates(id: &str) {
    activate_registry::reset(id);
    CTX_SUB_AT_IX.with(|m| {
        m.borrow_mut().remove(id);
    });
}

fn push_activate(id: &str, f: Rc<dyn Fn()>) {
    activate_registry::push(id, f);
}

fn activate_count(id: &str) -> usize {
    activate_registry::count(id)
}

fn take_activate(id: &str, ix: usize) -> Option<Rc<dyn Fn()>> {
    activate_registry::take(id, ix)
}

fn register_sub_at_ix(parent_id: &str, ix: usize, sid: &str) {
    CTX_SUB_AT_IX.with(|m| {
        m.borrow_mut()
            .entry(parent_id.to_string())
            .or_default()
            .insert(ix, sid.to_string());
    });
}

fn sub_id_at_ix(parent_id: &str, ix: usize) -> Option<String> {
    CTX_SUB_AT_IX.with(|m| {
        m.borrow()
            .get(parent_id)
            .and_then(|map| map.get(&ix).cloned())
    })
}

fn context_menu_open_sub_id(parent_id: &str) -> Option<String> {
    subtree::open_sub_id(parent_id)
}

fn focus_submenu_first_item(sid: &str) {
    subtree::focus_submenu_first_item(sid, "ctx_active", activate_count(sid));
}

fn close_open_submenu_keep_parent_highlight(sid: &str) {
    subtree::close_open_submenu_keep_parent_highlight(sid, "ctx_active");
}

fn reset_sub_seq(parent_id: &str) {
    subtree::reset_sub_seq(parent_id);
}

fn reset_radio_seq(parent_id: &str) {
    RADIO_SEQ.with(|m| {
        m.borrow_mut().insert(parent_id.to_string(), 0);
    });
}

fn next_sub_seq(parent_id: &str) -> usize {
    subtree::next_sub_seq(parent_id)
}

fn next_radio_seq(parent_id: &str) -> usize {
    RADIO_SEQ.with(|m| {
        let mut map = m.borrow_mut();
        let e = map.entry(parent_id.to_string()).or_insert(0);
        let i = *e;
        *e += 1;
        i
    })
}

fn set_ctx_active(active: Signal<usize>, next: usize) {
    subtree::set_menu_active(active, next);
}

fn clear_ctx_active_if(active: Signal<usize>, ix: usize) {
    if active.peek() == ix {
        set_ctx_active(active, ACTIVE_NONE);
    }
}

fn close_open_subs(parent_id: &str) {
    subtree::close_open_subs(parent_id);
}

fn close_open_subs_except(parent_id: &str, keep_sub_id: Option<&str>) {
    subtree::close_open_subs_except(parent_id, keep_sub_id);
}

fn open_or_remount_sub(sub_id: &str, sub_open: Signal<bool>) {
    subtree::open_or_remount_sub(sub_id, sub_open);
}

fn open_or_remount_sub_ex(sub_id: &str, sub_open: Signal<bool>, refresh_hover: bool) {
    subtree::open_or_remount_sub_ex(sub_id, sub_open, refresh_hover);
}

/// Read the most recent [`ContextMenu`] context.
pub fn use_context_menu() -> Option<ContextMenuCtx> {
    use_context(context_menu_context())
}

fn require_context_menu(part: &str) -> ContextMenuCtx {
    use_context_menu().unwrap_or_else(|| {
        panic!("{part} must be used within <ContextMenu> / context_menu_scope");
    })
}

/// 提供 ContextMenu 上下文并构建子树（`ui!` `<ContextMenu>` / Builder 共用）。
///
/// Context under a child owner — same isolation as [`dropdown_menu_scope`] for sibling
/// compound roots.
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
    let mut root = with_child_owner(|| {
        provide_anchored_overlay_context(open, id.clone(), trigger_fid);
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
        body()
    });
    if root.template_id.is_none() {
        root.template_id = Some(format!("{id}__context_menu"));
    }
    root
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
    item_chrome::has_shortcut_slot(el, ATTR_SLOT, SLOT_SHORTCUT)
}

fn style_item_text(el: &mut Element, color: [u8; 4]) {
    item_chrome::style_item_text(el, color, ATTR_SLOT, SLOT_SHORTCUT);
}

/// Shared item chrome. Do **not** use `min_w(0)` — that collapses wrapping labels to 1ch.
fn item_row_chrome(inset: bool) -> crate::primitives::r#box::BoxBuilder {
    item_chrome::item_row_chrome(inset)
}

fn attach_trigger_keys(mut trigger: Element, ctx: &ContextMenuCtx) -> Element {
    let open_keys = ctx.open;
    let active_keys = ctx.active;
    let menu_id = ctx.id.clone();
    trigger = trigger.on_key_down(move |ev| {
        if !open_keys.peek() {
            return false;
        }

        if let Some(sid) = context_menu_open_sub_id(&menu_id) {
            let n = activate_count(&sid);
            let sub_active = use_signal(format!("{sid}__ctx_active").as_str(), || ACTIVE_NONE);
            return match &ev.logical_key {
                Key::Named(NamedKey::ArrowDown) => {
                    if n == 0 {
                        return true;
                    }
                    let cur = sub_active.peek();
                    let next = if cur == ACTIVE_NONE || cur >= n {
                        0
                    } else if let Some(i) = roving_move(n, cur, 1) {
                        i
                    } else {
                        return true;
                    };
                    set_ctx_active(sub_active, next);
                    true
                }
                Key::Named(NamedKey::ArrowUp) => {
                    if n == 0 {
                        return true;
                    }
                    let cur = sub_active.peek();
                    let next = if cur == ACTIVE_NONE || cur >= n {
                        n - 1
                    } else if let Some(i) = roving_move(n, cur, -1) {
                        i
                    } else {
                        return true;
                    };
                    set_ctx_active(sub_active, next);
                    true
                }
                Key::Named(NamedKey::Home) => {
                    if let Some(i) = roving_edge(n, false) {
                        set_ctx_active(sub_active, i);
                    }
                    true
                }
                Key::Named(NamedKey::End) => {
                    if let Some(i) = roving_edge(n, true) {
                        set_ctx_active(sub_active, i);
                    }
                    true
                }
                k if crate::utils::keyboard::is_enter_or_space(&k) => {
                    let ix = sub_active.peek();
                    if ix == ACTIVE_NONE {
                        return true;
                    }
                    if let Some(cb) = take_activate(&sid, ix) {
                        cb();
                    }
                    true
                }
                Key::Named(NamedKey::ArrowLeft) | Key::Named(NamedKey::Escape) => {
                    close_open_submenu_keep_parent_highlight(&sid);
                    true
                }
                Key::Named(NamedKey::ArrowRight) => true,
                _ => false,
            };
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
                close_open_subs(&menu_id);
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
                close_open_subs(&menu_id);
                set_ctx_active(active_keys, next);
                true
            }
            Key::Named(NamedKey::Home) => {
                if let Some(i) = roving_edge(n, false) {
                    close_open_subs(&menu_id);
                    set_ctx_active(active_keys, i);
                }
                true
            }
            Key::Named(NamedKey::End) => {
                if let Some(i) = roving_edge(n, true) {
                    close_open_subs(&menu_id);
                    set_ctx_active(active_keys, i);
                }
                true
            }
            Key::Named(NamedKey::ArrowRight) => {
                let ix = active_keys.peek();
                if ix == ACTIVE_NONE {
                    return false;
                }
                let Some(sid) = sub_id_at_ix(&menu_id, ix) else {
                    return false;
                };
                let sub_open = use_signal(format!("{sid}__open").as_str(), || false);
                open_or_remount_sub_ex(&sid, sub_open, false);
                if sub_open.peek() {
                    focus_submenu_first_item(&sid);
                }
                true
            }
            k if crate::utils::keyboard::is_enter_or_space(&k) => {
                let ix = active_keys.peek();
                if ix == ACTIVE_NONE {
                    return false;
                }
                if let Some(cb) = take_activate(&menu_id, ix) {
                    cb();
                    if let Some(sid) = context_menu_open_sub_id(&menu_id) {
                        focus_submenu_first_item(&sid);
                    }
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
        trigger.set_focusable(true);
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
        use_signal("__jadix_button_focus_id", String::new).set(fid_focus.clone());
        open_ctx.set(true);
    });
    trigger = attach_trigger_keys(trigger, &ctx);
    trigger.set_attr(ATTR_SLOT, SLOT_TRIGGER);
    trigger
}

/// `ContextMenuContent` 选项（`ui!` / [`context_menu_content`] 共用）。
#[derive(Clone)]
pub struct ContextMenuContentOpts {
    /// `bottom-start` / `right` / … (default `bottom-start`).
    pub placement: String,
    /// Panel chrome (RSX `class` / Builder `.class()` / `.style()`).
    pub panel: crate::components::base::SlotChrome,
}

impl Default for ContextMenuContentOpts {
    fn default() -> Self {
        Self {
            placement: "bottom-start".into(),
            panel: Default::default(),
        }
    }
}

impl ContextMenuContentOpts {
    crate::impl_content_opts_chrome!(panel);
}

/// solid-ui / shadcn `ContextMenuContent`：Portal + floating 面板。
/// `ui!` lowers to this function via the macro (subtree rebuilt lazily when open).
pub fn context_menu_content(
    opts: ContextMenuContentOpts,
    body: impl Fn() -> Element + 'static,
) -> Element {
    let ctx = require_context_menu("ContextMenuContent");
    let open = ctx.open;
    let menu_id = ctx.id.clone();
    let cursor = ctx.cursor;
    let _ = ctx.active;
    let chrome = opts.panel;
    let ao_opts = AnchoredOverlayContentOpts {
        placement: Some(Placement::Point {
            pos: cursor,
            gap: 2.0,
        }),
        kind: OverlayKind::Menu,
        lock_page_scroll: Some(true),
        motion: FloatingMotionKind::Fade,
        mode: OverlayMode::Stacked,
        ..Default::default()
    };
    anchored_overlay_content(ao_opts, move || {
        if !open.peek() {
            close_open_subs(&menu_id);
        }

        reset_activates(&menu_id);
        reset_sub_seq(&menu_id);
        reset_radio_seq(&menu_id);
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
            .popup_shadow()
            .absolute();
        panel = chrome.apply(panel);
        panel.set_attr(ATTR_SLOT, SLOT_CONTENT);
        panel.accesskit_role("menu")
    })
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
        let open = open;
        Rc::new(move || {
            if !disabled {
                on_click.borrow_mut().call(());
                open.set(false);
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
    hit.set_attr("jadix_ctx_id", &ctx.id);
    hit.set_attr("jadix_ctx_ix", ix.to_string());
    hit.set_attr("data-inset", if inset { "true" } else { "false" });
    if destructive {
        hit.set_attr("data-variant", "destructive");
        hit.set_attr("jadix_ctx_destructive", "1");
    }
    if disabled {
        hit.set_attr("data-disabled", "true");
        hit.set_attr("jadix_ctx_disabled", "1");
    }
    hit.accesskit_role("menuitem")
}

/// solid-ui / shadcn `ContextMenuCheckboxItem`：左侧对齐 Checkbox 控制面；点击切换，**不**关闭菜单。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ContextMenuCheckboxItem(
    checked: Option<Signal<bool>>,
    default_checked: bool,
    disabled: bool,
    inset: bool,
    #[prop(default = "Handler::default()")] mut on_click: Handler,
    children: Option<Element>,
) -> Element {
    let ctx = require_context_menu("ContextMenuCheckboxItem");
    let active = ctx.active;
    let menu_id = ctx.id.clone();
    let ix = activate_count(&ctx.id);
    // Global key: survives across Content open/close. Do not use use_scoped_signal — after menu dispose, paint sync may still peek.
    let check = checked.unwrap_or_else(|| {
        use_signal(
            format!("{}__cbox{}__menu_cbox", ctx.id, ix).as_str(),
            || default_checked,
        )
    });

    let on_click = Rc::new(RefCell::new(on_click));
    let activate = {
        let on_click = on_click.clone();
        let check = check;
        Rc::new(move || {
            if disabled {
                return;
            }
            if check.is_alive() {
                write_bool_toggle(check, false);
            }
            on_click.borrow_mut().call(());
        }) as Rc<dyn Fn()>
    };
    let activate_click = activate.clone();
    push_activate(&ctx.id, activate);

    let mut kids = children
        .map(take_packed)
        .unwrap_or_else(|| vec![Element::new().text("Item")]);
    let fg = parse_text_color("popover-foreground");
    for k in &mut kids {
        if !has_shortcut_slot(k) {
            style_item_text(k, fg);
        }
    }

    let control = menu_checkbox_control(check);
    let mut row = item_row_chrome(inset)
        .bg([0.0, 0.0, 0.0, 0.0])
        .text_color(fg)
        .child(control);
    for k in kids {
        row = row.child(k);
    }
    let mut row: Element = row.into();
    if disabled {
        row = row.opacity(0.5);
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
        })
        .cursor_kind(if disabled {
            CursorKind::NotAllowed
        } else {
            CursorKind::Default
        })
        .into();
    hit = hit.rounded("0.25rem");
    hit.set_attr(ATTR_SLOT, SLOT_CHECKBOX_ITEM);
    hit.set_attr("jadix_ctx_id", &ctx.id);
    hit.set_attr("jadix_ctx_ix", ix.to_string());
    hit.set_attr("data-inset", if inset { "true" } else { "false" });
    if disabled {
        hit.set_attr("data-disabled", "true");
        hit.set_attr("jadix_ctx_disabled", "1");
    }
    hit.accesskit_role("menuitemcheckbox")
}

// ---------------------------------------------------------------------------
// RadioGroup / RadioItem
// ---------------------------------------------------------------------------

/// Nested radio-group context (under a root [`ContextMenuCtx`]).
#[derive(Clone)]
pub struct ContextMenuRadioGroupCtx {
    pub value: Signal<String>,
    pub id: String,
}

fn context_menu_radio_group_context() -> Context<ContextMenuRadioGroupCtx> {
    static CTX: OnceLock<Context<ContextMenuRadioGroupCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<ContextMenuRadioGroupCtx>)
}

fn use_context_menu_radio_group() -> Option<ContextMenuRadioGroupCtx> {
    use_context(context_menu_radio_group_context())
}

fn require_context_menu_radio_group(part: &str) -> ContextMenuRadioGroupCtx {
    use_context_menu_radio_group().unwrap_or_else(|| {
        panic!("{part} must be used within <ContextMenuRadioGroup>");
    })
}

/// solid-ui/shadcn `ContextMenuRadioGroup`: Wraps a group of [`ContextMenuRadioItem`].
/// `ui!` lowers to this function via the macro (subtree evaluated **after** `provide_context`).
pub fn context_menu_radio_group(
    id: Option<String>,
    value: Option<Signal<String>>,
    default_value: Option<String>,
    body: impl FnOnce() -> Element,
) -> Element {
    let parent = require_context_menu("ContextMenuRadioGroup");
    let seq = next_radio_seq(&parent.id);
    let rid = match id {
        Some(s) if !s.trim().is_empty() => s,
        _ => format!("{}__radio{seq}", parent.id),
    };
    let default_value = default_value.unwrap_or_default();
    let value = value.unwrap_or_else(|| {
        use_signal(format!("{rid}__value").as_str(), move || {
            default_value.clone()
        })
    });
    let kids = with_child_owner(|| {
        provide_context(
            context_menu_radio_group_context(),
            ContextMenuRadioGroupCtx {
                value,
                id: rid.clone(),
            },
        );
        take_packed(body())
    });
    let mut col = vstack().w("100%").gap("0");
    for k in kids {
        col = col.child(k);
    }
    let mut el: Element = col.into();
    el.set_attr(ATTR_SLOT, SLOT_RADIO_GROUP);
    el.set_attr("data-slot", SLOT_RADIO_GROUP);
    el
}

/// solid-ui / shadcn `ContextMenuRadioItem`：左侧圆点指示；选中后关闭菜单。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ContextMenuRadioItem(
    value: String,
    disabled: bool,
    inset: bool,
    #[prop(default = "Handler::default()")] mut on_click: Handler,
    children: Option<Element>,
) -> Element {
    let ctx = require_context_menu("ContextMenuRadioItem");
    let group = require_context_menu_radio_group("ContextMenuRadioItem");
    let open = ctx.open;
    let active = ctx.active;
    let menu_id = ctx.id.clone();
    let item_value = value.clone();
    let group_value = group.value;

    let on_click = Rc::new(RefCell::new(on_click));
    let activate = {
        let on_click = on_click.clone();
        let item_value = item_value.clone();
        let group_value = group_value;
        let open = open;
        Rc::new(move || {
            if disabled {
                return;
            }
            // Set value before close: global radio signal survives across Content; try_set guards extreme dispose races.
            let _ = group_value.try_set(item_value.clone());
            open.set(false);
            on_click.borrow_mut().call(());
        }) as Rc<dyn Fn()>
    };
    let activate_click = activate.clone();
    let ix = activate_count(&ctx.id);
    push_activate(&ctx.id, activate);

    let mut kids = children
        .map(take_packed)
        .unwrap_or_else(|| vec![Element::new().text("Item")]);
    let fg = parse_text_color("popover-foreground");
    for k in &mut kids {
        if !has_shortcut_slot(k) {
            style_item_text(k, fg);
        }
    }

    let control = menu_radio_control(group_value, &item_value);
    let mut row = item_row_chrome(inset)
        .bg([0.0, 0.0, 0.0, 0.0])
        .text_color(fg)
        .child(control);
    for k in kids {
        row = row.child(k);
    }
    let mut row: Element = row.into();
    if disabled {
        row = row.opacity(0.5);
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
        })
        .cursor_kind(if disabled {
            CursorKind::NotAllowed
        } else {
            CursorKind::Default
        })
        .into();
    hit = hit.rounded("0.25rem");
    hit.set_attr(ATTR_SLOT, SLOT_RADIO_ITEM);
    hit.set_attr("jadix_ctx_id", &ctx.id);
    hit.set_attr("jadix_ctx_ix", ix.to_string());
    hit.set_attr("data-inset", if inset { "true" } else { "false" });
    hit.set_attr("data-value", &value);
    if disabled {
        hit.set_attr("data-disabled", "true");
        hit.set_attr("jadix_ctx_disabled", "1");
    }
    hit.accesskit_role("menuitemradio")
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

/// solid-ui/shadcn `ContextMenuSub`: wraps SubTrigger + SubContent.
/// `ui!` lowers to this function via the macro (subtree evaluated **after** `provide_context`).
pub fn context_menu_sub(id: Option<String>, body: impl FnOnce() -> Element) -> Element {
    let parent = require_context_menu("ContextMenuSub");
    let seq = next_sub_seq(&parent.id);
    let sid = match id {
        Some(s) if !s.trim().is_empty() => s,
        _ => format!("{}__sub{seq}", parent.id),
    };
    let open = use_signal(format!("{sid}__open").as_str(), || false);
    let trigger_fid = use_signal(format!("{sid}__tfid").as_str(), || {
        format!("{sid}__trigger")
    });
    let kids = with_child_owner(|| {
        provide_anchored_overlay_context(open, sid.clone(), trigger_fid);
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
    frag.set_attr("jadix_fragment", "1");
    frag.set_attr("data-slot", "context-menu-sub");
    frag.children(kids)
}

/// solid-ui / shadcn `ContextMenuSubTrigger`：右侧 chevron；悬停打开子菜单。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ContextMenuSubTrigger(disabled: bool, inset: bool, children: Option<Element>) -> Element {
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
        let sub_id = sub_id.clone();
        let sub_open = sub_open;
        Rc::new(move || {
            if !disabled {
                open_or_remount_sub_ex(&sub_id, sub_open, false);
                if sub_open.peek() {
                    focus_submenu_first_item(&sub_id);
                }
            }
        }) as Rc<dyn Fn()>
    };
    let ix = activate_count(&parent.id);
    push_activate(&parent.id, activate);
    register_sub_at_ix(&parent.id, ix, &sub_id);

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
            open_or_remount_sub(&sub_id, sub_open);
        })
        .on_click(move || {
            if disabled {
                return;
            }
            close_open_subs_except(&parent_id_click, Some(sub_id_click.as_str()));
            open_or_remount_sub(&sub_id_click, sub_open_click);
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
    hit.set_attr("jadix_ctx_id", &parent.id);
    hit.set_attr("jadix_ctx_ix", ix.to_string());
    hit.set_attr("data-inset", if inset { "true" } else { "false" });
    if disabled {
        hit.set_attr("data-disabled", "true");
        hit.set_attr("jadix_ctx_disabled", "1");
    }
    hit.accesskit_role("menuitem")
}

/// solid-ui / shadcn `ContextMenuSubContent`：相对 SubTrigger 向右弹出。
///
/// Independent [`reactive_anchor`] (registered in the parent Content's `isolate_reads`). Parent Menu **Don't**
/// 订阅 sub open，否则整树重建会打断悬停。
pub fn context_menu_sub_content(
    chrome: crate::components::base::SlotChrome,
    body: impl Fn() -> Element + 'static,
) -> Element {
    let parent = require_context_menu("ContextMenuSubContent");
    let sub = require_context_menu_sub("ContextMenuSubContent");
    let parent_open = parent.open;
    let sub_open = sub.open;
    let sid = sub.id.clone();
    let trigger_fid = sub.trigger_fid;
    let cursor = parent.cursor;
    let ao_opts = AnchoredOverlayContentOpts {
        placement: Some(Placement::trigger(
            String::new(),
            PopupPlacement::RightStart,
            SUBMENU_VISUAL_GAP_PX,
            FloatingStrategy::Auto,
        )),
        kind: OverlayKind::Menu,
        lock_page_scroll: Some(true),
        motion: FloatingMotionKind::Fade,
        mode: OverlayMode::Stacked,
        ..Default::default()
    };
    anchored_overlay_content(ao_opts, move || {
        let parent_is_open = parent_open.get();
        // Remount bump from SubTrigger (same-value open would not re-run).
        let _ = use_signal(format!("{sid}__remount").as_str(), || 0u32).get();
        if !parent_is_open {
            if sub_open.peek() {
                sub_open.set(false);
            }
            // Clear sticky submenu highlight when the root menu closes.
            use_signal(format!("{sid}__ctx_active").as_str(), || ACTIVE_NONE)
                .set_silent(ACTIVE_NONE);
            return Element::new().w(0.0).h(0.0);
        }

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
        // Silhouette = floating root (same as root Content): bg/border/rounded/shadow live
        // together. Own overflow does not clip own box-shadow. Visual gap is placement gap
        // (not a padded shell), so the shadow outline matches the menu chrome.
        let mut rows = vstack().w("100%").gap("0");
        for k in kids {
            rows = rows.child(k);
        }
        let mut panel: Element = vstack()
            .w("auto")
            .h("auto")
            .min_w("12rem")
            .p("0.25rem")
            .gap("0")
            .overflow_x("hidden")
            .overflow_y("hidden")
            .child(rows)
            .into();
        panel = panel
            .bg(tokens::popover())
            .rounded("0.375rem")
            .border_style("solid")
            .border_width(1.0)
            .border_color(tokens::border())
            .popup_shadow()
            .absolute();
        panel = chrome.apply(panel);
        panel.set_attr(ATTR_SLOT, SLOT_SUB_CONTENT);
        panel.accesskit_role("menu")
    })
}

/// 每帧按 `{id}__ctx_active` 写回高亮（`peek`，不订阅根 effect）。
pub(crate) fn sync_context_menu_item_paint_subtree(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
) {
    const CTX_SLOTS: &[&str] = &[
        SLOT_ITEM,
        SLOT_SUB_TRIGGER,
        SLOT_CHECKBOX_ITEM,
        SLOT_RADIO_ITEM,
    ];
    let cfg = item_chrome::ItemPaintConfig {
        attr_slot: ATTR_SLOT,
        shortcut_slot: SLOT_SHORTCUT,
        interactive_slots: CTX_SLOTS,
        attr_disabled: "jadix_ctx_disabled",
        attr_destructive: "jadix_ctx_destructive",
        attr_ix: "jadix_ctx_ix",
        attr_menu_id: "jadix_ctx_id",
        active_key_suffix: "__ctx_active",
        active_none: ACTIVE_NONE,
    };
    item_chrome::sync_item_paint_subtree(el, layout_tree, impact, &cfg);
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
/// Cancel Content `p-1` via [`Element::menu_bleed`]; do not hand-write `w-full` + negative `mx`.
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

/// solid-ui/shadcn `ContextMenuGroup`: Group container.
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

impl_simple_slot_builder!(
    ContextMenuTriggerBuilder,
    context_menu_trigger,
    ContextMenuTrigger,
    ContextMenuTriggerProps
);
impl_simple_slot_builder!(
    ContextMenuItemBuilder,
    context_menu_item,
    ContextMenuItem,
    ContextMenuItemProps
);
impl_simple_slot_builder!(
    ContextMenuLabelBuilder,
    context_menu_label,
    ContextMenuLabel,
    ContextMenuLabelProps
);
impl_simple_slot_builder!(
    ContextMenuSeparatorBuilder,
    context_menu_separator,
    ContextMenuSeparator,
    ContextMenuSeparatorProps
);
impl_simple_slot_builder!(
    ContextMenuShortcutBuilder,
    context_menu_shortcut,
    ContextMenuShortcut,
    ContextMenuShortcutProps
);
impl_simple_slot_builder!(
    ContextMenuGroupBuilder,
    context_menu_group,
    ContextMenuGroup,
    ContextMenuGroupProps
);

pub struct ContextMenuContentSlotBuilder {
    opts: ContextMenuContentOpts,
}

pub fn context_menu_content_slot() -> ContextMenuContentSlotBuilder {
    ContextMenuContentSlotBuilder {
        opts: ContextMenuContentOpts::default(),
    }
}

impl ContextMenuContentSlotBuilder {
    pub fn class(mut self, classes: impl Into<String>) -> Self {
        self.opts = self.opts.class(classes);
        self
    }
    pub fn style(
        mut self,
        f: impl Fn(
                crate::components::base::BaseComponentBuilder,
            ) -> crate::components::base::BaseComponentBuilder
            + 'static,
    ) -> Self {
        self.opts = self.opts.style(f);
        self
    }
    pub fn child(self, body: impl Fn() -> Element + 'static) -> Element {
        context_menu_content(self.opts, body)
    }
}

// ---------------------------------------------------------------------------
// Builder (legacy API compatible)
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

/// Old name alias ([`ContextMenuAction`]).
#[deprecated(note = "use ContextMenuAction; ContextMenuItem is now the RSX component")]
pub type ContextMenuItemData = ContextMenuAction;

pub struct ContextMenuBuilder {
    id: String,
    items: Vec<ContextMenuAction>,
    open: Option<Signal<bool>>,
    placement: PopupPlacement,
    slots: SlotMap,
}

/// Builder 入口：便捷 Trigger + Content + Items（内部接到复合实现）。
pub fn context_menu(id: impl Into<String>) -> ContextMenuBuilder {
    ContextMenuBuilder {
        id: id.into(),
        items: Vec::new(),
        open: None,
        placement: PopupPlacement::BottomStart,
        slots: SlotMap::default(),
    }
}

impl ContextMenuBuilder {
    crate::impl_slot_map_methods!();
    crate::impl_overlay_content_slot!(SLOT_CONTENT);

    pub fn open_signal(mut self, s: Signal<bool>) -> Self {
        self.open = Some(s);
        self
    }

    pub fn placement(mut self, p: PopupPlacement) -> Self {
        self.placement = p;
        self
    }

    /// Overlay `class` on the menu panel. Alias of [`Self::content_class`].
    pub fn class(self, c: impl Into<String>) -> Self {
        self.content_class(c)
    }

    /// Overlay style on the menu panel. Alias of [`Self::content_style`].
    pub fn style(
        self,
        f: impl Fn(
                crate::components::base::BaseComponentBuilder,
            ) -> crate::components::base::BaseComponentBuilder
            + 'static,
    ) -> Self {
        self.content_style(f)
    }

    /// Build Trigger / Content (and related) subtrees inside the context.
    pub fn child(self, body: impl FnOnce() -> Element) -> Element {
        context_menu_scope(self.id, self.open, body)
    }

    pub fn item(mut self, label: impl Into<String>, on_click: impl Fn() + 'static) -> Self {
        self.items.push(ContextMenuAction::new(label, on_click));
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
        let content_chrome = self.slots.get(SLOT_CONTENT);
        context_menu_scope(id, open, || {
            let trigger = ContextMenuTrigger(ContextMenuTriggerProps {
                id: String::new(),
                children: Some(build()),
                base: None,
            });
            let tip = context_menu_content(
                ContextMenuContentOpts {
                    placement: side,
                    panel: content_chrome,
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
                        frag.set_attr("jadix_fragment", "1");
                        frag = frag.children(kids);
                        let row = ContextMenuItem(ContextMenuItemProps {
                            disabled: Some(it.disabled),
                            inset: Some(false),
                            variant: if it.destructive {
                                Some("destructive".into())
                            } else {
                                None
                            },
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
            frag.set_attr("jadix_fragment", "1");
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
    fn rsx_menu_hugs_content_in_tall_viewport_after_open_and_idle() {
        use crate as jadix;
        use crate::app::portal_overlay::clear_overlay_stack;
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::core::reactivity::{create_effect, create_root};

        clear_overlay_stack();
        reset_registered_anchors();
        let live = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(live.clone());
        let (owner, (open, cursor)) = create_root(|| {
            let open = Signal::new(false);
            let cursor = use_signal("height-menu__ctx_cursor", || [1200.0_f32, 1450.0]);
            let live = live.clone();
            create_effect(move || {
                *live.borrow_mut() = crate::ui! {
                    <ContextMenu id="height-menu" open={open}>
                        <ContextMenuTrigger>"Right click"</ContextMenuTrigger>
                        <ContextMenuContent>
                            <ContextMenuLabel>"Actions"</ContextMenuLabel>
                            <ContextMenuItem>"Copy"</ContextMenuItem>
                            <ContextMenuItem>"Paste"</ContextMenuItem>
                            <ContextMenuSeparator />
                            <ContextMenuCheckboxItem defaultChecked={true}>"Show Bookmarks Bar"</ContextMenuCheckboxItem>
                            <ContextMenuCheckboxItem>"Show Full URLs"</ContextMenuCheckboxItem>
                            <ContextMenuSeparator />
                            <ContextMenuRadioGroup defaultValue="pedro">
                                <ContextMenuLabel>"People"</ContextMenuLabel>
                                <ContextMenuRadioItem value="pedro">"Pedro Duarte"</ContextMenuRadioItem>
                                <ContextMenuRadioItem value="colm">"Colm Tuite"</ContextMenuRadioItem>
                            </ContextMenuRadioGroup>
                            <ContextMenuSeparator />
                            <ContextMenuSub>
                                <ContextMenuSubTrigger>"More Tools"</ContextMenuSubTrigger>
                                <ContextMenuSubContent>
                                    <ContextMenuItem>"Save Page As..."</ContextMenuItem>
                                    <ContextMenuItem>"Developer Tools"</ContextMenuItem>
                                </ContextMenuSubContent>
                            </ContextMenuSub>
                            <ContextMenuSeparator />
                            <ContextMenuItem variant="destructive">"Delete"</ContextMenuItem>
                        </ContextMenuContent>
                    </ContextMenu>
                };
            });
            (open, cursor)
        });
        let mut layout = LayoutTree::new();
        layout.build_and_layout(&mut live.borrow_mut(), 2500.0, 1500.0, [0.0, 0.0]);
        open.set(true);
        drain_deferred_view_remounts();
        for frame in 0..64 {
            let mut root = live.borrow_mut();
            layout.build_and_layout_fast_ex(
                &mut root,
                2500.0,
                1500.0,
                [0.0, 0.0],
                true,
                false,
                None,
            );
            crate::utils::floating_motion::flush_floating_motion_subtree(&mut root);
            crate::utils::floating_motion::tick_hoisted_floating_css(&mut root);
            let mut panels = 0;
            crate::utils::popup::visit_floating_panels(&root, |panel| {
                if panel.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_CONTENT) {
                    panels += 1;
                    assert!(
                        (panel.layout_bounds[3] - 326.0).abs() < 0.5,
                        "menu must hug its rows from the first frame, frame={frame}: {:?}",
                        panel.layout_bounds
                    );
                    assert!(
                        (panel.layout_bounds[1] + panel.layout_bounds[3] - 1448.0).abs() < 0.5,
                        "menu must remain above the cursor: {:?}",
                        panel.layout_bounds
                    );
                }
            });
            assert_eq!(panels, 1);
        }
        // A short viewport must clip for scrolling, then restore the content
        // height when the menu is reopened in a larger window.
        for (height, point, expected_h) in [
            (200.0, [1200.0, 150.0], 184.0),
            (1500.0, [1200.0, 1450.0], 326.0),
            (1500.0, [1200.0, 100.0], 326.0),
        ] {
            open.set(false);
            drain_deferred_view_remounts();
            cursor.set(point);
            open.set(true);
            drain_deferred_view_remounts();
            for frame in 0..4 {
                let mut root = live.borrow_mut();
                layout.build_and_layout_fast_ex(
                    &mut root,
                    2500.0,
                    height,
                    [0.0, 0.0],
                    true,
                    false,
                    None,
                );
                let mut panels = 0;
                crate::utils::popup::visit_floating_panels(&root, |panel| {
                    if panel.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_CONTENT) {
                        panels += 1;
                        assert!((panel.layout_bounds[3] - expected_h).abs() < 0.5,
                            "height must follow content and viewport after reopen, frame={frame}: {:?}", panel.layout_bounds);
                        assert!(
                            panel.layout_bounds[1] >= 8.0
                                && panel.layout_bounds[1] + panel.layout_bounds[3] <= height - 7.5,
                            "menu must remain inside viewport: {:?}",
                            panel.layout_bounds
                        );
                    }
                });
                assert_eq!(panels, 1);
            }
        }
        clear_test_reconcile_root();
        drop(live);
        owner.dispose();
        reset_registered_anchors();
        clear_overlay_stack();
    }

    #[test]
    fn context_menu_records_items() {
        let fired = Rc::new(Cell::new(false));
        let f = fired.clone();
        let _el: Element = context_menu("cm1").item("Copy", move || f.set(true)).into();
        assert!(!fired.get());
    }

    #[test]
    fn builder_trigger_style() {
        use crate::core::reactivity::create_root;
        let (_o, el): (_, Element) = create_root(|| {
            context_menu("cm-chrome").child(|| {
                context_menu_trigger()
                    .style(|b| b.text_size(20.0))
                    .text("area")
                    .into()
            })
        });
        fn find_size(el: &Element, want: f32) -> bool {
            el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
                || (el.text_style.size_resolved() - want).abs() < 0.01
                || el.children.iter().any(|c| find_size(c, want))
        }
        assert!(
            find_size(&el, 20.0),
            "context_menu_trigger .style(|b| b.text_size(20)) must land on the trigger tree"
        );
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
                    on_click: None,
                    children: Some(Element::new().text("One")),
                    base: None,
                })
            });
            let mut frag = Element::new();
            frag.set_attr("jadix_fragment", "1");
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
                        children: Some(Element::new().text("More")),
                        base: None,
                    });
                    let sc = context_menu_sub_content(Default::default(), || {
                        ContextMenuItem(ContextMenuItemProps {
                            disabled: Some(false),
                            inset: Some(false),
                            variant: None,
                            on_click: None,
                            children: Some(Element::new().text("Nested")),
                            base: None,
                        })
                    });
                    let mut frag = Element::new();
                    frag.set_attr("jadix_fragment", "1");
                    frag.children(vec![st, sc])
                })
            })
        });
    }
}
