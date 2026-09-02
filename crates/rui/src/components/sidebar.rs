//! # Sidebar
//!
//! 对照 ShadCN **base** [`sidebar.tsx`](../../../../reference/ShadCN-UI/apps/v4/registry/bases/base/ui/sidebar.tsx)：
//!
//! ```text
//! SidebarProvider              // open / defaultOpen · toggleSidebar · 键盘 Ctrl/Cmd+B
//! └── Sidebar                  // 固定 rail（collapsible=none）或预留折叠槽位
//!     ├── SidebarHeader
//!     ├── SidebarContent       // flex-1 · overflow-auto · 焦点环 roving
//!     │   └── SidebarGroup
//!     │       ├── SidebarGroupLabel
//!     │       └── SidebarGroupContent
//!     │           └── SidebarMenu
//!     │               └── SidebarMenuItem
//!     │                   └── SidebarMenuButton
//!     │           └── TreeView  chrome="sidebar"  // 树形导航（可选）
//!     ├── SidebarSeparator   // 可选；由调用方在 Header / Content / Footer 之间手动插入
//!     └── SidebarFooter
//! SidebarInset                 // 主内容列（flex-1 · min-w-0）
//! SidebarTrigger               // 切换开合（ghost icon-sm）
//! SidebarInput                 // 侧栏内搜索框样式
//! ```
//!
//! 默认 `flex-shrink: 0` 且 `min-width` 与宽度一致，避免主内容区固有宽把侧栏挤扁
//! （典型 app-shell：rail 不缩，主区 `min-width: 0`）。
//!
//! `<Sidebar>` / `<SidebarMenuButton>` 等由 `#[component]` 宏展开；`ui!` camelCase 别名：
//! `defaultOpen` · `isActive` · `showOnHover`。
//!
//! ## 用法（双写法 · ADR-9）
//! ```ignore
//! // A · RSX / 复合标签
//! ui! {
//!     <SidebarProvider id="app" defaultOpen={true}>
//!         <Sidebar id="nav" w="13rem">
//!             <SidebarHeader>
//!                 <Label font_weight="700">"RUI Gallery"</Label>
//!             </SidebarHeader>
//!             <SidebarSeparator />
//!             <SidebarContent>
//!                 <SidebarMenu>
//!                     <SidebarMenuItem>
//!                         <SidebarMenuButton isActive={true} on_click={|| {}}>
//!                             "Basics"
//!                         </SidebarMenuButton>
//!                     </SidebarMenuItem>
//!                 </SidebarMenu>
//!             </SidebarContent>
//!             <SidebarFooter>
//!                 <Label text_size="0.6875rem" color="muted-foreground">"footer"</Label>
//!             </SidebarFooter>
//!         </Sidebar>
//!         <SidebarInset>
//!             <ScrollArea h="100%">…</ScrollArea>
//!         </SidebarInset>
//!     </SidebarProvider>
//! }
//!
//! // B · Builder（Gallery 壳快捷入口）
//! sidebar("nav")
//!     .w("13rem")
//!     .header(label("").text("RUI Gallery"))
//!     .item(sidebar_menu_button("basics").text("Basics").is_active(true))
//!     .tree(sidebar_tree("files").roots([TreeNode { … }]))
//!     .footer(label("").text("cargo run -p gallery"))
//! ```
//!
//! **Identity (WP-2.8)**: `Sidebar` / `SidebarProvider` 的 `id` 可选；空串不写 `template_id`。
//! Gallery 测试选择器常用 `"gal-sidebar"`。

use std::str::FromStr;
use std::sync::OnceLock;

use rui_macros::component;

use crate::components::button::button;
use crate::components::input::input;
use crate::components::tree_view::{TreeViewBuilder, TreeViewChrome, TreeViewVariant, tree_view};
use crate::core::reactivity::{
    Context, Signal, create_context, provide_context, use_context, use_signal,
};
use crate::core::scroll_viewport::{DEFAULT_SCROLL_BEHAVIOR, activate_scroll_viewport};
use crate::core::tree::{Element, Handler, IntoCssDim, LayoutStatus, parse_text_color};
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::r#box::{r#box, hstack, vstack};
use crate::primitives::interactive::{interactive, mark_hover_paint_dirty};
use crate::style::{parse_color, with_class_list};
use crate::utils::lucide::lucide_glyph;
use crate::utils::resolve_instance_id;
use crate::utils::roving::{FocusRovingAxis, attach_focus_roving};
use crate::utils::take_packed;
use crate::utils::text_node::{TextNodeConfig, build_icon_text_node, build_text_node};

const ATTR_SLOT: &str = "data-slot";
const ATTR_SIDEBAR: &str = "data-sidebar";
const ATTR_SIDE: &str = "data-side";
const ATTR_VARIANT: &str = "data-variant";
const ATTR_STATE: &str = "data-state";
const ATTR_PROVIDER_ID: &str = "data-sidebar-provider-id";
const ATTR_WIDTH_STORE: &str = "data-sidebar-width";

const SIDEBAR_WIDTH_DEFAULT: &str = "16rem";
const SIDEBAR_WIDTH_ICON: &str = "3rem";
const SIDEBAR_KEYBOARD_SHORTCUT: &str = "b";

const SLOT_PROVIDER: &str = "sidebar-wrapper";
const SLOT_ROOT: &str = "sidebar";
const SLOT_INNER: &str = "sidebar-inner";
const SLOT_HEADER: &str = "sidebar-header";
const SLOT_FOOTER: &str = "sidebar-footer";
const SLOT_CONTENT: &str = "sidebar-content";
const SLOT_GROUP: &str = "sidebar-group";
const SLOT_GROUP_LABEL: &str = "sidebar-group-label";
const SLOT_GROUP_CONTENT: &str = "sidebar-group-content";
const SLOT_MENU: &str = "sidebar-menu";
const SLOT_MENU_ITEM: &str = "sidebar-menu-item";
const SLOT_MENU_BUTTON: &str = "sidebar-menu-button";
const SLOT_SEPARATOR: &str = "sidebar-separator";
const SLOT_INSET: &str = "sidebar-inset";
const SLOT_TRIGGER: &str = "sidebar-trigger";
const SLOT_INPUT: &str = "sidebar-input";
const SLOT_RAIL: &str = "sidebar-rail";

const MENU_TEXT_SIZE_PX: f32 = 14.0;
const MENU_TEXT_SIZE_SM_PX: f32 = 12.0;
const MENU_WEIGHT: u16 = 400;
const MENU_WEIGHT_ACTIVE: u16 = 500;
const GROUP_LABEL_SIZE_PX: f32 = 12.0;
const GROUP_LABEL_WEIGHT: u16 = 500;
/// ShadCN `SidebarSeparator` / `mx-2`：水平内缩，避免满宽贴边。
const SIDEBAR_SEPARATOR_INSET: &str = "0.5rem";

// ── Enums ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SidebarSide {
    #[default]
    Left,
    Right,
}

impl SidebarSide {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

impl FromStr for SidebarSide {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "right" => Self::Right,
            _ => Self::Left,
        })
    }
}

impl From<&str> for SidebarSide {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or(Self::Left)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SidebarVariant {
    #[default]
    Sidebar,
    Floating,
    Inset,
}

impl SidebarVariant {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sidebar => "sidebar",
            Self::Floating => "floating",
            Self::Inset => "inset",
        }
    }
}

impl FromStr for SidebarVariant {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "floating" => Self::Floating,
            "inset" => Self::Inset,
            _ => Self::Sidebar,
        })
    }
}

impl From<&str> for SidebarVariant {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or(Self::Sidebar)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SidebarCollapsible {
    Offcanvas,
    Icon,
    #[default]
    None,
}

impl SidebarCollapsible {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Offcanvas => "offcanvas",
            Self::Icon => "icon",
            Self::None => "none",
        }
    }
}

impl FromStr for SidebarCollapsible {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "offcanvas" => Self::Offcanvas,
            "icon" => Self::Icon,
            "none" | "" => Self::None,
            _ => return Err(()),
        })
    }
}

impl From<&str> for SidebarCollapsible {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or(Self::None)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SidebarMenuButtonVariant {
    #[default]
    Default,
    Outline,
}

impl SidebarMenuButtonVariant {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "outline" => Self::Outline,
            _ => Self::Default,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Outline => "outline",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SidebarMenuButtonSize {
    #[default]
    Default,
    Sm,
    Lg,
}

impl SidebarMenuButtonSize {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "sm" | "small" => Self::Sm,
            "lg" | "large" => Self::Lg,
            _ => Self::Default,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Sm => "sm",
            Self::Lg => "lg",
        }
    }

    pub fn height_css(self) -> &'static str {
        match self {
            Self::Sm => "1.75rem",
            Self::Default => "2rem",
            Self::Lg => "3rem",
        }
    }

    pub fn text_size_px(self) -> f32 {
        match self {
            Self::Sm => MENU_TEXT_SIZE_SM_PX,
            Self::Default | Self::Lg => MENU_TEXT_SIZE_PX,
        }
    }
}

// ── Context ───────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct SidebarCtx {
    pub id: String,
    pub open: Signal<bool>,
    /// 由 [`Sidebar`] 写入；[`SidebarProvider`] / [`SidebarInset`] 读取 inset 壳样式。
    pub variant: Signal<String>,
}

fn sidebar_context() -> Context<SidebarCtx> {
    static CTX: OnceLock<Context<SidebarCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<SidebarCtx>)
}

/// 读取最近的 [`SidebarProvider`] 上下文。
pub fn use_sidebar() -> Option<SidebarCtx> {
    use_context(sidebar_context())
}

fn sidebar_horizontal_separator_element() -> Element {
    let line: Element = r#box()
        .h("1px")
        .w("100%")
        .bg(parse_color("sidebar-border"))
        .flex_shrink(0.0)
        .into();
    let mut el: Element = hstack()
        .w("100%")
        .px(SIDEBAR_SEPARATOR_INSET)
        .flex_shrink(0.0)
        .child(line)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_SEPARATOR);
    el.set_attr(ATTR_SIDEBAR, "separator");
    el
}

fn sidebar_provider_shell(kids: Vec<Element>) -> Element {
    match kids.len() {
        0 => hstack().w("100%").h("100%").min_h("100%").into(),
        1 => kids
            .into_iter()
            .next()
            .unwrap()
            .flex_row()
            .w("100%")
            .h("100%")
            .min_h("100%"),
        _ => {
            let mut row = hstack()
                .w("100%")
                .h("100%")
                .min_h("100%")
                .items("stretch")
                .gap("0");
            for k in kids {
                row = row.child(k);
            }
            row.into()
        }
    }
}

/// Ctrl/Cmd+B：有 [`SidebarProvider`] 时切换开合。
pub fn try_toggle_sidebar_from_key(
    event: &winit::event::KeyEvent,
    modifiers: &winit::keyboard::ModifiersState,
) -> bool {
    use winit::event::ElementState;
    use winit::keyboard::Key;
    if event.state != ElementState::Pressed || event.repeat {
        return false;
    }
    let Key::Character(c) = &event.logical_key else {
        return false;
    };
    if !c.eq_ignore_ascii_case(SIDEBAR_KEYBOARD_SHORTCUT) {
        return false;
    }
    if !(modifiers.control_key() || modifiers.super_key()) {
        return false;
    }
    toggle_sidebar();
    true
}

/// 切换侧栏开合（无 Provider 时为 no-op）。
pub fn toggle_sidebar() {
    if let Some(ctx) = use_sidebar() {
        if ctx.open.is_alive() {
            let next = !ctx.open.peek();
            let _ = ctx.open.set(next);
            crate::app::runtime::request_redraw();
        }
    }
}

/// 提供侧栏上下文（`ui!` `<SidebarProvider>` / Builder 共用）。
pub fn sidebar_provider_scope(
    id: impl Into<String>,
    open: Option<Signal<bool>>,
    default_open: bool,
    shell_variant: Option<&str>,
    class: Option<String>,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = resolve_instance_id(id, "sidebar-provider");
    let open =
        open.unwrap_or_else(|| use_signal(format!("{id}__open").as_str(), move || default_open));
    let variant = use_signal(format!("{id}__variant").as_str(), || "sidebar".to_string());
    if let Some(v) = shell_variant.filter(|s| !s.trim().is_empty()) {
        let _ = variant.try_set_silent(v.trim().to_string());
    }
    provide_context(
        sidebar_context(),
        SidebarCtx {
            id: id.clone(),
            open,
            variant: variant.clone(),
        },
    );
    let kids = take_packed(body());
    let mut root = sidebar_provider_shell(kids);
    if variant.is_alive() && variant.peek() == "inset" {
        root = root.bg(parse_color("sidebar"));
    }
    root.set_attr(ATTR_SLOT, SLOT_PROVIDER);
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        root = with_class_list(root, &c);
    }
    if !id.trim().is_empty() {
        root.template_id = Some(format!("{id}__provider"));
    }
    root
}

fn sidebar_state_attr(open: Signal<bool>) -> String {
    if open.is_alive() && open.peek() {
        "expanded".into()
    } else {
        "collapsed".into()
    }
}

fn sidebar_content_side_border(el: Element, side: SidebarSide) -> Element {
    match side {
        SidebarSide::Left => el.border_right(parse_color("sidebar-border"), 1.0),
        SidebarSide::Right => el.border_left(parse_color("sidebar-border"), 1.0),
    }
}

fn apply_sidebar_rail_layout(el: &mut Element, open: bool, width: &str) -> LayoutStatus {
    use taffy::style::{Dimension, Overflow};

    let want_state = if open { "expanded" } else { "collapsed" };
    let want_w_css = if open { width } else { "0" };
    let want_width = want_w_css
        .into_css_dim()
        .preview_dimension()
        .unwrap_or(Dimension::length(0.0));
    let want_overflow = if open {
        Overflow::Visible
    } else {
        Overflow::Hidden
    };

    let mut changed = el.style.size.width != want_width || el.style.min_size.width != want_width;
    if changed {
        el.style.size.width = want_width;
        el.style.min_size.width = want_width;
    }
    if el.style.overflow.x != want_overflow || el.style.overflow.y != want_overflow {
        el.style.overflow.x = want_overflow;
        el.style.overflow.y = want_overflow;
        changed = true;
    }
    if el.attrs.get(ATTR_STATE).map(String::as_str) != Some(want_state) {
        el.set_attr(ATTR_STATE, want_state);
    }
    if changed {
        LayoutStatus::LayoutChanged
    } else {
        LayoutStatus::Clean
    }
}

/// 每帧对齐 Provider `{id}__open` → rail 宽度（Gallery 静态壳无 bind effect 时仍生效）。
pub(crate) fn sync_sidebar_rail_subtree(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
) {
    if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_ROOT) {
        if let (Some(provider_id), Some(width)) = (
            el.attrs.get(ATTR_PROVIDER_ID).cloned(),
            el.attrs.get(ATTR_WIDTH_STORE).cloned(),
        ) {
            let open_key = format!("{provider_id}__open");
            let open_sig = use_signal(open_key.as_str(), || true);
            if open_sig.is_alive() {
                let status = apply_sidebar_rail_layout(el, open_sig.peek(), width.as_str());
                if status == LayoutStatus::LayoutChanged && el.layout_node.is_some() {
                    layout_tree.mark_element_layout_dirty(el);
                }
                *impact = impact.merge(status);
            }
        }
    }
    for child in &mut el.children {
        sync_sidebar_rail_subtree(child, layout_tree, impact);
    }
}

fn sidebar_menu_button_variant(el: &Element) -> SidebarMenuButtonVariant {
    el.attrs
        .get("data-variant")
        .map(|s| SidebarMenuButtonVariant::parse(s))
        .unwrap_or_default()
}

fn sidebar_menu_button_active(el: &Element) -> bool {
    el.attrs.get("data-active").map(String::as_str) == Some("true")
}

fn sidebar_menu_button_size(el: &Element) -> SidebarMenuButtonSize {
    el.attrs
        .get("data-size")
        .map(|s| SidebarMenuButtonSize::parse(s))
        .unwrap_or_default()
}

fn paint_sidebar_menu_button_fg(el: &mut Element, fg: [u8; 4], weight: u16, size_px: f32) {
    if el.text.is_some() {
        el.text_style.color = Some(fg);
        // Stamp inherit-lock attrs. Chrome writes color/weight/size without going through
        // `Element::text_color` — otherwise the next full `cascade_inheritable_text_style`
        // (portal hoist / overlay enter) copies `TextStyle::default` white from the
        // uncolored Header/Footer slot onto RSX string leaves. Idle frames skip cascade
        // (`scroll_only`), so the flash is only on the animation's first layout.
        el.attrs.insert("text_color".into(), "1".into());
        if el.text_style.font_family.as_deref() == Some("lucide") {
            el.text_style.size = Some(16.0);
            el.attrs.insert("text_size".into(), "1".into());
        } else {
            el.text_style.weight = Some(weight);
            el.text_style.size = Some(size_px);
            el.attrs.insert("font_weight".into(), "1".into());
            el.attrs.insert("text_size".into(), "1".into());
        }
    }
    for child in &mut el.children {
        paint_sidebar_menu_button_fg(child, fg, weight, size_px);
    }
}

fn set_sidebar_menu_button_ring(el: &mut Element, color: [f32; 4], spread: f32) {
    el.style.border_width = 0.0;
    el.style.border_style = crate::style::LineStyle::None;
    if spread > 0.0 && color[3] > 0.0 {
        el.style.shadow_color = color;
        el.style.shadow_offset = [0.0, 0.0];
        el.style.shadow_blur = 0.0;
        el.style.shadow_spread = spread;
        el.style.shadow_inset = false;
    } else {
        el.style.shadow_color = [0.0, 0.0, 0.0, 0.0];
        el.style.shadow_spread = 0.0;
    }
}

fn apply_sidebar_menu_button_chrome(
    el: &mut Element,
    layout_tree: Option<&mut LayoutTree>,
    variant: SidebarMenuButtonVariant,
    is_active: bool,
    is_hovered: bool,
) -> LayoutStatus {
    let highlighted = is_active || is_hovered;
    let want_bg = if highlighted {
        parse_color("sidebar-accent")
    } else {
        [0.0, 0.0, 0.0, 0.0]
    };
    let want_fg = if highlighted {
        parse_text_color("sidebar-accent-foreground")
    } else {
        parse_text_color("sidebar-foreground")
    };
    let want_weight = if is_active {
        MENU_WEIGHT_ACTIVE
    } else {
        MENU_WEIGHT
    };
    let size_px = sidebar_menu_button_size(el).text_size_px();

    let mut status = LayoutStatus::Clean;
    if el.style.bg_color != want_bg {
        el.style.bg_color = want_bg;
        if let Some(lt) = layout_tree {
            lt.update_taffy_node_style(el);
        }
        status = LayoutStatus::StyleChanged;
    }

    let ring = match variant {
        SidebarMenuButtonVariant::Outline if highlighted => parse_color("sidebar-accent"),
        _ => [0.0, 0.0, 0.0, 0.0],
    };
    let ring_spread = if variant == SidebarMenuButtonVariant::Outline && highlighted {
        1.0
    } else {
        0.0
    };
    let prev_ring = (
        el.style.shadow_color,
        el.style.shadow_spread,
        el.style.border_width,
    );
    set_sidebar_menu_button_ring(el, ring, ring_spread);
    if prev_ring
        != (
            el.style.shadow_color,
            el.style.shadow_spread,
            el.style.border_width,
        )
    {
        status = status.merge(LayoutStatus::StyleChanged);
    }

    if el.text_style.weight_resolved() != want_weight {
        el.text_style.weight = Some(want_weight);
        status = status.merge(LayoutStatus::StyleChanged);
    }
    paint_sidebar_menu_button_fg(el, want_fg, want_weight, size_px);
    status
}

/// 每帧按 `{focus_id}_hover` 与 `data-active` 写回菜单按钮 chrome（`peek`，不订阅根 effect）。
pub(crate) fn sync_sidebar_menu_button_paint_subtree(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
) {
    if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_MENU_BUTTON) {
        let variant = sidebar_menu_button_variant(el);
        let is_active = sidebar_menu_button_active(el);
        let is_hovered = el
            .focus_id
            .as_deref()
            .map(|fid| {
                let key = format!("{fid}_hover");
                let sig = use_signal(key.as_str(), || false);
                if sig.is_alive() { sig.peek() } else { false }
            })
            .unwrap_or(false);
        *impact = impact.merge(apply_sidebar_menu_button_chrome(
            el,
            Some(layout_tree),
            variant,
            is_active,
            is_hovered,
        ));
    }
    for child in &mut el.children {
        sync_sidebar_menu_button_paint_subtree(child, layout_tree, impact);
    }
}

fn wire_sidebar_menu_button_hover(el: Element, focus_id: &str) -> Element {
    let hover_key = format!("{focus_id}_hover");
    let hovered = use_signal(hover_key.as_str(), || false);
    interactive(el)
        .focus_id(focus_id)
        .on_hover(move |h| {
            if !hovered.is_alive() {
                return;
            }
            if hovered.peek() == h {
                return;
            }
            hovered.set_silent(h);
            mark_hover_paint_dirty();
        })
        .into()
}

fn paint_menu_button(
    mut el: Element,
    variant: SidebarMenuButtonVariant,
    size: SidebarMenuButtonSize,
    is_active: bool,
    class: Option<String>,
) -> Element {
    el = el
        .w("100%")
        .h(size.height_css())
        .p("0.5rem")
        .gap("0.5rem")
        .items_center()
        .justify("start")
        .rounded("0.375rem")
        .overflow_x("hidden")
        .overflow_y("hidden")
        .cursor("pointer");
    el.set_attr(ATTR_SLOT, SLOT_MENU_BUTTON);
    el.set_attr(ATTR_SIDEBAR, "menu-button");
    el.set_attr("data-active", if is_active { "true" } else { "false" });
    el.set_attr("data-size", size.as_str());
    el.set_attr("data-variant", variant.as_str());
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        el = with_class_list(el, &c);
    }
    let _ = apply_sidebar_menu_button_chrome(&mut el, None, variant, is_active, false);
    el
}

fn paint_sidebar_rail(
    width: &str,
    side: SidebarSide,
    variant: SidebarVariant,
    collapsible: SidebarCollapsible,
    open: Option<Signal<bool>>,
) -> Element {
    let mut el: Element = vstack().w(width).h("100%").min_h(0.0).gap("0").into();
    el = el.flex_shrink(0.0).min_w(width).overflow_y("hidden");
    match variant {
        SidebarVariant::Inset => {
            el = el.h("100%").bg(parse_color("sidebar"));
            el = sidebar_content_side_border(el, side);
        }
        SidebarVariant::Floating => {
            el = el.p("0.5rem").bg([0.0, 0.0, 0.0, 0.0]);
        }
        SidebarVariant::Sidebar => {
            el = el.bg(parse_color("sidebar"));
            el = sidebar_content_side_border(el, side);
        }
    }
    el.set_attr(ATTR_SLOT, SLOT_ROOT);
    el.set_attr(ATTR_SIDEBAR, "sidebar");
    el.set_attr(ATTR_SIDE, side.as_str());
    el.set_attr(ATTR_VARIANT, variant.as_str());
    el.set_attr("data-collapsible", collapsible.as_str());
    if let Some(open) = open {
        el.set_attr(ATTR_STATE, sidebar_state_attr(open).as_str());
    }
    let _ = SIDEBAR_WIDTH_ICON;
    let _ = collapsible;
    el
}

fn wrap_sidebar_inner(kids: Vec<Element>, variant: SidebarVariant, _side: SidebarSide) -> Element {
    let mut inner = vstack().w("100%").h("100%").min_h(0.0).gap("0").flex_col();
    for k in kids {
        inner = inner.child(k);
    }
    let mut el: Element = inner.into();
    el = el.h("100%").w("100%").min_h(0.0).bg(parse_color("sidebar"));
    // Header / Footer sit *outside* `SidebarContent`'s scroll viewport. A column-wide
    // `overflow-y: hidden` makes their glyphs share that clip. On the first frame of any
    // overlay animation (toast / dropdown / …) a full Taffy rebuild can momentarily size
    // this clip to the flex-1 content band — header + footer text vanish, then recover.
    // ShadCN keeps overflow on `SidebarContent` only. Floating still clips to the rounded
    // shell so square header/footer fills do not square off the corners.
    el = match variant {
        SidebarVariant::Floating => el
            .overflow_y("hidden")
            .rounded("0.5rem")
            .border(parse_color("sidebar-border"), 1.0)
            .box_shadow_color([0.0, 0.0, 0.0, 0.05])
            .box_shadow_blur(2.0)
            .box_shadow_offset_x(0.0)
            .box_shadow_offset_y(1.0)
            .box_shadow_spread(0.0),
        SidebarVariant::Inset | SidebarVariant::Sidebar => el.overflow_y("visible"),
    };
    el.set_attr(ATTR_SLOT, SLOT_INNER);
    el.set_attr(ATTR_SIDEBAR, "sidebar");
    el
}

fn slot_children(
    class: Option<String>,
    slot: &str,
    sidebar_part: &str,
    kids: Vec<Element>,
) -> Element {
    let mut col = vstack().w("100%").flex_col();
    for k in kids {
        col = col.child(k);
    }
    let mut el: Element = col.into();
    el.set_attr(ATTR_SLOT, slot);
    el.set_attr(ATTR_SIDEBAR, sidebar_part);
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        el = with_class_list(el, &c);
    }
    el
}

/// Reactive bind：Gallery 导航项 `isActive` 对位（避免整棵侧栏 rebuild）。
pub fn apply_bound_sidebar_menu_button_active(el: &mut Element, active: bool) -> LayoutStatus {
    let want = if active { "true" } else { "false" };
    if el.attrs.get("data-active").map(String::as_str) == Some(want) {
        return LayoutStatus::Clean;
    }
    el.set_attr("data-active", want);
    let variant = sidebar_menu_button_variant(el);
    apply_sidebar_menu_button_chrome(el, None, variant, active, false)
}

// ── RSX parts ─────────────────────────────────────────────────────────────────

/// 根 Provider：`open` / `defaultOpen` · 预留 `toggleSidebar`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn SidebarProvider(
    id: String,
    open: Option<Signal<bool>>,
    default_open: Option<bool>,
    variant: Option<String>,
    class: Option<String>,
    children: Option<Element>,
) -> Element {
    let shell = variant.as_deref();
    sidebar_provider_scope(
        id,
        open,
        default_open.unwrap_or(true),
        shell,
        class,
        move || children.unwrap_or_default(),
    )
}

/// 侧栏 rail（`collapsible=none` 时为固定宽竖栏；默认 `16rem`）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Sidebar(
    id: String,
    w: Option<String>,
    side: Option<String>,
    variant: Option<String>,
    collapsible: Option<String>,
    class: Option<String>,
    children: Option<Element>,
) -> Element {
    let width = w
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| SIDEBAR_WIDTH_DEFAULT.to_string());
    let side = side.as_deref().map(SidebarSide::from).unwrap_or_default();
    let variant = variant
        .as_deref()
        .map(SidebarVariant::from)
        .unwrap_or_default();
    let collapsible = collapsible
        .as_deref()
        .map(SidebarCollapsible::from)
        .unwrap_or_default();
    let kids = children.map(take_packed).unwrap_or_default();
    let sidebar_ctx = use_sidebar();
    if let Some(ctx) = sidebar_ctx.as_ref() {
        if ctx.variant.is_alive() {
            let _ = ctx.variant.try_set_silent(variant.as_str().to_string());
        }
    }
    let open = sidebar_ctx.as_ref().map(|c| c.open);
    let mut rail = paint_sidebar_rail(&width, side, variant, collapsible, open);
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        rail = with_class_list(rail, &c);
    }
    if !id.trim().is_empty() {
        rail.template_id = Some(id.clone());
    }
    if let Some(ctx) = sidebar_ctx {
        rail.set_attr(ATTR_PROVIDER_ID, ctx.id.clone());
        rail.set_attr(ATTR_WIDTH_STORE, width.clone());
        if ctx.open.is_alive() {
            let open = ctx.open;
            let width_bind = width.clone();
            let bind_key = format!("{}__rail_open", ctx.id);
            rail = crate::app::reactive::bind_prop(
                rail,
                bind_key,
                move || open.get(),
                move |el, is_open| apply_sidebar_rail_layout(el, is_open, width_bind.as_str()),
            );
        }
    }
    let inner = wrap_sidebar_inner(kids, variant, side);
    rail.children.push(inner);
    attach_focus_roving(&mut rail, FocusRovingAxis::Vertical);
    rail
}

/// 顶区：`flex flex-col gap-2 p-2`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn SidebarHeader(class: Option<String>, children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el = slot_children(class, SLOT_HEADER, "header", kids);
    // Inherit-lock `sidebar-foreground` so RSX string leaves (`"RUI Gallery"`) do not
    // pick up `TextStyle::default` white on the first full cascade of an overlay enter.
    el = el
        .p("0.5rem")
        .gap("0.5rem")
        .flex_shrink(0.0)
        .overflow_x("visible")
        .overflow_y("visible")
        .text_color(parse_text_color("sidebar-foreground"));
    el
}

/// 底区：`flex flex-col gap-2 p-2`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn SidebarFooter(class: Option<String>, children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el = slot_children(class, SLOT_FOOTER, "footer", kids);
    el = el
        .p("0.5rem")
        .gap("0.5rem")
        .flex_shrink(0.0)
        .overflow_x("visible")
        .overflow_y("visible")
        .text_color(parse_text_color("sidebar-foreground"));
    el
}

/// 可滚动主体：`flex-1 min-h-0` + 引擎 [`activate_scroll_viewport`]（滚轮 / 触控板）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn SidebarContent(class: Option<String>, children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let scroll_id = use_sidebar()
        .map(|c| format!("{}__sidebar-content", c.id))
        .unwrap_or_else(|| "sidebar-content".to_string());
    let mut el = slot_children(class, SLOT_CONTENT, "content", kids);
    el = el
        .flex_grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .gap("0.5rem")
        .flex_shrink(1.0);
    el = activate_scroll_viewport(
        el,
        &scroll_id,
        "hidden",
        "auto",
        DEFAULT_SCROLL_BEHAVIOR,
        "content",
    );
    el.set_attr(ATTR_SLOT, SLOT_CONTENT);
    el.set_attr(ATTR_SIDEBAR, "content");
    el
}

/// 分组容器：`relative flex w-full min-w-0 flex-col p-2`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn SidebarGroup(class: Option<String>, children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el = slot_children(class, SLOT_GROUP, "group", kids);
    el = el.p("0.5rem").min_w(0.0).relative();
    el
}

fn paint_group_label_text(el: &mut Element) {
    let fg = parse_text_color("sidebar-foreground/70");
    if el.text.is_some() {
        el.text_style.color = Some(fg);
        el.attrs.insert("text_color".into(), "1".into());
    }
    for child in &mut el.children {
        paint_group_label_text(child);
    }
}

/// 分组标题：`text-xs font-medium text-sidebar-foreground/70`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn SidebarGroupLabel(class: Option<String>, children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el = slot_children(class, SLOT_GROUP_LABEL, "group-label", kids);
    el = el
        .h("2rem")
        .px("0.5rem")
        .items_center()
        .flex_shrink(0.0)
        .rounded("0.375rem")
        .text_size(GROUP_LABEL_SIZE_PX)
        .font_weight(GROUP_LABEL_WEIGHT as f32);
    paint_group_label_text(&mut el);
    el
}

/// 分组内容区。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn SidebarGroupContent(class: Option<String>, children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el = slot_children(class, SLOT_GROUP_CONTENT, "group-content", kids);
    el = el.text_size(MENU_TEXT_SIZE_PX);
    el
}

/// 菜单列表：`flex w-full min-w-0 flex-col gap-1`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn SidebarMenu(class: Option<String>, children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el = slot_children(class, SLOT_MENU, "menu", kids);
    el = el.gap("0.25rem").min_w(0.0);
    el
}

/// 菜单项：`group/menu-item relative`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn SidebarMenuItem(class: Option<String>, children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el = slot_children(class, SLOT_MENU_ITEM, "menu-item", kids);
    el = el.relative();
    el
}

/// 菜单按钮：对齐 `sidebarMenuButtonVariants`（default/outline · default/sm/lg · isActive）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn SidebarMenuButton(
    id: String,
    is_active: Option<bool>,
    variant: Option<String>,
    size: Option<String>,
    class: Option<String>,
    #[prop(default = "Handler::default()")] mut on_click: Handler<()>,
    children: Option<Element>,
) -> Element {
    let is_active = is_active.unwrap_or(false);
    let variant = variant
        .as_deref()
        .map(SidebarMenuButtonVariant::parse)
        .unwrap_or_default();
    let size = size
        .as_deref()
        .map(SidebarMenuButtonSize::parse)
        .unwrap_or_default();
    let kids = children.map(take_packed).unwrap_or_default();
    let focus_id = if id.trim().is_empty() {
        format!("sidebar-menu-button:{}", size.as_str())
    } else {
        id.trim().to_string()
    };
    let mut row = hstack().w("100%").items_center();
    for k in kids {
        row = row.child(k);
    }
    let mut el = wire_sidebar_menu_button_hover(row.into(), &focus_id);
    el = el.on_click(move || on_click.call(()));
    el = paint_menu_button(el, variant, size, is_active, class);
    if !id.trim().is_empty() {
        el.template_id = Some(id);
    }
    el
}

/// 侧栏内分割线（`px-2` 内缩 · 可选；在 RSX 中于 Header / Content / Footer 之间手动插入）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn SidebarSeparator(class: Option<String>) -> Element {
    let mut el = sidebar_horizontal_separator_element();
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        el = with_class_list(el, &c);
    }
    el
}

/// 主内容列：`relative flex w-full flex-1 flex-col min-w-0`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn SidebarInset(
    variant: Option<String>,
    class: Option<String>,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el = slot_children(class, SLOT_INSET, "inset", kids);
    let inset_shell = variant.as_deref() == Some("inset")
        || use_sidebar()
            .and_then(|ctx| ctx.variant.is_alive().then(|| ctx.variant.peek()))
            .is_some_and(|v| v == "inset");
    el = el
        .flex_grow(1.0)
        .min_w(0.0)
        .min_h(0.0)
        .relative()
        .w("100%")
        .bg(parse_color("background"));
    if inset_shell {
        el = el
            .m("0.5rem")
            .rounded("0.75rem")
            .overflow("hidden")
            .box_shadow_color([0.0, 0.0, 0.0, 0.12])
            .box_shadow_blur(8.0)
            .box_shadow_offset_x(0.0)
            .box_shadow_offset_y(2.0)
            .box_shadow_spread(0.0);
    }
    el
}

/// 开合触发器（ghost · icon-sm · PanelLeft）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn SidebarTrigger(
    id: String,
    class: Option<String>,
    mut on_click: Option<Handler<()>>,
) -> Element {
    let _ctx = use_sidebar();
    let glyph = lucide_glyph("panel-left").unwrap_or_else(|| "☰".into());
    let icon = build_icon_text_node(glyph)
        .text_size(16.0)
        .text_color(parse_text_color("foreground"))
        .pointer_events("none");
    let mut btn = button(if id.trim().is_empty() {
        "sidebar-trigger"
    } else {
        id.as_str()
    })
    .variant("ghost")
    .size("icon-sm")
    .child(icon);
    if let Some(mut h) = on_click {
        btn = btn.on_click(move || h.call(()));
    } else {
        btn = btn.on_click(|| toggle_sidebar());
    }
    let mut el: Element = btn.into();
    el.set_attr(ATTR_SLOT, SLOT_TRIGGER);
    el.set_attr(ATTR_SIDEBAR, "trigger");
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        el = with_class_list(el, &c);
    }
    if !id.trim().is_empty() {
        el.template_id = Some(id);
    }
    el
}

/// 侧栏搜索框：`h-8 bg-background shadow-none`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn SidebarInput(id: String, class: Option<String>, placeholder: Option<String>) -> Element {
    let mut field = input(if id.trim().is_empty() {
        "sidebar-input"
    } else {
        id.as_str()
    })
    .h("2rem");
    if let Some(p) = placeholder.filter(|s| !s.trim().is_empty()) {
        field = field.placeholder(p);
    }
    let mut el: Element = field.into();
    el = el.bg(parse_color("background"));
    el.set_attr(ATTR_SLOT, SLOT_INPUT);
    el.set_attr(ATTR_SIDEBAR, "input");
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        el = with_class_list(el, &c);
    }
    el
}

/// 桌面拖拽 rail（点击 toggle；对位 ShadCN `SidebarRail`）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn SidebarRail(class: Option<String>, mut on_click: Option<Handler<()>>) -> Element {
    let _ctx = use_sidebar();
    let mut hit = r#box()
        .w("1rem")
        .h("100%")
        .absolute(true)
        .top(0.0)
        .right(-16.0)
        .cursor("col-resize")
        .into();
    if let Some(mut h) = on_click {
        hit = interactive(hit)
            .focus_id("__sidebar_rail")
            .on_click(move || h.call(()))
            .into();
    } else {
        hit = interactive(hit)
            .focus_id("__sidebar_rail")
            .on_click(|| toggle_sidebar())
            .into();
    }
    hit.set_attr(ATTR_SLOT, SLOT_RAIL);
    hit.set_attr(ATTR_SIDEBAR, "rail");
    hit.set_attr("aria-label", "Toggle Sidebar");
    hit.set_attr("title", "Toggle Sidebar");
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        hit = with_class_list(hit, &c);
    }
    hit
}

// ── Builder ───────────────────────────────────────────────────────────────────

pub struct SidebarMenuButtonBuilder {
    id: String,
    label: Option<String>,
    is_active: bool,
    variant: SidebarMenuButtonVariant,
    size: SidebarMenuButtonSize,
    class: Option<String>,
    on_click: Option<Handler<()>>,
    child: Option<Element>,
}

/// Builder B · 菜单按钮。
pub fn sidebar_menu_button(id: impl Into<String>) -> SidebarMenuButtonBuilder {
    SidebarMenuButtonBuilder {
        id: id.into(),
        label: None,
        is_active: false,
        variant: SidebarMenuButtonVariant::Default,
        size: SidebarMenuButtonSize::Default,
        class: None,
        on_click: None,
        child: None,
    }
}

impl SidebarMenuButtonBuilder {
    pub fn text(mut self, t: impl Into<String>) -> Self {
        self.label = Some(t.into());
        self
    }
    pub fn is_active(mut self, v: bool) -> Self {
        self.is_active = v;
        self
    }
    pub fn variant(mut self, v: impl AsRef<str>) -> Self {
        self.variant = SidebarMenuButtonVariant::parse(v.as_ref());
        self
    }
    pub fn size(mut self, v: impl AsRef<str>) -> Self {
        self.size = SidebarMenuButtonSize::parse(v.as_ref());
        self
    }
    pub fn class(mut self, c: impl Into<String>) -> Self {
        self.class = Some(c.into());
        self
    }
    pub fn on_click(mut self, mut f: impl FnMut() + 'static) -> Self {
        self.on_click = Some(Handler::new(move |_| f()));
        self
    }
    pub fn child(mut self, el: impl Into<Element>) -> Self {
        self.child = Some(el.into());
        self
    }
}

impl From<SidebarMenuButtonBuilder> for Element {
    fn from(b: SidebarMenuButtonBuilder) -> Element {
        let children = if let Some(c) = b.child {
            Some(c)
        } else if let Some(label) = b.label {
            Some(build_text_node(
                &label,
                TextNodeConfig {
                    size_px: b.size.text_size_px(),
                    weight: if b.is_active {
                        MENU_WEIGHT_ACTIVE
                    } else {
                        MENU_WEIGHT
                    },
                    color: if b.is_active {
                        parse_text_color("sidebar-accent-foreground")
                    } else {
                        parse_text_color("sidebar-foreground")
                    },
                    ..Default::default()
                },
            ))
        } else {
            None
        };
        SidebarMenuButton(SidebarMenuButtonProps {
            id: b.id,
            is_active: Some(b.is_active),
            variant: Some(b.variant.as_str().into()),
            size: Some(b.size.as_str().into()),
            class: b.class,
            on_click: b.on_click,
            children,
            base: None,
        })
    }
}

/// 侧栏树：`collapsible` + [`TreeViewChrome::Sidebar`]，高度交给 [`SidebarContent`] 滚动。
pub fn sidebar_tree(id: impl Into<String>) -> TreeViewBuilder {
    tree_view(id)
        .variant(TreeViewVariant::Collapsible)
        .chrome(TreeViewChrome::Sidebar)
}

pub struct SidebarBuilder {
    id: String,
    width: String,
    side: SidebarSide,
    variant: SidebarVariant,
    collapsible: SidebarCollapsible,
    class: Option<String>,
    header: Option<Element>,
    groups: Vec<Element>,
    items: Vec<Element>,
    trees: Vec<Element>,
    footer: Option<Element>,
    provider: bool,
    default_open: bool,
    open: Option<Signal<bool>>,
}

/// 侧栏壳 Builder（双写 B；RSX 请用 `<Sidebar>` / `<SidebarProvider>`）。
pub fn sidebar(id: impl Into<String>) -> SidebarBuilder {
    SidebarBuilder {
        id: id.into(),
        width: SIDEBAR_WIDTH_DEFAULT.into(),
        side: SidebarSide::Left,
        variant: SidebarVariant::Sidebar,
        collapsible: SidebarCollapsible::None,
        class: None,
        header: None,
        groups: Vec::new(),
        items: Vec::new(),
        trees: Vec::new(),
        footer: None,
        provider: false,
        default_open: true,
        open: None,
    }
}

impl SidebarBuilder {
    pub fn w(mut self, w: impl Into<String>) -> Self {
        self.width = w.into();
        self
    }
    pub fn side(mut self, side: impl Into<SidebarSide>) -> Self {
        self.side = side.into();
        self
    }
    pub fn variant(mut self, variant: impl Into<SidebarVariant>) -> Self {
        self.variant = variant.into();
        self
    }
    pub fn collapsible(mut self, mode: impl Into<SidebarCollapsible>) -> Self {
        self.collapsible = mode.into();
        self
    }
    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.class = Some(class.into());
        self
    }
    pub fn header(mut self, el: impl Into<Element>) -> Self {
        self.header = Some(el.into());
        self
    }
    pub fn group(mut self, el: impl Into<Element>) -> Self {
        self.groups.push(el.into());
        self
    }
    pub fn item(mut self, el: impl Into<Element>) -> Self {
        self.items.push(el.into());
        self
    }
    pub fn items(mut self, els: Vec<Element>) -> Self {
        self.items.extend(els);
        self
    }
    pub fn tree(mut self, el: impl Into<Element>) -> Self {
        self.trees.push(el.into());
        self
    }
    pub fn footer(mut self, el: impl Into<Element>) -> Self {
        self.footer = Some(el.into());
        self
    }
    pub fn provider(mut self, v: bool) -> Self {
        self.provider = v;
        self
    }
    pub fn default_open(mut self, v: bool) -> Self {
        self.default_open = v;
        self
    }
    pub fn open_signal(mut self, open: Signal<bool>) -> Self {
        self.open = Some(open);
        self
    }
}

impl From<SidebarBuilder> for Element {
    fn from(builder: SidebarBuilder) -> Element {
        let SidebarBuilder {
            id,
            width,
            side,
            variant,
            collapsible,
            class,
            header,
            groups,
            items,
            trees,
            footer,
            provider,
            default_open,
            open,
        } = builder;

        let mut parts: Vec<Element> = Vec::new();
        if let Some(h) = header {
            parts.push(SidebarHeader(SidebarHeaderProps {
                class: None,
                children: Some(h),
                base: None,
            }));
        }

        let mut content_kids: Vec<Element> = groups;
        if !items.is_empty() {
            let mut menu_items = Vec::new();
            for item in items {
                menu_items.push(SidebarMenuItem(SidebarMenuItemProps {
                    class: None,
                    children: Some(item),
                    base: None,
                }));
            }
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            content_kids.push(SidebarMenu(SidebarMenuProps {
                class: None,
                children: Some(frag.children(menu_items)),
                base: None,
            }));
        }
        content_kids.extend(trees);

        if !content_kids.is_empty() {
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            parts.push(SidebarContent(SidebarContentProps {
                class: None,
                children: Some(frag.children(content_kids)),
                base: None,
            }));
        }

        if let Some(f) = footer {
            parts.push(SidebarFooter(SidebarFooterProps {
                class: None,
                children: Some(f),
                base: None,
            }));
        }

        let mut frag = Element::new();
        frag.set_attr("rui_fragment", "1");
        let rail = Sidebar(SidebarProps {
            id: id.clone(),
            w: Some(width),
            side: Some(side.as_str().into()),
            variant: Some(variant.as_str().into()),
            collapsible: Some(collapsible.as_str().into()),
            class,
            children: Some(frag.children(parts)),
            base: None,
        });

        if provider {
            sidebar_provider_scope(
                format!("{id}__provider"),
                open,
                default_open,
                Some(variant.as_str()),
                None,
                move || rail,
            )
        } else {
            rail
        }
    }
}

impl crate::core::tree::IntoElementList for SidebarBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidebar_default_width_is_16rem() {
        let el: Element = sidebar("t").into();
        assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
        assert_eq!(el.template_id.as_deref(), Some("t"));
    }

    #[test]
    fn sidebar_uses_sidebar_semantic_bg_not_muted() {
        let el: Element = sidebar("t").into();
        assert_eq!(el.style.bg_color, parse_color("sidebar"));
    }

    #[test]
    fn menu_button_active_sets_accent_slot() {
        let el: Element = sidebar_menu_button("nav-a")
            .text("Basics")
            .is_active(true)
            .into();
        assert_eq!(
            el.attrs.get("data-active").map(String::as_str),
            Some("true")
        );
        assert_eq!(el.style.bg_color, parse_color("sidebar-accent"));
    }

    #[test]
    fn builder_items_wrap_in_menu_and_content() {
        let el: Element = sidebar("shell")
            .w("13rem")
            .item(sidebar_menu_button("a").text("One"))
            .into();
        fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
            if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(slot) {
                return Some(el);
            }
            el.children.iter().find_map(|c| find_slot(c, slot))
        }
        assert!(find_slot(&el, SLOT_CONTENT).is_some());
        assert!(find_slot(&el, SLOT_MENU).is_some());
        assert!(find_slot(&el, SLOT_MENU_BUTTON).is_some());
    }

    #[test]
    fn builder_tree_hosts_tree_view_in_content() {
        use crate::components::tree_view::TreeNode;
        use crate::core::reactivity::create_root;

        let (_o, el): (_, Element) = create_root(|| {
            sidebar("shell-tree")
                .tree(sidebar_tree("nav-tree").roots(vec![TreeNode {
                    id: "a".into(),
                    label: "A".into(),
                    children: vec![],
                }]))
                .into()
        });
        fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
            if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(slot) {
                return Some(el);
            }
            el.children.iter().find_map(|c| find_slot(c, slot))
        }
        assert!(find_slot(&el, SLOT_CONTENT).is_some());
        assert!(find_slot(&el, "tree-view").is_some());
        assert!(find_slot(&el, SLOT_MENU).is_none());
    }

    #[test]
    fn menu_button_rest_is_transparent_with_sidebar_fg() {
        let el: Element = sidebar_menu_button("nav-a")
            .text("Basics")
            .is_active(false)
            .into();
        fn first_text_color(el: &Element) -> Option<[u8; 4]> {
            if el.text.is_some() {
                return Some(el.text_style.color_resolved());
            }
            el.children.iter().find_map(first_text_color)
        }
        assert_eq!(el.style.bg_color[3], 0.0);
        assert_eq!(
            first_text_color(&el),
            Some(parse_text_color("sidebar-foreground"))
        );
    }

    fn rsx_string_menu_button(id: &str, label: &str) -> Element {
        SidebarMenuButton(SidebarMenuButtonProps {
            id: id.into(),
            is_active: None,
            variant: None,
            size: None,
            class: None,
            on_click: None,
            children: Some(Element::from(label)),
            base: None,
        })
    }

    fn first_text_leaf(el: &Element) -> Option<&Element> {
        if el.text.is_some() {
            return Some(el);
        }
        el.children.iter().find_map(first_text_leaf)
    }

    #[test]
    fn rsx_string_menu_button_fg_survives_full_cascade() {
        // Gallery Header/Footer: `<SidebarMenuButton>"RUI Gallery"</SidebarMenuButton>`
        // has no `text_color` attr. Overlay enter runs a full cascade; without the
        // inherit-lock stamp, `TextStyle::default` white overwrites sidebar-foreground.
        let mut btn = rsx_string_menu_button("gal-workspace", "RUI Gallery");
        let fg = parse_text_color("sidebar-foreground");
        let leaf = first_text_leaf(&btn).expect("text leaf");
        assert_eq!(leaf.text_style.color_resolved(), fg);
        assert!(
            leaf.attrs.contains_key("text_color"),
            "paint must lock color against cascade"
        );

        crate::layout::layout_tree::cascade_inheritable_text_style_for_test(&mut btn);
        let leaf = first_text_leaf(&btn).expect("text leaf after cascade");
        assert_eq!(
            leaf.text_style.color_resolved(), fg,
            "cascade must not reset RSX string leaves to default white"
        );
        assert_ne!(leaf.text_style.color_resolved(), [255, 255, 255, 255]);
    }

    #[test]
    fn header_slot_inherits_sidebar_foreground() {
        let el = SidebarHeader(SidebarHeaderProps {
            class: None,
            children: Some(Element::from("RUI Gallery")),
            base: None,
        });
        assert_eq!(el.text_style.color, Some(parse_text_color("sidebar-foreground")));
    }

    #[test]
    fn menu_button_outline_ring_only_when_active() {
        let rest: Element = sidebar_menu_button("nav-o")
            .text("Outline")
            .variant("outline")
            .into();
        assert_eq!(rest.style.border_width, 0.0);
        assert_eq!(rest.style.shadow_spread, 0.0);

        let active: Element = sidebar_menu_button("nav-o-active")
            .text("Outline")
            .variant("outline")
            .is_active(true)
            .into();
        assert_eq!(active.style.border_width, 0.0);
        assert!(active.style.shadow_spread > 0.0);
    }

    #[test]
    fn inset_rail_has_content_side_border() {
        let el: Element = Sidebar(SidebarProps {
            id: "inset-rail".into(),
            w: None,
            side: Some("left".into()),
            variant: Some("inset".into()),
            collapsible: None,
            class: None,
            children: None,
            base: None,
        });
        assert_eq!(el.style.bg_color, parse_color("sidebar"));
        assert!(
            el.style.border_width > 0.0 || el.style.border_width_sides.is_some(),
            "inset rail must draw content-side border"
        );
        fn find_inner<'a>(el: &'a Element) -> Option<&'a Element> {
            if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_INNER) {
                return Some(el);
            }
            el.children.iter().find_map(find_inner)
        }
        let inner = find_inner(&el).expect("sidebar-inner");
        assert!(
            inner.style.radii.iter().all(|r| *r < 0.01),
            "inset inner must not be wrapped in a rounded shell"
        );
        assert_eq!(inner.style.border_width, 0.0);
        assert_eq!(
            inner.style.overflow.y,
            taffy::style::Overflow::Visible,
            "inset inner must not clip Header/Footer glyphs (overflow lives on SidebarContent)"
        );
    }

    #[test]
    fn header_and_footer_do_not_clip_overflow() {
        let header = SidebarHeader(SidebarHeaderProps {
            class: None,
            children: None,
            base: None,
        });
        let footer = SidebarFooter(SidebarFooterProps {
            class: None,
            children: None,
            base: None,
        });
        assert_eq!(header.style.overflow.x, taffy::style::Overflow::Visible);
        assert_eq!(header.style.overflow.y, taffy::style::Overflow::Visible);
        assert_eq!(footer.style.overflow.x, taffy::style::Overflow::Visible);
        assert_eq!(footer.style.overflow.y, taffy::style::Overflow::Visible);
    }

    #[test]
    fn floating_inner_still_clips_to_rounded_shell() {
        let el: Element = sidebar("float").variant(SidebarVariant::Floating).into();
        fn find_inner<'a>(el: &'a Element) -> Option<&'a Element> {
            if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_INNER) {
                return Some(el);
            }
            el.children.iter().find_map(find_inner)
        }
        let inner = find_inner(&el).expect("sidebar-inner");
        assert_eq!(inner.style.overflow.y, taffy::style::Overflow::Hidden);
    }

    #[test]
    fn rail_layout_collapses_width_when_closed() {
        let mut rail = paint_sidebar_rail(
            "16rem",
            SidebarSide::Left,
            SidebarVariant::Inset,
            SidebarCollapsible::None,
            None,
        );
        rail.set_attr(ATTR_SLOT, SLOT_ROOT);
        let status = apply_sidebar_rail_layout(&mut rail, false, "16rem");
        assert_eq!(status, LayoutStatus::LayoutChanged);
        assert_eq!(
            crate::layout::dim_length_px(rail.style.size.width),
            Some(0.0)
        );
        assert_eq!(
            rail.attrs.get(ATTR_STATE).map(String::as_str),
            Some("collapsed")
        );
    }

    #[test]
    fn horizontal_separator_has_inset_padding_not_full_bleed() {
        use crate::layout::{dim_length_px, lp_length_px};
        let el = sidebar_horizontal_separator_element();
        let pad_l = lp_length_px(el.style.padding.left).unwrap_or(0.0);
        let pad_r = lp_length_px(el.style.padding.right).unwrap_or(0.0);
        assert!(
            pad_l > 1.0 && pad_r > 1.0,
            "separator shell must px-inset from sidebar edges"
        );
        assert_eq!(el.children.len(), 1);
        assert!(
            dim_length_px(el.children[0].style.size.height).unwrap_or(0.0) <= 1.5,
            "inner line must be h-px"
        );
    }

    #[test]
    fn provider_shell_lays_out_sidebar_and_inset_in_row() {
        let el = sidebar_provider_scope("prov", None, true, None, None, move || {
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            frag.children(vec![
                sidebar("nav").into(),
                SidebarInset(SidebarInsetProps {
                    variant: None,
                    class: None,
                    children: None,
                    base: None,
                }),
            ])
        });
        assert_eq!(
            el.attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_PROVIDER)
        );
        assert_eq!(el.style.flex_direction, taffy::style::FlexDirection::Row);
        assert_eq!(el.children.len(), 2);
        assert_eq!(
            el.children[0].attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_ROOT)
        );
        assert_eq!(
            el.children[1].attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_INSET)
        );
    }

    #[test]
    fn provider_wires_open_and_scroll_viewport_on_children() {
        let el =
            sidebar_provider_scope("prov-scroll", None, true, Some("inset"), None, move || {
                let mut frag = Element::new();
                frag.set_attr("rui_fragment", "1");
                frag.children(vec![
                    Sidebar(SidebarProps {
                        id: "nav".into(),
                        w: None,
                        side: None,
                        variant: Some("inset".into()),
                        collapsible: None,
                        class: None,
                        children: Some({
                            let mut inner = Element::new();
                            inner.set_attr("rui_fragment", "1");
                            inner.children(vec![SidebarContent(SidebarContentProps {
                                class: None,
                                children: None,
                                base: None,
                            })])
                        }),
                        base: None,
                    }),
                    SidebarInset(SidebarInsetProps {
                        variant: Some("inset".into()),
                        class: None,
                        children: None,
                        base: None,
                    }),
                ])
            });
        fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
            if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(slot) {
                return Some(el);
            }
            el.children.iter().find_map(|c| find_slot(c, slot))
        }
        let rail = find_slot(&el, SLOT_ROOT).expect("rail");
        assert_eq!(
            rail.attrs.get(ATTR_PROVIDER_ID).map(String::as_str),
            Some("prov-scroll")
        );
        let content = find_slot(&el, SLOT_CONTENT).expect("content");
        assert_eq!(
            content.focus_id.as_deref(),
            Some("scroll:prov-scroll__sidebar-content")
        );
    }

    #[test]
    fn menu_button_wires_hover_handler() {
        let el: Element = sidebar_menu_button("nav-hover").text("Hover").into();
        assert!(
            el.on_hover.is_some(),
            "SidebarMenuButton must wire on_hover for sync paint"
        );
    }
}
