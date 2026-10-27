//! # NavigationMenu
//!
//! 对齐 ShadCN / Base UI **NavigationMenu**：顶栏/区段的链接导览 + 可选富内容下拉。
//!
//! ```text
//! NavigationMenu
//! ├── NavigationMenuList
//! │   ├── NavigationMenuItem
//! │   │   ├── NavigationMenuTrigger   // + chevron-down
//! │   │   └── NavigationMenuContent   // Portal / Viewport 面板
//! │   │       └── NavigationMenuLink
//! │   └── NavigationMenuItem
//! │       └── NavigationMenuLink      // 顶栏直达
//! └── NavigationMenuViewport          // 可选共享视口（默认开启）
//! ```
//!
//! | 模块 | 职责 |
//! |---|---|
//! | [`crate::components::navbar`] | 顶栏布局壳（brand / center / actions）；center 可嵌本组件 |
//! | [`crate::components::menubar`] | 应用命令菜单（File / Edit） |
//! | [`crate::app::navigator::navigation_host`] | `Navigator` 页面栈视图（与本组件无关） |
//! | **本模块** | 导览链接 + Trigger / Content / Viewport |
//!
//! `ui!` 中 `<NavigationMenu>` / `<NavigationMenuItem>` / `<NavigationMenuContent>` 由宏展开到
//! [`navigation_menu_scope`] / [`navigation_menu_item`] / [`navigation_menu_content`]。
//! Builder [`navigation_menu`] 仍可用。
//!
//! ## Usage (dual style)
//! ```ignore
//! // A. Composite RSX (recommended)
//! ui! {
//!     <NavigationMenu id="main">
//!         <NavigationMenuList>
//!             <NavigationMenuItem>
//!                 <NavigationMenuLink on_click={|| {}}>
//!                     "Docs"
//!                 </NavigationMenuLink>
//!             </NavigationMenuItem>
//!             <NavigationMenuItem value="comp">
//!                 <NavigationMenuTrigger>"Components"</NavigationMenuTrigger>
//!                 <NavigationMenuContent>
//!                     <NavigationMenuLink on_click={|| {}}>
//!                         "Dialog"
//!                         <Label text_size="0.75rem" color="muted-foreground">
//!                             "Modal confirmation"
//!                         </Label>
//!                     </NavigationMenuLink>
//!                 </NavigationMenuContent>
//!             </NavigationMenuItem>
//!         </NavigationMenuList>
//!     </NavigationMenu>
//! }
//!
//! // B. Builder（便捷：link / trigger → 复合实现）
//! navigation_menu("main")
//!     .link("docs", "Docs", || {})
//!     .trigger("comp", "Components", vec![
//!         NavigationMenuLinkData::new("dialog", "Dialog", || {})
//!             .description("Modal confirmation"),
//!     ])
//! ```
//!
//! ## 样式（对齐 ShadCN v4）
//! - Root：`relative flex max-w-max flex-1 items-center justify-center`
//! - Trigger：`h-9` · `px-4` · medium · open → muted accent；右侧 chevron-down
//! - Content 面板：`popover` 底 · `rounded-md` · border · shadow · `min-w-14rem`
//! - Link（面板内）：标题 + 可选描述；hover / focus 用 accent 浅底

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::OnceLock;

use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{Key, NamedKey};

use crate::app::portal_overlay::{OverlayKind, OverlayMode};
use crate::app::reactive::{reactive_anchor, reactive_placeholder_sized};
use crate::core::reactivity::{
    create_computed, create_context, provide_context, use_context, use_signal, with_child_owner,
    Context, Signal,
};
use crate::core::tree::{parse_color, parse_text_color, Element, Handler};
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::tokens;
use crate::utils::anchored_overlay::{
    anchored_overlay_content, provide_anchored_overlay_context, AnchoredOverlayContentOpts,
    FloatingMotionKind, FloatingStrategy, Placement,
};
use crate::utils::lucide_glyph;
use crate::utils::popup::{PopupPlacement, FLOATING_TRIGGER_GAP};
use crate::utils::resolve_instance_id;
use crate::utils::roving::{focus_by_id, focus_roving_key, FocusRovingAxis};
use crate::utils::text_node::{build_icon_text_node, build_text_node, TextNodeConfig};
use crate::utils::{take_fragment, take_packed};

const ATTR_SLOT: &str = "data-slot";
const SLOT_ROOT: &str = "navigation-menu";
const SLOT_LIST: &str = "navigation-menu-list";
const SLOT_ITEM: &str = "navigation-menu-item";
const SLOT_TRIGGER: &str = "navigation-menu-trigger";
const SLOT_CONTENT: &str = "navigation-menu-content";
const SLOT_LINK: &str = "navigation-menu-link";
const SLOT_VIEWPORT: &str = "navigation-menu-viewport";
const ATTR_ITEM_VALUE: &str = "jadix_nav_menu_item_value";

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

/// Root 上下文：稳定实例 `id` + 当前展开的 Item `value`。
#[derive(Clone)]
pub struct NavigationMenuCtx {
    pub id: String,
    /// 当前展开的 [`NavigationMenuItem`] `value`；`None` = 全关。
    pub expanded: Signal<Option<String>>,
    /// 是否使用共享 Viewport 槽（ShadCN 默认 true）。
    pub viewport: bool,
}

fn navigation_menu_context() -> Context<NavigationMenuCtx> {
    static CTX: OnceLock<Context<NavigationMenuCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<NavigationMenuCtx>)
}

/// Item 上下文：本段 `value` + trigger focus id + 专属 open 信号（portal_sync）。
#[derive(Clone)]
pub struct NavigationMenuItemCtx {
    pub value: String,
    pub trigger_fid: Signal<String>,
    pub open: Signal<bool>,
}

fn navigation_menu_item_context() -> Context<NavigationMenuItemCtx> {
    static CTX: OnceLock<Context<NavigationMenuItemCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<NavigationMenuItemCtx>)
}

thread_local! {
    static NM_ITEM_SEQ: RefCell<HashMap<String, usize>> = RefCell::new(HashMap::new());
    static NM_LINK_SEQ: RefCell<HashMap<String, usize>> = RefCell::new(HashMap::new());
    static NM_TOP_FOCUS: RefCell<HashMap<String, Vec<String>>> = RefCell::new(HashMap::new());
    static NM_IN_CONTENT: Cell<usize> = const { Cell::new(0) };
}

fn reset_item_seq(menu_id: &str) {
    NM_ITEM_SEQ.with(|m| {
        m.borrow_mut().insert(menu_id.to_string(), 0);
    });
    NM_LINK_SEQ.with(|m| {
        m.borrow_mut().insert(menu_id.to_string(), 0);
    });
    NM_TOP_FOCUS.with(|m| {
        m.borrow_mut().insert(menu_id.to_string(), Vec::new());
    });
}

fn next_item_seq(menu_id: &str) -> usize {
    NM_ITEM_SEQ.with(|m| {
        let mut map = m.borrow_mut();
        let e = map.entry(menu_id.to_string()).or_insert(0);
        let i = *e;
        *e += 1;
        i
    })
}

fn next_link_seq(menu_id: &str) -> usize {
    NM_LINK_SEQ.with(|m| {
        let mut map = m.borrow_mut();
        let e = map.entry(menu_id.to_string()).or_insert(0);
        let i = *e;
        *e += 1;
        i
    })
}

struct ContentDepthGuard;
impl Drop for ContentDepthGuard {
    fn drop(&mut self) {
        NM_IN_CONTENT.with(|c| c.set(c.get().saturating_sub(1)));
    }
}

fn enter_content_body() -> ContentDepthGuard {
    NM_IN_CONTENT.with(|c| c.set(c.get() + 1));
    ContentDepthGuard
}

fn in_content_body() -> bool {
    NM_IN_CONTENT.with(|c| c.get() > 0)
}

fn register_top_focus(menu_id: &str, fid: String) {
    NM_TOP_FOCUS.with(|m| {
        m.borrow_mut()
            .entry(menu_id.to_string())
            .or_default()
            .push(fid);
    });
}

fn top_focus_of(menu_id: &str) -> Vec<String> {
    NM_TOP_FOCUS.with(|m| m.borrow().get(menu_id).cloned().unwrap_or_default())
}

/// 读取最近的 [`NavigationMenu`] 上下文。
pub fn use_navigation_menu() -> Option<NavigationMenuCtx> {
    use_context(navigation_menu_context())
}

fn require_navigation_menu(part: &str) -> NavigationMenuCtx {
    use_navigation_menu().unwrap_or_else(|| {
        panic!("{part} must be used within <NavigationMenu> / navigation_menu_scope");
    })
}

/// 读取最近的 [`NavigationMenuItem`] 上下文。
pub fn use_navigation_menu_item() -> Option<NavigationMenuItemCtx> {
    use_context(navigation_menu_item_context())
}

fn require_navigation_menu_item(part: &str) -> NavigationMenuItemCtx {
    use_navigation_menu_item().unwrap_or_else(|| {
        panic!("{part} must be used within <NavigationMenuItem>");
    })
}

fn is_text_leaf(el: &Element) -> bool {
    el.children.is_empty() && el.text.as_ref().is_some_and(|t| !t.is_empty())
}

fn extract_text_leaf(el: &Element) -> String {
    if let Some(t) = &el.text {
        if !t.is_empty() {
            return t.clone();
        }
    }
    for c in &el.children {
        let t = extract_text_leaf(c);
        if !t.is_empty() {
            return t;
        }
    }
    String::new()
}

fn accent_muted() -> [f32; 4] {
    let mut c = parse_color("accent");
    c[3] *= 0.55;
    c
}

fn chevron_down_icon() -> Element {
    let glyph = lucide_glyph("chevron-down").unwrap_or_else(|| "▾".into());
    build_icon_text_node(glyph)
        .text_size(12.0)
        .text_color(parse_text_color("muted-foreground"))
}

/// 顶栏 chip 文案：与 Label `leading-none` 对齐，避免相对 Brand 上下漂移。
fn nav_chip_label(text: &str) -> Element {
    build_text_node(
        text,
        TextNodeConfig {
            size_px: 14.0,
            weight: 500,
            color: parse_text_color("foreground"),
            leading_scale: Some(1.0),
            ..Default::default()
        },
    )
}

/// 顶栏 Trigger / Link 共用 `h-9` 横排 chip（垂直居中）。
fn nav_top_chip(bg: [f32; 4], label: Element, trailing: Option<Element>) -> Element {
    let mut row = hstack()
        .h("2.25rem")
        .px("1rem")
        .gap("0.25rem")
        .items_center()
        .justify("center")
        .rounded("0.375rem")
        .bg(bg)
        .flex_shrink(0.0)
        .child(label);
    if let Some(t) = trailing {
        row = row.child(t);
    }
    row.into()
}

fn chain_key_down(mut el: Element, handler: impl FnMut(KeyEvent) -> bool + 'static) -> Element {
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

// ---------------------------------------------------------------------------
// Scope
// ---------------------------------------------------------------------------

/// 提供 NavigationMenu 上下文并构建根壳（`ui!` `<NavigationMenu>` / Builder 共用）。
///
/// 嵌入 Navbar 时 hug 内容：`max-w-max` · `w-auto` · `shrink`；垂直对齐交给父级 `items-center`。
/// 勿 `justify-center` / Viewport `w-full`——会把 List 居中拉开，窄屏溢出时对称外溢。
/// 勿 `align-self: start`——在 NavbarCenter(hstack) 里会变成顶对齐。
pub fn navigation_menu_scope(
    id: impl Into<String>,
    viewport: bool,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = resolve_instance_id(id, "navigation_menu");
    reset_item_seq(&id);
    let expanded = use_signal(
        format!("{id}__navmenu_expanded").as_str(),
        || None::<String>,
    );
    let mut root = with_child_owner(|| {
        provide_context(
            navigation_menu_context(),
            NavigationMenuCtx {
                id: id.clone(),
                expanded,
                viewport,
            },
        );
        let kids = take_fragment(body());
        // 根壳只包 List（横排）+ 可选 Viewport；不要 flex_grow / w-full 撑破 Navbar 中栏。
        // 勿设 align-self:start：父级是 NavbarCenter(hstack) 时 cross-axis 为竖直，
        // start 会顶对齐并盖掉 items-center（Builder/RSX 均会「上移」）。
        let mut shell = vstack()
            .gap("0")
            .relative(true)
            .w("auto")
            .max_w("100%")
            .min_w(0.0)
            .flex_grow(0.0)
            .flex_shrink(1.0)
            .items_center();
        let mut has_viewport = false;
        for k in kids {
            if k.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_VIEWPORT) {
                has_viewport = true;
            }
            shell = shell.child(k);
        }
        if viewport && !has_viewport {
            shell = shell.child(NavigationMenuViewport(NavigationMenuViewportProps {
                children: None,
                base: None,
            }));
        }
        let mut el: Element = shell.into();
        el.set_attr(ATTR_SLOT, SLOT_ROOT);
        el.set_attr("jadix_navigation_menu", "1");
        if el.template_id.is_none() {
            el.template_id = Some(format!("{id}__navigation_menu"));
        }
        el
    });
    if root.template_id.is_none() {
        root.template_id = Some(format!("{id}__navigation_menu"));
    }
    root
}

/// `NavigationMenuContent` 选项（`ui!` / [`navigation_menu_content`] 共用）。
#[derive(Clone, Default)]
pub struct NavigationMenuContentOpts {
    pub panel: crate::components::base::SlotChrome,
}

impl NavigationMenuContentOpts {
    crate::impl_content_opts_chrome!();
}

/// shadcn `NavigationMenuContent`：Portal + floating 面板。
/// `ui!` lowers to this function via the macro (subtree rebuilt lazily when open).
pub fn navigation_menu_content(
    opts: NavigationMenuContentOpts,
    body: impl Fn() -> Element + 'static,
) -> Element {
    let menu = require_navigation_menu("NavigationMenuContent");
    let item = require_navigation_menu_item("NavigationMenuContent");
    let expanded = menu.expanded;
    let open = item.open;
    let value = item.value.clone();
    let overlay_key = format!("{}:{}", menu.id, value);
    let trigger_fid = item.trigger_fid;
    let chrome = opts.panel;
    let _ = menu.viewport;
    // Content is built lazily only after `open` is true; sync before that gate.
    let sync_value = value.clone();
    create_computed(move || {
        open.set(expanded.get().as_deref() == Some(sync_value.as_str()));
    });
    // Portal dismissal (Escape / outside press) must also reset the trigger state.
    let dismiss_value = value.clone();
    create_computed(move || {
        if !open.get() && expanded.peek().as_deref() == Some(dismiss_value.as_str()) {
            expanded.set(None);
        }
    });
    let ao_opts = AnchoredOverlayContentOpts {
        placement: Some(Placement::trigger(
            String::new(),
            PopupPlacement::BottomStart,
            FLOATING_TRIGGER_GAP,
            FloatingStrategy::Auto,
        )),
        kind: OverlayKind::Menu,
        lock_page_scroll: Some(true),
        motion: FloatingMotionKind::Expand,
        mode: OverlayMode::Stacked,
        width: Some("14rem".into()),
        ..Default::default()
    };
    anchored_overlay_content(ao_opts, move || {
        let _guard = enter_content_body();
        let rows = body();
        let kids = take_packed(rows);
        let mut list = vstack()
            .gap("0.125rem")
            .p("0.75rem")
            .min_w("14rem")
            .w("auto");
        let link_ids: Vec<String> = kids
            .iter()
            .enumerate()
            .map(|(i, k)| {
                k.focus_id
                    .clone()
                    .unwrap_or_else(|| format!("{overlay_key}:link:{i}"))
            })
            .collect();
        for (i, mut el) in kids.into_iter().enumerate() {
            if el.focus_id.is_none() {
                el = el.focus_id(format!("{overlay_key}:link:{i}"));
            }
            let ids = link_ids.clone();
            let trigger = trigger_fid.peek();
            let exp = expanded;
            el = chain_key_down(el, move |ev| {
                if ev.state != ElementState::Pressed || ev.repeat {
                    return false;
                }
                if matches!(ev.logical_key, Key::Named(NamedKey::Escape)) {
                    exp.set(None);
                    focus_by_id(trigger.clone());
                    return true;
                }
                focus_roving_key(&ev, &ids, i, FocusRovingAxis::Vertical)
            });
            if el
                .authoring_state
                .get::<crate::components::button::ButtonHoverPaint>()
                .is_none()
            {
                el.set_focusable(true);
            }
            list = list.child(el);
        }

        let mut panel: Element = list
            .bg(tokens::popover())
            .rounded("0.375rem")
            .border_style("solid")
            .border_width(1.0)
            .border_color(tokens::border())
            .popup_shadow()
            .absolute(true)
            .into();
        panel = chrome.apply(panel);
        panel.set_attr(ATTR_SLOT, SLOT_CONTENT);
        panel
    })
}

// ---------------------------------------------------------------------------
// RSX parts
// ---------------------------------------------------------------------------

crate::__component_contract! {
NavigationMenuListProps [];
/// 列表行：横排 hug · `justify-start` · 窄屏 `overflow-x: auto`（避免 center 对称外溢）。
#[allow(non_snake_case)]
pub fn NavigationMenuList(
plain [] children: Option<Element>,
) -> Element {
    let _ = require_navigation_menu("NavigationMenuList");
    let kids = children.map(take_packed).unwrap_or_default();
    let mut row = hstack()
        .gap("0.25rem")
        .items_center()
        .justify("start")
        .flex_grow(0.0)
        .flex_shrink(1.0)
        .w("auto")
        .max_w("100%")
        .min_w(0.0)
        .overflow_x("auto")
        .overflow_y("hidden");
    for k in kids {
        row = row.child(k);
    }
    let mut el: Element = row.into();
    el.set_attr(ATTR_SLOT, SLOT_LIST);
    el.set_attr("jadix_navigation_menu_list", "1");
    el
}
}

/// 单项：相对定位壳；可带 `value`（Trigger/Content 关联键）。
///
/// `ui!` 中由宏展开为本函数（闭包内再构建 Trigger/Content，保证上下文可用）。
pub fn navigation_menu_item(value: Option<String>, body: impl FnOnce() -> Element) -> Element {
    let menu = require_navigation_menu("NavigationMenuItem");
    let seq = next_item_seq(&menu.id);
    let value = match value {
        Some(v) if !v.trim().is_empty() => v,
        _ => format!("item-{seq}"),
    };
    let open = use_signal(format!("{}:{}__open", menu.id, value).as_str(), || false);
    let trigger_fid = use_signal(
        format!("{}:{}__trigger_fid", menu.id, value).as_str(),
        || format!("{}:{}:trigger", menu.id, value),
    );

    let overlay_key = format!("{}:{}", menu.id, value);
    with_child_owner(|| {
        provide_anchored_overlay_context(open, overlay_key, trigger_fid);
        provide_context(
            navigation_menu_item_context(),
            NavigationMenuItemCtx {
                value: value.clone(),
                trigger_fid,
                open,
            },
        );
        let kids = take_packed(body());
        // Trigger + Content(0×0) 横排，避免默认块布局把项撑成竖列。
        let mut row = hstack()
            .gap("0")
            .items_center()
            .relative(true)
            .w("auto")
            .flex_shrink(0.0);
        for k in kids {
            row = row.child(k);
        }
        let mut el: Element = row.into();
        el.set_attr(ATTR_SLOT, SLOT_ITEM);
        el.set_attr(ATTR_ITEM_VALUE, &value);
        el.template_id = Some(format!("{}:item:{}", menu.id, seq));
        el
    })
}

crate::__component_contract! {
NavigationMenuItemProps [];
/// 兼容旧 RSX 组件名：请用宏 `<NavigationMenuItem>`（走 [`navigation_menu_item`]）。
#[allow(non_snake_case)]
pub fn NavigationMenuItem(
default [] value: String,
plain [] children: Option<Element>,
) -> Element {
    // Eager children already built — only safe for leaf-only Items (Link without Trigger).
    navigation_menu_item(
        if value.trim().is_empty() {
            None
        } else {
            Some(value)
        },
        || children.unwrap_or_else(Element::new),
    )
}
}

crate::__component_contract! {
NavigationMenuTriggerProps [];
/// Trigger：`h-9` chip + chevron；点击切换本 Item 展开。
#[allow(non_snake_case)]
pub fn NavigationMenuTrigger(
identity [] id: String,
plain [] children: Option<Element>,
) -> Element {
    let menu = require_navigation_menu("NavigationMenuTrigger");
    let item = require_navigation_menu_item("NavigationMenuTrigger");
    let expanded = menu.expanded;
    let value = item.value.clone();
    let trigger_id = if id.is_empty() {
        format!("{}:{}:trigger", menu.id, value)
    } else {
        id
    };
    item.trigger_fid.set(trigger_id.clone());
    register_top_focus(&menu.id, trigger_id.clone());

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
    let menu_id = menu.id.clone();
    let view_id = format!("{trigger_id}__nm_trig_view");

    // 占位须 hug 内容（auto×h-9）；reactive_view 默认 100%×100% 会在 List 里把每项撑满一行。
    reactive_anchor(view_id.clone(), move || {
        let is_open = expanded.get().as_deref() == Some(value.as_str());
        let exp_click = expanded;
        let value_click = value.clone();
        let exp_keys = expanded;
        let value_keys = value.clone();
        let top_ids = top_focus_of(&menu_id);
        let my_ix = top_ids.iter().position(|f| f == &trigger_id).unwrap_or(0);

        let bg = if is_open {
            accent_muted()
        } else {
            tokens::background()
        };
        let mut chip: Element =
            nav_top_chip(bg, nav_chip_label(&label_text), Some(chevron_down_icon()));
        chip = interactive(chip)
            .focus_id(&trigger_id)
            .focusable(true)
            .focus_ring(true)
            .on_click(move || {
                if exp_click.peek().as_deref() == Some(value_click.as_str()) {
                    exp_click.set(None);
                } else {
                    exp_click.set(Some(value_click.clone()));
                }
            })
            .into();
        chip = chain_key_down(chip, move |ev| {
            if ev.state != ElementState::Pressed || ev.repeat {
                return false;
            }
            if matches!(ev.logical_key, Key::Named(NamedKey::ArrowDown)) {
                if exp_keys.peek().as_deref() != Some(value_keys.as_str()) {
                    exp_keys.set(Some(value_keys.clone()));
                }
                return true;
            }
            if matches!(ev.logical_key, Key::Named(NamedKey::Escape)) && exp_keys.peek().is_some() {
                exp_keys.set(None);
                return true;
            }
            focus_roving_key(&ev, &top_ids, my_ix, FocusRovingAxis::Horizontal)
        });
        chip.set_attr(ATTR_SLOT, SLOT_TRIGGER);
        chip
    });
    reactive_placeholder_sized(view_id, "auto", "2.25rem")
}
}

crate::__component_contract! {
NavigationMenuLinkProps [];
/// 导览链接：顶栏直达或 Content 内条目；可选多行（标题 + 描述子节点）。
#[allow(non_snake_case)]
pub fn NavigationMenuLink(
identity [] id: String,
default [] on_click: Handler<()>,
plain [] children: Option<Element>,
) -> Element {
    let menu = require_navigation_menu("NavigationMenuLink");
    let expanded = menu.expanded;
    let panel = in_content_body();

    let kids = children.map(take_packed).unwrap_or_default();
    let (title, description) = split_link_children(&kids);

    let fid = if id.is_empty() {
        let seq = next_link_seq(&menu.id);
        format!("{}:link:{seq}", menu.id)
    } else {
        id
    };

    // Top-bar link: register for horizontal roving.
    if !panel {
        register_top_focus(&menu.id, fid.clone());
    }

    let mut on_click = on_click;
    let menu_id = menu.id.clone();

    if panel {
        let mut col = vstack().gap("0.25rem").w("100%");
        col = col.child(build_text_node(
            &title,
            TextNodeConfig {
                size_px: 14.0,
                weight: 500,
                color: parse_text_color("foreground"),
                leading_scale: Some(1.0),
                ..Default::default()
            },
        ));
        if let Some(desc) = description {
            col = col.child(build_text_node(
                &desc,
                TextNodeConfig {
                    size_px: 12.0,
                    weight: 400,
                    color: parse_text_color("muted-foreground"),
                    leading_scale: Some(1.25),
                    ..Default::default()
                },
            ));
        }
        let hit: Element = r#box()
            .w("100%")
            .p("0.5rem")
            .rounded("0.375rem")
            .child(col)
            .into();
        let mut el: Element = interactive(hit)
            .focus_id(&fid)
            .focusable(true)
            .focus_ring(true)
            .on_click(move || {
                on_click.call(());
                expanded.set(None);
            })
            .into();
        el.set_attr(ATTR_SLOT, SLOT_LINK);
        el
    } else {
        // Top-bar chip：与 Trigger 同构 hstack（勿用块级 box，否则文案顶对齐）。
        let top_ids = top_focus_of(&menu_id);
        let my_ix = top_ids.iter().position(|f| f == &fid).unwrap_or(0);
        let mut chip: Element = nav_top_chip(tokens::background(), nav_chip_label(&title), None);
        chip = interactive(chip)
            .focus_id(&fid)
            .focusable(true)
            .focus_ring(true)
            .on_click(move || {
                on_click.call(());
            })
            .into();
        chip = chain_key_down(chip, move |ev| {
            focus_roving_key(&ev, &top_ids, my_ix, FocusRovingAxis::Horizontal)
        });
        chip.set_attr(ATTR_SLOT, SLOT_LINK);
        chip
    }
}
}

fn split_link_children(kids: &[Element]) -> (String, Option<String>) {
    if kids.is_empty() {
        return ("Link".into(), None);
    }
    if kids.len() == 1 {
        return (extract_text_leaf(&kids[0]), None);
    }
    let title = extract_text_leaf(&kids[0]);
    let desc = extract_text_leaf(&kids[1]);
    (
        if title.is_empty() {
            "Link".into()
        } else {
            title
        },
        if desc.is_empty() { None } else { Some(desc) },
    )
}

crate::__component_contract! {
NavigationMenuViewportProps [];
/// 共享 Viewport 槽（solid-ui / ShadCN 默认开启；Root 会在缺省时自动补上）。
///
/// 必须 `w-auto`/`0`：`w-full` 会把根壳撑满 NavbarCenter，List 被居中拉开。
#[allow(non_snake_case)]
pub fn NavigationMenuViewport(
plain [] children: Option<crate::core::tree::Element>,
) -> Element {
    let _ = require_navigation_menu("NavigationMenuViewport");
    let mut slot: Element = r#box().w(0.0).h(0.0).relative(true).into();
    slot.set_attr(ATTR_SLOT, SLOT_VIEWPORT);
    slot.set_attr("jadix_navigation_menu_viewport", "1");
    slot
}
}

// ---------------------------------------------------------------------------
// Builder data + entry
// ---------------------------------------------------------------------------

/// Builder / 便捷 API 用的链接数据（RSX 请用组件 [`NavigationMenuLink`]）。
#[derive(Clone)]
pub struct NavigationMenuLinkData {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
    pub on_select: Rc<dyn Fn()>,
}

impl NavigationMenuLinkData {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        on_select: impl Fn() + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: None,
            on_select: Rc::new(on_select),
        }
    }

    pub fn description(mut self, text: impl Into<String>) -> Self {
        self.description = Some(text.into());
        self
    }
}

/// Builder 列表项（RSX 请用 [`NavigationMenuItem`]）。
#[derive(Clone)]
pub enum NavigationMenuEntry {
    Link(NavigationMenuLinkData),
    Trigger {
        id: String,
        label: String,
        links: Vec<NavigationMenuLinkData>,
    },
}

pub struct NavigationMenuBuilder {
    id: String,
    items: Vec<NavigationMenuEntry>,
    /// 共享 Viewport 槽（ShadCN 默认开启）。
    viewport: bool,
}

pub fn navigation_menu(id: impl Into<String>) -> NavigationMenuBuilder {
    NavigationMenuBuilder {
        id: id.into(),
        items: Vec::new(),
        viewport: true,
    }
}

impl NavigationMenuBuilder {
    pub fn viewport(mut self, enabled: bool) -> Self {
        self.viewport = enabled;
        self
    }

    pub fn item(mut self, item: NavigationMenuEntry) -> Self {
        self.items.push(item);
        self
    }

    pub fn link(
        self,
        id: impl Into<String>,
        label: impl Into<String>,
        on_select: impl Fn() + 'static,
    ) -> Self {
        self.item(NavigationMenuEntry::Link(NavigationMenuLinkData::new(
            id, label, on_select,
        )))
    }

    pub fn trigger(
        self,
        id: impl Into<String>,
        label: impl Into<String>,
        links: Vec<NavigationMenuLinkData>,
    ) -> Self {
        self.item(NavigationMenuEntry::Trigger {
            id: id.into(),
            label: label.into(),
            links,
        })
    }
}

impl From<NavigationMenuBuilder> for Element {
    fn from(builder: NavigationMenuBuilder) -> Self {
        let NavigationMenuBuilder {
            id,
            items,
            viewport,
        } = builder;
        navigation_menu_scope(id, viewport, move || {
            let mut list_kids = Vec::new();
            for entry in items {
                match entry {
                    NavigationMenuEntry::Link(link) => {
                        let run = link.on_select.clone();
                        let item = navigation_menu_item(Some(link.id.clone()), move || {
                            NavigationMenuLink(NavigationMenuLinkProps {
                                id: link.id,
                                on_click: Some(Handler::new(move |_| run())),
                                children: Some(Element::new().text(link.label)),
                                base: None,
                            })
                        });
                        list_kids.push(item);
                    }
                    NavigationMenuEntry::Trigger {
                        id: tid,
                        label,
                        links,
                    } => {
                        list_kids.push(navigation_menu_item(Some(tid), move || {
                            let trigger = NavigationMenuTrigger(NavigationMenuTriggerProps {
                                id: String::new(),
                                children: Some(Element::new().text(label)),
                                base: None,
                            });
                            let content = navigation_menu_content(
                                NavigationMenuContentOpts::default(),
                                move || {
                                    let mut col = vstack().w("100%").gap("0.125rem");
                                    for link in &links {
                                        let run = link.on_select.clone();
                                        let mut kids =
                                            vec![Element::new().text(link.label.clone())];
                                        if let Some(ref d) = link.description {
                                            kids.push(Element::new().text(d.clone()));
                                        }
                                        let mut frag = Element::new();
                                        frag.set_attr("jadix_fragment", "1");
                                        let row = NavigationMenuLink(NavigationMenuLinkProps {
                                            id: link.id.clone(),
                                            on_click: Some(Handler::new(move |_| run())),
                                            children: Some(frag.children(kids)),
                                            base: None,
                                        });
                                        col = col.child(row);
                                    }
                                    col.into()
                                },
                            );
                            let mut frag = Element::new();
                            frag.set_attr("jadix_fragment", "1");
                            frag.children(vec![trigger, content])
                        }));
                    }
                }
            }
            let mut frag = Element::new();
            frag.set_attr("jadix_fragment", "1");
            NavigationMenuList(NavigationMenuListProps {
                children: Some(frag.children(list_kids)),
                base: None,
            })
        })
    }
}

impl crate::core::tree::IntoElementList for NavigationMenuBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[cfg(all(test, feature = "product-tests"))]
mod tests {
    use super::*;

    #[test]
    fn components_click_mounts_dropdown_in_rsx_and_builder() {
        use crate::app::portal_overlay::{
            clear_overlay_stack, try_close_top, try_dismiss_outside_press,
        };
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::core::reactivity::{create_effect, create_root};
        use crate::core::tree::find_by_attr;

        for rsx in [true, false] {
            let id = if rsx {
                "nm-click-rsx"
            } else {
                "nm-click-builder"
            };
            clear_overlay_stack();
            reset_registered_anchors();
            let live = Rc::new(RefCell::new(Element::new()));
            install_test_reconcile_root(live.clone());
            let (owner, (expanded, open, picked)) = create_root(|| {
                let expanded = use_signal(&format!("{id}__navmenu_expanded"), || None::<String>);
                let open = use_signal(&format!("{id}:comp__open"), || false);
                let picked = use_signal(&format!("{id}-picked"), || false);
                let live = live.clone();
                create_effect(move || {
                    *live.borrow_mut() = if rsx {
                        crate::ui! {
                            <NavigationMenu id={id}>
                                <NavigationMenuList>
                                    <NavigationMenuItem value="comp">
                                        <NavigationMenuTrigger>"Components"</NavigationMenuTrigger>
                                        <NavigationMenuContent>
                                            <NavigationMenuLink id="dialog" on_click={move || picked.set(true)}>
                                                "Dialog"
                                            </NavigationMenuLink>
                                        </NavigationMenuContent>
                                    </NavigationMenuItem>
                                </NavigationMenuList>
                            </NavigationMenu>
                        }
                    } else {
                        navigation_menu(id)
                            .trigger(
                                "comp",
                                "Components",
                                vec![NavigationMenuLinkData::new("dialog", "Dialog", move || {
                                    picked.set(true)
                                })],
                            )
                            .into()
                    };
                });
                (expanded, open, picked)
            });
            drain_deferred_view_remounts();
            assert!(find_by_attr(&live.borrow(), ATTR_SLOT, SLOT_CONTENT).is_none());
            let click = find_by_attr(&live.borrow(), ATTR_SLOT, SLOT_TRIGGER)
                .expect("Components trigger must mount")
                .on_click
                .clone()
                .expect("Components click handler");
            click.borrow_mut()();
            drain_deferred_view_remounts();
            assert_eq!(expanded.peek().as_deref(), Some("comp"));
            assert!(open.peek(), "Components must open its portal signal");
            assert!(
                find_by_attr(&live.borrow(), ATTR_SLOT, SLOT_CONTENT).is_some(),
                "Components click must mount its dropdown (rsx={rsx})"
            );

            assert!(try_close_top(), "Escape must close the open dropdown");
            assert!(!open.peek());
            assert!(
                expanded.peek().is_none(),
                "Escape must clear expanded state"
            );
            click.borrow_mut()();
            assert!(open.peek(), "one click after Escape must reopen");

            // The event loop arms outside dismissal at the start of a pointer press.
            crate::utils::popup::arm_underlay_close();
            assert!(try_dismiss_outside_press());
            assert!(!open.peek());
            assert!(
                expanded.peek().is_none(),
                "outside press must clear expanded state"
            );
            click.borrow_mut()();
            assert!(open.peek(), "one click after outside dismissal must reopen");

            let select = crate::core::tree::find_in_tree(&live.borrow(), |el| {
                el.focus_id.as_deref() == Some("dialog")
            })
            .expect("dropdown link must mount")
            .on_click
            .clone()
            .unwrap();
            select.borrow_mut()();
            assert!(
                picked.peek(),
                "dropdown link must run its selection handler"
            );
            assert!(!open.peek());
            assert!(expanded.peek().is_none());
            click.borrow_mut()();
            assert!(open.peek());
            expanded.set(Some("other".into()));
            assert!(!open.peek());
            assert_eq!(expanded.peek().as_deref(), Some("other"));
            click.borrow_mut()();
            assert!(open.peek());
            click.borrow_mut()();
            assert!(!open.peek(), "second trigger click must close");
            clear_test_reconcile_root();
            drop(live);
            owner.dispose();
            clear_overlay_stack();
            reset_registered_anchors();
        }
    }

    #[test]
    fn navigation_menu_scope_exposes_template() {
        let el = navigation_menu_scope("nm-scope", true, || {
            NavigationMenuList(NavigationMenuListProps {
                children: Some(navigation_menu_item(Some("docs".into()), || {
                    NavigationMenuLink(NavigationMenuLinkProps {
                        id: "docs".into(),
                        on_click: Some(Handler::default()),
                        children: Some(Element::new().text("Docs")),
                        base: None,
                    })
                })),
                base: None,
            })
        });
        assert_eq!(el.template_id.as_deref(), Some("nm-scope__navigation_menu"));
        assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
    }

    #[test]
    fn builder_builds_compound() {
        let el: Element = navigation_menu("nm-b")
            .link("docs", "Docs", || {})
            .trigger(
                "comp",
                "Components",
                vec![NavigationMenuLinkData::new("a", "Alert", || {}).description("Modal")],
            )
            .into();
        assert!(
            el.template_id
                .as_deref()
                .is_some_and(|id| id.contains("navigation_menu")),
            "builder must route through scope"
        );
        assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
    }

    #[test]
    fn builder_keeps_list_under_root_not_unpacked() {
        let el: Element = navigation_menu("nm-layout")
            .link("a", "A", || {})
            .link("b", "B", || {})
            .into();
        assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
        // Root children: List (+ optional Viewport). Items must live under List, not root.
        let list = el
            .children
            .iter()
            .find(|c| c.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_LIST))
            .expect("NavigationMenuList must remain a child of root");
        assert!(
            list.children.len() >= 2,
            "List should hold items; got {} children (root kids={})",
            list.children.len(),
            el.children.len()
        );
        let root_items = el
            .children
            .iter()
            .filter(|c| c.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_ITEM))
            .count();
        assert_eq!(
            root_items, 0,
            "Items must not be unpacked into root vstack (that caused vertical chaos)"
        );
        let vp = el
            .children
            .iter()
            .find(|c| c.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_VIEWPORT))
            .expect("default Viewport slot");
        // Viewport must hug / zero — w(100%) stretches the shell across NavbarCenter.
        let w = crate::layout::dim_length_px(vp.style.size.width);
        assert!(
            w.is_some_and(|px| px < 1.0),
            "Viewport width must be ~0px, got {w:?}"
        );
    }
}
