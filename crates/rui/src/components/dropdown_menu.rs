//! # DropdownMenu
//!
//! 对齐 solid-ui / ShadCN **复合 DropdownMenu**（左键触发，锚点相对 Trigger）：
//!
//! ```text
//! DropdownMenu
//! ├── DropdownMenuTrigger      // 点击切换；子节点为触发器（常为 Button）
//! └── DropdownMenuContent      // Portal + floating；bg-popover / border / shadow
//!     ├── DropdownMenuLabel
//!     ├── DropdownMenuItem     // hover/focus:bg-accent；可选 inset / destructive
//!     ├── DropdownMenuCheckboxItem
//!     ├── DropdownMenuRadioGroup / DropdownMenuRadioItem
//!     ├── DropdownMenuShortcut // ml-auto text-xs tracking-widest
//!     ├── DropdownMenuSeparator
//!     ├── DropdownMenuGroup
//!     └── DropdownMenuSub
//!         ├── DropdownMenuSubTrigger  // chevron-right；悬停打开
//!         └── DropdownMenuSubContent  // 右侧嵌套菜单
//! ```
//!
//! `ui!` 中 `<DropdownMenu>` / `<DropdownMenuContent>` / `<DropdownMenuRadioGroup>` 由宏展开。
//! Builder [`dropdown_menu`]（别名 [`dropdown`]）仍可用。
//!
//! ## 用法（双写法）
//! ```ignore
//! // A. 复合 RSX（推荐；宏走 dropdown_menu_scope）
//! ui! {
//!     <DropdownMenu id="dd">
//!         <DropdownMenuTrigger>
//!             <Button variant="outline" size="sm">"Open"</Button>
//!         </DropdownMenuTrigger>
//!         <DropdownMenuContent>
//!             <DropdownMenuLabel>"Actions"</DropdownMenuLabel>
//!             <DropdownMenuItem on_click={|| toast_push("Copy", 1500)}>
//!                 "Copy"
//!                 <DropdownMenuShortcut>"⌘C"</DropdownMenuShortcut>
//!             </DropdownMenuItem>
//!             <DropdownMenuSeparator />
//!             <DropdownMenuItem variant="destructive" on_click={|| {}}>
//!                 "Delete"
//!             </DropdownMenuItem>
//!         </DropdownMenuContent>
//!     </DropdownMenu>
//! }
//!
//! // B. Builder（便捷入口）
//! dropdown_menu("dd", || button("t").text("Open").into(), || {
//!     /* menu body Element */
//! })
//! ```
//!
//! ## 注意
//! - 打开后 ↑↓ / Home / End 走 roving；Enter / Space 激活高亮项；Esc 经 overlay 栈关闭。
//! - 未传 `open` 时用内部信号 `{id}__dd_open`；多实例请保证 `id` 唯一。
//! - 与 ContextMenu 的差异：左键切换、始终相对 Trigger 锚点（无鼠标原点）。
//! - 本模块是**菜单**（命令/勾选/单选），不是 Select 类选项列表。

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::OnceLock;

use rui_macros::component;
use winit::keyboard::{Key, NamedKey};

use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode};
use crate::app::reactive::{reactive_anchor, reactive_placeholder_sized};
use crate::components::internal::menu_checkable::{menu_checkbox_control, menu_radio_control};
use crate::components::internal::menu_item_chrome;
use crate::components::internal::menu_subtree;
use crate::components::label::label;
use crate::components::scroll_area::scroll_area;
use crate::components::separator::separator;
use crate::core::reactivity::{
    create_context, isolate_reads, provide_context, untrack, use_context, use_signal,
    with_child_owner, Context, Signal,
};
use crate::core::tree::{CursorKind, Element, Handler, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::primitives::text::text;
use crate::style::{parse_text_color, with_class_list};
use crate::tokens;
use crate::utils::activate_registry;
use crate::utils::lucide_glyph;
use crate::utils::popup::{
    apply_stored_floating_place, disarm_underlay_close, mark_floating, mark_floating_with_gap,
    mount_anchored_overlay, PopupPlacement, ATTR_FLOATING_MATCH_TRIGGER, ATTR_FLOATING_MIN_W,
    FLOATING_TRIGGER_GAP,
};
use crate::utils::resolve_instance_id;
use crate::utils::roving::{roving_edge, roving_move};
use crate::utils::take_packed;
use crate::utils::text_node::build_icon_text_node;
use crate::utils::toggle_state::write_bool_toggle;

const ATTR_SLOT: &str = "data-slot";
const SLOT_TRIGGER: &str = "dropdown-menu-trigger";
const SLOT_CONTENT: &str = "dropdown-menu-content";
const SLOT_ITEM: &str = "dropdown-menu-item";
const SLOT_CHECKBOX_ITEM: &str = "dropdown-menu-checkbox-item";
const SLOT_RADIO_ITEM: &str = "dropdown-menu-radio-item";
const SLOT_LABEL: &str = "dropdown-menu-label";
const SLOT_SEPARATOR: &str = "dropdown-menu-separator";
const SLOT_SHORTCUT: &str = "dropdown-menu-shortcut";
const SLOT_GROUP: &str = "dropdown-menu-group";
const SLOT_RADIO_GROUP: &str = "dropdown-menu-radio-group";
const SLOT_SUB_TRIGGER: &str = "dropdown-menu-sub-trigger";
const SLOT_SUB_CONTENT: &str = "dropdown-menu-sub-content";

/// Visual gap between SubTrigger and SubContent (same as default floating gap).
const SUBMENU_VISUAL_GAP_PX: f32 = FLOATING_TRIGGER_GAP;

/// Roving / hover highlight: no item selected.
const ACTIVE_NONE: usize = usize::MAX;

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

/// Root 上下文：`open` + 稳定实例 `id` + 触发器锚点 + roving 高亮。
#[derive(Clone)]
pub struct DropdownMenuCtx {
    pub open: Signal<bool>,
    pub id: String,
    /// 触发器 focus_id（键盘 / outside-press / floating 锚点）。
    pub trigger_fid: Signal<String>,
    pub active: Signal<usize>,
    pub placement: PopupPlacement,
}

fn dropdown_menu_context() -> Context<DropdownMenuCtx> {
    static CTX: OnceLock<Context<DropdownMenuCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<DropdownMenuCtx>)
}

thread_local! {
    /// Parent menu item index → nested submenu id (for ArrowRight / keyboard enter).
    static DD_SUB_AT_IX: RefCell<HashMap<String, HashMap<usize, String>>> =
        RefCell::new(HashMap::new());
    static DD_RADIO_SEQ: RefCell<HashMap<String, usize>> = RefCell::new(HashMap::new());
}

fn reset_activates(id: &str) {
    activate_registry::reset(id);
    DD_SUB_AT_IX.with(|m| {
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
    DD_SUB_AT_IX.with(|m| {
        m.borrow_mut()
            .entry(parent_id.to_string())
            .or_default()
            .insert(ix, sid.to_string());
    });
}

fn sub_id_at_ix(parent_id: &str, ix: usize) -> Option<String> {
    DD_SUB_AT_IX.with(|m| {
        m.borrow()
            .get(parent_id)
            .and_then(|map| map.get(&ix).cloned())
    })
}

/// First open nested submenu under `parent_id`, if any.
pub(crate) fn dropdown_menu_open_sub_id(parent_id: &str) -> Option<String> {
    menu_subtree::open_sub_id(parent_id)
}

/// Whether the highlighted parent row is a SubTrigger.
pub(crate) fn dropdown_menu_active_is_sub_trigger(parent_id: &str, ix: usize) -> bool {
    sub_id_at_ix(parent_id, ix).is_some()
}

fn focus_submenu_first_item(sid: &str) {
    menu_subtree::focus_submenu_first_item(sid, "dd_active", activate_count(sid));
}

fn close_open_submenu_keep_parent_highlight(sid: &str) {
    menu_subtree::close_open_submenu_keep_parent_highlight(sid, "dd_active");
}

fn reset_sub_seq(parent_id: &str) {
    menu_subtree::reset_sub_seq(parent_id);
}

fn reset_radio_seq(parent_id: &str) {
    DD_RADIO_SEQ.with(|m| {
        m.borrow_mut().insert(parent_id.to_string(), 0);
    });
}

fn next_sub_seq(parent_id: &str) -> usize {
    menu_subtree::next_sub_seq(parent_id)
}

fn next_radio_seq(parent_id: &str) -> usize {
    DD_RADIO_SEQ.with(|m| {
        let mut map = m.borrow_mut();
        let e = map.entry(parent_id.to_string()).or_insert(0);
        let i = *e;
        *e += 1;
        i
    })
}

fn set_dd_active(active: Signal<usize>, next: usize) {
    menu_subtree::set_menu_active(active, next);
}

fn clear_dd_active_if(active: Signal<usize>, ix: usize) {
    if active.peek() == ix {
        set_dd_active(active, ACTIVE_NONE);
    }
}

fn close_open_subs(parent_id: &str) {
    menu_subtree::close_open_subs(parent_id);
}

fn close_open_subs_except(parent_id: &str, keep_sub_id: Option<&str>) {
    menu_subtree::close_open_subs_except(parent_id, keep_sub_id);
}

fn open_or_remount_sub(sub_id: &str, sub_open: Signal<bool>) {
    menu_subtree::open_or_remount_sub(sub_id, sub_open);
}

fn open_or_remount_sub_ex(sub_id: &str, sub_open: Signal<bool>, refresh_hover: bool) {
    menu_subtree::open_or_remount_sub_ex(sub_id, sub_open, refresh_hover);
}

/// 读取最近的 [`DropdownMenu`] 上下文。
pub fn use_dropdown_menu() -> Option<DropdownMenuCtx> {
    use_context(dropdown_menu_context())
}

fn require_dropdown_menu(part: &str) -> DropdownMenuCtx {
    use_dropdown_menu().unwrap_or_else(|| {
        panic!("{part} must be used within <DropdownMenu> / dropdown_menu_scope");
    })
}

/// 提供 DropdownMenu 上下文并构建子树（`ui!` `<DropdownMenu>` / Builder 共用）。
///
/// Context is installed under a **child owner** so sibling scopes (Menubar File / Edit /
/// View) do not overwrite each other on the shared parent. Without isolation, the last
/// menu wins and later Content rebuilds (e.g. Share) bind to the wrong `open` / id —
/// gallery logged `gal-menubar__view__sub0` while File was open, then SubContent closed
/// the sub because View's `open` was still false.
pub fn dropdown_menu_scope(
    id: impl Into<String>,
    open: Option<Signal<bool>>,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = resolve_instance_id(id, "dropdown_menu");
    let open = open.unwrap_or_else(|| use_signal(format!("{id}__dd_open").as_str(), || false));
    let active = use_signal(format!("{id}__dd_active").as_str(), || ACTIVE_NONE);
    let trigger_fid = use_signal(format!("{id}__dd_trigger_fid").as_str(), || {
        format!("{id}__dd_trigger")
    });
    let mut root = with_child_owner(|| {
        provide_context(
            dropdown_menu_context(),
            DropdownMenuCtx {
                open,
                id: id.clone(),
                trigger_fid,
                active,
                placement: PopupPlacement::BottomStart,
            },
        );
        body()
    });
    if root.template_id.is_none() {
        root.template_id = Some(format!("{id}__dropdown_menu"));
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
    menu_item_chrome::has_shortcut_slot(el, ATTR_SLOT, SLOT_SHORTCUT)
}

fn chain_click(mut el: Element, handler: impl FnMut() + 'static) -> Element {
    let prev = el.on_click.clone();
    let mut handler = handler;
    el = el.on_click(move || {
        if let Some(ref rc) = prev {
            rc.borrow_mut()();
        }
        handler();
    });
    el
}

fn style_item_text(el: &mut Element, color: [u8; 4]) {
    menu_item_chrome::style_item_text(el, color, ATTR_SLOT, SLOT_SHORTCUT);
}

/// Shared item chrome. Do **not** use `min_w(0)` — that collapses wrapping labels to 1ch.
fn item_row_chrome(inset: bool) -> crate::primitives::r#box::BoxBuilder {
    menu_item_chrome::item_row_chrome(inset)
}

pub(crate) fn attach_dropdown_menu_trigger_keys(
    mut trigger: Element,
    ctx: &DropdownMenuCtx,
) -> Element {
    let open_keys = ctx.open;
    let active_keys = ctx.active;
    let menu_id = ctx.id.clone();
    trigger = trigger.on_key_down(move |ev| {
        if !open_keys.peek() {
            return false;
        }

        // While a nested submenu is open: ↑↓/Enter stay inside it; ← closes it
        // (Esc is usually handled by overlay `try_close_top` before this runs).
        if let Some(sid) = dropdown_menu_open_sub_id(&menu_id) {
            let n = activate_count(&sid);
            let sub_active = use_signal(format!("{sid}__dd_active").as_str(), || ACTIVE_NONE);
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
                    set_dd_active(sub_active, next);
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
                    set_dd_active(sub_active, next);
                    true
                }
                Key::Named(NamedKey::Home) => {
                    if let Some(i) = roving_edge(n, false) {
                        set_dd_active(sub_active, i);
                    }
                    true
                }
                Key::Named(NamedKey::End) => {
                    if let Some(i) = roving_edge(n, true) {
                        set_dd_active(sub_active, i);
                    }
                    true
                }
                Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
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
                set_dd_active(active_keys, next);
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
                set_dd_active(active_keys, next);
                true
            }
            Key::Named(NamedKey::Home) => {
                if let Some(i) = roving_edge(n, false) {
                    close_open_subs(&menu_id);
                    set_dd_active(active_keys, i);
                }
                true
            }
            Key::Named(NamedKey::End) => {
                if let Some(i) = roving_edge(n, true) {
                    close_open_subs(&menu_id);
                    set_dd_active(active_keys, i);
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
            Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                let ix = active_keys.peek();
                if ix == ACTIVE_NONE {
                    return false;
                }
                if let Some(cb) = take_activate(&menu_id, ix) {
                    // Close is owned by each activate (Item/Radio)；Checkbox 不关。
                    // SubTrigger open + move highlight into the submenu first row.
                    cb();
                    if let Some(sid) = dropdown_menu_open_sub_id(&menu_id) {
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

/// solid-ui / shadcn `DropdownMenuTrigger`：点击切换；子节点为触发器。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DropdownMenuTrigger(id: String, children: Option<Element>) -> Element {
    let ctx = require_dropdown_menu("DropdownMenuTrigger");
    let open = ctx.open;
    let active = ctx.active;
    let trigger_id = if id.is_empty() {
        format!("{}__dd_trigger", ctx.id)
    } else {
        id
    };
    ctx.trigger_fid.set(trigger_id.clone());

    let open_toggle = open;
    let active_open = active;
    let menu_id_open = ctx.id.clone();
    let toggle = move || {
        set_dd_active(active_open, ACTIVE_NONE);
        close_open_subs(&menu_id_open);
        let next = !open_toggle.peek();
        if next {
            disarm_underlay_close();
        }
        open_toggle.set(next);
    };

    let mut trigger = match children {
        None => {
            let mut el: Element = r#box().px("0.75rem").py("0.5rem").into();
            el = el.child(text("Open"));
            el = el.focus_id(trigger_id.as_str());
            chain_click(el, toggle)
        }
        Some(c) if is_text_leaf(&c) => {
            let t = c.text.clone().unwrap_or_default();
            let mut el: Element = r#box().px("0.75rem").py("0.5rem").into();
            el = el.child(text(t.as_str()));
            el = el.focus_id(trigger_id.as_str());
            chain_click(el, toggle)
        }
        Some(mut c) => {
            if c.focus_id.is_none() {
                c = c.focus_id(trigger_id.as_str());
            }
            let fid = c.focus_id.clone().unwrap_or(trigger_id.clone());
            ctx.trigger_fid.set(fid);
            chain_click(c, toggle)
        }
    };

    if trigger.button_hover_paint.is_none() && trigger.input_paint_sync.is_none() {
        trigger.focusable = true;
        trigger.focus_ring = true;
    }
    let fid_focus = trigger
        .focus_id
        .clone()
        .unwrap_or_else(|| trigger_id.clone());
    ctx.trigger_fid.set(fid_focus);

    trigger = attach_dropdown_menu_trigger_keys(trigger, &ctx);
    trigger.set_attr(ATTR_SLOT, SLOT_TRIGGER);
    trigger
}

/// `DropdownMenuContent` 选项（`ui!` / [`dropdown_menu_content`] 共用）。
#[derive(Clone, Debug)]
pub struct DropdownMenuContentOpts {
    /// `bottom-start` / `right` / …（默认 `bottom-start`）。
    pub placement: String,
    /// 额外 class（utility）。
    pub class: Option<String>,
}

impl Default for DropdownMenuContentOpts {
    fn default() -> Self {
        Self {
            placement: "bottom-start".into(),
            class: None,
        }
    }
}

/// solid-ui / shadcn `DropdownMenuContent`：Portal + floating 面板。
/// `ui!` 中由宏编成此函数（子树在打开时惰性重建）。
pub fn dropdown_menu_content(
    opts: DropdownMenuContentOpts,
    body: impl Fn() -> Element + 'static,
) -> Element {
    let ctx = require_dropdown_menu("DropdownMenuContent");
    let open = ctx.open;
    let id = ctx.id.clone();
    let trigger_fid = ctx.trigger_fid;
    let menu_id = id.clone();
    let _ = ctx.active;
    let placement = if opts.placement.trim().is_empty() {
        ctx.placement
    } else {
        PopupPlacement::from_attr(&opts.placement)
    };
    let class = opts.class.filter(|s| !s.trim().is_empty());
    let view_id = format!("{id}__dd_view");

    reactive_anchor(view_id.clone(), move || {
        let is_open = open.get();
        let Some(oz) = portal_sync(&id, open, OverlayKind::Menu, OverlayMode::Stacked) else {
            return Element::new().w(0.0).h(0.0);
        };
        if !is_open {
            close_open_subs(&menu_id);
            return Element::new().w(0.0).h(0.0);
        }

        reset_activates(&menu_id);
        reset_sub_seq(&menu_id);
        reset_radio_seq(&menu_id);
        let rows = isolate_reads(|| body());
        let scroll_id = format!("{menu_id}__dd_scroll");
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
        mark_floating(&mut panel, &id, &anchor, placement);
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

/// solid-ui / shadcn `DropdownMenuItem`：`px-2 py-1.5 text-sm`；高亮 `bg-accent`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DropdownMenuItem(
    disabled: bool,
    inset: bool,
    variant: Option<String>,
    class: Option<String>,
    #[prop(default = "Handler::default()")] mut on_click: Handler,
    children: Option<Element>,
) -> Element {
    let ctx = require_dropdown_menu("DropdownMenuItem");
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
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        row = with_class_list(row, &c);
    }

    let mut hit: Element = interactive(row)
        .on_hover(move |h| {
            if disabled {
                return;
            }
            if !h {
                clear_dd_active_if(active, ix);
                return;
            }
            close_open_subs(&menu_id);
            set_dd_active(active, ix);
        })
        .on_click(move || {
            if disabled {
                return;
            }
            set_dd_active(active, ix);
            activate_click();
        })
        .cursor_kind(if disabled {
            CursorKind::NotAllowed
        } else {
            CursorKind::Default
        })
        .into();
    hit = hit.rounded("0.25rem");
    hit.set_attr(ATTR_SLOT, SLOT_ITEM);
    hit.set_attr("rui_dd_id", &ctx.id);
    hit.set_attr("rui_dd_ix", ix.to_string());
    hit.set_attr("data-inset", if inset { "true" } else { "false" });
    if destructive {
        hit.set_attr("data-variant", "destructive");
        hit.set_attr("rui_dd_destructive", "1");
    }
    if disabled {
        hit.set_attr("data-disabled", "true");
        hit.set_attr("rui_dd_disabled", "1");
    }
    hit.accesskit_role("menuitem")
}

/// solid-ui / shadcn `DropdownMenuCheckboxItem`：左侧对齐 Checkbox 控制面；点击切换，**不**关闭菜单。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DropdownMenuCheckboxItem(
    checked: Option<Signal<bool>>,
    default_checked: bool,
    disabled: bool,
    inset: bool,
    class: Option<String>,
    #[prop(default = "Handler::default()")] mut on_click: Handler,
    children: Option<Element>,
) -> Element {
    let ctx = require_dropdown_menu("DropdownMenuCheckboxItem");
    let active = ctx.active;
    let menu_id = ctx.id.clone();
    let ix = activate_count(&ctx.id);
    // 全局键：跨 Content 开关存活。勿用 use_scoped_signal——关菜单 dispose 后 paint sync 仍可能 peek。
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
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        row = with_class_list(row, &c);
    }

    let mut hit: Element = interactive(row)
        .on_hover(move |h| {
            if disabled {
                return;
            }
            if !h {
                clear_dd_active_if(active, ix);
                return;
            }
            close_open_subs(&menu_id);
            set_dd_active(active, ix);
        })
        .on_click(move || {
            if disabled {
                return;
            }
            set_dd_active(active, ix);
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
    hit.set_attr("rui_dd_id", &ctx.id);
    hit.set_attr("rui_dd_ix", ix.to_string());
    hit.set_attr("data-inset", if inset { "true" } else { "false" });
    if disabled {
        hit.set_attr("data-disabled", "true");
        hit.set_attr("rui_dd_disabled", "1");
    }
    hit.accesskit_role("menuitemcheckbox")
}

// ---------------------------------------------------------------------------
// RadioGroup / RadioItem
// ---------------------------------------------------------------------------

/// Nested radio-group context (under a root [`DropdownMenuCtx`]).
#[derive(Clone)]
pub struct DropdownMenuRadioGroupCtx {
    pub value: Signal<String>,
    pub id: String,
}

fn dropdown_menu_radio_group_context() -> Context<DropdownMenuRadioGroupCtx> {
    static CTX: OnceLock<Context<DropdownMenuRadioGroupCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<DropdownMenuRadioGroupCtx>)
}

fn use_dropdown_menu_radio_group() -> Option<DropdownMenuRadioGroupCtx> {
    use_context(dropdown_menu_radio_group_context())
}

fn require_dropdown_menu_radio_group(part: &str) -> DropdownMenuRadioGroupCtx {
    use_dropdown_menu_radio_group().unwrap_or_else(|| {
        panic!("{part} must be used within <DropdownMenuRadioGroup>");
    })
}

/// solid-ui / shadcn `DropdownMenuRadioGroup`：包裹一组 [`DropdownMenuRadioItem`]。
pub fn dropdown_menu_radio_group(
    id: Option<String>,
    value: Option<Signal<String>>,
    default_value: Option<String>,
    body: impl FnOnce() -> Element,
) -> Element {
    let parent = require_dropdown_menu("DropdownMenuRadioGroup");
    let seq = next_radio_seq(&parent.id);
    let rid = match id {
        Some(s) if !s.trim().is_empty() => s,
        _ => format!("{}__radio{seq}", parent.id),
    };
    let default_value = default_value.unwrap_or_default();
    let value = value.unwrap_or_else(|| {
        use_signal(format!("{rid}__value").as_str(), move || default_value.clone())
    });
    let kids = with_child_owner(|| {
        provide_context(
            dropdown_menu_radio_group_context(),
            DropdownMenuRadioGroupCtx {
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

/// solid-ui / shadcn `DropdownMenuRadioItem`：左侧圆点指示；选中后关闭菜单。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DropdownMenuRadioItem(
    value: String,
    disabled: bool,
    inset: bool,
    class: Option<String>,
    #[prop(default = "Handler::default()")] mut on_click: Handler,
    children: Option<Element>,
) -> Element {
    let ctx = require_dropdown_menu("DropdownMenuRadioItem");
    let group = require_dropdown_menu_radio_group("DropdownMenuRadioItem");
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
            // 先写值再关：全局 radio 信号跨 Content 存活；try_set 防极端 dispose 竞态。
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
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        row = with_class_list(row, &c);
    }

    let mut hit: Element = interactive(row)
        .on_hover(move |h| {
            if disabled {
                return;
            }
            if !h {
                clear_dd_active_if(active, ix);
                return;
            }
            close_open_subs(&menu_id);
            set_dd_active(active, ix);
        })
        .on_click(move || {
            if disabled {
                return;
            }
            set_dd_active(active, ix);
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
    hit.set_attr("rui_dd_id", &ctx.id);
    hit.set_attr("rui_dd_ix", ix.to_string());
    hit.set_attr("data-inset", if inset { "true" } else { "false" });
    hit.set_attr("data-value", &value);
    if disabled {
        hit.set_attr("data-disabled", "true");
        hit.set_attr("rui_dd_disabled", "1");
    }
    hit.accesskit_role("menuitemradio")
}

// ---------------------------------------------------------------------------
// Submenu (Sub / SubTrigger / SubContent)
// ---------------------------------------------------------------------------

/// Nested submenu context (under a root [`DropdownMenuCtx`]).
#[derive(Clone)]
pub struct DropdownMenuSubCtx {
    pub open: Signal<bool>,
    pub id: String,
    pub trigger_fid: Signal<String>,
}

fn dropdown_menu_sub_context() -> Context<DropdownMenuSubCtx> {
    static CTX: OnceLock<Context<DropdownMenuSubCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<DropdownMenuSubCtx>)
}

fn use_dropdown_menu_sub() -> Option<DropdownMenuSubCtx> {
    use_context(dropdown_menu_sub_context())
}

fn require_dropdown_menu_sub(part: &str) -> DropdownMenuSubCtx {
    use_dropdown_menu_sub().unwrap_or_else(|| {
        panic!("{part} must be used within <DropdownMenuSub>");
    })
}

/// solid-ui / shadcn `DropdownMenuSub`：包裹 SubTrigger + SubContent。
pub fn dropdown_menu_sub(id: Option<String>, body: impl FnOnce() -> Element) -> Element {
    let parent = require_dropdown_menu("DropdownMenuSub");
    let seq = next_sub_seq(&parent.id);
    let sid = match id {
        Some(s) if !s.trim().is_empty() => s,
        _ => format!("{}__sub{seq}", parent.id),
    };
    let open = use_signal(format!("{sid}__open").as_str(), || false);
    let trigger_fid = use_signal(format!("{sid}__tfid").as_str(), || format!("{sid}__trigger"));
    let kids = with_child_owner(|| {
        provide_context(
            dropdown_menu_sub_context(),
            DropdownMenuSubCtx {
                open,
                id: sid,
                trigger_fid,
            },
        );
        take_packed(body())
    });
    let mut frag = Element::new();
    frag.set_attr("rui_fragment", "1");
    frag.set_attr("data-slot", "dropdown-menu-sub");
    frag.children(kids)
}

/// solid-ui / shadcn `DropdownMenuSubTrigger`：右侧 chevron；悬停打开子菜单。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DropdownMenuSubTrigger(
    disabled: bool,
    inset: bool,
    class: Option<String>,
    children: Option<Element>,
) -> Element {
    let parent = require_dropdown_menu("DropdownMenuSubTrigger");
    let sub = require_dropdown_menu_sub("DropdownMenuSubTrigger");
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
                // Keyboard Enter/Space: do not refresh hover (resting pointer would steal).
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
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        row = with_class_list(row, &c);
    }

    let mut hit: Element = interactive(row)
        .on_hover(move |h| {
            if disabled {
                return;
            }
            if !h {
                if !sub_open.peek() {
                    clear_dd_active_if(active, ix);
                }
                return;
            }
            close_open_subs_except(&parent_id, Some(sub_id.as_str()));
            set_dd_active(active, ix);
            open_or_remount_sub(&sub_id, sub_open);
        })
        .on_click(move || {
            if disabled {
                return;
            }
            close_open_subs_except(&parent_id_click, Some(sub_id_click.as_str()));
            open_or_remount_sub(&sub_id_click, sub_open_click);
            set_dd_active(active, ix);
        })
        .cursor_kind(if disabled {
            CursorKind::NotAllowed
        } else {
            CursorKind::Default
        })
        .into();
    hit = hit.focus_id(trigger_id.as_str()).rounded("0.25rem");
    hit.set_attr(ATTR_SLOT, SLOT_SUB_TRIGGER);
    hit.set_attr("rui_dd_id", &parent.id);
    hit.set_attr("rui_dd_ix", ix.to_string());
    hit.set_attr("data-inset", if inset { "true" } else { "false" });
    if disabled {
        hit.set_attr("data-disabled", "true");
        hit.set_attr("rui_dd_disabled", "1");
    }
    hit.accesskit_role("menuitem")
}

/// solid-ui / shadcn `DropdownMenuSubContent`：相对 SubTrigger 向右弹出。
pub fn dropdown_menu_sub_content(
    class: Option<String>,
    body: impl Fn() -> Element + 'static,
) -> Element {
    let parent = require_dropdown_menu("DropdownMenuSubContent");
    let sub = require_dropdown_menu_sub("DropdownMenuSubContent");
    let parent_open = parent.open;
    let sub_open = sub.open;
    let sid = sub.id.clone();
    let trigger_fid = sub.trigger_fid;
    let class = class.filter(|s| !s.trim().is_empty());
    let view_id = format!("{sid}__view");

    reactive_anchor(view_id.clone(), move || {
        let parent_is_open = parent_open.get();
        let is_open = sub_open.get();
        // Remount bump from SubTrigger hover/click (same-value open would not re-run).
        let remount_n = use_signal(format!("{sid}__remount").as_str(), || 0u32).get();
        if !parent_is_open {
            if sub_open.peek() {
                sub_open.set(false);
            }
            use_signal(format!("{sid}__dd_active").as_str(), || ACTIVE_NONE)
                .set_silent(ACTIVE_NONE);
            return Element::new().w(0.0).h(0.0);
        }
        if !is_open {
            use_signal(format!("{sid}__dd_active").as_str(), || ACTIVE_NONE)
                .set_silent(ACTIVE_NONE);
            let _ = portal_sync(&sid, sub_open, OverlayKind::Menu, OverlayMode::Stacked);
            return Element::new().w(0.0).h(0.0);
        }
        let Some(oz) = portal_sync(&sid, sub_open, OverlayKind::Menu, OverlayMode::Stacked) else {
            log::debug!(
                target: "rui::menu_sub",
                "sub_content portal_sync NONE sid={sid} parent_open={parent_is_open} remount={remount_n}"
            );
            return Element::new().w(0.0).h(0.0);
        };

        let kids = with_child_owner(|| {
            let sub_active = use_signal(format!("{sid}__dd_active").as_str(), || ACTIVE_NONE);
            provide_context(
                dropdown_menu_context(),
                DropdownMenuCtx {
                    open: parent_open,
                    id: sid.clone(),
                    trigger_fid,
                    active: sub_active,
                    placement: PopupPlacement::RightStart,
                },
            );
            reset_activates(&sid);
            take_packed(body())
        });
        // Silhouette = floating root (same as ContextMenuSubContent / root Content).
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
            .shadow([0.0, 0.0, 0.0, 0.14], 12.0)
            .absolute();
        if let Some(ref c) = class {
            panel = with_class_list(panel, c);
        }
        panel.set_attr(ATTR_SLOT, SLOT_SUB_CONTENT);
        panel = panel.accesskit_role("menu");
        let fid = trigger_fid.peek();
        mark_floating_with_gap(
            &mut panel,
            &sid,
            fid.as_str(),
            PopupPlacement::RightStart,
            SUBMENU_VISUAL_GAP_PX,
        );
        panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
        panel.set_attr(ATTR_FLOATING_MIN_W, "12rem");
        apply_stored_floating_place(&mut panel, &sid);
        let mounted = mount_anchored_overlay(panel, oz.panel);
        log::debug!(
            target: "rui::menu_sub",
            "sub_content BUILD sid={sid} fid={fid} oz={} remount={remount_n} kids_portal={}",
            oz.panel,
            mounted
                .children
                .first()
                .and_then(|c| c.attrs.get("rui_portal").map(String::as_str))
                .unwrap_or("-")
        );
        mounted
    });
    reactive_placeholder_sized(view_id, 0.0, 0.0).pointer_events_box_none()
}

/// 每帧按 `{id}__dd_active` 写回高亮（`peek`，不订阅根 effect）。
pub(crate) fn sync_dropdown_menu_item_paint_subtree(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
) {
    const DD_SLOTS: &[&str] = &[
        SLOT_ITEM,
        SLOT_SUB_TRIGGER,
        SLOT_CHECKBOX_ITEM,
        SLOT_RADIO_ITEM,
        // Menubar wrappers may annotate the same interactive rows.
        "menubar-item",
        "menubar-sub-trigger",
        "menubar-checkbox-item",
        "menubar-radio-item",
    ];
    let cfg = menu_item_chrome::ItemPaintConfig {
        attr_slot: ATTR_SLOT,
        shortcut_slot: SLOT_SHORTCUT,
        interactive_slots: DD_SLOTS,
        attr_disabled: "rui_dd_disabled",
        attr_destructive: "rui_dd_destructive",
        attr_ix: "rui_dd_ix",
        attr_menu_id: "rui_dd_id",
        active_key_suffix: "__dd_active",
        active_none: ACTIVE_NONE,
    };
    menu_item_chrome::sync_item_paint_subtree(el, layout_tree, impact, &cfg);
}

/// solid-ui / shadcn `DropdownMenuLabel`：`px-2 py-1.5 text-sm font-semibold`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DropdownMenuLabel(inset: bool, children: Option<Element>) -> Element {
    let _ = require_dropdown_menu("DropdownMenuLabel");
    let t = match &children {
        Some(c) => c
            .text
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| extract_text_leaf(c)),
        None => String::new(),
    };
    let mut el: Element = label(format!("dd-label:{t}"))
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

/// solid-ui / shadcn `DropdownMenuSeparator`：`-mx-1 my-1 h-px bg-border`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DropdownMenuSeparator(children: Option<Element>) -> Element {
    let _ = children;
    let _ = require_dropdown_menu("DropdownMenuSeparator");
    let mut el = separator().menu_bleed().pointer_events_none();
    el.set_attr(ATTR_SLOT, SLOT_SEPARATOR);
    el
}

/// solid-ui / shadcn `DropdownMenuShortcut`：`ml-auto text-xs tracking-widest opacity-60`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DropdownMenuShortcut(children: Option<Element>) -> Element {
    let _ = require_dropdown_menu("DropdownMenuShortcut");
    let t = match &children {
        Some(c) => c
            .text
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| extract_text_leaf(c)),
        None => String::new(),
    };
    let mut kbd: Element = label(format!("dd-sc:{t}"))
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

/// solid-ui / shadcn `DropdownMenuGroup`：分组容器。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn DropdownMenuGroup(children: Option<Element>) -> Element {
    let _ = require_dropdown_menu("DropdownMenuGroup");
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
// Builder
// ---------------------------------------------------------------------------

/// 与 [`DropdownMenuBuilder`] 相同（历史别名）。
pub type DropdownBuilder = DropdownMenuBuilder;

pub struct DropdownMenuBuilder {
    id: String,
    open: Option<Signal<bool>>,
    trigger: Box<dyn Fn() -> Element>,
    menu: Box<dyn Fn() -> Element>,
    anchor_focus_id: Option<String>,
    placement: PopupPlacement,
}

/// 触发器 + 菜单壳。Esc 经 overlay 栈关闭。见模块文档。
pub fn dropdown_menu(
    id: impl Into<String>,
    trigger: impl Fn() -> Element + 'static,
    menu: impl Fn() -> Element + 'static,
) -> DropdownMenuBuilder {
    DropdownMenuBuilder {
        id: id.into(),
        open: None,
        trigger: Box::new(trigger),
        menu: Box::new(menu),
        anchor_focus_id: None,
        placement: PopupPlacement::BottomStart,
    }
}

/// 历史别名：请优先用 [`dropdown_menu`]。
pub fn dropdown(
    id: impl Into<String>,
    trigger: impl Fn() -> Element + 'static,
    menu: impl Fn() -> Element + 'static,
) -> DropdownMenuBuilder {
    dropdown_menu(id, trigger, menu)
}

impl DropdownMenuBuilder {
    pub fn open_signal(mut self, s: Signal<bool>) -> Self {
        self.open = Some(s);
        self
    }

    pub fn anchor_focus_id(mut self, fid: impl Into<String>) -> Self {
        self.anchor_focus_id = Some(fid.into());
        self
    }

    pub fn placement(mut self, p: PopupPlacement) -> Self {
        self.placement = p;
        self
    }
}

impl From<DropdownMenuBuilder> for Element {
    fn from(builder: DropdownMenuBuilder) -> Self {
        let DropdownMenuBuilder {
            id,
            open,
            trigger,
            menu,
            anchor_focus_id,
            placement,
        } = builder;
        let side = placement.as_attr().to_string();
        let trigger_fid = anchor_focus_id.unwrap_or_else(|| format!("{id}__dd_trigger"));
        dropdown_menu_scope(id, open, || {
            let mut trig = trigger();
            if trig.focus_id.is_none() {
                trig = trig.focus_id(trigger_fid.as_str());
            }
            let trigger_el = DropdownMenuTrigger(DropdownMenuTriggerProps {
                id: trigger_fid,
                children: Some(trig),
                base: None,
            });
            let content = dropdown_menu_content(
                DropdownMenuContentOpts {
                    placement: side,
                    class: None,
                },
                move || menu(),
            );
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            frag.children(vec![trigger_el, content])
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropdown_builder_sets_anchor() {
        let open = use_signal("dd-test-open", || false);
        let el: Element = dropdown(
            "dd-test",
            || Element::new().focus_id("dd-test__trigger").text("t"),
            || Element::new().text("menu"),
        )
        .open_signal(open)
        .anchor_focus_id("dd-test__trigger")
        .into();
        assert_eq!(
            el.template_id.as_deref(),
            Some("dd-test__dropdown_menu"),
            "dropdown must expose a stable dropdown_menu scope template"
        );
    }

    #[test]
    fn compound_scope_exposes_template() {
        let el = dropdown_menu_scope("dd-scope", None, || {
            let trigger = DropdownMenuTrigger(DropdownMenuTriggerProps {
                id: String::new(),
                children: Some(Element::new().text("Open")),
                base: None,
            });
            let content = dropdown_menu_content(DropdownMenuContentOpts::default(), || {
                DropdownMenuItem(DropdownMenuItemProps {
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
            Some("dd-scope__dropdown_menu"),
            "scope must expose a stable template id"
        );
    }

    #[test]
    fn compound_sub_provides_context_before_children() {
        let _el = dropdown_menu_scope("dd-sub", None, || {
            dropdown_menu_content(DropdownMenuContentOpts::default(), || {
                dropdown_menu_sub(None, || {
                    let st = DropdownMenuSubTrigger(DropdownMenuSubTriggerProps {
                        disabled: Some(false),
                        inset: Some(false),
                        class: None,
                        children: Some(Element::new().text("More")),
                        base: None,
                    });
                    let sc = dropdown_menu_sub_content(None, || {
                        DropdownMenuItem(DropdownMenuItemProps {
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

    /// Enter on SubTrigger opens the sub and highlights its first item; Esc/← close
    /// restores parent-only highlighting (sub active cleared).
    #[test]
    fn keyboard_enter_sub_focuses_first_item_then_escape_clears_sub_active() {
        use std::cell::RefCell;
        use std::rc::Rc;

        use crate::app::portal_overlay::clear_overlay_stack;
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::core::reactivity::{create_effect, create_root, use_signal};

        clear_overlay_stack();
        reset_registered_anchors();
        clear_test_reconcile_root();

        let root: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(Rc::clone(&root));

        let (handle, parent_open) = create_root(|| {
            let parent_open = use_signal("dd_kb_sub__dd_open", || false);
            let root_e = Rc::clone(&root);
            create_effect(move || {
                let el = dropdown_menu_scope("dd_kb_sub", Some(parent_open), || {
                    let trigger = DropdownMenuTrigger(DropdownMenuTriggerProps {
                        id: String::new(),
                        children: Some(Element::new().text("Open")),
                        base: None,
                    });
                    let content = dropdown_menu_content(DropdownMenuContentOpts::default(), || {
                        let _ = DropdownMenuItem(DropdownMenuItemProps {
                            disabled: Some(false),
                            inset: Some(false),
                            variant: None,
                            class: None,
                            on_click: None,
                            children: Some(Element::new().text("Above")),
                            base: None,
                        });
                        dropdown_menu_sub(None, || {
                            let st = DropdownMenuSubTrigger(DropdownMenuSubTriggerProps {
                                disabled: Some(false),
                                inset: Some(false),
                                class: None,
                                children: Some(Element::new().text("More")),
                                base: None,
                            });
                            let sc = dropdown_menu_sub_content(None, || {
                                DropdownMenuItem(DropdownMenuItemProps {
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
                    });
                    let mut frag = Element::new();
                    frag.set_attr("rui_fragment", "1");
                    frag.children(vec![trigger, content])
                });
                *root_e.borrow_mut() = el;
            });
            parent_open
        });

        parent_open.set(true);
        {
            let mut tree = root.borrow_mut();
            let _ = crate::layout::layout_tree::hoist_portal_subtrees(&mut tree);
        }

        // Simulate Enter on SubTrigger (ix 1 after "Above").
        let parent_active = use_signal("dd_kb_sub__dd_active", || ACTIVE_NONE);
        set_dd_active(parent_active, 1);
        assert!(
            dropdown_menu_active_is_sub_trigger("dd_kb_sub", 1),
            "Share/More row must register as a sub trigger"
        );
        if let Some(cb) = take_activate("dd_kb_sub", 1) {
            cb();
        }
        assert_eq!(
            dropdown_menu_open_sub_id("dd_kb_sub").as_deref(),
            Some("dd_kb_sub__sub0")
        );
        let sub_active = use_signal("dd_kb_sub__sub0__dd_active", || ACTIVE_NONE).peek();
        assert_eq!(sub_active, 0, "Enter must highlight the submenu first item");
        assert_eq!(
            parent_active.peek(),
            1,
            "parent SubTrigger highlight must remain while sub is open"
        );

        close_open_submenu_keep_parent_highlight("dd_kb_sub__sub0");
        assert!(dropdown_menu_open_sub_id("dd_kb_sub").is_none());
        assert_eq!(
            use_signal("dd_kb_sub__sub0__dd_active", || ACTIVE_NONE).peek(),
            ACTIVE_NONE
        );
        assert_eq!(
            parent_active.peek(),
            1,
            "closing sub must keep parent highlight on the SubTrigger"
        );

        clear_test_reconcile_root();
        drop(root);
        handle.dispose();
        clear_overlay_stack();
        reset_registered_anchors();
    }
}
