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

use rui_macros::component;
use winit::event::ElementState;
use winit::keyboard::{Key, NamedKey};

use crate::app::reactive::reactive_view;
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
use crate::core::tree::{Element, Handler};
use crate::primitives::r#box::hstack;
use crate::style::{parse_color, parse_text_color, with_class_list};
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
    root.set_attr("rui_menubar_id", &bar.id);
    root.set_attr("rui_menubar_menu_ix", seq.to_string());
    if root.template_id.is_none() {
        root.template_id = Some(format!("{menu_id}__menubar_menu"));
    }
    root
}

/// `MenubarContent` 选项（`ui!` / [`menubar_content`] 共用）。
#[derive(Clone, Debug)]
pub struct MenubarContentOpts {
    /// `bottom-start` / …（默认 `bottom-start`；ShadCN align=start）。
    pub placement: String,
    pub class: Option<String>,
}

impl Default for MenubarContentOpts {
    fn default() -> Self {
        Self {
            placement: "bottom-start".into(),
            class: None,
        }
    }
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
            class: opts.class,
            motion: crate::components::internal::floating_motion::FloatingMotionKind::Expand,
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
pub fn menubar_sub_content(class: Option<String>, body: impl Fn() -> Element + 'static) -> Element {
    let _ = require_dropdown_in_menubar("MenubarSubContent");
    // Keep dropdown-menu-sub-content slot / placeholder identity for floating sync.
    crate::components::dropdown_menu::dropdown_menu_sub_content_ex(
        class,
        crate::components::internal::floating_motion::FloatingMotionKind::Expand,
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

/// solid-ui `MenubarTrigger`：`px-3 py-1.5 text-sm font-medium`；open → accent。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn MenubarTrigger(id: String, class: Option<String>, children: Option<Element>) -> Element {
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
    let class = class.filter(|s| !s.trim().is_empty());
    let view_id = format!("{trigger_id}__mb_trig_view");
    let trigger_fid_sig = ctx.trigger_fid;
    // Capture menu_id for live registry lookup (siblings may register after this Trigger).
    let menu_id_for_ix = menu_id.clone();

    reactive_view(view_id, move || {
        let is_open = open.get();
        let open_t = open;
        let active_t = active;
        let menu_id_t = menu_id.clone();
        let bar_id_t = bar_id.clone();
        let class_k = class.clone();
        let menu_id_ix = menu_id_for_ix.clone();
        let toggle = {
            let open_t = open_t;
            let active_t = active_t;
            move || {
                active_t.set(usize::MAX);
                let next = !open_t.peek();
                if next {
                    disarm_underlay_close();
                }
                open_t.set(next);
            }
        };

        let mut trigger: Element = button(&trigger_id)
            .variant("ghost")
            .size("sm")
            .on_click(toggle)
            .text(label_text.clone())
            .into();
        // solid-ui: rounded-sm px-3 py-1.5; open → accent (disable button hover paint so sync won't overwrite).
        trigger = trigger.px("0.75rem").py("0.375rem").rounded("0.125rem");
        if is_open {
            trigger.button_hover_paint = None;
            trigger = trigger
                .bg(parse_color("accent"))
                .text_color(parse_text_color("accent-foreground"));
        }

        if let Some(ref c) = class_k {
            trigger = with_class_list(trigger, c);
        }

        // Hover-open when another menu in this bar is already open.
        // Chain — do not replace Button's hover handler (ghost accent paint).
        let open_h = open;
        let bar_h = bar_id_t.clone();
        trigger = chain_hover(trigger, move |h| {
            if !h || open_h.peek() {
                return;
            }
            if any_menu_open(&bar_h) {
                // Peer switch: keep underlay armed (portal_sync also skips disarm).
                open_h.set(true);
            }
        });

        // Dropdown ↑↓ / Enter / submenu ←→ when open, then menubar peer ←→ / closed roving.
        trigger = attach_dropdown_menu_trigger_keys(trigger, &ctx);
        let open_keys = open;
        let bar_keys = bar_id_t.clone();
        let menu_id_keys = menu_id_t.clone();
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

        if trigger.button_hover_paint.is_none() && trigger.input_paint_sync.is_none() {
            trigger.focusable = true;
            trigger.focus_ring = true;
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
        trigger.set_attr("data-state", if is_open { "open" } else { "closed" });
        trigger.accesskit_role("menuitem")
    })
}

// ---------------------------------------------------------------------------
// Item / Label / Separator / Shortcut / Group / Checkbox / Radio / SubTrigger
// ---------------------------------------------------------------------------

/// solid-ui/shadcn `MenubarItem` (underlying [`DropdownMenuItem`]).
///
/// 保留 `dropdown-menu-item` data-slot，供 [`sync_dropdown_menu_item_paint_subtree`] 写回悬停高亮。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn MenubarItem(
    disabled: bool,
    inset: bool,
    variant: Option<String>,
    class: Option<String>,
    #[prop(default = "Handler::default()")] on_click: Handler,
    children: Option<Element>,
) -> Element {
    let _ = require_dropdown_in_menubar("MenubarItem");
    let mut el = DropdownMenuItem(DropdownMenuItemProps {
        disabled: Some(disabled),
        inset: Some(inset),
        variant,
        class,
        on_click: Some(on_click),
        children,
        base: None,
    });
    // Keep dropdown-menu-* slot for paint sync; annotate menubar for a11y/debug.
    el.set_attr("data-menubar-part", "item");
    el
}

/// solid-ui / shadcn `MenubarCheckboxItem`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn MenubarCheckboxItem(
    checked: Option<Signal<bool>>,
    default_checked: bool,
    disabled: bool,
    inset: bool,
    class: Option<String>,
    #[prop(default = "Handler::default()")] on_click: Handler,
    children: Option<Element>,
) -> Element {
    let _ = require_dropdown_in_menubar("MenubarCheckboxItem");
    let mut el = DropdownMenuCheckboxItem(DropdownMenuCheckboxItemProps {
        checked,
        default_checked: Some(default_checked),
        disabled: Some(disabled),
        inset: Some(inset),
        class,
        on_click: Some(on_click),
        children,
        base: None,
    });
    el.set_attr("data-menubar-part", "checkbox-item");
    el
}

/// solid-ui / shadcn `MenubarRadioItem`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn MenubarRadioItem(
    value: String,
    disabled: bool,
    inset: bool,
    class: Option<String>,
    #[prop(default = "Handler::default()")] on_click: Handler,
    children: Option<Element>,
) -> Element {
    let _ = require_dropdown_in_menubar("MenubarRadioItem");
    let mut el = DropdownMenuRadioItem(DropdownMenuRadioItemProps {
        value: Some(value),
        disabled: Some(disabled),
        inset: Some(inset),
        class,
        on_click: Some(on_click),
        children,
        base: None,
    });
    el.set_attr("data-menubar-part", "radio-item");
    el
}

/// solid-ui / shadcn `MenubarLabel`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn MenubarLabel(inset: bool, children: Option<Element>) -> Element {
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

/// solid-ui / shadcn `MenubarSeparator`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn MenubarSeparator(children: Option<Element>) -> Element {
    let _ = require_dropdown_in_menubar("MenubarSeparator");
    retag_slot(
        DropdownMenuSeparator(DropdownMenuSeparatorProps {
            children,
            base: None,
        }),
        SLOT_SEPARATOR,
    )
}

/// solid-ui / shadcn `MenubarShortcut`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn MenubarShortcut(children: Option<Element>) -> Element {
    let _ = require_dropdown_in_menubar("MenubarShortcut");
    retag_slot(
        DropdownMenuShortcut(DropdownMenuShortcutProps {
            children,
            base: None,
        }),
        SLOT_SHORTCUT,
    )
}

/// solid-ui / shadcn `MenubarGroup`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn MenubarGroup(children: Option<Element>) -> Element {
    let _ = require_dropdown_in_menubar("MenubarGroup");
    retag_slot(
        DropdownMenuGroup(DropdownMenuGroupProps {
            children,
            base: None,
        }),
        SLOT_GROUP,
    )
}

/// solid-ui / shadcn `MenubarSubTrigger`。
///
/// 必须保留 `dropdown-menu-sub-trigger` slot，悬停打开与高亮 paint 都依赖它。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn MenubarSubTrigger(
    disabled: bool,
    inset: bool,
    class: Option<String>,
    children: Option<Element>,
) -> Element {
    let _ = require_dropdown_in_menubar("MenubarSubTrigger");
    let mut el = DropdownMenuSubTrigger(DropdownMenuSubTriggerProps {
        disabled: Some(disabled),
        inset: Some(inset),
        class,
        children,
        base: None,
    });
    el.set_attr("data-menubar-part", "sub-trigger");
    el
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
}

/// Builder 入口：sections → [`menubar_scope`] + [`menubar_menu`]。
pub fn menubar(id: impl Into<String>) -> MenubarBuilder {
    MenubarBuilder {
        id: id.into(),
        sections: Vec::new(),
    }
}

impl MenubarBuilder {
    pub fn sections(mut self, sections: Vec<MenuSection>) -> Self {
        self.sections = sections;
        self
    }
    pub fn section(mut self, section: MenuSection) -> Self {
        self.sections.push(section);
        self
    }
}

impl From<MenubarBuilder> for Element {
    fn from(builder: MenubarBuilder) -> Self {
        let MenubarBuilder { id, sections } = builder;
        menubar_scope(id, move || {
            let mut menus = Vec::with_capacity(sections.len());
            for section in sections {
                let title = section.title.clone();
                let items = section.items;
                let menu_id = section.id.clone();
                menus.push(menubar_menu(Some(menu_id), None, move || {
                    let trigger = MenubarTrigger(MenubarTriggerProps {
                        id: String::new(),
                        class: None,
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
                                class: None,
                                on_click: Some(Handler::new(move |_| cb())),
                                children: Some(Element::new().text(it.label.clone())),
                                base: None,
                            });
                            col = col.child(row);
                        }
                        col.into()
                    });
                    let mut frag = Element::new();
                    frag.set_attr("rui_fragment", "1");
                    frag.children(vec![trigger, content])
                }));
            }
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            frag.children(menus)
        })
    }
}

impl crate::core::tree::IntoElementList for MenubarBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menubar_scope_exposes_template() {
        let el = menubar_scope("mb-scope", || {
            menubar_menu(Some("file".into()), None, || {
                let trigger = MenubarTrigger(MenubarTriggerProps {
                    id: String::new(),
                    class: None,
                    children: Some(Element::new().text("File")),
                    base: None,
                });
                let content = menubar_content(MenubarContentOpts::default(), || {
                    MenubarItem(MenubarItemProps {
                        disabled: Some(false),
                        inset: Some(false),
                        variant: None,
                        class: None,
                        on_click: None,
                        children: Some(Element::new().text("New")),
                        base: None,
                    })
                });
                let mut frag = Element::new();
                frag.set_attr("rui_fragment", "1");
                frag.children(vec![trigger, content])
            })
        });
        assert_eq!(
            el.template_id.as_deref(),
            Some("mb-scope__menubar"),
            "scope must expose a stable menubar template id"
        );
        assert_eq!(
            el.attrs.get("data-slot").map(String::as_str),
            Some("menubar")
        );
    }

    #[test]
    fn builder_builds_compound() {
        let el: Element = menubar("mb-b")
            .section(MenuSection::new(
                "file",
                "File",
                vec![MenuItem::new("n", "New", || {})],
            ))
            .into();
        assert!(el.template_id.as_deref().is_some());
    }

    /// Gallery symptom proxy: namespaced menubar parent open + sub_open must leave
    /// parent open and push a nested Menu overlay (SubContent `portal_sync` path).
    #[test]
    fn menubar_sub_open_keeps_parent_and_pushes_sub_overlay() {
        use std::cell::RefCell;
        use std::rc::Rc;

        use crate::app::portal_overlay::{clear_overlay_stack, overlay_stack_depth_for_test};
        use crate::app::reactive::{registered_anchor_count, reset_registered_anchors};
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::core::reactivity::{create_effect, create_root, use_signal};

        clear_overlay_stack();
        reset_registered_anchors();
        clear_test_reconcile_root();

        let root: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(Rc::clone(&root));

        let (handle, parent_open) = create_root(|| {
            let parent_open = use_signal("mb_sub_loop_bar__file__dd_open", || false);
            let root_e = Rc::clone(&root);
            create_effect(move || {
                let el = menubar_scope("mb_sub_loop_bar", || {
                    menubar_menu(Some("file".into()), Some(parent_open), || {
                        let trigger = MenubarTrigger(MenubarTriggerProps {
                            id: String::new(),
                            class: None,
                            children: Some(Element::new().text("File")),
                            base: None,
                        });
                        let content = menubar_content(MenubarContentOpts::default(), || {
                            menubar_sub(None, || {
                                let st = MenubarSubTrigger(MenubarSubTriggerProps {
                                    disabled: Some(false),
                                    inset: Some(false),
                                    class: None,
                                    children: Some(Element::new().text("Share")),
                                    base: None,
                                });
                                let sc = menubar_sub_content(None, || {
                                    MenubarItem(MenubarItemProps {
                                        disabled: Some(false),
                                        inset: Some(false),
                                        variant: None,
                                        class: None,
                                        on_click: None,
                                        children: Some(Element::new().text("Email")),
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
                    })
                });
                *root_e.borrow_mut() = el;
            });
            parent_open
        });

        let anchors_before_open = registered_anchor_count();
        let depth_before = overlay_stack_depth_for_test();
        parent_open.set(true);
        let anchors_after_open = registered_anchor_count();
        let depth_after_open = overlay_stack_depth_for_test();
        assert!(parent_open.peek(), "parent_open.set(true) must stick");
        assert!(
            depth_after_open > depth_before,
            "Content open must portal_sync parent Menu (before={depth_before} after={depth_after_open}); anchors {anchors_before_open}->{anchors_after_open}"
        );
        assert!(
            anchors_after_open > anchors_before_open,
            "opening menubar content must register SubContent reactive_anchor (before={anchors_before_open} after={anchors_after_open}, depth={depth_after_open})"
        );

        // Gallery path: layout hoists the Content portal before the user can hover Share.
        {
            let mut tree = root.borrow_mut();
            let _ = crate::layout::layout_tree::hoist_portal_subtrees(&mut tree);
        }

        let sub_anchor_after_hoist = {
            let tree = root.borrow();
            fn find_tid(el: &Element, want: &str) -> bool {
                if el.template_id.as_deref() == Some(want) {
                    return true;
                }
                el.children.iter().any(|c| find_tid(c, want))
            }
            find_tid(&tree, "mb_sub_loop_bar__file__sub0__view")
        };
        assert!(
            sub_anchor_after_hoist,
            "SubContent placeholder must remain findable after Content portal hoist"
        );

        let sub_trigger_fid = {
            let tree = root.borrow();
            fn find_sub_trigger_fid(el: &Element) -> Option<String> {
                if el.attrs.get("data-slot").map(String::as_str)
                    == Some("dropdown-menu-sub-trigger")
                {
                    return el.focus_id.clone();
                }
                for c in &el.children {
                    if let Some(f) = find_sub_trigger_fid(c) {
                        return Some(f);
                    }
                }
                None
            }
            find_sub_trigger_fid(&tree)
        };
        assert_eq!(
            sub_trigger_fid.as_deref(),
            Some("mb_sub_loop_bar__file__sub0__trigger"),
            "MenubarSubTrigger must expose the floating trigger focus_id after hoist"
        );

        let sub_open = use_signal("mb_sub_loop_bar__file__sub0__open", || false);
        assert!(!sub_open.peek(), "sub starts closed after parent open");
        sub_open.set(true);

        assert!(
            parent_open.peek(),
            "parent must stay open after sub opens (portal must not exclusive-dismiss)"
        );
        assert!(
            sub_open.peek(),
            "sub open signal must remain true after SubContent portal_sync"
        );
        assert!(
            overlay_stack_depth_for_test() >= 2,
            "SubContent open must push nested Menu overlay (got depth {})",
            overlay_stack_depth_for_test()
        );

        // Nested sub portal must hoist out of the parent menu (same as Select-in-Dialog).
        {
            let mut tree = root.borrow_mut();
            let _ = crate::layout::layout_tree::hoist_portal_subtrees(&mut tree);
        }

        let (has_sub_panel, sub_on_portal_layer, layer_len) = {
            let tree = root.borrow();
            fn find_slot(el: &Element, slot: &str) -> bool {
                if el.attrs.get("data-slot").map(String::as_str) == Some(slot) {
                    return true;
                }
                el.children.iter().any(|c| find_slot(c, slot))
            }
            let layer = tree.children.iter().find(|c| {
                c.template_id.as_deref() == Some(crate::layout::layout_tree::PORTAL_LAYER_ID)
            });
            let layer_len = layer.map(|l| l.children.len()).unwrap_or(0);
            let sub_on_layer = layer.is_some_and(|l| {
                l.children
                    .iter()
                    .any(|c| find_slot(c, "dropdown-menu-sub-content"))
            });
            (
                find_slot(&tree, "dropdown-menu-sub-content"),
                sub_on_layer,
                layer_len,
            )
        };
        assert!(
            has_sub_panel,
            "SubContent open must reconcile a dropdown-menu-sub-content panel into the tree"
        );
        assert!(
            sub_on_portal_layer,
            "submenu must be a portal-layer sibling after hoist (layer_len={layer_len}); \
             staying under parent overflow:hidden clips RightStart panels — gallery Share symptom"
        );
        assert!(
            layer_len >= 2,
            "parent File menu + Share submenu must both live on the portal layer (got {layer_len})"
        );

        // Stuck-open recovery: clear panel chrome under the live `{sid}__view` island
        // without dropping its UnmountHook (that would dispose the SubContent effect).
        {
            let mut tree = root.borrow_mut();
            fn hollow_sub_view(el: &mut Element, want: &str) -> bool {
                if el.template_id.as_deref() == Some(want) {
                    el.children.clear();
                    el.attrs.remove("data-slot");
                    el.text = None;
                    return true;
                }
                for c in &mut el.children {
                    if hollow_sub_view(c, want) {
                        return true;
                    }
                }
                false
            }
            assert!(
                hollow_sub_view(&mut tree, "mb_sub_loop_bar__file__sub0__view"),
                "must find sub view island"
            );
        }
        let remount = use_signal("mb_sub_loop_bar__file__sub0__remount", || 0u32);
        remount.set(remount.peek().wrapping_add(1));
        assert!(
            {
                let tree = root.borrow();
                fn find_slot(el: &Element, slot: &str) -> bool {
                    if el.attrs.get("data-slot").map(String::as_str) == Some(slot) {
                        return true;
                    }
                    el.children.iter().any(|c| find_slot(c, slot))
                }
                find_slot(&tree, "dropdown-menu-sub-content")
            },
            "remount bump must rebuild SubContent panel while sub_open stays true"
        );

        clear_test_reconcile_root();
        drop(root);
        handle.dispose();
        clear_overlay_stack();
        reset_registered_anchors();
    }

    /// Gallery root cause: sibling MenubarMenu scopes share one owner; without child-owner
    /// isolation the last menu (View) overwrites DropdownMenuCtx. File→Share then binds as
    /// `{bar}__view__sub0` while View.open is false — SubContent immediately closes it.
    #[test]
    fn menubar_sibling_menus_isolate_sub_parent_id() {
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

        let (handle, file_open) = create_root(|| {
            let file_open = use_signal("mb_iso_bar__file__dd_open", || false);
            let root_e = Rc::clone(&root);
            create_effect(move || {
                let el = menubar_scope("mb_iso_bar", || {
                    let file = menubar_menu(Some("file".into()), Some(file_open), || {
                        let trigger = MenubarTrigger(MenubarTriggerProps {
                            id: String::new(),
                            class: None,
                            children: Some(Element::new().text("File")),
                            base: None,
                        });
                        let content = menubar_content(MenubarContentOpts::default(), || {
                            menubar_sub(None, || {
                                let st = MenubarSubTrigger(MenubarSubTriggerProps {
                                    disabled: Some(false),
                                    inset: Some(false),
                                    class: None,
                                    children: Some(Element::new().text("Share")),
                                    base: None,
                                });
                                let sc = menubar_sub_content(None, || {
                                    MenubarItem(MenubarItemProps {
                                        disabled: Some(false),
                                        inset: Some(false),
                                        variant: None,
                                        class: None,
                                        on_click: None,
                                        children: Some(Element::new().text("Email")),
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
                    // Last sibling must not steal File's DropdownMenuCtx.
                    let view = menubar_menu(Some("view".into()), None, || {
                        let trigger = MenubarTrigger(MenubarTriggerProps {
                            id: String::new(),
                            class: None,
                            children: Some(Element::new().text("View")),
                            base: None,
                        });
                        let content = menubar_content(MenubarContentOpts::default(), || {
                            MenubarItem(MenubarItemProps {
                                disabled: Some(false),
                                inset: Some(false),
                                variant: None,
                                class: None,
                                on_click: None,
                                children: Some(Element::new().text("Status Bar")),
                                base: None,
                            })
                        });
                        let mut frag = Element::new();
                        frag.set_attr("rui_fragment", "1");
                        frag.children(vec![trigger, content])
                    });
                    let mut frag = Element::new();
                    frag.set_attr("rui_fragment", "1");
                    frag.children(vec![file, view])
                });
                *root_e.borrow_mut() = el;
            });
            file_open
        });

        file_open.set(true);
        {
            let mut tree = root.borrow_mut();
            let _ = crate::layout::layout_tree::hoist_portal_subtrees(&mut tree);
        }

        let sub_trigger_fid = {
            let tree = root.borrow();
            fn find_sub_trigger_fid(el: &Element) -> Option<String> {
                if el.attrs.get("data-slot").map(String::as_str)
                    == Some("dropdown-menu-sub-trigger")
                    || el.attrs.get("data-slot").map(String::as_str) == Some("menubar-sub-trigger")
                {
                    return el.focus_id.clone();
                }
                for c in &el.children {
                    if let Some(f) = find_sub_trigger_fid(c) {
                        return Some(f);
                    }
                }
                None
            }
            find_sub_trigger_fid(&tree)
        };
        assert_eq!(
            sub_trigger_fid.as_deref(),
            Some("mb_iso_bar__file__sub0__trigger"),
            "Share under File must keep File's id even after a sibling View menu is built \
             (regression: was mb_iso_bar__view__sub0__trigger)"
        );
        assert!(
            !use_signal("mb_iso_bar__view__sub0__open", || false).peek(),
            "View must not own a phantom Share sub signal"
        );

        clear_test_reconcile_root();
        drop(root);
        handle.dispose();
        clear_overlay_stack();
        reset_registered_anchors();
    }

    /// Gallery-faithful: Content body is a VStack (macro multi-child) with Separators around Sub.
    /// Drive open via the real SubTrigger `on_hover` (set_dd_active + open_or_remount), then
    /// hoist + layout + floating — assert the submenu has on-screen geometry.
    #[test]
    fn menubar_share_hover_opens_visible_submenu_panel() {
        use std::cell::RefCell;
        use std::rc::Rc;

        use crate::app::portal_overlay::clear_overlay_stack;
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::core::reactivity::{create_effect, create_root, use_signal};
        use crate::core::tree::LayoutTree;
        use crate::utils::popup::sync_floating_placement;

        clear_overlay_stack();
        reset_registered_anchors();
        clear_test_reconcile_root();

        let root: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(Rc::clone(&root));

        let (handle, parent_open) = create_root(|| {
            let parent_open = use_signal("mb_share_vis_bar__file__dd_open", || false);
            let root_e = Rc::clone(&root);
            create_effect(move || {
                let el = menubar_scope("mb_share_vis_bar", || {
                    menubar_menu(Some("file".into()), Some(parent_open), || {
                        let trigger = MenubarTrigger(MenubarTriggerProps {
                            id: String::new(),
                            class: None,
                            children: Some(Element::new().text("File")),
                            base: None,
                        });
                        // Mirror gallery RSX multi-child Content → VStack wrapping.
                        let content = menubar_content(MenubarContentOpts::default(), || {
                            let mut col = crate::primitives::vstack().w("100%").gap("0");
                            col = col.child(MenubarItem(MenubarItemProps {
                                disabled: Some(false),
                                inset: Some(false),
                                variant: None,
                                class: None,
                                on_click: None,
                                children: Some(Element::new().text("New File")),
                                base: None,
                            }));
                            col = col.child(MenubarSeparator(MenubarSeparatorProps {
                                children: None,
                                base: None,
                            }));
                            col = col.child(menubar_sub(None, || {
                                let st = MenubarSubTrigger(MenubarSubTriggerProps {
                                    disabled: Some(false),
                                    inset: Some(false),
                                    class: None,
                                    children: Some(Element::new().text("Share")),
                                    base: None,
                                });
                                let sc = menubar_sub_content(None, || {
                                    MenubarItem(MenubarItemProps {
                                        disabled: Some(false),
                                        inset: Some(false),
                                        variant: None,
                                        class: None,
                                        on_click: None,
                                        children: Some(Element::new().text("Email link")),
                                        base: None,
                                    })
                                });
                                let mut frag = Element::new();
                                frag.set_attr("rui_fragment", "1");
                                frag.children(vec![st, sc])
                            }));
                            col.into()
                        });
                        let mut frag = Element::new();
                        frag.set_attr("rui_fragment", "1");
                        frag.children(vec![trigger, content])
                    })
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

        // Fire the real SubTrigger hover handler (same path as gallery pointer enter).
        let hover_cb = {
            let tree = root.borrow();
            fn find_sub_hover(
                el: &Element,
            ) -> Option<std::rc::Rc<std::cell::RefCell<dyn FnMut(bool)>>> {
                if el.attrs.get("data-slot").map(String::as_str)
                    == Some("dropdown-menu-sub-trigger")
                {
                    return el.on_hover.clone();
                }
                for c in &el.children {
                    if let Some(h) = find_sub_hover(c) {
                        return Some(h);
                    }
                }
                None
            }
            find_sub_hover(&tree)
        };
        let hover_cb = hover_cb.expect("Share SubTrigger must expose on_hover after File open");
        hover_cb.borrow_mut()(true);

        // Same-frame contract: reconcile of SubContent must hoist the nested portal out of
        // File's overflow:hidden scroll — no extra hoist_portal_subtrees call required.
        {
            let tree = root.borrow();
            let layer = tree.children.iter().find(|c| {
                c.template_id.as_deref() == Some(crate::layout::layout_tree::PORTAL_LAYER_ID)
            });
            fn find_sub_panel(el: &Element) -> bool {
                el.attrs.get("data-slot").map(String::as_str) == Some("dropdown-menu-sub-content")
                    || el.children.iter().any(find_sub_panel)
            }
            assert!(
                layer.is_some_and(|l| l.children.iter().any(find_sub_panel)),
                "Share submenu must sit on portal layer immediately after hover reconcile (same-frame hoist)"
            );
        }

        {
            let mut tree = root.borrow_mut();
            // Give the File trigger + Share row non-zero geometry so floating can place.
            fn stamp_bounds(el: &mut Element) {
                if el.focus_id.as_deref() == Some("mb_share_vis_bar__file__dd_trigger") {
                    el.layout_bounds = [40.0, 40.0, 48.0, 28.0];
                }
                if el.focus_id.as_deref() == Some("mb_share_vis_bar__file__sub0__trigger") {
                    el.layout_bounds = [48.0, 120.0, 160.0, 28.0];
                }
                for c in &mut el.children {
                    stamp_bounds(c);
                }
            }
            stamp_bounds(&mut tree);
            let _ = sync_floating_placement(&mut tree, 800.0, 600.0);
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut tree, 800.0, 600.0, [-1.0, -1.0]);
            let _ = crate::layout::layout_tree::hoist_portal_subtrees(&mut tree);
            let _ = sync_floating_placement(&mut tree, 800.0, 600.0);
            let _ = layout.build_and_layout(&mut tree, 800.0, 600.0, [-1.0, -1.0]);
        }

        let (sub_open, on_layer, panel_left, panel_w) = {
            let tree = root.borrow();
            let sub_open = use_signal("mb_share_vis_bar__file__sub0__open", || false).peek();
            let layer = tree.children.iter().find(|c| {
                c.template_id.as_deref() == Some(crate::layout::layout_tree::PORTAL_LAYER_ID)
            });
            fn find_sub_panel(el: &Element) -> Option<&Element> {
                if el.attrs.get("data-slot").map(String::as_str)
                    == Some("dropdown-menu-sub-content")
                {
                    return Some(el);
                }
                el.children.iter().find_map(find_sub_panel)
            }
            let panel = layer.and_then(|l| l.children.iter().find_map(find_sub_panel));
            let on_layer = panel.is_some();
            let left = panel
                .and_then(|p| crate::layout::lpa_length_px(p.style.inset.left))
                .unwrap_or(-1.0);
            let w = panel.map(|p| p.layout_bounds[2]).unwrap_or(0.0);
            (sub_open, on_layer, left, w)
        };

        assert!(sub_open, "Share hover must set sub_open true");
        assert!(
            on_layer,
            "Share submenu panel must sit on the portal layer after hover+hoist"
        );
        assert!(
            panel_left > 40.0,
            "submenu must be placed to the right of Share (left={panel_left}), not stuck at 0/clipped"
        );
        assert!(
            panel_w > 8.0 || panel_left > 40.0,
            "submenu must have visible width or at least a placed left edge (w={panel_w}, left={panel_left})"
        );

        clear_test_reconcile_root();
        drop(root);
        handle.dispose();
        clear_overlay_stack();
        reset_registered_anchors();
    }

    /// Gallery stuck path: Share hover opens while reconcile misses (borrow / missing slot).
    /// Without deferred remount, highlight sticks and no further enter fires — panel never appears.
    #[test]
    fn menubar_share_hover_recovers_after_reconcile_miss() {
        use std::cell::RefCell;
        use std::rc::Rc;

        use crate::app::portal_overlay::clear_overlay_stack;
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::core::reactivity::{create_effect, create_root, use_signal};

        clear_overlay_stack();
        reset_registered_anchors();
        clear_test_reconcile_root();

        let root: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(Rc::clone(&root));

        let (handle, parent_open) = create_root(|| {
            let parent_open = use_signal("mb_miss_bar__file__dd_open", || false);
            let root_e = Rc::clone(&root);
            create_effect(move || {
                let el = menubar_scope("mb_miss_bar", || {
                    menubar_menu(Some("file".into()), Some(parent_open), || {
                        let trigger = MenubarTrigger(MenubarTriggerProps {
                            id: String::new(),
                            class: None,
                            children: Some(Element::new().text("File")),
                            base: None,
                        });
                        let content = menubar_content(MenubarContentOpts::default(), || {
                            menubar_sub(None, || {
                                let st = MenubarSubTrigger(MenubarSubTriggerProps {
                                    disabled: Some(false),
                                    inset: Some(false),
                                    class: None,
                                    children: Some(Element::new().text("Share")),
                                    base: None,
                                });
                                let sc = menubar_sub_content(None, || {
                                    MenubarItem(MenubarItemProps {
                                        disabled: Some(false),
                                        inset: Some(false),
                                        variant: None,
                                        class: None,
                                        on_click: None,
                                        children: Some(Element::new().text("Email link")),
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
                    })
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

        let hover_cb = {
            let tree = root.borrow();
            fn find_sub_hover(
                el: &Element,
            ) -> Option<std::rc::Rc<std::cell::RefCell<dyn FnMut(bool)>>> {
                if el.attrs.get("data-slot").map(String::as_str)
                    == Some("dropdown-menu-sub-trigger")
                {
                    return el.on_hover.clone();
                }
                for c in &el.children {
                    if let Some(h) = find_sub_hover(c) {
                        return Some(h);
                    }
                }
                None
            }
            find_sub_hover(&tree)
        };
        let hover_cb = hover_cb.expect("Share SubTrigger on_hover");

        // Simulate reconcile miss during open (e.g. AppState borrowed): drop the test root.
        clear_test_reconcile_root();
        hover_cb.borrow_mut()(true);
        let sub_open_after_miss = use_signal("mb_miss_bar__file__sub0__open", || false).peek();
        assert!(
            sub_open_after_miss,
            "hover must still set sub_open even when reconcile misses"
        );

        fn find_sub_panel(el: &Element) -> bool {
            if el.attrs.get("data-slot").map(String::as_str) == Some("dropdown-menu-sub-content") {
                return true;
            }
            el.children.iter().any(find_sub_panel)
        }
        assert!(
            !find_sub_panel(&root.borrow()),
            "panel must be absent while reconcile root is cleared (miss path)"
        );

        // Restore tree + drain deferred remount (same as CursorMoved/redraw after hover).
        install_test_reconcile_root(Rc::clone(&root));
        drain_deferred_view_remounts();
        {
            let mut tree = root.borrow_mut();
            let _ = crate::layout::layout_tree::hoist_portal_subtrees(&mut tree);
        }

        assert!(
            find_sub_panel(&root.borrow()),
            "deferred remount after reconcile miss must mount Share submenu panel"
        );

        clear_test_reconcile_root();
        drop(root);
        handle.dispose();
        clear_overlay_stack();
        reset_registered_anchors();
    }
}
