//! # Sidebar
//!
//! 对照 ShadCN **base** [`sidebar.tsx`](../../../../reference/ShadCN-UI/apps/v4/registry/bases/base/ui/sidebar.tsx)：
//!
//! ```text
//! SidebarProvider              // open / defaultOpen · toggleSidebar · 键盘 Ctrl/Cmd+B
//! └── Sidebar                  // collapsible=none 固定宽；icon 收成图标轨；offcanvas 宽为 0
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
//! `collapsible="icon"` 收起对齐 ShadCN：rail 宽 200ms linear 收到 `3rem`；`SidebarGroupLabel` /
//! `SidebarInput` 高度与透明度收掉；菜单按钮 `size-8` + `p-2`（16px 内容区只留图标）；
//! 非图标文案淡出并宽度归零。选中只换强调色，不改 `font-weight`（cosmic-text 换字重会把标签顶下去）。
//!
//! `<Sidebar>` / `<SidebarMenuButton>` 等由 `#[component]` 宏展开；`ui!` camelCase 别名：
//! `defaultOpen` · `isActive` · `showOnHover`。
//! 轨宽是领域 prop `rail_w`（不是根布局 `w`）。RSX 写 `rail_w`；Builder `.w()` 是别名。
//! 见 `docs/COMPONENT_LIBRARY.md` §1.1。
//!
//! Provider 内可折叠侧栏在 `<768px` 时改为 Sheet：同一 Trigger / Ctrl/Cmd+B
//! 切换独立的 `use_sidebar().open_mobile`，保留桌面 `open`。`collapsible="none"`
//! 仍为固定侧栏。移动面板复用原内容、遮罩、Esc、焦点约束与滑入/退场动画。
//!
//! ## 用法（双写法 · ADR-9）
//! ```ignore
//! // A · RSX / 复合标签
//! ui! {
//!     <SidebarProvider id="app" defaultOpen={true}>
//!         <Sidebar id="nav" rail_w="13rem" collapsible="icon">
//!             <SidebarHeader>
//!                 <Label font_weight="700">"Jadix Gallery"</Label>
//!             </SidebarHeader>
//!             <SidebarSeparator />
//!             <SidebarContent>
//!                 <SidebarMenu>
//!                     <SidebarMenuItem>
//!                         <SidebarMenuButton isActive={section.get() == "basics"} on_click={|| {}}>
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
//!     .header(label("").text("Jadix Gallery"))
//!     .item(sidebar_menu_button("basics").text("Basics").is_active(true))
//!     .tree(sidebar_tree("files").roots([tree_node("src", "src")]))
//!     .footer(label("").text("cargo run -p gallery"))
//! ```
//!
//! **Identity (WP-2.8)**: `Sidebar` / `SidebarProvider` 的 `id` 可选；空串不写 `template_id`。
//! Gallery 测试选择器常用 `"gal-sidebar"`。

use std::cell::Cell;
use std::str::FromStr;
use std::sync::OnceLock;

use crate::components::base::{ensure_data_slot, SlotChrome, SlotStyleFn};
use crate::components::button::button;
use crate::components::input::input;
use crate::components::scrollbar::{scrollbar, ScrollbarOrientation};
use crate::components::sheet::{collect_sheet_focus_ids, sheet_shell_panel, SheetSide};
use crate::components::tree_view::{tree_view, TreeViewBuilder, TreeViewChrome, TreeViewVariant};
use crate::core::reactivity::{
    create_context, provide_context, use_context, use_signal, Context, Signal,
};
use crate::core::scroll_viewport::{activate_scroll_viewport, DEFAULT_SCROLL_BEHAVIOR};
use crate::core::tree::{parse_text_color, Element, Handler, IntoCssDim, LayoutStatus};
use crate::impl_simple_slot_builder;
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::interactive::{interactive, mark_hover_paint_dirty};
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::style::parse_color;
use crate::tokens;
use crate::utils::icon_provider::ATTR_ICON_SRC;
use crate::utils::resolve_instance_id;
use crate::utils::roving::{attach_focus_roving, FocusRovingAxis};
use crate::utils::take_packed;
use crate::utils::text_node::{build_icon_text_node, build_text_node, TextNodeConfig};
use crate::utils::viewport_modal::{
    dismiss_viewport_modal, retained_viewport_modal_layer, viewport_modal_phase,
    ViewportModalLayerOpts,
};
use crate::utils::{lucide_glyph, ATTR_ICON_NAME};

const ATTR_SLOT: &str = "data-slot";
const ATTR_SIDEBAR: &str = "data-sidebar";
const ATTR_SIDE: &str = "data-side";
const ATTR_VARIANT: &str = "data-variant";
const ATTR_STATE: &str = "data-state";
const ATTR_PROVIDER_ID: &str = "data-sidebar-provider-id";
const ATTR_WIDTH_STORE: &str = "data-sidebar-width";
const ATTR_COLLAPSIBLE: &str = "data-collapsible";
const ATTR_MOBILE_ID: &str = "data-sidebar-mobile-id";
const ATTR_MOBILE_PANEL: &str = "data-sidebar-mobile-panel";
/// Stashed `flex-grow` while icon-collapse zeros label width.
const ATTR_SAVED_GROW: &str = "data-sidebar-saved-grow";

const SIDEBAR_WIDTH_DEFAULT: &str = "16rem";
const SIDEBAR_WIDTH_ICON: &str = "3rem";
const SIDEBAR_WIDTH_MOBILE: &str = "18rem";
const SIDEBAR_MOBILE_BREAKPOINT: f32 = 768.0;

fn sidebar_viewport_width() -> f32 {
    crate::utils::modal_motion::modal_viewport_size().0
}
/// ShadCN `transition-[width] duration-200 ease-linear`.
const SIDEBAR_WIDTH_TRANSITION_MS: f32 = 200.0;
/// ShadCN `group-data-[collapsible=icon]:size-8`.
const SIDEBAR_MENU_BUTTON_ICON_PX: f32 = 32.0;
/// ShadCN menu-button / group-label `p-2` / `px-2`.
const SIDEBAR_PAD_PX: f32 = 8.0;
const SIDEBAR_GAP_PX: f32 = 8.0;
const SIDEBAR_GROUP_LABEL_H_PX: f32 = 32.0;
const SIDEBAR_KEYBOARD_SHORTCUT: &str = "b";

const SLOT_PROVIDER: &str = "sidebar-wrapper";
const SLOT_ROOT: &str = "sidebar";
const SLOT_INNER: &str = "sidebar-inner";
const SLOT_HEADER: &str = "sidebar-header";
const SLOT_FOOTER: &str = "sidebar-footer";
const SLOT_CONTENT: &str = "sidebar-content";
/// Host around the content viewport + overlay bar. Must carry `data-slot` so
/// [`crate::utils::element_pack::take_packed`] (used by [`Sidebar`]) does not peel
/// this anonymous two-child shell — that would make the bar a sibling of Header
/// and position it against the full rail (top at Header, height = content).
const SLOT_CONTENT_HOST: &str = "sidebar-content-host";
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
/// Stable face for idle + active. Changing 400↔500 on select remeasures glyphs and
/// the label appears to drop (not a ShadCN layout intent — their `h-8` + `items-center`
/// CSS line box hides it; cosmic-text does not).
const MENU_WEIGHT: u16 = 500;
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
    /// 收起时宽度为 0（滑出画布）。
    Offcanvas,
    /// 收起时保留 3rem 图标轨，隐藏文案。
    Icon,
    /// 不随 `open` 改宽度。
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

impl From<&str> for SidebarMenuButtonVariant {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for SidebarMenuButtonVariant {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

impl From<&String> for SidebarMenuButtonVariant {
    fn from(s: &String) -> Self {
        Self::parse(s)
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

    /// Tailwind v4 `text-sm` / `text-xs` default line-height ÷ font-size.
    /// ShadCN `sidebarMenuButtonVariants` is `h-8 text-sm` (sm: `h-7 text-xs`).
    pub fn leading_scale(self) -> f32 {
        match self {
            Self::Sm => crate::style::TAILWIND_TEXT_XS_LEADING_SCALE,
            Self::Default | Self::Lg => crate::style::TAILWIND_TEXT_SM_LEADING_SCALE,
        }
    }
}

impl From<&str> for SidebarMenuButtonSize {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for SidebarMenuButtonSize {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

impl From<&String> for SidebarMenuButtonSize {
    fn from(s: &String) -> Self {
        Self::parse(s)
    }
}

// ── Context ───────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct SidebarCtx {
    pub id: String,
    pub open: Signal<bool>,
    /// 移动面板的独立开合状态；不改变桌面折叠偏好。
    pub open_mobile: Signal<bool>,
    /// 当前窗口是否小于 `md`（768 logical px）。
    pub is_mobile: Signal<bool>,
    /// 由 [`Sidebar`] 写入；[`SidebarProvider`] / [`SidebarInset`] 读取 inset 壳样式。
    pub variant: Signal<String>,
}

impl PartialEq for SidebarCtx {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.open.id() == other.open.id()
            && self.open_mobile.id() == other.open_mobile.id()
            && self.is_mobile.id() == other.is_mobile.id()
            && self.variant.id() == other.variant.id()
    }
}

impl SidebarCtx {
    pub fn toggle(&self) {
        let is_mobile = sidebar_viewport_width() < SIDEBAR_MOBILE_BREAKPOINT;
        let _ = self.is_mobile.try_set_silent(is_mobile);
        let open = if is_mobile {
            self.open_mobile
        } else {
            self.open
        };
        if let Some(value) = open.try_peek() {
            open.set(!value);
            crate::app::runtime::request_redraw();
        }
    }
}

fn sidebar_state(id: &str) -> SidebarCtx {
    use_signal::<SidebarCtx>(format!("{id}__context").as_str(), || SidebarCtx {
        id: id.to_string(),
        open: use_signal(format!("{id}__open").as_str(), || true),
        open_mobile: use_signal(format!("{id}__open_mobile").as_str(), || false),
        is_mobile: use_signal(format!("{id}__is_mobile").as_str(), || false),
        variant: use_signal(format!("{id}__variant").as_str(), || "sidebar".into()),
    })
    .peek()
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
    if sidebar_shortcut_matches(crate::authoring::keys::Context {
        key: &event.logical_key,
        modifiers: *modifiers,
        state: event.state,
        repeat: event.repeat,
    }) {
        toggle_sidebar();
        true
    } else {
        false
    }
}

fn sidebar_shortcut_matches(context: crate::authoring::keys::Context<'_>) -> bool {
    context.state == winit::event::ElementState::Pressed
        && !context.repeat
        && (context.modifiers.control_key() || context.modifiers.meta_key())
        && matches!(context.key, winit::keyboard::Key::Character(key)
            if key.eq_ignore_ascii_case(SIDEBAR_KEYBOARD_SHORTCUT))
}

/// 切换侧栏开合（无 Provider 时为 no-op）。
pub fn toggle_sidebar() {
    if let Some(ctx) = use_sidebar() {
        ctx.toggle();
    }
}

fn bind_sidebar_toggle(on_click: Option<Handler<()>>) -> Handler<()> {
    if let Some(h) = on_click {
        return h;
    }
    if let Some(ctx) = use_sidebar() {
        return Handler::new(move |_| ctx.toggle());
    }
    Handler::new(|_| toggle_sidebar())
}

/// 提供侧栏上下文（`ui!` `<SidebarProvider>` / Builder 共用）。
pub fn sidebar_provider_scope(
    id: impl Into<String>,
    open: Option<Signal<bool>>,
    default_open: bool,
    shell_variant: Option<&str>,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = resolve_instance_id(id, "sidebar-provider");
    let open =
        open.unwrap_or_else(|| use_signal(format!("{id}__open").as_str(), move || default_open));
    let variant = use_signal(format!("{id}__variant").as_str(), || "sidebar".to_string());
    if let Some(v) = shell_variant.filter(|s| !s.trim().is_empty()) {
        let _ = variant.try_set_silent(v.trim().to_string());
    }
    let open_mobile = use_signal(format!("{id}__open_mobile").as_str(), || false);
    let is_mobile = use_signal(format!("{id}__is_mobile").as_str(), || {
        sidebar_viewport_width() < SIDEBAR_MOBILE_BREAKPOINT
    });
    let ctx = SidebarCtx {
        id: id.clone(),
        open,
        open_mobile,
        is_mobile,
        variant,
    };
    // Frame synchronization must use the actual controlled signal, not an inferred key.
    use_signal(format!("{id}__context").as_str(), || ctx.clone()).try_set_silent(ctx.clone());
    provide_context(sidebar_context(), ctx.clone());
    let kids = take_packed(body());
    let mut root = sidebar_provider_shell(kids);
    if variant.is_alive() && variant.peek() == "inset" {
        root = root.bg(parse_color("sidebar"));
    }
    crate::authoring::keys::attach(&mut root, move |context| {
        if sidebar_shortcut_matches(context) {
            ctx.toggle();
            true
        } else {
            false
        }
    });
    let provider_id = id.clone();
    crate::authoring::presentation::attach_tree(
        &mut root,
        crate::authoring::presentation::Phase::State,
        move |root, _, _| {
            if sync_sidebar_mobile_subtree(root, Some(&provider_id)) {
                LayoutStatus::StructureChanged
            } else {
                LayoutStatus::Clean
            }
        },
    );
    root.set_attr(ATTR_SLOT, SLOT_PROVIDER);
    root.set_attr(ATTR_PROVIDER_ID, id.clone());
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
        SidebarSide::Left => el.border_inline_end(parse_color("sidebar-border"), 1.0),
        SidebarSide::Right => el.border_inline_start(parse_color("sidebar-border"), 1.0),
    }
}

fn sidebar_collapsible_attr(el: &Element) -> SidebarCollapsible {
    el.attrs
        .get(ATTR_COLLAPSIBLE)
        .map(|s| SidebarCollapsible::from(s.as_str()))
        .unwrap_or_default()
}

fn rail_width_css_for_state(open: bool, expanded: &str, collapsible: SidebarCollapsible) -> &str {
    if open {
        return expanded;
    }
    match collapsible {
        SidebarCollapsible::None => expanded,
        SidebarCollapsible::Offcanvas => "0",
        SidebarCollapsible::Icon => SIDEBAR_WIDTH_ICON,
    }
}

fn is_sidebar_icon_node(el: &Element) -> bool {
    el.attrs.contains_key(ATTR_ICON_NAME)
        || el.attrs.contains_key(ATTR_ICON_SRC)
        || el.text_style.font_family.as_deref() == Some("lucide")
}

fn subtree_has_sidebar_icon(el: &Element) -> bool {
    crate::core::tree::any_in_tree(el, is_sidebar_icon_node)
}

fn set_sidebar_opacity(el: &mut Element, want: f32) -> bool {
    if (el.style.opacity - want).abs() > 0.001 {
        el.style.opacity = want;
        true
    } else {
        false
    }
}

fn set_dim_if_neq(slot: &mut taffy::style::Dimension, want: taffy::style::Dimension) -> bool {
    if *slot != want {
        *slot = want;
        true
    } else {
        false
    }
}

fn set_overflow_xy(el: &mut Element, x: taffy::style::Overflow, y: taffy::style::Overflow) -> bool {
    if el.style.overflow.x != x || el.style.overflow.y != y {
        el.style.overflow.x = x;
        el.style.overflow.y = y;
        true
    } else {
        false
    }
}

fn set_pad_xy(el: &mut Element, x: f32, y: f32) -> bool {
    use crate::layout::dimension::CssDim;
    use crate::style::UnitSlot;
    let prev = el.style.padding;
    el.style.set_css_dim(UnitSlot::PadL, CssDim::Px(x));
    el.style.set_css_dim(UnitSlot::PadR, CssDim::Px(x));
    el.style.set_css_dim(UnitSlot::PadT, CssDim::Px(y));
    el.style.set_css_dim(UnitSlot::PadB, CssDim::Px(y));
    el.style.padding != prev
}

fn set_gap_px(el: &mut Element, px: f32) -> bool {
    use crate::layout::dimension::CssDim;
    use crate::style::UnitSlot;
    let prev = el.style.gap;
    el.style.set_css_dim(UnitSlot::GapX, CssDim::Px(px));
    el.style.set_css_dim(UnitSlot::GapY, CssDim::Px(px));
    el.style.gap != prev
}

fn set_gap_rem(el: &mut Element, rem: f32) -> bool {
    use crate::layout::dimension::CssDim;
    use crate::style::UnitSlot;
    let prev = el.style.gap;
    el.style.set_css_dim(UnitSlot::GapX, CssDim::Rem(rem));
    el.style.set_css_dim(UnitSlot::GapY, CssDim::Rem(rem));
    el.style.gap != prev
}

/// Map current rail width onto icon-collapse chrome (`t=0` expanded, `t=1` icon-collapsed).
///
/// Driven by **width**, not elapsed-time direction — reversing mid-tween stays continuous.
fn collapse_t_from_width(current_px: f32, expanded_px: f32, collapsed_px: f32) -> f32 {
    let span = expanded_px - collapsed_px;
    if span.abs() < 0.5 {
        0.0
    } else {
        ((expanded_px - current_px) / span).clamp(0.0, 1.0)
    }
}

/// `t=0` expanded, `t=1` icon-collapsed. Follows the element's computed rail width.
fn icon_collapse_progress(
    el: &Element,
    expanded_px: f32,
    collapsed_px: f32,
    want_collapsed: bool,
) -> f32 {
    let rest = if want_collapsed { 1.0 } else { 0.0 };
    let Some(w) = crate::layout::dim_length_px(el.style.size.width) else {
        return rest;
    };
    collapse_t_from_width(w, expanded_px, collapsed_px)
}

fn rail_width_tween_running(el: &Element) -> bool {
    el.transition_snapshot
        .prop_tween_active(crate::style::TransitionProperty::Width)
        || el
            .transition_snapshot
            .prop_tween_active(crate::style::TransitionProperty::MinWidth)
}

thread_local! {
    static RAIL_ANIM_NEEDS_SETTLE: Cell<bool> = const { Cell::new(false) };
}

fn preview_width_px(css: &str) -> f32 {
    css.into_css_dim()
        .preview_dimension()
        .and_then(crate::layout::dim_length_px)
        .unwrap_or(0.0)
}

fn packed_px(px: f32) -> [f32; 8] {
    [px.max(0.0), 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]
}

fn ensure_rail_width_transition(el: &mut Element) {
    use crate::style::{TimingFunction, TransitionEntry, TransitionProperty};
    let has_width = el.style.transition.iter().any(|e| {
        matches!(
            e.property,
            TransitionProperty::Width | TransitionProperty::MinWidth | TransitionProperty::All
        )
    });
    if has_width {
        return;
    }
    for property in [TransitionProperty::Width, TransitionProperty::MinWidth] {
        el.style.transition.push(TransitionEntry {
            property,
            duration_ms: SIDEBAR_WIDTH_TRANSITION_MS,
            delay_ms: 0.0,
            timing: TimingFunction::Linear,
        });
    }
}

fn assign_rail_width_target(el: &mut Element, target_px: f32) -> bool {
    use crate::style::{assign_transition_specified, TransitionProperty};
    ensure_rail_width_transition(el);
    let seed = !el.transition_snapshot.has_any_tweens();
    let before_w = el.style.size.width;
    let before_min = el.style.min_size.width;
    let packed = packed_px(target_px);
    {
        let inner = &mut *el.inner;
        assign_transition_specified(
            &mut inner.style,
            &inner.transition_snapshot,
            TransitionProperty::Width,
            packed,
        );
        assign_transition_specified(
            &mut inner.style,
            &inner.transition_snapshot,
            TransitionProperty::MinWidth,
            packed,
        );
    }
    if seed {
        // First mount: land on the target without animating from paint's expanded width.
        el.snap_css_transition_baseline();
    }
    before_w != el.style.size.width || before_min != el.style.min_size.width
}

fn ensure_nowrap_clip(el: &mut Element) -> bool {
    use crate::style::{TextOverflow, WhiteSpace};
    use taffy::style::Overflow;

    let mut changed = false;
    if el.text_style.white_space != WhiteSpace::Nowrap {
        el.text_style.white_space = WhiteSpace::Nowrap;
        el.attrs.insert("white_space".into(), "nowrap".into());
        changed = true;
    }
    if el.text_style.wrap_mode != crate::text::WrapMode::None {
        el.text_style.wrap_mode = crate::text::WrapMode::None;
        changed = true;
    }
    if el.text_style.overflow == TextOverflow::Newline {
        el.text_style.overflow = TextOverflow::Clip;
        el.attrs.insert("text_overflow".into(), "clip".into());
        changed = true;
    }
    changed |= set_overflow_xy(el, Overflow::Hidden, Overflow::Hidden);
    changed
}

/// ShadCN group-label: `transition-[margin,opa] …: -mt-8 opacity-0` (net 0 height).
fn apply_icon_collapsed_group_label(el: &mut Element, t: f32) -> bool {
    use taffy::style::{Dimension, Overflow};

    let mut changed = set_sidebar_opacity(el, 1.0 - t);
    let h = SIDEBAR_GROUP_LABEL_H_PX * (1.0 - t);
    changed |= set_dim_if_neq(&mut el.style.size.height, Dimension::length(h));
    let clip = t > 0.001;
    changed |= set_overflow_xy(
        el,
        if clip {
            Overflow::Hidden
        } else {
            Overflow::Visible
        },
        if clip {
            Overflow::Hidden
        } else {
            Overflow::Visible
        },
    );
    let pad = SIDEBAR_PAD_PX * (1.0 - t);
    changed |= set_pad_xy(el, pad, 0.0);
    changed
}

fn apply_icon_text_collapse(el: &mut Element, t: f32) -> bool {
    use taffy::style::Dimension;

    let mut changed = set_sidebar_opacity(el, 1.0 - t);
    // ShadCN `truncate`: nowrap + clip so shrinking rail width cannot reflow labels
    // onto extra lines (height jumps). Grow is stashed as soon as collapse starts so
    // the icon stays at flex-start while the label is clipped, not only at t≈1.
    changed |= ensure_nowrap_clip(el);
    let collapsing = t > 0.001;
    let fully = t >= 0.999;
    if collapsing {
        if el.style.flex_grow > 0.001 {
            let grow = el.style.flex_grow;
            el.attrs.insert(ATTR_SAVED_GROW.into(), format!("{grow}"));
            el.style.flex_grow = 0.0;
            changed = true;
        }
    } else if let Some(g) = el.attrs.remove(ATTR_SAVED_GROW) {
        if let Ok(v) = g.parse::<f32>() {
            if (el.style.flex_grow - v).abs() > 0.001 {
                el.style.flex_grow = v;
                changed = true;
            }
        }
    }
    if fully {
        changed |= set_dim_if_neq(&mut el.style.size.width, Dimension::length(0.0));
        changed |= set_dim_if_neq(&mut el.style.max_size.width, Dimension::length(0.0));
        changed |= set_dim_if_neq(&mut el.style.min_size.width, Dimension::length(0.0));
    } else {
        changed |= set_dim_if_neq(&mut el.style.size.width, Dimension::auto());
        changed |= set_dim_if_neq(&mut el.style.max_size.width, Dimension::auto());
        changed |= set_dim_if_neq(&mut el.style.min_size.width, Dimension::length(0.0));
    }
    changed
}

fn apply_icon_row_gap(el: &mut Element, t: f32) -> bool {
    if t >= 0.999 {
        set_gap_px(el, 0.0)
    } else if t <= 0.001 {
        set_gap_rem(el, 0.5)
    } else {
        set_gap_px(el, SIDEBAR_GAP_PX * (1.0 - t))
    }
}

fn apply_icon_collapsed_menu_button(el: &mut Element, t: f32) -> bool {
    use taffy::style::{Dimension, JustifyContent, Overflow};

    let mut changed = false;
    if el.style.justify_content != Some(JustifyContent::FLEX_START) {
        el.style.justify_content = Some(JustifyContent::FLEX_START);
        changed = true;
    }
    changed |= set_overflow_xy(el, Overflow::Hidden, Overflow::Hidden);
    let icon_px = SIDEBAR_MENU_BUTTON_ICON_PX;
    let fully = t >= 0.999;
    let size = sidebar_menu_button_size(el);
    if fully {
        changed |= set_dim_if_neq(&mut el.style.size.width, Dimension::length(icon_px));
        changed |= set_dim_if_neq(&mut el.style.min_size.width, Dimension::length(icon_px));
        changed |= set_dim_if_neq(&mut el.style.max_size.width, Dimension::length(icon_px));
        changed |= set_dim_if_neq(&mut el.style.size.height, Dimension::length(icon_px));
        // `size-8` + `p-2` (lg: `p-0`) so the 16px icon fills the content box.
        let pad = if size == SidebarMenuButtonSize::Lg {
            0.0
        } else {
            SIDEBAR_PAD_PX
        };
        changed |= set_pad_xy(el, pad, pad);
        changed |= set_gap_px(el, 0.0);
    } else {
        changed |= set_dim_if_neq(&mut el.style.size.width, Dimension::percent(1.0));
        changed |= set_dim_if_neq(&mut el.style.min_size.width, Dimension::auto());
        changed |= set_dim_if_neq(&mut el.style.max_size.width, Dimension::auto());
        let h = size
            .height_css()
            .into_css_dim()
            .preview_dimension()
            .unwrap_or(Dimension::length(icon_px));
        changed |= set_dim_if_neq(&mut el.style.size.height, h);
        changed |= set_pad_xy(el, SIDEBAR_PAD_PX, SIDEBAR_PAD_PX);
        changed |= apply_icon_row_gap(el, t);
    }
    for child in &mut el.children {
        changed |= apply_icon_leaf_visibility(child, t);
    }
    changed
}

/// Keep Lucide / `<Icon>` nodes; fade + zero-width labels (ShadCN clips last span).
fn apply_icon_leaf_visibility(el: &mut Element, t: f32) -> bool {
    let mut changed = false;
    crate::core::tree::walk_elements_mut_pruned(el, |el| {
        if is_sidebar_icon_node(el) {
            changed |= set_sidebar_opacity(el, 1.0);
            return false;
        }
        if subtree_has_sidebar_icon(el) {
            changed |= apply_icon_row_gap(el, t);
            return true;
        }
        changed |= apply_icon_text_collapse(el, t);
        false
    });
    changed
}

fn apply_sidebar_icon_chrome(el: &mut Element, t: f32) -> bool {
    use taffy::style::Overflow;

    let mut changed = false;
    crate::core::tree::walk_elements_mut_pruned(el, |el| {
        let slot = el.attrs.get(ATTR_SLOT).map(String::as_str);
        match slot {
            Some(SLOT_GROUP_LABEL) | Some(SLOT_INPUT) => {
                changed |= apply_icon_collapsed_group_label(el, t);
                false
            }
            Some(SLOT_MENU_BUTTON) => {
                changed |= apply_icon_collapsed_menu_button(el, t);
                false
            }
            Some(SLOT_CONTENT) => {
                changed |= if t > 0.001 {
                    set_overflow_xy(el, Overflow::Hidden, Overflow::Hidden)
                } else {
                    set_overflow_xy(el, Overflow::Hidden, Overflow::Scroll)
                };
                true
            }
            Some(SLOT_CONTENT_HOST) => true,
            Some(SLOT_HEADER) | Some(SLOT_FOOTER) | Some(SLOT_GROUP) => {
                changed |= if t > 0.001 {
                    set_overflow_xy(el, Overflow::Hidden, Overflow::Hidden)
                } else {
                    set_overflow_xy(el, Overflow::Visible, Overflow::Visible)
                };
                true
            }
            _ => {
                if el.text.is_some() && !is_sidebar_icon_node(el) && !subtree_has_sidebar_icon(el) {
                    changed |= apply_icon_text_collapse(el, t);
                }
                true
            }
        }
    });
    changed
}

fn apply_sidebar_rail_layout(el: &mut Element, open: bool, width: &str) -> LayoutStatus {
    use taffy::style::Overflow;

    let mobile = el.attrs.contains_key(ATTR_MOBILE_ID)
        && sidebar_viewport_width() < SIDEBAR_MOBILE_BREAKPOINT;
    let collapsible = sidebar_collapsible_attr(el);
    let want_state = if open { "expanded" } else { "collapsed" };
    let want_w_css = rail_width_css_for_state(open, width, collapsible);
    let target_px = if mobile {
        0.0
    } else {
        preview_width_px(want_w_css)
    };
    let visually_collapsed = mobile
        || (!open
            && matches!(
                collapsible,
                SidebarCollapsible::Offcanvas | SidebarCollapsible::Icon
            ));

    let mut changed = assign_rail_width_target(el, target_px);
    let was_running = rail_width_tween_running(el);
    if el.tick_css_transitions(0.0) || was_running {
        changed = true;
    }
    let anim_running = rail_width_tween_running(el);
    if was_running && !anim_running {
        RAIL_ANIM_NEEDS_SETTLE.set(true);
    }
    let display = if mobile && !anim_running {
        taffy::style::Display::None
    } else {
        taffy::style::Display::Flex
    };
    if el.style.display != display {
        el.style.display = display;
        changed = true;
    }
    let want_overflow = if visually_collapsed || anim_running {
        Overflow::Hidden
    } else {
        Overflow::Visible
    };
    if el.style.overflow.x != want_overflow || el.style.overflow.y != want_overflow {
        el.style.overflow.x = want_overflow;
        el.style.overflow.y = want_overflow;
        changed = true;
    }
    if el.attrs.get(ATTR_STATE).map(String::as_str) != Some(want_state) {
        el.set_attr(ATTR_STATE, want_state);
    }
    if !mobile && collapsible == SidebarCollapsible::Icon {
        let expanded_px = preview_width_px(width);
        let collapsed_px = preview_width_px(SIDEBAR_WIDTH_ICON);
        let chrome_t = icon_collapse_progress(el, expanded_px, collapsed_px, !open);
        if apply_sidebar_icon_chrome(el, chrome_t) {
            changed = true;
        }
    }
    // Responsive class resolution runs after rail chrome. Preserve the live width
    // so restoring the class baseline cannot undo a controlled collapse or resize.
    let width = el.style.size.width;
    let min_width = el.style.min_size.width;
    let overflow = el.style.overflow;
    if let Some(classes) = el.class_set.as_mut() {
        if let Some(base) = classes.base_style.as_mut() {
            if base.size.width != width
                || base.min_size.width != min_width
                || base.overflow != overflow
                || base.display != display
            {
                base.size.width = width;
                base.min_size.width = min_width;
                base.overflow = overflow;
                base.display = display;
                classes.set_base_dimension(
                    crate::style::UnitSlot::Width,
                    crate::layout::dimension::CssDim::Px(
                        crate::layout::dim_length_px(width).unwrap_or(target_px),
                    ),
                );
                classes.set_base_dimension(
                    crate::style::UnitSlot::MinWidth,
                    crate::layout::dimension::CssDim::Px(
                        crate::layout::dim_length_px(min_width).unwrap_or(target_px),
                    ),
                );
            }
        }
    }
    if changed {
        LayoutStatus::LayoutChanged
    } else {
        LayoutStatus::Clean
    }
}

fn sidebar_node_mut<'a>(el: &'a mut Element, attr: &str, value: &str) -> Option<&'a mut Element> {
    if el.attrs.get(attr).map(String::as_str) == Some(value) {
        return Some(el);
    }
    el.children
        .iter_mut()
        .find_map(|child| sidebar_node_mut(child, attr, value))
}

fn sidebar_restore_mobile_header(inner: &mut Element) {
    if let Some(header) = sidebar_node_mut(inner, ATTR_SLOT, SLOT_HEADER) {
        if let Some(index) = header.children.iter().position(|child| {
            child
                .attrs
                .get("data-sidebar-mobile-header-row")
                .map(String::as_str)
                == Some("true")
        }) {
            let mut row = header.children.remove(index);
            let mut content = row.children.remove(0);
            header
                .children
                .splice(index..index, std::mem::take(&mut content.children));
        }
    }
    fn remove_generated_header(el: &mut Element) {
        el.children.retain(|child| {
            child
                .attrs
                .get("data-sidebar-mobile-header")
                .map(String::as_str)
                != Some("true")
        });
        for child in &mut el.children {
            remove_generated_header(child);
        }
    }
    remove_generated_header(inner);
}

fn sidebar_mobile_header_with_close(mut inner: Element, close: Element) -> Element {
    sidebar_restore_mobile_header(&mut inner);
    let row = |children: Vec<Element>, close: Element| {
        let mut row: Element = hstack()
            .w("100%")
            .items_center()
            .children([
                vstack().flex_grow(1.0).min_w(0.0).children(children).into(),
                close.flex_shrink(0.0),
            ])
            .into();
        row.set_attr("data-sidebar-mobile-header-row", "true");
        row
    };

    if let Some(header) = sidebar_node_mut(&mut inner, ATTR_SLOT, SLOT_HEADER) {
        let children = std::mem::take(&mut header.children);
        header.children = vec![row(children, close)];
    } else {
        let mut header: Element = sidebar_header().child(row(Vec::new(), close)).into();
        header.set_attr("data-sidebar-mobile-header", "true");
        inner.children.insert(0, header);
    }
    inner
}

/// Move the original content, including hooks and reactive bindings, across the portal boundary.
/// Keep the portal slot until the exit finishes so hoisting cannot prune that content early.
fn sync_sidebar_mobile_subtree(root: &mut Element, only: Option<&str>) -> bool {
    let mobile = sidebar_viewport_width() < SIDEBAR_MOBILE_BREAKPOINT;
    let mut rails = Vec::new();
    crate::core::tree::walk_elements(root, |el| {
        if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_PROVIDER) {
            if let Some(provider) = el
                .attrs
                .get(ATTR_PROVIDER_ID)
                .filter(|provider| only.is_none_or(|only| only == provider.as_str()))
            {
                let ctx = sidebar_state(provider);
                if ctx.is_mobile.peek() != mobile {
                    ctx.is_mobile.set(mobile);
                }
            }
        }
        if let (Some(id), Some(provider)) =
            (el.attrs.get(ATTR_MOBILE_ID), el.attrs.get(ATTR_PROVIDER_ID))
        {
            if only.is_none_or(|only| only == provider) {
                rails.push((id.clone(), sidebar_state(provider)));
            }
        }
    });
    let mut changed = false;
    for (id, ctx) in rails {
        if ctx.is_mobile.peek() != mobile {
            ctx.is_mobile.set(mobile);
        }
        let owner = format!("sidebar:{id}");
        let in_panel = sidebar_node_mut(root, ATTR_MOBILE_PANEL, &id).is_some();
        if !mobile && (ctx.open_mobile.peek() || in_panel) {
            dismiss_viewport_modal(&id, ctx.open_mobile);
        }
        if !mobile && !in_panel {
            continue;
        }
        let mounted = viewport_modal_phase(&id, ctx.open_mobile).is_some();
        if in_panel && (!mobile || !mounted) {
            let panel = sidebar_node_mut(root, ATTR_MOBILE_PANEL, &id).unwrap();
            let mut inner = panel.children.remove(0);
            sidebar_restore_mobile_header(&mut inner);
            let rail = sidebar_node_mut(root, ATTR_MOBILE_ID, &id).unwrap();
            rail.children.clear();
            rail.children.push(inner);
            crate::utils::sync_focus_trap(&owner, false, "");
            changed = true;
        } else if mobile && ctx.open_mobile.peek() && !in_panel {
            let rail = sidebar_node_mut(root, ATTR_MOBILE_ID, &id).unwrap();
            apply_sidebar_icon_chrome(rail, 0.0);
            let Some(index) = rail
                .children
                .iter()
                .position(|c| c.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_INNER))
            else {
                continue;
            };
            let inner = rail.children.remove(index);
            let side = if rail.attrs.get(ATTR_SIDE).map(String::as_str) == Some("right") {
                SheetSide::Right
            } else {
                SheetSide::Left
            };
            let close: Element = button(format!("{id}:close"))
                .variant("ghost")
                .size("icon-sm")
                .on_click(move || ctx.open_mobile.set(false))
                .child(
                    build_icon_text_node(lucide_glyph("x").unwrap_or_else(|| "×".into()))
                        .text_size(16.0),
                )
                .into();
            let close = close.aria_label(crate::tr_default_zh(
                "jadix.sidebar.close",
                "Close sidebar",
                "关闭侧栏",
            ));
            let mut close = close;
            close.set_attr("data-sidebar-mobile-close", "true");
            let inner = sidebar_mobile_header_with_close(inner, close);
            let mut panel = sheet_shell_panel(
                inner,
                side,
                Some(SIDEBAR_WIDTH_MOBILE),
                None,
                None,
                SlotChrome::default().class("bg-sidebar"),
            );
            panel.set_attr(ATTR_SLOT, "sidebar-mobile");
            panel.set_attr(ATTR_SIDEBAR, "sidebar");
            panel.set_attr(ATTR_MOBILE_PANEL, &id);
            panel.set_attr("data-mobile", "true");
            panel.set_attr("role", "dialog");
            panel.set_attr("aria-modal", "true");
            panel = panel.aria_label(crate::tr_default_zh(
                "jadix.sidebar.title",
                "Sidebar",
                "侧栏",
            ));
            rail.children.push(retained_viewport_modal_layer(
                &id,
                ctx.open_mobile,
                &crate::utils::modal_shell::ShellOverlayOpts::from_content_parts(
                    None, None, true, None,
                ),
                ViewportModalLayerOpts::slide(
                    side.to_surface_motion(),
                    preview_width_px(SIDEBAR_WIDTH_MOBILE),
                ),
                panel,
            ));
            changed = true;
        }
        if let Some(panel) = sidebar_node_mut(root, ATTR_MOBILE_PANEL, &id) {
            let (index, count) = crate::app::portal_overlay::modal_stack_metrics(&id);
            let active = ctx.open_mobile.peek() && index + 1 == count;
            let state = if active { "true" } else { "false" };
            let mut ring = Vec::new();
            if active {
                collect_sheet_focus_ids(panel, &mut ring);
                ring.retain(|id| !id.ends_with("__sidebar_rail"));
            }
            let ring = ring.join(",");
            if panel.attrs.get("data-sidebar-trap").map(String::as_str) != Some(state)
                || panel.attrs.get("data-sidebar-trap-ring") != Some(&ring)
            {
                crate::utils::sync_focus_trap(&owner, active, &ring);
                panel.set_attr("data-sidebar-trap", state);
                panel.set_attr("data-sidebar-trap-ring", ring);
            }
        }
    }
    changed
}

fn sync_sidebar_rail(el: &mut Element, layout_tree: &mut LayoutTree) -> LayoutStatus {
    let (Some(provider), Some(width)) = (
        el.attrs.get(ATTR_PROVIDER_ID).cloned(),
        el.attrs.get(ATTR_WIDTH_STORE).cloned(),
    ) else {
        return LayoutStatus::Clean;
    };
    let open = sidebar_state(&provider).open;
    if !open.is_alive() {
        return LayoutStatus::Clean;
    }
    let status = apply_sidebar_rail_layout(el, open.peek(), &width);
    if rail_width_tween_running(el) {
        layout_tree.request_geometry_motion();
        el.style.overflow.x = taffy::style::Overflow::Hidden;
        el.style.overflow.y = taffy::style::Overflow::Hidden;
        crate::app::runtime::request_redraw();
    }
    if RAIL_ANIM_NEEDS_SETTLE.replace(false) {
        layout_tree.request_text_settle();
        crate::app::runtime::request_redraw();
    }
    if status == LayoutStatus::LayoutChanged && el.layout_node_id().is_some() {
        layout_tree.mark_element_layout_dirty(el);
        crate::core::tree::walk_elements(el, |node| {
            layout_tree.update_taffy_node_style(node);
        });
    }
    status
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

/// Returns `true` when any text leaf's resolved color changed.
fn paint_sidebar_menu_button_fg(el: &mut Element, fg: [u8; 4], size_px: f32) -> bool {
    let mut changed = false;
    crate::core::tree::walk_elements_mut(el, |el| {
        if el.text.is_none() {
            return;
        }
        if el.text_style.color_resolved() != fg {
            changed = true;
        }
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
            return;
        }
        // Size stays on the leaf (`sm` is 12px; Label defaults are 14). Line-height
        // lives on the button root (`text-sm` / `text-xs`); caption leaves inherit.
        el.text_style.size = Some(size_px);
        el.attrs.insert("text_size".into(), "1".into());
        // Label `leading-none` is specified 1em — CSS would win over the ancestor
        // token. Release that default so cascade copies the button's leading.
        // Author `text_leading*` other than 1em (or `text_line_height`) is kept.
        // Do not clear already-inherited values (`specified == false`): paint runs
        // before cascade, and hover frames must not wipe last frame's copy.
        let is_label_leading_none = el.text_style.line_height_specified()
            && el.text_style.line_height_px.is_none()
            && el.text_style.line_height_scale == Some(1.0);
        if is_label_leading_none {
            el.text_style.line_height_px = None;
            el.text_style.line_height_scale = None;
            el.text_style.set_line_height_specified(false);
        }
    });
    changed
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
    mut layout_tree: Option<&mut LayoutTree>,
    variant: SidebarMenuButtonVariant,
    is_active: bool,
    is_hovered: bool,
) -> LayoutStatus {
    let highlighted = is_active || is_hovered;
    let fluent_material = crate::style::with_theme(|theme| {
        if theme.preset != crate::style::ThemePreset::Fluent {
            return None;
        }
        let opacity = theme.styles.surface_opacity.unwrap_or(1.0);
        let mut color = theme.colors.card;
        if is_hovered && opacity == 1.0 {
            for channel in &mut color[..3] {
                *channel += (1.0 - *channel) * 0.08;
            }
        }
        color[3] *= if !highlighted {
            0.0
        } else if is_hovered {
            opacity + (1.0 - opacity) * 0.08
        } else {
            opacity
        };
        let blur = if highlighted && opacity < 1.0 {
            theme.styles.backdrop_blur.unwrap_or(0.0)
        } else {
            0.0
        };
        Some((color, blur))
    });
    let want_bg = if let Some((color, _)) = fluent_material {
        color
    } else if highlighted {
        parse_color("sidebar-accent")
    } else {
        [0.0, 0.0, 0.0, 0.0]
    };
    let want_fg = if highlighted && fluent_material.is_none() {
        parse_text_color("sidebar-accent-foreground")
    } else {
        parse_text_color("sidebar-foreground")
    };
    let btn_size = sidebar_menu_button_size(el);
    let size_px = btn_size.text_size_px();
    let leading = btn_size.leading_scale();
    // ShadCN puts `text-sm` / `text-xs` on the button; children inherit unless they
    // specify otherwise. Lock the token on the root so cascade from an unstyled
    // Header/Footer slot cannot wipe it before paint. Caption leaves do not get
    // a second copy of line-height — see `paint_sidebar_menu_button_fg`.
    el.text_style.size = Some(size_px);
    el.text_style.line_height_px = None;
    el.text_style.line_height_scale = Some(leading);
    el.text_style.set_line_height_specified(true);

    let mut status = LayoutStatus::Clean;
    if let Some((_, blur)) = fluent_material {
        if el.style.backdrop_blur != blur {
            el.style.backdrop_blur = blur;
            status = LayoutStatus::StyleChanged;
        }
    }
    if el.style.bg_color != want_bg {
        el.style.bg_color = want_bg;
        if let Some(lt) = layout_tree.as_mut() {
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

    if paint_sidebar_menu_button_fg(el, want_fg, size_px) {
        status = status.merge(LayoutStatus::StyleChanged);
    }

    // Same contract as Button hover paint: named PartialLoad dest-reset + bust
    // ViewportsOnly `reuse_idle` so an idle gallery rail cannot skip-replay the
    // previous rest bake after a paint-only accent write.
    if status != LayoutStatus::Clean {
        if let Some(lt) = layout_tree {
            let b = el.layout_bounds;
            if b[2] > 0.0 && b[3] > 0.0 {
                crate::app::damage::push_frame_paint_element(el, el.paint_scroll_offset());
            }
            lt.note_collect_skip_hash_bust_if_idle(el.collect_cache_live());
        }
    }
    status
}

/// 每帧按 `{focus_id}_hover` 与 `data-active` 写回菜单按钮 chrome（`peek`，不订阅根 effect）。
#[cfg(all(test, feature = "product-tests"))]
pub(crate) fn sync_sidebar_menu_button_paint_subtree(
    root: &mut Element,
    layout_tree: &mut crate::core::tree::LayoutTree,
    impact: &mut LayoutStatus,
) {
    crate::core::tree::walk_elements_mut(root, |el| {
        sync_sidebar_menu_button_paint_node(el, layout_tree, impact)
    });
}

fn sync_sidebar_menu_button_paint_node(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
) {
    if el.attrs.get(ATTR_SIDEBAR).map(String::as_str) != Some("menu-button") {
        return;
    }
    let variant = sidebar_menu_button_variant(el);
    let is_active = sidebar_menu_button_active(el);
    let is_hovered = sidebar_menu_button_hovered(el);
    *impact = impact.merge(apply_sidebar_menu_button_chrome(
        el,
        Some(layout_tree),
        variant,
        is_active,
        is_hovered,
    ));
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
    crate::authoring::presentation::attach(
        &mut el,
        crate::authoring::presentation::Phase::Hover,
        |element, layout, _context| {
            let mut impact = LayoutStatus::Clean;
            sync_sidebar_menu_button_paint_node(element, layout, &mut impact);
            impact
        },
    );
    el.set_attr(ATTR_SIDEBAR, "menu-button");
    el.set_attr("data-active", if is_active { "true" } else { "false" });
    el.set_attr("data-size", size.as_str());
    el.set_attr("data-variant", variant.as_str());
    // Rebuild under an existing hover must not flash rest chrome; paint sync peeks
    // the same signal, but idle frames between rebuild and sync would otherwise flicker.
    let is_hovered = sidebar_menu_button_hovered(&el);
    let _ = apply_sidebar_menu_button_chrome(&mut el, None, variant, is_active, is_hovered);
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
    el = el.transition(format!(
        "width {SIDEBAR_WIDTH_TRANSITION_MS}ms linear, min-width {SIDEBAR_WIDTH_TRANSITION_MS}ms linear"
    ));
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
    el.set_attr(ATTR_COLLAPSIBLE, collapsible.as_str());
    if let Some(open) = open {
        el.set_attr(ATTR_STATE, sidebar_state_attr(open).as_str());
    }
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
            .box_shadow_color(tokens::shadow_ink(0.05))
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

fn slot_children(slot: &str, sidebar_part: &str, kids: Vec<Element>) -> Element {
    let mut col = vstack().w("100%").flex_col();
    for k in kids {
        col = col.child(k);
    }
    let mut el: Element = col.into();
    el.set_attr(ATTR_SLOT, slot);
    el.set_attr(ATTR_SIDEBAR, sidebar_part);
    el
}

/// Reactive bind：RSX `isActive={sig.get() == id}` 与手动 `bind_prop` 对位（避免整棵侧栏 rebuild）。
pub fn apply_bound_sidebar_menu_button_active(el: &mut Element, active: bool) -> LayoutStatus {
    let want = if active { "true" } else { "false" };
    if el.attrs.get("data-active").map(String::as_str) == Some(want) {
        return LayoutStatus::Clean;
    }
    el.set_attr("data-active", want);
    let variant = sidebar_menu_button_variant(el);
    // Keep live `{focus_id}_hover` — forcing `is_hovered=false` here wiped accent
    // chrome whenever page/active bindings updated under a still pointer.
    let is_hovered = sidebar_menu_button_hovered(el);
    apply_sidebar_menu_button_chrome(el, None, variant, active, is_hovered)
}

fn sidebar_menu_button_hovered(el: &Element) -> bool {
    el.focus_id
        .as_deref()
        .map(|fid| {
            let key = format!("{fid}_hover");
            let sig = use_signal(key.as_str(), || false);
            sig.is_alive() && sig.peek()
        })
        .unwrap_or(false)
}

// ── RSX parts ─────────────────────────────────────────────────────────────────

crate::__component_contract! {
SidebarProviderProps [];
/// 根 Provider：`open` / `defaultOpen` · 预留 `toggleSidebar`。
#[allow(non_snake_case)]
pub fn SidebarProvider(
identity [] id: String,
plain [] open: Option<Signal<bool>>,
plain [] default_open: Option<bool>,
plain [] variant: Option<String>,
plain [] children: Option<Element>,
) -> Element {
    let shell = variant.as_deref();
    sidebar_provider_scope(id, open, default_open.unwrap_or(true), shell, move || {
        children.unwrap_or_default()
    })
}
}

crate::__component_contract! {
SidebarProps [];
/// 侧栏 rail（`collapsible=none` 时为固定宽竖栏；默认 `16rem`）。
#[allow(non_snake_case)]
pub fn Sidebar(
identity [] id: String,
plain [] rail_w: Option<String>,
plain [] side: Option<String>,
plain [] variant: Option<String>,
plain [] collapsible: Option<String>,
plain [] children: Option<Element>,
) -> Element {
    let width = rail_w
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
    if !id.trim().is_empty() {
        rail.template_id = Some(id.clone());
    }
    let inner = wrap_sidebar_inner(kids, variant, side);
    rail.children.push(inner);
    attach_focus_roving(&mut rail, FocusRovingAxis::Vertical);
    if let Some(ctx) = sidebar_ctx {
        rail.set_attr(ATTR_PROVIDER_ID, ctx.id.clone());
        rail.set_attr(ATTR_WIDTH_STORE, width.clone());
        if collapsible != SidebarCollapsible::None {
            let mobile_id = format!("{}__mobile", resolve_instance_id(id, "sidebar"));
            rail.set_attr(ATTR_MOBILE_ID, &mobile_id);
            let open_mobile = ctx.open_mobile;
            rail = rail.on_unmount(move || {
                if open_mobile.is_alive() {
                    dismiss_viewport_modal(&mobile_id, open_mobile);
                    crate::utils::sync_focus_trap(&format!("sidebar:{mobile_id}"), false, "");
                }
            });
        }
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
    crate::authoring::presentation::attach(&mut rail,
        crate::authoring::presentation::Phase::State,
        |element, layout, _| sync_sidebar_rail(element, layout));
    rail
}
}

crate::__component_contract! {
SidebarHeaderProps [];
/// 顶区：`flex flex-col gap-2 p-2`。
#[allow(non_snake_case)]
pub fn SidebarHeader(
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el = slot_children(SLOT_HEADER, "header", kids);
    // Inherit-lock `sidebar-foreground` so RSX string leaves (`"Jadix Gallery"`) do not
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
}

crate::__component_contract! {
SidebarFooterProps [];
/// 底区：`flex flex-col gap-2 p-2`。
#[allow(non_snake_case)]
pub fn SidebarFooter(
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el = slot_children(SLOT_FOOTER, "footer", kids);
    el = el
        .p("0.5rem")
        .gap("0.5rem")
        .flex_shrink(0.0)
        .overflow_x("visible")
        .overflow_y("visible")
        .text_color(parse_text_color("sidebar-foreground"));
    el
}
}

crate::__component_contract! {
SidebarContentProps [];
/// 可滚动主体：`flex-1 min-h-0` + 引擎 [`activate_scroll_viewport`]（滚轮 / 触控板）
/// + overlay 滚动条（项溢出时可见；`z-index: 0` 以免嵌套进页面 stacking bands）。
#[allow(non_snake_case)]
pub fn SidebarContent(
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let scroll_id = use_sidebar()
        .map(|c| format!("{}__sidebar-content", c.id))
        .unwrap_or_else(|| "sidebar-content".to_string());
    let mut inner = slot_children(SLOT_CONTENT, "content", kids);
    inner = inner
        .w("100%")
        .h("100%")
        .flex_grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .gap("0.5rem")
        .flex_shrink(1.0);
    inner = activate_scroll_viewport(
        inner,
        &scroll_id,
        "hidden",
        "auto",
        DEFAULT_SCROLL_BEHAVIOR,
        "content",
    );
    inner.set_attr(ATTR_SLOT, SLOT_CONTENT);
    inner.set_attr(ATTR_SIDEBAR, "content");

    let bar: Element = scrollbar(&format!("__jadix_sidebar_sb_{scroll_id}"))
        .target(scroll_id)
        .orientation(ScrollbarOrientation::Vertical)
        .visibility("auto")
        .overlay(true)
        .absolute()
        .top(2.0)
        .bottom(2.0)
        .inset_inline_end(2.0)
        // `overlay(true)` defaults z-index to 10. Nested page stacking would
        // desync glyphs vs rects (same as MessageScroller overlay bars).
        .z_index(0)
        .into();

    // Flex leftover under Header/Footer — do not `h(100%)` (that sizes to the
    // full rail and fights the column). Slot the host so `take_packed` keeps it.
    let mut host: Element = vstack()
        .relative(true)
        .w("100%")
        .min_w(0.0)
        .min_h(0.0)
        .flex_grow(1.0)
        .flex_shrink(1.0)
        .overflow("hidden")
        .child(inner)
        .child(bar)
        .into();
    host.set_attr(ATTR_SLOT, SLOT_CONTENT_HOST);
    host.set_attr(ATTR_SIDEBAR, "content-host");
    host
}
}

crate::__component_contract! {
SidebarGroupProps [];
/// 分组容器：`relative flex w-full min-w-0 flex-col p-2`。
#[allow(non_snake_case)]
pub fn SidebarGroup(
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el = slot_children(SLOT_GROUP, "group", kids);
    el = el.p("0.5rem").min_w(0.0).relative();
    el
}
}

fn paint_group_label_text(el: &mut Element) {
    let fg = parse_text_color("sidebar-foreground/70");
    crate::core::tree::walk_elements_mut(el, |el| {
        if el.text.is_some() {
            el.text_style.color = Some(fg);
            el.attrs.insert("text_color".into(), "1".into());
        }
    });
}

crate::__component_contract! {
SidebarGroupLabelProps [];
/// 分组标题：`text-xs font-medium text-sidebar-foreground/70`。
#[allow(non_snake_case)]
pub fn SidebarGroupLabel(
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el = slot_children(SLOT_GROUP_LABEL, "group-label", kids);
    el = el
        .h("2rem")
        .px("0.5rem")
        .items_center()
        .justify_center()
        .flex_shrink(0.0)
        .rounded("0.375rem")
        .text_size(GROUP_LABEL_SIZE_PX)
        .font_weight(GROUP_LABEL_WEIGHT as f32);
    paint_group_label_text(&mut el);
    el
}
}

crate::__component_contract! {
SidebarGroupContentProps [];
/// 分组内容区。
#[allow(non_snake_case)]
pub fn SidebarGroupContent(
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el = slot_children(SLOT_GROUP_CONTENT, "group-content", kids);
    el = el.text_size(MENU_TEXT_SIZE_PX);
    el
}
}

crate::__component_contract! {
SidebarMenuProps [];
/// 菜单列表：`flex w-full min-w-0 flex-col gap-1`。
#[allow(non_snake_case)]
pub fn SidebarMenu(
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el = slot_children(SLOT_MENU, "menu", kids);
    el = el.gap("0.25rem").min_w(0.0);
    el
}
}

crate::__component_contract! {
SidebarMenuItemProps [];
/// 菜单项：`group/menu-item relative`。
#[allow(non_snake_case)]
pub fn SidebarMenuItem(
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el = slot_children(SLOT_MENU_ITEM, "menu-item", kids);
    el = el.relative();
    el
}
}

crate::__component_contract! {
SidebarMenuButtonProps [];
/// 菜单按钮：对齐 `sidebarMenuButtonVariants`（default/outline · default/sm/lg · isActive）。
#[allow(non_snake_case)]
pub fn SidebarMenuButton(
identity [] id: String,
plain [] is_active: Option<bool>,
plain [] variant: Option<String>,
plain [] size: Option<String>,
default [mut] on_click: Handler<()>,
plain [] children: Option<Element>,
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
    el = paint_menu_button(el, variant, size, is_active);
    if !id.trim().is_empty() {
        el.template_id = Some(id);
    }
    el
}
}

crate::__component_contract! {
SidebarSeparatorProps [];
/// 侧栏内分割线（`px-2` 内缩 · 可选；在 RSX 中于 Header / Content / Footer 之间手动插入）。
#[allow(non_snake_case)]
pub fn SidebarSeparator(
plain [] children: Option<crate::core::tree::Element>,
) -> Element {
    sidebar_horizontal_separator_element()
}
}

crate::__component_contract! {
SidebarInsetProps [];
/// 主内容列：`relative flex w-full flex-1 flex-col min-w-0`。
#[allow(non_snake_case)]
pub fn SidebarInset(
plain [] variant: Option<String>,
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el = slot_children(SLOT_INSET, "inset", kids);
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
            .transition("margin 200ms linear, border-radius 200ms linear")
            .overflow("hidden")
            .box_shadow_color(tokens::shadow_rgba(tokens::Elevation::Md))
            .box_shadow_blur(8.0)
            .box_shadow_offset_x(0.0)
            .box_shadow_offset_y(2.0)
            .box_shadow_spread(0.0);
        el = crate::authoring::widgets::classes::with_default_class_list(
            el,
            "max-md:m-0 max-md:rounded-none max-md:shadow-none",
        );
    }
    el
}
}

crate::__component_contract! {
SidebarTriggerProps [];
/// 开合触发器（ghost · icon-sm · PanelLeft）。
#[allow(non_snake_case)]
pub fn SidebarTrigger(
identity [] id: String,
plain [mut] on_click: Option<Handler<()>>,
plain [] children: Option<crate::core::tree::Element>,
) -> Element {
    let glyph = lucide_glyph("panel-left").unwrap_or_else(|| "☰".into());
    let icon = build_icon_text_node(glyph)
        .text_size(16.0)
        .text_color(parse_text_color("foreground"))
        .pointer_events("none");
    let mut toggle = bind_sidebar_toggle(on_click);
    let btn = button(if id.trim().is_empty() {
        "sidebar-trigger"
    } else {
        id.as_str()
    })
    .variant("ghost")
    .size("icon-sm")
    .child(icon)
    .on_click(move || toggle.call(()));
    let mut el: Element = btn.into();
    el.set_attr(ATTR_SLOT, SLOT_TRIGGER);
    el.set_attr(ATTR_SIDEBAR, "trigger");
    el = el.aria_label(crate::tr_default_zh(
        "jadix.sidebar.toggle",
        "Toggle sidebar",
        "切换侧栏",
    ));
    if let Some(ctx) = use_sidebar() {
        let key = format!("{}__trigger_state:{id}", ctx.id);
        el = crate::app::reactive::bind_prop(
            el,
            key,
            move || {
                if ctx.is_mobile.get() {
                    ctx.open_mobile.get()
                } else {
                    ctx.open.get()
                }
            },
            |el, open| {
                el.set_attr("aria-expanded", if open { "true" } else { "false" });
                LayoutStatus::StyleChanged
            },
        );
    }
    if !id.trim().is_empty() {
        el.template_id = Some(id);
    }
    el
}
}

crate::__component_contract! {
SidebarInputProps [];
/// 侧栏搜索框：`h-8 bg-background shadow-none`。
#[allow(non_snake_case)]
pub fn SidebarInput(
identity [] id: String,
plain [] placeholder: Option<String>,
plain [] children: Option<crate::core::tree::Element>,
) -> Element {
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
    el
}
}

crate::__component_contract! {
SidebarRailProps [];
/// 桌面拖拽 rail（点击 toggle；对位 ShadCN `SidebarRail`）。
#[allow(non_snake_case)]
pub fn SidebarRail(
plain [mut] on_click: Option<Handler<()>>,
plain [] children: Option<crate::core::tree::Element>,
) -> Element {
    let mut toggle = bind_sidebar_toggle(on_click);
    let mut hit = r#box()
        .w("1rem")
        .h("100%")
        .absolute(true)
        .top(0.0)
        .inset_inline_end(-16.0)
        .cursor("col-resize")
        .into();
    hit = interactive(hit)
        .focus_id("__sidebar_rail")
        .on_click(move || toggle.call(()))
        .into();
    hit.set_attr(ATTR_SLOT, SLOT_RAIL);
    hit.set_attr(ATTR_SIDEBAR, "rail");
    hit.set_attr(
        "aria-label",
        crate::tr_default_zh("jadix.sidebar.toggle", "Toggle Sidebar", "切换侧边栏"),
    );
    hit.set_attr(
        "title",
        crate::tr_default_zh("jadix.sidebar.toggle", "Toggle Sidebar", "切换侧边栏"),
    );
    crate::authoring::widgets::classes::with_default_class_list(hit, "max-md:hidden")
}
}

impl_simple_slot_builder!(
    SidebarHeaderBuilder,
    sidebar_header,
    SidebarHeader,
    SidebarHeaderProps
);
impl_simple_slot_builder!(
    SidebarFooterBuilder,
    sidebar_footer,
    SidebarFooter,
    SidebarFooterProps
);
impl_simple_slot_builder!(
    SidebarContentBuilder,
    sidebar_content,
    SidebarContent,
    SidebarContentProps
);
impl_simple_slot_builder!(
    SidebarGroupBuilder,
    sidebar_group,
    SidebarGroup,
    SidebarGroupProps
);
impl_simple_slot_builder!(
    SidebarGroupLabelBuilder,
    sidebar_group_label,
    SidebarGroupLabel,
    SidebarGroupLabelProps
);
impl_simple_slot_builder!(
    SidebarGroupContentBuilder,
    sidebar_group_content,
    SidebarGroupContent,
    SidebarGroupContentProps
);
impl_simple_slot_builder!(
    SidebarMenuBuilder,
    sidebar_menu,
    SidebarMenu,
    SidebarMenuProps
);
impl_simple_slot_builder!(
    SidebarMenuItemBuilder,
    sidebar_menu_item,
    SidebarMenuItem,
    SidebarMenuItemProps
);
impl_simple_slot_builder!(
    SidebarInsetBuilder,
    sidebar_inset,
    SidebarInset,
    SidebarInsetProps
);
impl_simple_slot_builder!(
    SidebarTriggerBuilder,
    sidebar_trigger,
    SidebarTrigger,
    SidebarTriggerProps
);

// ── Builder ───────────────────────────────────────────────────────────────────

pub struct SidebarMenuButtonBuilder {
    id: String,
    label: Option<String>,
    is_active: bool,
    variant: SidebarMenuButtonVariant,
    size: SidebarMenuButtonSize,
    class: Option<String>,
    style: Option<SlotStyleFn>,
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
        style: None,
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
    pub fn variant(mut self, v: impl Into<SidebarMenuButtonVariant>) -> Self {
        self.variant = v.into();
        self
    }
    pub fn size(mut self, v: impl Into<SidebarMenuButtonSize>) -> Self {
        self.size = v.into();
        self
    }
    pub fn class(mut self, c: impl Into<String>) -> Self {
        self.class = Some(c.into());
        self
    }
    pub fn style(
        mut self,
        f: impl Fn(
                crate::components::base::BaseComponentBuilder,
            ) -> crate::components::base::BaseComponentBuilder
            + 'static,
    ) -> Self {
        let prev = self.style;
        self.style = Some(std::rc::Rc::new(move |b| {
            let b = if let Some(ref prev) = prev {
                prev(b)
            } else {
                b
            };
            f(b)
        }));
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
                    weight: MENU_WEIGHT,
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
        let el = SidebarMenuButton(SidebarMenuButtonProps {
            id: b.id,
            is_active: Some(b.is_active),
            variant: Some(b.variant.as_str().into()),
            size: Some(b.size.as_str().into()),
            on_click: b.on_click,
            children,
            base: None,
        });
        SlotChrome::from_opts(b.class, b.style).apply(el)
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
    style: Option<SlotStyleFn>,
    header: Option<Element>,
    groups: Vec<Element>,
    items: Vec<Element>,
    trees: Vec<Element>,
    footer: Option<Element>,
    custom_body: Option<Box<dyn FnOnce() -> Element>>,
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
        style: None,
        header: None,
        groups: Vec::new(),
        items: Vec::new(),
        trees: Vec::new(),
        footer: None,
        custom_body: None,
        provider: false,
        default_open: true,
        open: None,
    }
}

impl SidebarBuilder {
    pub fn rail_w(mut self, w: impl Into<String>) -> Self {
        self.width = w.into();
        self
    }
    /// Alias of [`Self::rail_w`] (collapsible track, not layout `w`).
    pub fn w(self, w: impl Into<String>) -> Self {
        self.rail_w(w)
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
    pub fn style(
        mut self,
        f: impl Fn(
                crate::components::base::BaseComponentBuilder,
            ) -> crate::components::base::BaseComponentBuilder
            + 'static,
    ) -> Self {
        self.style = Some(std::rc::Rc::new(f));
        self
    }
    pub fn child(mut self, body: impl FnOnce() -> Element + 'static) -> Self {
        self.custom_body = Some(Box::new(body));
        self
    }
    pub fn header(mut self, el: impl Into<Element>) -> Self {
        self.header = Some(el.into());
        self
    }
    pub fn header_with(self, f: impl FnOnce(SidebarHeaderBuilder) -> SidebarHeaderBuilder) -> Self {
        self.header(f(sidebar_header()))
    }
    pub fn group(mut self, el: impl Into<Element>) -> Self {
        self.groups.push(el.into());
        self
    }
    pub fn group_with(self, f: impl FnOnce(SidebarGroupBuilder) -> SidebarGroupBuilder) -> Self {
        self.group(f(sidebar_group()))
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
    pub fn footer_with(self, f: impl FnOnce(SidebarFooterBuilder) -> SidebarFooterBuilder) -> Self {
        self.footer(f(sidebar_footer()))
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
            style,
            header,
            groups,
            items,
            trees,
            footer,
            custom_body,
            provider,
            default_open,
            open,
        } = builder;

        let provider_id = format!("{id}__provider");
        let build = move || {
            let parts: Vec<Element> = if let Some(body) = custom_body {
                take_packed(body())
            } else {
                let mut parts: Vec<Element> = Vec::new();
                if let Some(h) = header {
                    parts.push(ensure_data_slot(h, SLOT_HEADER, |el| {
                        sidebar_header().child(el).into()
                    }));
                }

                let mut content_kids: Vec<Element> = Vec::new();
                for g in groups {
                    content_kids.push(ensure_data_slot(g, SLOT_GROUP, |el| {
                        sidebar_group().child(el).into()
                    }));
                }
                if !items.is_empty() {
                    let mut menu_items = Vec::new();
                    for item in items {
                        menu_items.push(ensure_data_slot(item, SLOT_MENU_ITEM, |el| {
                            sidebar_menu_item().child(el).into()
                        }));
                    }
                    let mut frag = Element::new();
                    frag.set_attr("jadix_fragment", "1");
                    content_kids.push(sidebar_menu().child(frag.children(menu_items)).into());
                }
                content_kids.extend(trees);

                if !content_kids.is_empty() {
                    let mut frag = Element::new();
                    frag.set_attr("jadix_fragment", "1");
                    parts.push(sidebar_content().child(frag.children(content_kids)).into());
                }

                if let Some(f) = footer {
                    parts.push(ensure_data_slot(f, SLOT_FOOTER, |el| {
                        sidebar_footer().child(el).into()
                    }));
                }
                parts
            };

            let mut frag = Element::new();
            frag.set_attr("jadix_fragment", "1");
            let mut rail = Sidebar(SidebarProps {
                id: id.clone(),
                rail_w: Some(width),
                side: Some(side.as_str().into()),
                variant: Some(variant.as_str().into()),
                collapsible: Some(collapsible.as_str().into()),
                children: Some(frag.children(parts)),
                base: None,
            });
            rail = SlotChrome::from_opts(class, style).apply(rail);
            rail
        };

        if provider {
            sidebar_provider_scope(
                provider_id,
                open,
                default_open,
                Some(variant.as_str()),
                build,
            )
        } else {
            build()
        }
    }
}

impl crate::core::tree::IntoElementList for SidebarBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/sidebar.rs"]
mod tests;
