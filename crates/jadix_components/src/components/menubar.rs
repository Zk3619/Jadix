//! # Menubar
//!
//! 对齐 solid-ui / ShadCN **复合 Menubar**（应用命令栏；每项为下拉，复用 DropdownMenu 上下文）：
//!
//! ```text
//! Menubar
//! ├── MenubarMenu                 // 单段下拉（内部 dropdown_menu_scope）
//! │   ├── MenubarTrigger          // 顶栏触发器；open 时 accent；←→ 切邻菜单
//! │ └── MenubarContent // Portal + floating (same as DropdownMenuContent)
//! │       ├── MenubarLabel
//! │       ├── MenubarItem
//! │       ├── MenubarCheckboxItem
//! │       ├── MenubarRadioGroup / MenubarRadioItem
//! │       ├── MenubarShortcut
//! │       ├── MenubarSeparator
//! │       ├── MenubarGroup
//! │       └── MenubarSub
//! │           ├── MenubarSubTrigger
//! │           └── MenubarSubContent
//! ```
//!
//! `ui!` in `<Menubar>` / `<MenubarMenu>` / `<MenubarContent>` / `<MenubarSub>` /
//! `<MenubarRadioGroup>` / `<MenubarSubContent>` are expanded by macros. Builder [`menubar`] is still available.
//!
//! ## Usage (dual style)
//! ```ignore
//! // A. Composite RSX (recommended; macro menubar_scope / menubar_menu)
//! ui! {
//!     <Menubar id="mb">
//!         <MenubarMenu id="file">
//!             <MenubarTrigger>"File"</MenubarTrigger>
//!             <MenubarContent>
//!                 <MenubarItem on_click={|| toast_push("New", 1500)}>
//!                     "New"
//!                     <MenubarShortcut>"⌘N"</MenubarShortcut>
//!                 </MenubarItem>
//!                 <MenubarSeparator />
//!                 <MenubarItem variant="destructive" on_click={|| {}}>
//!                     "Quit"
//!                 </MenubarItem>
//!             </MenubarContent>
//!         </MenubarMenu>
//!     </Menubar>
//! }
//!
//! // B. Builder (convenience: sections → composite implementation)
//! menubar("mb")
//!     .section(MenuSection::new("file", "File", vec![
//!         MenuItem::new("new", "New", || toast_push("New", 1500)),
//!     ]))
//! ```
//!
//! ## Notes
//! - 关闭时：←→ / Home / End **只移动触发器焦点环**。
//! - 打开时：↓ 进入菜单 roving；←→ **切换相邻菜单**（portal Menu 排他关其它）。
//! - 任一菜单已打开时，悬停其它 Trigger 会切换打开。
//! - 顶栏样式对齐 solid-ui：`h-10 … rounded-md border bg-background p-1`。
//! - 与 NavigationMenu 的差异：本组件是**应用命令**，不是链接导览。

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::OnceLock;

use winit::event::ElementState;
use winit::keyboard::{Key, NamedKey};

use crate::app::reactive::bind_prop;
use crate::components::base::SlotChrome;
use crate::components::button::button;
use crate::components::dropdown_menu::{
    attach_dropdown_menu_trigger_keys, dropdown_menu_active_is_sub_trigger, dropdown_menu_content,
    dropdown_menu_open_sub_id, dropdown_menu_radio_group, dropdown_menu_scope, dropdown_menu_sub,
    use_dropdown_menu, DropdownMenuCheckboxItem, DropdownMenuCheckboxItemProps,
    DropdownMenuContentOpts, DropdownMenuGroup, DropdownMenuGroupProps, DropdownMenuItem,
    DropdownMenuItemProps, DropdownMenuLabel, DropdownMenuLabelProps, DropdownMenuRadioItem,
    DropdownMenuRadioItemProps, DropdownMenuSeparator, DropdownMenuSeparatorProps,
    DropdownMenuShortcut, DropdownMenuShortcutProps, DropdownMenuSubTrigger,
    DropdownMenuSubTriggerProps,
};
use crate::core::reactivity::{
    create_context, provide_context, use_context, use_signal, Context, Signal,
};
use crate::core::tree::{Element, Handler, LayoutStatus};
use crate::impl_simple_slot_builder;
use crate::impl_slot_chrome_methods;
use crate::primitives::r#box::hstack;
use crate::style::{parse_color, parse_text_color};
use crate::tokens;
use crate::utils::popup::disarm_underlay_close;
use crate::utils::resolve_instance_id;
use crate::utils::roving::{focus_by_id, focus_roving_key, roving_move, FocusRovingAxis};
use crate::utils::take_packed;

const ATTR_SLOT: &str = "data-slot";
const SLOT_MENUBAR: &str = "menubar";
const SLOT_MENU: &str = "menubar-menu";
const SLOT_TRIGGER: &str = "menubar-trigger";
const SLOT_CONTENT: &str = "menubar-content";
const SLOT_LABEL: &str = "menubar-label";
const SLOT_SEPARATOR: &str = "menubar-separator";
const SLOT_SHORTCUT: &str = "menubar-shortcut";
const SLOT_GROUP: &str = "menubar-group";

// ---------------------------------------------------------------------------
// Context / menu registry
// ---------------------------------------------------------------------------

/// Root 上下文：稳定实例 `id`（菜单注册表键）。
#[derive(Clone)]
pub struct MenubarCtx {
    pub id: String,
}

fn menubar_context() -> Context<MenubarCtx> {
    static CTX: OnceLock<Context<MenubarCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<MenubarCtx>)
}

#[derive(Clone)]
struct MenuReg {
    id: String,
    open: Signal<bool>,
}

thread_local! {
    static MB_MENUS: RefCell<HashMap<String, Vec<MenuReg>>> = RefCell::new(HashMap::new());
    static MB_MENU_SEQ: RefCell<HashMap<String, usize>> = RefCell::new(HashMap::new());
}

fn reset_menus(bar_id: &str) {
    MB_MENUS.with(|m| {
        m.borrow_mut().insert(bar_id.to_string(), Vec::new());
    });
    MB_MENU_SEQ.with(|m| {
        m.borrow_mut().insert(bar_id.to_string(), 0);
    });
}

fn next_menu_seq(bar_id: &str) -> usize {
    MB_MENU_SEQ.with(|m| {
        let mut map = m.borrow_mut();
        let e = map.entry(bar_id.to_string()).or_insert(0);
        let i = *e;
        *e += 1;
        i
    })
}

fn register_menu(bar_id: &str, reg: MenuReg) {
    MB_MENUS.with(|m| {
        m.borrow_mut()
            .entry(bar_id.to_string())
            .or_default()
            .push(reg);
    });
}

fn menus_of(bar_id: &str) -> Vec<MenuReg> {
    MB_MENUS.with(|m| m.borrow().get(bar_id).cloned().unwrap_or_default())
}

fn any_menu_open(bar_id: &str) -> bool {
    menus_of(bar_id)
        .iter()
        .any(|r| r.open.is_alive() && r.open.peek())
}

fn open_neighbor_menu(bar_id: &str, from_ix: usize, delta: i32) {
    let menus = menus_of(bar_id);
    let Some(next) = roving_move(menus.len(), from_ix, delta) else {
        return;
    };
    let underlay_armed = crate::utils::popup::underlay_close_is_armed();
    if let Some(cur) = menus.get(from_ix) {
        if cur.open.is_alive() {
            cur.open.set(false);
        }
    }
    let other = &menus[next];
    if other.open.is_alive() {
        // Do not disarm — peer switch; underlay should stay armed from the first open.
        other.open.set(true);
    }
    if underlay_armed {
        crate::utils::popup::arm_underlay_close();
    }
    focus_by_id(format!("{}__dd_trigger", other.id));
}

/// Read the most recent [`Menubar`] context.
pub fn use_menubar() -> Option<MenubarCtx> {
    use_context(menubar_context())
}

fn require_menubar(part: &str) -> MenubarCtx {
    use_menubar().unwrap_or_else(|| {
        panic!("{part} must be used within <Menubar> / menubar_scope");
    })
}

fn require_dropdown_in_menubar(part: &str) -> crate::components::dropdown_menu::DropdownMenuCtx {
    use_dropdown_menu().unwrap_or_else(|| {
        panic!("{part} must be used within <MenubarMenu> / menubar_menu");
    })
}
fn is_text_leaf(el: &Element) -> bool {
    el.children.is_empty() && el.text.as_ref().is_some_and(|t| !t.is_empty())
}

fn extract_text_leaf(el: &Element) -> String {
    crate::core::tree::first_text_leaf(el)
}

fn retag_slot(mut el: Element, slot: &str) -> Element {
    el.set_attr(ATTR_SLOT, slot);
    el
}

/// Chain `on_key_down`: try `handler` first; if it returns false, run previous.
fn chain_key_down(
    mut el: Element,
    handler: impl FnMut(winit::event::KeyEvent) -> bool + 'static,
) -> Element {
    let prev = el.on_key_down.clone();
    let mut handler = handler;
    el = el.on_key_down(move |ev| {
        if handler(ev.clone()) {
            return true;
        }
        if let Some(ref rc) = prev {
            return rc.borrow_mut()(ev);
        }
        false
    });
    el
}

/// Chain `on_hover` so Button hover-paint (`{focus_id}_hover`) keeps working.
fn chain_hover(mut el: Element, handler: impl FnMut(bool) + 'static) -> Element {
    let prev = el.on_hover.clone();
    let mut handler = handler;
    el = el.on_hover(move |h| {
        if let Some(ref rc) = prev {
            rc.borrow_mut()(h);
        }
        handler(h);
    });
    el
}

/// Ghost rest chrome captured after the trigger button is built (closed).
#[derive(Clone)]
struct MenubarTriggerRestChrome {
    hover: Option<crate::components::button::ButtonHoverPaint>,
    bg: [f32; 4],
    parent_fg: Option<[u8; 4]>,
    child_fg: Option<[u8; 4]>,
}

/// Open accent is paint-only: patch bg / label / `data-state` in place.
/// Clear [`crate::components::button::ButtonHoverPaint`] while open so frame sync cannot overwrite accent.
fn apply_menubar_trigger_open_chrome(
    el: &mut Element,
    is_open: bool,
    rest: &MenubarTriggerRestChrome,
) -> LayoutStatus {
    let want = if is_open { "open" } else { "closed" };
    let mut impact = LayoutStatus::Clean;
    if el.attrs.get("data-state").map(String::as_str) != Some(want) {
        el.set_attr("data-state", want);
        impact = impact.merge(LayoutStatus::StyleChanged);
    }
    if is_open {
        let accent_bg = parse_color("accent");
        let accent_fg = parse_text_color("accent-foreground");
        el.authoring_state
            .remove::<crate::components::button::ButtonHoverPaint>();
        el.owns_focus_paint = false;
        el.paint_caption_scope = false;
        el.disabled_sync = None;
        if el.style.bg_color != accent_bg {
            el.style.bg_color = accent_bg;
            impact = impact.merge(LayoutStatus::StyleChanged);
        }
        if el.text_style.color != Some(accent_fg) {
            el.text_style.color = Some(accent_fg);
            impact = impact.merge(LayoutStatus::StyleChanged);
        }
        for child in &mut el.children {
            if child.text.is_some() && child.text_style.color != Some(accent_fg) {
                child.text_style.color = Some(accent_fg);
                impact = impact.merge(LayoutStatus::StyleChanged);
            }
        }
    } else {
        if let Some(hover) = rest.hover.clone() {
            el.authoring_state.set(hover);
            super::button::configure_hover_presentation(el);
        }
        if el.style.bg_color != rest.bg {
            el.style.bg_color = rest.bg;
            impact = impact.merge(LayoutStatus::StyleChanged);
        }
        if el.text_style.color != rest.parent_fg {
            el.text_style.color = rest.parent_fg;
            impact = impact.merge(LayoutStatus::StyleChanged);
        }
        if let Some(fg) = rest.child_fg {
            for child in &mut el.children {
                if child.text.is_some() && child.text_style.color != Some(fg) {
                    child.text_style.color = Some(fg);
                    impact = impact.merge(LayoutStatus::StyleChanged);
                }
            }
        }
    }
    impact
}

// ---------------------------------------------------------------------------
// Scope
// ---------------------------------------------------------------------------

/// 提供 Menubar 上下文并构建顶栏壳（`ui!` `<Menubar>` / Builder 共用）。
///
/// solid-ui：`flex h-10 items-center space-x-1 rounded-md border bg-background p-1`。
pub fn menubar_scope(id: impl Into<String>, body: impl FnOnce() -> Element) -> Element {
    let id = resolve_instance_id(id, "menubar");
    reset_menus(&id);
    provide_context(menubar_context(), MenubarCtx { id: id.clone() });
    let kids = take_packed(body());
    let mut row = hstack()
        .h("2.5rem")
        .items_center()
        .gap("0.25rem")
        .p("0.25rem")
        .rounded("0.375rem")
        .w("auto")
        .flex_shrink(0.0)
        .flex_grow(0.0);
    for k in kids {
        row = row.child(k);
    }
    let mut el: Element = row.into();
    el = el
        .bg(tokens::background())
        .border_style("solid")
        .border_width(1.0)
        .border_color(tokens::border())
        // Hug content inside stretch parents (gallery VStack).
        .align_self_kw("start");
    el.set_attr(ATTR_SLOT, SLOT_MENUBAR);
    if el.template_id.is_none() {
        el.template_id = Some(format!("{id}__menubar"));
    }
    el.accesskit_role("menubar")
}

/// 单段菜单：注册到 Menubar，并提供 [`dropdown_menu_scope`]。
pub fn menubar_menu(
    id: Option<String>,
    open: Option<Signal<bool>>,
    body: impl FnOnce() -> Element,
) -> Element {
    let bar = require_menubar("MenubarMenu");
    let seq = next_menu_seq(&bar.id);
    let menu_id = match id.filter(|s| !s.trim().is_empty()) {
        Some(s) => {
            // Namespace under bar so short ids ("file") cannot collide on the overlay stack.
            if s.starts_with(&format!("{}__", bar.id)) || s == bar.id {
                resolve_instance_id(s, "menubar_menu")
            } else {
                format!("{}__{}", bar.id, s)
            }
        }
        None => format!("{}__menu{seq}", bar.id),
    };
    let open = open.unwrap_or_else(|| use_signal(format!("{menu_id}__dd_open").as_str(), || false));
    register_menu(
        &bar.id,
        MenuReg {
            id: menu_id.clone(),
            open,
        },
    );
    let mut root = dropdown_menu_scope(menu_id.clone(), Some(open), body);
    root.set_attr(ATTR_SLOT, SLOT_MENU);
    root.set_attr("jadix_menubar_id", &bar.id);
    root.set_attr("jadix_menubar_menu_ix", seq.to_string());
    if root.template_id.is_none() {
        root.template_id = Some(format!("{menu_id}__menubar_menu"));
    }
    root
}

/// `MenubarContent` 选项（`ui!` / [`menubar_content`] 共用）。
#[derive(Clone)]
pub struct MenubarContentOpts {
    /// `bottom-start` / …（默认 `bottom-start`；ShadCN align=start）。
    pub placement: String,
    /// Panel chrome (RSX `class` / Builder `.class()` / `.style()`).
    pub panel: crate::components::base::SlotChrome,
}

impl Default for MenubarContentOpts {
    fn default() -> Self {
        Self {
            placement: "bottom-start".into(),
            panel: Default::default(),
        }
    }
}

impl MenubarContentOpts {
    crate::impl_content_opts_chrome!(panel);
}

/// solid-ui / shadcn `MenubarContent`: reuse DropdownMenuContent (gap≈8); expand animation.
pub fn menubar_content(opts: MenubarContentOpts, body: impl Fn() -> Element + 'static) -> Element {
    let _ = require_dropdown_in_menubar("MenubarContent");
    let el = dropdown_menu_content(
        DropdownMenuContentOpts {
            placement: if opts.placement.trim().is_empty() {
                "bottom-start".into()
            } else {
                opts.placement
            },
            panel: opts.panel,
            motion: crate::utils::floating_kind::FloatingMotionKind::Expand,
        },
        body,
    );
    retag_slot(el, SLOT_CONTENT)
}

/// Nested submenu (reuse dropdown_menu_sub).
pub fn menubar_sub(id: Option<String>, body: impl FnOnce() -> Element) -> Element {
    let _ = require_dropdown_in_menubar("MenubarSub");
    dropdown_menu_sub(id, body)
}

/// Submenu content (expansion animation).
pub fn menubar_sub_content(
    chrome: crate::components::base::SlotChrome,
    body: impl Fn() -> Element + 'static,
) -> Element {
    let _ = require_dropdown_in_menubar("MenubarSubContent");
    // Keep dropdown-menu-sub-content slot / placeholder identity for floating sync.
    crate::components::dropdown_menu::dropdown_menu_sub_content_ex(
        chrome,
        crate::utils::floating_kind::FloatingMotionKind::Expand,
        body,
    )
}

/// Radio group (reuse dropdown_menu_radio_group).
pub fn menubar_radio_group(
    id: Option<String>,
    value: Option<Signal<String>>,
    default_value: Option<String>,
    body: impl FnOnce() -> Element,
) -> Element {
    let _ = require_dropdown_in_menubar("MenubarRadioGroup");
    dropdown_menu_radio_group(id, value, default_value, body)
}

// ---------------------------------------------------------------------------
// Trigger
// ---------------------------------------------------------------------------

crate::__component_contract! {
MenubarTriggerProps [];
/// solid-ui `MenubarTrigger`：`px-3 py-1.5 text-sm font-medium`；open → accent。
#[allow(non_snake_case)]
pub fn MenubarTrigger(
identity [] id: String,
plain [] children: Option<Element>,
) -> Element {
    let bar = require_menubar("MenubarTrigger");
    let ctx = require_dropdown_in_menubar("MenubarTrigger");
    let open = ctx.open;
    let active = ctx.active;
    let menu_id = ctx.id.clone();
    let bar_id = bar.id.clone();
    let trigger_id = if id.is_empty() {
        format!("{menu_id}__dd_trigger")
    } else {
        id
    };
    ctx.trigger_fid.set(trigger_id.clone());

    let label_text = match &children {
        None => "Menu".to_string(),
        Some(c) if is_text_leaf(c) => c.text.clone().unwrap_or_else(|| "Menu".into()),
        Some(c) => {
            let t = extract_text_leaf(c);
            if t.is_empty() {
                "Menu".into()
            } else {
                t
            }
        }
    };
    let _ = children;
    let trigger_fid_sig = ctx.trigger_fid;
    // Capture menu_id for live registry lookup (siblings may register after this Trigger).
    let menu_id_for_ix = menu_id.clone();
    let open_t = open;
    let active_t = active;
    let toggle = move || {
        active_t.set(usize::MAX);
        let next = !open_t.peek();
        if next {
            disarm_underlay_close();
        }
        open_t.set(next);
    };

    let mut trigger: Element = button(&trigger_id)
        .variant("ghost")
        .size("sm")
        .on_click(toggle)
        .text(label_text)
        .into();
    // solid-ui: rounded-sm px-3 py-1.5; open accent patches in place (WP-N.4).
    trigger = trigger.px("0.75rem").py("0.375rem").rounded("0.125rem");

    // Hover-open when another menu in this bar is already open.
    // Chain — do not replace Button's hover handler (ghost accent paint).
    let open_h = open;
    let bar_h = bar_id.clone();
    trigger = chain_hover(trigger, move |h| {
        if !h || open_h.peek() {
            return;
        }
        if any_menu_open(&bar_h) {
            // Peer switch: keep underlay armed (portal_sync also skips disarm).
            // A rapidly revisited menu can still have an exiting portal entry;
            // portal_sync updates that entry without applying peer exclusivity.
            let underlay_armed = crate::utils::popup::underlay_close_is_armed();
            crate::core::reactivity::batch(|| {
                for menu in menus_of(&bar_h) {
                    if menu.open.is_alive() {
                        menu.open.set(false);
                    }
                }
                open_h.set(true);
            });
            if underlay_armed {
                crate::utils::popup::arm_underlay_close();
            }
        }
    });

    // Dropdown ↑↓ / Enter / submenu ←→ when open, then menubar peer ←→ / closed roving.
    trigger = attach_dropdown_menu_trigger_keys(trigger, &ctx);
    let open_keys = open;
    let bar_keys = bar_id.clone();
    let menu_id_keys = menu_id.clone();
    let menu_id_ix = menu_id_for_ix;
    let active_keys = ctx.active;
    trigger = chain_key_down(trigger, move |ev| {
        if ev.state != ElementState::Pressed || ev.repeat {
            return false;
        }
        let menus = menus_of(&bar_keys);
        let menu_ix = menus.iter().position(|m| m.id == menu_id_ix).unwrap_or(0);
        let trigger_ids: Vec<String> = menus
            .iter()
            .map(|m| format!("{}__dd_trigger", m.id))
            .collect();
        let n_sec = menus.len();
        if !open_keys.peek() {
            return focus_roving_key(&ev, &trigger_ids, menu_ix, FocusRovingAxis::Horizontal);
        }
        match &ev.logical_key {
            Key::Named(NamedKey::ArrowRight) if n_sec > 0 => {
                // Nested submenu open, or highlight on a SubTrigger → dropdown keys.
                if dropdown_menu_open_sub_id(&menu_id_keys).is_some()
                    || dropdown_menu_active_is_sub_trigger(&menu_id_keys, active_keys.peek())
                {
                    return false;
                }
                open_neighbor_menu(&bar_keys, menu_ix, 1);
                true
            }
            Key::Named(NamedKey::ArrowLeft) if n_sec > 0 => {
                if dropdown_menu_open_sub_id(&menu_id_keys).is_some() {
                    return false;
                }
                open_neighbor_menu(&bar_keys, menu_ix, -1);
                true
            }
            Key::Named(NamedKey::Escape) => {
                open_keys.set(false);
                focus_by_id(format!("{}__dd_trigger", menu_id_keys));
                true
            }
            // Let attach_dropdown_menu_trigger_keys handle ↑↓ / Enter / Home / End / submenu.
            _ => false,
        }
    });

    if trigger.authoring_state.get::<crate::components::button::ButtonHoverPaint>().is_none() && trigger.input_paint_sync.is_none() {
        trigger.set_focusable(true);
    }
    if trigger.focus_id.is_none() {
        trigger = trigger.focus_id(trigger_id.as_str());
    }
    trigger_fid_sig.set(
        trigger
            .focus_id
            .clone()
            .unwrap_or_else(|| trigger_id.clone()),
    );
    trigger.set_attr(ATTR_SLOT, SLOT_TRIGGER);
    trigger = trigger.accesskit_role("menuitem");

    let rest = MenubarTriggerRestChrome {
        hover: trigger.authoring_state.get::<crate::components::button::ButtonHoverPaint>().cloned(),
        bg: trigger.style.bg_color,
        parent_fg: trigger.text_style.color,
        child_fg: trigger
            .children
            .iter()
            .find(|c| c.text.is_some())
            .and_then(|c| c.text_style.color)
            .or(trigger.text_style.color),
    };
    bind_prop(
        trigger,
        format!("{trigger_id}__open_chrome"),
        move || open.get(),
        move |el, is_open| apply_menubar_trigger_open_chrome(el, is_open, &rest),
    )
}
}

// ---------------------------------------------------------------------------
// Item / Label / Separator / Shortcut / Group / Checkbox / Radio / SubTrigger
// ---------------------------------------------------------------------------

crate::__component_contract! {
MenubarItemProps [];
/// solid-ui/shadcn `MenubarItem` (underlying [`DropdownMenuItem`]).
///
/// 保留 `dropdown-menu-item` data-slot，供 [`crate::authoring::presentation`] 写回悬停高亮。
#[allow(non_snake_case)]
pub fn MenubarItem(
default [] disabled: bool,
default [] inset: bool,
plain [] variant: Option<String>,
default [] on_click: Handler,
plain [] children: Option<Element>,
) -> Element {
    let _ = require_dropdown_in_menubar("MenubarItem");
    let mut el = DropdownMenuItem(DropdownMenuItemProps {
        disabled: Some(disabled),
        inset: Some(inset),
        variant,
        on_click: Some(on_click),
        children,
        base: None,
    });
    // Keep dropdown-menu-* slot for paint sync; annotate menubar for a11y/debug.
    el.set_attr("data-menubar-part", "item");
    el
}
}

crate::__component_contract! {
MenubarCheckboxItemProps [];
/// solid-ui / shadcn `MenubarCheckboxItem`。
#[allow(non_snake_case)]
pub fn MenubarCheckboxItem(
plain [] checked: Option<Signal<bool>>,
default [] default_checked: bool,
default [] disabled: bool,
default [] inset: bool,
default [] on_click: Handler,
plain [] children: Option<Element>,
) -> Element {
    let _ = require_dropdown_in_menubar("MenubarCheckboxItem");
    let mut el = DropdownMenuCheckboxItem(DropdownMenuCheckboxItemProps {
        checked,
        default_checked: Some(default_checked),
        disabled: Some(disabled),
        inset: Some(inset),
        on_click: Some(on_click),
        children,
        base: None,
    });
    el.set_attr("data-menubar-part", "checkbox-item");
    el
}
}

crate::__component_contract! {
MenubarRadioItemProps [];
/// solid-ui / shadcn `MenubarRadioItem`。
#[allow(non_snake_case)]
pub fn MenubarRadioItem(
default [] value: String,
default [] disabled: bool,
default [] inset: bool,
default [] on_click: Handler,
plain [] children: Option<Element>,
) -> Element {
    let _ = require_dropdown_in_menubar("MenubarRadioItem");
    let mut el = DropdownMenuRadioItem(DropdownMenuRadioItemProps {
        value: Some(value),
        disabled: Some(disabled),
        inset: Some(inset),
        on_click: Some(on_click),
        children,
        base: None,
    });
    el.set_attr("data-menubar-part", "radio-item");
    el
}
}

crate::__component_contract! {
MenubarLabelProps [];
/// solid-ui / shadcn `MenubarLabel`。
#[allow(non_snake_case)]
pub fn MenubarLabel(
default [] inset: bool,
plain [] children: Option<Element>,
) -> Element {
    let _ = require_dropdown_in_menubar("MenubarLabel");
    retag_slot(
        DropdownMenuLabel(DropdownMenuLabelProps {
            inset: Some(inset),
            children,
            base: None,
        }),
        SLOT_LABEL,
    )
}
}

crate::__component_contract! {
MenubarSeparatorProps [];
/// solid-ui / shadcn `MenubarSeparator`。
#[allow(non_snake_case)]
pub fn MenubarSeparator(
plain [] children: Option<Element>,
) -> Element {
    let _ = require_dropdown_in_menubar("MenubarSeparator");
    retag_slot(
        DropdownMenuSeparator(DropdownMenuSeparatorProps {
            children,
            base: None,
        }),
        SLOT_SEPARATOR,
    )
}
}

crate::__component_contract! {
MenubarShortcutProps [];
/// solid-ui / shadcn `MenubarShortcut`。
#[allow(non_snake_case)]
pub fn MenubarShortcut(
plain [] children: Option<Element>,
) -> Element {
    let _ = require_dropdown_in_menubar("MenubarShortcut");
    retag_slot(
        DropdownMenuShortcut(DropdownMenuShortcutProps {
            children,
            base: None,
        }),
        SLOT_SHORTCUT,
    )
}
}

crate::__component_contract! {
MenubarGroupProps [];
/// solid-ui / shadcn `MenubarGroup`。
#[allow(non_snake_case)]
pub fn MenubarGroup(
plain [] children: Option<Element>,
) -> Element {
    let _ = require_dropdown_in_menubar("MenubarGroup");
    retag_slot(
        DropdownMenuGroup(DropdownMenuGroupProps {
            children,
            base: None,
        }),
        SLOT_GROUP,
    )
}
}

crate::__component_contract! {
MenubarSubTriggerProps [];
/// solid-ui / shadcn `MenubarSubTrigger`。
///
/// 必须保留 `dropdown-menu-sub-trigger` slot，悬停打开与高亮 paint 都依赖它。
#[allow(non_snake_case)]
pub fn MenubarSubTrigger(
default [] disabled: bool,
default [] inset: bool,
plain [] children: Option<Element>,
) -> Element {
    let _ = require_dropdown_in_menubar("MenubarSubTrigger");
    let mut el = DropdownMenuSubTrigger(DropdownMenuSubTriggerProps {
        disabled: Some(disabled),
        inset: Some(inset),
        children,
        base: None,
    });
    el.set_attr("data-menubar-part", "sub-trigger");
    el
}
}

impl_simple_slot_builder!(
    MenubarTriggerBuilder,
    menubar_trigger,
    MenubarTrigger,
    MenubarTriggerProps
);
impl_simple_slot_builder!(
    MenubarItemBuilder,
    menubar_item,
    MenubarItem,
    MenubarItemProps
);
impl_simple_slot_builder!(
    MenubarLabelBuilder,
    menubar_label,
    MenubarLabel,
    MenubarLabelProps
);
impl_simple_slot_builder!(
    MenubarSeparatorBuilder,
    menubar_separator,
    MenubarSeparator,
    MenubarSeparatorProps
);
impl_simple_slot_builder!(
    MenubarShortcutBuilder,
    menubar_shortcut,
    MenubarShortcut,
    MenubarShortcutProps
);
impl_simple_slot_builder!(
    MenubarGroupBuilder,
    menubar_group,
    MenubarGroup,
    MenubarGroupProps
);

pub struct MenubarContentSlotBuilder {
    opts: MenubarContentOpts,
}

pub fn menubar_content_slot() -> MenubarContentSlotBuilder {
    MenubarContentSlotBuilder {
        opts: MenubarContentOpts::default(),
    }
}

impl MenubarContentSlotBuilder {
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
        menubar_content(self.opts, body)
    }
}

// ---------------------------------------------------------------------------
//Builder (double-write convenient entry)
// ---------------------------------------------------------------------------

/// Builder / 便捷 API 用的数据项（RSX 请用组件 [`MenubarItem`]）。
#[derive(Clone)]
pub struct MenuItem {
    pub id: String,
    pub label: String,
    pub on_select: Rc<dyn Fn()>,
}

impl MenuItem {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        on_select: impl Fn() + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            on_select: Rc::new(on_select),
        }
    }
}

#[derive(Clone)]
pub struct MenuSection {
    pub id: String,
    pub title: String,
    pub items: Vec<MenuItem>,
}

impl MenuSection {
    pub fn new(id: impl Into<String>, title: impl Into<String>, items: Vec<MenuItem>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            items,
        }
    }
}

pub struct MenubarBuilder {
    id: String,
    sections: Vec<MenuSection>,
    chrome: SlotChrome,
}

/// Builder 入口：sections → [`menubar_scope`] + [`menubar_menu`]。
pub fn menubar(id: impl Into<String>) -> MenubarBuilder {
    MenubarBuilder {
        id: id.into(),
        sections: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl MenubarBuilder {
    impl_slot_chrome_methods!();

    pub fn sections(mut self, sections: Vec<MenuSection>) -> Self {
        self.sections = sections;
        self
    }
    pub fn section(mut self, section: MenuSection) -> Self {
        self.sections.push(section);
        self
    }
    /// Build Menu / Trigger / Content subtrees inside the context.
    pub fn child(self, body: impl FnOnce() -> Element) -> Element {
        self.chrome.apply(menubar_scope(self.id, body))
    }
}

impl From<MenubarBuilder> for Element {
    fn from(builder: MenubarBuilder) -> Self {
        let MenubarBuilder {
            id,
            sections,
            chrome,
        } = builder;
        let el = menubar_scope(id, move || {
            let mut menus = Vec::with_capacity(sections.len());
            for section in sections {
                let title = section.title.clone();
                let items = section.items;
                let menu_id = section.id.clone();
                menus.push(menubar_menu(Some(menu_id), None, move || {
                    let trigger = MenubarTrigger(MenubarTriggerProps {
                        id: String::new(),
                        children: Some(Element::new().text(title)),
                        base: None,
                    });
                    let content = menubar_content(MenubarContentOpts::default(), move || {
                        let mut col = crate::primitives::r#box::vstack().w("100%").gap("0");
                        for it in &items {
                            let cb = it.on_select.clone();
                            let row = MenubarItem(MenubarItemProps {
                                disabled: Some(false),
                                inset: Some(false),
                                variant: None,
                                on_click: Some(Handler::new(move |_| cb())),
                                children: Some(Element::new().text(it.label.clone())),
                                base: None,
                            });
                            col = col.child(row);
                        }
                        col.into()
                    });
                    let mut frag = Element::new();
                    frag.set_attr("jadix_fragment", "1");
                    frag.children(vec![trigger, content])
                }));
            }
            let mut frag = Element::new();
            frag.set_attr("jadix_fragment", "1");
            frag.children(menus)
        });
        chrome.apply(el)
    }
}

impl crate::core::tree::IntoElementList for MenubarBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/menubar.rs"]
mod tests;
