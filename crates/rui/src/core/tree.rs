use taffy::prelude::*;
use winit::event::KeyEvent;

use crate::core::reactivity::Signal;

pub use crate::layout::dimension::{
    dim_kind, dim_length_px, dimension_to_lp, dimension_to_lpa, lp_kind, lp_length_px, lpa_kind,
    lpa_length_px, CssDim, CssRectPair, DimKind, IntoBorderWidths, IntoCssDim, IntoCssRectPair,
    IntoDimension, IntoF32, IntoF32Raw, IntoLengthPercentage, PercentAxis, UnitCtx,
    CSS_ROOT_FONT_SIZE_PX,
};
pub use crate::layout::layout_tree::{LayoutProfileMs, LayoutTree, RenderResult, TextLayoutInfo};
pub use crate::style::{
    parse_color, parse_text_color, CursorKind, CursorValue, LineStyle, TextDecorationStyle,
    TextOverflow, TextStyle,
};
pub use crate::style::{Style, UnitSlot};

/// Pointer hit policy for [`Element`] (`rui_pointer_events`), RN-aligned subset.
///
/// | Mode | Self | Children |
/// |------|------|----------|
/// | [`Auto`](Self::Auto) | yes | yes |
/// | [`None`](Self::None) | no | no |
/// | [`BoxNone`](Self::BoxNone) | no | yes |
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PointerEvents {
    #[default]
    Auto,
    None,
    BoxNone,
}

pub const ATTR_POINTER_EVENTS: &str = "rui_pointer_events";

impl PointerEvents {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "none" => Self::None,
            "box-none" | "box_none" => Self::BoxNone,
            "auto" | "" => Self::Auto,
            _ => Self::Auto,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::None => "none",
            Self::BoxNone => "box-none",
        }
    }
}

/// Parse HTML-ish bool literals for RSX attrs (`clearable="true"`, `absolute="1"`, …).
pub fn parse_bool_like(value: impl ParseBoolLike) -> bool {
    value.to_bool()
}

pub trait ParseBoolLike {
    fn to_bool(self) -> bool;
}

impl ParseBoolLike for bool {
    fn to_bool(self) -> bool {
        self
    }
}

impl ParseBoolLike for &str {
    fn to_bool(self) -> bool {
        match self.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => true,
            "false" | "0" | "no" | "off" | "" => false,
            _ => false,
        }
    }
}

impl ParseBoolLike for String {
    fn to_bool(self) -> bool {
        self.as_str().to_bool()
    }
}

impl ParseBoolLike for &String {
    fn to_bool(self) -> bool {
        self.as_str().to_bool()
    }
}

/// Whether hit-testing may target this node's own bounds (not only descendants).
#[inline]
/// Parse CSS time value like `"300ms"` or `"0.3s"` into milliseconds.
pub fn parse_time_ms(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(ms) = s.strip_suffix("ms") {
        ms.trim().parse::<f32>().ok()
    } else if let Some(secs) = s.strip_suffix('s') {
        secs.trim().parse::<f32>().ok().map(|v| v * 1000.0)
    } else {
        s.parse::<f32>().ok()
    }
}

pub fn pointer_events_targets_self(el: &Element) -> bool {
    el.pointer_events_mode() == PointerEvents::Auto
}

/// Whether hit-testing may descend into children.
#[inline]
pub fn pointer_events_targets_children(el: &Element) -> bool {
    el.pointer_events_mode() != PointerEvents::None
}

/// [`Element::button_hover_paint`]：用于无整树重建的按钮 hover / focus 环同步（见 `components/button.rs`）。
#[derive(Debug, Clone)]
pub struct ButtonHoverPaint {
    pub variant: String,
    pub disabled: bool,
    /// `"spring"`（欠阻尼弹簧）或 `"cubic"`（`cubic-bezier` 时长型过渡）。
    pub hover_easing: String,
    /// 显式字色（如 Calendar 非本月 `muted-foreground`）；`None` 跟变体 chrome。
    pub text_color: Option<[u8; 4]>,
}

/// [`Element::input_paint_sync`]：每帧按信号 `peek` 刷新 Input/Textarea 边框、阴影、文本与光标（避免根 `create_effect` 订阅焦点与按键）。
#[derive(Debug, Clone)]
pub struct TextEditPaintSync {
    pub base_id: String,
    pub placeholder: String,
    pub invalid: bool,
    pub disabled: bool,
    pub text_size_px: f32,
    pub font_weight: u16,
    pub line_height_px: Option<f32>,
    pub leading_scale: Option<f32>,
    /// HTML 风格 `type`（`text` / `password` / `email` / …），影响显示掩码与输入过滤。
    pub input_type: String,
    /// 非空时可点的清除（叉号）按钮。
    pub clearable: bool,
    /// `type=password` 时显示睁眼/闭眼切换明文。
    pub password_toggle: bool,
    /// `true` → `textarea:{id}` 焦点与多行编辑；`false` → `input:{id}`。
    pub multiline: bool,
    /// 多行内部滚动视口 rest id（`{id}__ta_scroll`）；单行为 `None`。
    pub scroll_rest: Option<String>,
    /// 多行可拖拽改高。
    pub resizable: bool,
    pub resize_min_h: f32,
    pub resize_max_h: f32,
    /// Command 过滤框等：无边框/阴影/焦点环，视觉壳由外层提供。
    pub plain_chrome: bool,
}

/// 兼容旧名：与 [`TextEditPaintSync`] 相同。
pub type InputPaintSync = TextEditPaintSync;

/// Generic signal-to-element synchronization hook.
///
/// Stateful components attach one of these to their root when a signal only needs to update the
/// existing element tree (style/layout/text) rather than reconstruct a subtree through
/// [`crate::app::reactive`]. The callback runs during the frame prepass and reports its layout
/// impact. This is the reusable component-level reactive capability; structural changes such as
/// mounting an overlay still use a reactive subtree boundary.
#[derive(Clone)]
pub struct ReactiveSync {
    callback: std::rc::Rc<dyn Fn(&mut Element, &mut LayoutTree) -> LayoutStatus>,
}

impl ReactiveSync {
    pub fn new(callback: impl Fn(&mut Element, &mut LayoutTree) -> LayoutStatus + 'static) -> Self {
        Self {
            callback: std::rc::Rc::new(callback),
        }
    }

    pub fn run(&self, element: &mut Element, layout_tree: &mut LayoutTree) -> LayoutStatus {
        (self.callback)(element, layout_tree)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutStatus {
    /// No changes needed.
    Clean,
    /// Styles changed (color, etc.) - only repaint needed, layout is valid.
    StyleChanged,
    /// Layout properties changed (width, margin, etc.) - need Taffy recompute, but tree structure is same.
    LayoutChanged,
    /// Tree structure changed (nodes added/removed) - need full Taffy rebuild.
    StructureChanged,
}

impl LayoutStatus {
    /// Strongest-wins composition for merging per-frame sync sources (hover paint, toggles, etc.).
    pub fn merge(self, other: Self) -> Self {
        use LayoutStatus::*;
        match (self, other) {
            (StructureChanged, _) | (_, StructureChanged) => StructureChanged,
            (LayoutChanged, _) | (_, LayoutChanged) => LayoutChanged,
            (StyleChanged, _) | (_, StyleChanged) => StyleChanged,
            _ => Clean,
        }
    }
}

/// 挂在 [`Element::on_unmount`]：最后一个 `Rc` 释放时执行一次（树节点丢弃即卸载）。
pub struct UnmountHook {
    f: std::cell::RefCell<Option<Box<dyn FnOnce()>>>,
}

impl UnmountHook {
    pub fn new(f: impl FnOnce() + 'static) -> std::rc::Rc<Self> {
        std::rc::Rc::new(Self {
            f: std::cell::RefCell::new(Some(Box::new(f))),
        })
    }

    /// 取消回调（例如已手动 [`Navigator::dispose`](crate::app::navigator::Navigator::dispose)）。
    pub fn disarm(&self) {
        *self.f.borrow_mut() = None;
    }

    /// Run the hook early (e.g. when chaining multiple dispose callbacks on one element).
    pub fn fire(&self) {
        if let Some(f) = self.f.borrow_mut().take() {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        }
    }
}

impl Drop for UnmountHook {
    fn drop(&mut self) {
        let Some(f) = self.f.borrow_mut().take() else {
            return;
        };
        // Skip if reactive TLS is already tearing down (HashMap/TLS dtor re-entrancy).
        if !crate::core::reactivity::runtime_tls_alive() {
            return;
        }
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    }
}

impl Drop for Element {
    fn drop(&mut self) {
        // Default recursive `Vec<Element>` Drop overflows on tall UI trees (Gallery) with
        // Windows' ~1MB stack in debug builds. Drain the subtree iteratively instead.
        let mut stack = std::mem::take(&mut self.children);
        while let Some(mut node) = stack.pop() {
            stack.append(&mut node.children);
            // `node` drops here with empty `children` → O(1) stack frame per element.
        }
    }
}

pub struct Element {
    pub style: crate::style::Style,
    pub children: Vec<Element>,
    pub text: Option<String>,
    /// Rich-text spans; when set, prepare uses set_rich_text. `text` is concat fallback.
    pub text_spans: Option<Vec<crate::style::TextSpan>>,
    /// 仅参与 glyph 排版（`set_text` / 宽度估计），不计入 `text` 的字节长度；用于减轻纯中文 IME 下 cosmic-text 行高随拼音 bbox 抖动。
    pub text_shape_suffix: Option<String>,
    pub text_style: TextStyle,
    pub layout_bounds: [f32; 4],
    /// Visual offset applied by `position: sticky` clamping (computed during layout collect).
    pub sticky_visual_offset: [f32; 2],
    /// Per-element CSS transition tween state (persists across frames).
    pub transition_snapshot: crate::style::TransitionSnapshot,
    pub on_click: Option<std::rc::Rc<std::cell::RefCell<dyn FnMut()>>>,
    pub on_hover: Option<std::rc::Rc<std::cell::RefCell<dyn FnMut(bool)>>>,
    /// DOM [`mousedown`](https://developer.mozilla.org/zh-CN/docs/Web/API/Element/mousedown_event).
    /// [`Self::on_pointer_down`] is a compatibility alias of this slot.
    pub on_mousedown: Option<
        std::rc::Rc<std::cell::RefCell<dyn FnMut(&crate::app::pointer_events::PointerEvent)>>,
    >,
    /// DOM `mouseup`. [`Self::on_pointer_up`] is a compatibility alias.
    pub on_mouseup: Option<
        std::rc::Rc<std::cell::RefCell<dyn FnMut(&crate::app::pointer_events::PointerEvent)>>,
    >,
    /// Right-click / context menu.
    pub on_context_menu: Option<
        std::rc::Rc<std::cell::RefCell<dyn FnMut(&crate::app::pointer_events::PointerEvent)>>,
    >,
    /// Double-click (detail >= 2). DOM `dblclick`. [`Self::on_dblclick`] is an alias.
    pub on_double_click: Option<
        std::rc::Rc<std::cell::RefCell<dyn FnMut(&crate::app::pointer_events::PointerEvent)>>,
    >,
    /// DOM `mousemove`.
    pub on_mousemove: Option<
        std::rc::Rc<std::cell::RefCell<dyn FnMut(&crate::app::pointer_events::PointerEvent)>>,
    >,
    /// DOM `mouseover` (bubbles; [`crate::app::pointer_events::MouseEvent::related_target`] is the node being left).
    pub on_mouseover: Option<
        std::rc::Rc<std::cell::RefCell<dyn FnMut(&crate::app::pointer_events::PointerEvent)>>,
    >,
    /// DOM `mouseout` (bubbles; [`crate::app::pointer_events::MouseEvent::related_target`] is the node being entered).
    pub on_mouseout: Option<
        std::rc::Rc<std::cell::RefCell<dyn FnMut(&crate::app::pointer_events::PointerEvent)>>,
    >,
    /// Keyboard on focused node. Return `true` if the event is consumed.
    pub on_key_down: Option<std::rc::Rc<std::cell::RefCell<dyn FnMut(KeyEvent) -> bool>>>,
    /// 节点从树中丢弃且 `Rc` 计数归零时触发（用于 Navigator Host dispose 等）。
    pub on_unmount: Option<std::rc::Rc<UnmountHook>>,
    pub focus_id: Option<String>,
    pub focusable: bool,
    pub cursor: Option<CursorValue>,
    pub scroll_offset: [f32; 2],
    /// True if this node or any descendant is a scroll viewport (`scroll:` focus id).
    /// Used to prune subtrees in the ViewportsOnly scrollbar prepass (see
    /// [`scroll_viewport_subtree_count`] for nested viewports).
    pub contains_scroll_viewport: bool,
    /// Count of `scroll:` viewport nodes in this subtree (including self). Nested viewports need
    /// scrollbar geometry prepass to recurse past ancestor viewports when this is > 1 on a viewport.
    pub scroll_viewport_subtree_count: u32,
    pub template_hash: Option<String>,
    pub template_id: Option<String>,
    /// 若存在，则每帧由 `sync_button_hover_paint_subtree` 根据 `{focus_id}_hover` 与全局焦点信号刷新样式（避免 `hover.get()` / `focused.get()` 订阅根 effect）。
    pub button_hover_paint: Option<ButtonHoverPaint>,
    /// 若存在，则每帧由 [`crate::components::internal::text_edit::sync_text_edit_paint_subtree`] 刷新（`peek` 全部输入相关信号）。
    pub input_paint_sync: Option<TextEditPaintSync>,
    /// Switch track: [`crate::components::switch::sync_switch_checked_subtree`] reads `checked` and animates track bg + thumb translate.
    pub switch_checked_sync: Option<Signal<bool>>,
    /// Checkbox root: [`crate::components::checkbox::sync_checkbox_checked_subtree`] updates fill + checkmark color.
    pub checkbox_sync: Option<Signal<bool>>,
    /// Radio item control: group [`Signal<String>`] + attrs `rui_radio_value`；见 `sync_radio_checked_subtree`。
    pub radio_selected_sync: Option<Signal<String>>,
    /// Opt-in keyboard focus ring ([`crate::style::sync_focus_ring_subtree`]); not used by Button / Input.
    pub focus_ring: bool,
    /// Generic component-owned signal synchronization; see [`ReactiveSync`].
    pub reactive_sync: Option<ReactiveSync>,
    /// GPU 贴图资源键（见 [`crate::queue_image_rgba8`]）；布局收集时生成纹理四边形实例。
    pub image_texture: Option<String>,
    /// Parsed utility class list + base style snapshot for variant re-resolve (`hover:` / `dark:` …).
    pub class_set: Option<crate::style::ClassSet>,
    pub(crate) layout_node: Option<taffy::prelude::NodeId>,
    pub attrs: std::collections::HashMap<String, String>,
}

/// A wrapper for event handlers to allow them to be passed as props in functional components.
pub struct Handler<T = ()>(pub Box<dyn FnMut(T)>);

impl<T> Handler<T> {
    pub fn new(f: impl FnMut(T) + 'static) -> Self {
        Self(Box::new(f))
    }
    pub fn call(&mut self, arg: T) {
        (self.0)(arg);
    }
}

pub trait IntoHandler<T> {
    fn into_handler(self) -> Handler<T>;
}

impl<T, F> From<F> for Handler<T>
where
    F: FnMut(T) + 'static,
{
    fn from(f: F) -> Self {
        Self::new(f)
    }
}

pub struct ZeroArgFn<F>(pub F);
impl<F> From<ZeroArgFn<F>> for Handler<()>
where
    F: FnMut() + 'static,
{
    fn from(mut f: ZeroArgFn<F>) -> Self {
        Handler::new(move |_| (f.0)())
    }
}

impl Default for Handler<()> {
    fn default() -> Self {
        Self(Box::new(|_| {}))
    }
}

impl From<Box<dyn FnMut()>> for Handler<()> {
    fn from(mut f: Box<dyn FnMut()>) -> Self {
        Self(Box::new(move |_| f()))
    }
}

/// A wrapper for key event handlers.
pub struct KeyHandler(pub Box<dyn FnMut(KeyEvent) -> bool>);

impl KeyHandler {
    pub fn new(f: impl FnMut(KeyEvent) -> bool + 'static) -> Self {
        Self(Box::new(f))
    }
    pub fn call(&mut self, ev: KeyEvent) -> bool {
        (self.0)(ev)
    }
}

impl<F> From<F> for KeyHandler
where
    F: FnMut(KeyEvent) -> bool + 'static,
{
    fn from(f: F) -> Self {
        Self::new(f)
    }
}

impl Default for KeyHandler {
    fn default() -> Self {
        Self(Box::new(|_| false))
    }
}

pub trait IntoElement {
    fn into_element(self) -> Element;
}

impl IntoElement for Element {
    fn into_element(self) -> Element {
        self
    }
}

impl IntoElement for String {
    fn into_element(self) -> Element {
        Element::new().text(self)
    }
}

impl IntoElement for &str {
    fn into_element(self) -> Element {
        Element::new().text(self.to_string())
    }
}

pub trait IntoElementList {
    fn into_element_list(self) -> Vec<Element>;
}

impl IntoElementList for Element {
    fn into_element_list(mut self) -> Vec<Element> {
        if self.attrs.get("rui_fragment").map(|s| s.as_str()) == Some("1") {
            std::mem::take(&mut self.children)
        } else {
            vec![self]
        }
    }
}

/// Bind an element's `template_id` into a reactive slot (`ref={...}` macro DX).
pub fn bind_element_ref(slot: &Signal<Option<String>>, el: &Element) {
    if let Some(tid) = el.template_id.as_ref() {
        slot.set(Some(tid.clone()));
    }
}

/// Resolve `<Dynamic component={...}>`: `FnOnce() -> impl IntoElement` or an [`Element`].
pub trait IntoDynamicComponent {
    fn into_dynamic_component(self) -> Element;
}

impl IntoDynamicComponent for Element {
    fn into_dynamic_component(self) -> Element {
        self
    }
}

impl<F, R> IntoDynamicComponent for F
where
    F: FnOnce() -> R,
    R: IntoElement,
{
    fn into_dynamic_component(self) -> Element {
        self().into_element()
    }
}

impl IntoElementList for Vec<Element> {
    fn into_element_list(self) -> Vec<Element> {
        self
    }
}

impl IntoElementList for Option<Element> {
    fn into_element_list(self) -> Vec<Element> {
        self.map(|e| vec![e]).unwrap_or_default()
    }
}

impl IntoElementList for String {
    fn into_element_list(self) -> Vec<Element> {
        vec![Element::new().text(self)]
    }
}

impl IntoElementList for &str {
    fn into_element_list(self) -> Vec<Element> {
        vec![Element::new().text(self.to_string())]
    }
}

/// Argument to [`Element::text`]: a static string, or (via impls in `app/reactive`) a
/// [`crate::core::reactivity::Signal`] / `FnMut() -> String` live binding.
pub trait IntoNodeText {
    fn apply_node_text(self, el: Element) -> Element;
}

impl IntoNodeText for String {
    fn apply_node_text(self, mut el: Element) -> Element {
        el.text = Some(self);
        el.style.flex_shrink = 0.0;
        el
    }
}

impl IntoNodeText for &str {
    fn apply_node_text(self, el: Element) -> Element {
        el.text(self.to_string())
    }
}

impl IntoNodeText for &String {
    fn apply_node_text(self, el: Element) -> Element {
        el.text(self.clone())
    }
}

impl Element {
    pub(crate) fn into_margin_auto(val: impl IntoDimension) -> LengthPercentageAuto {
        crate::layout::dimension_to_lpa(val.into_dimension())
    }

    fn set_css(mut self, slot: UnitSlot, dim: impl IntoCssDim) -> Self {
        self.style.set_css_dim(slot, dim.into_css_dim());
        self
    }

    fn apply_rect_pair(self, start: UnitSlot, end: UnitSlot, pair: impl IntoCssRectPair) -> Self {
        match pair.into_css_rect_pair() {
            CssRectPair::Both(d) => self.set_css(start, d.clone()).set_css(end, d),
            CssRectPair::StartEnd(a, b) => self.set_css(start, a).set_css(end, b),
        }
    }

    fn parse_overflow_mode(mode: &str) -> taffy::style::Overflow {
        match mode.to_ascii_lowercase().as_str() {
            "hidden" => taffy::style::Overflow::Hidden,
            "clip" => taffy::style::Overflow::Clip,
            "scroll" => taffy::style::Overflow::Scroll,
            // Taffy does not expose CSS `auto`; map to scrollable behavior.
            "auto" => taffy::style::Overflow::Scroll,
            _ => taffy::style::Overflow::Visible,
        }
    }

    pub fn new() -> Self {
        Self {
            style: crate::style::Style::default(),
            children: Vec::new(),
            text: None,
            text_spans: None,
            text_shape_suffix: None,
            text_style: TextStyle::default(),
            layout_bounds: [0.0, 0.0, 0.0, 0.0],
            sticky_visual_offset: [0.0, 0.0],
            transition_snapshot: Default::default(),
            on_click: None,
            on_hover: None,
            on_mousedown: None,
            on_mouseup: None,
            on_context_menu: None,
            on_double_click: None,
            on_mousemove: None,
            on_mouseover: None,
            on_mouseout: None,
            on_key_down: None,
            on_unmount: None,
            focus_id: None,
            focusable: false,
            cursor: None,
            scroll_offset: [0.0, 0.0],
            contains_scroll_viewport: false,
            scroll_viewport_subtree_count: 0,
            template_hash: None,
            template_id: None,
            button_hover_paint: None,
            input_paint_sync: None,
            switch_checked_sync: None,
            checkbox_sync: None,
            radio_selected_sync: None,
            focus_ring: false,
            reactive_sync: None,
            image_texture: None,
            class_set: None,
            layout_node: None,
            attrs: std::collections::HashMap::new(),
        }
    }
}

impl Default for Element {
    fn default() -> Self {
        Self::new()
    }
}

impl Element {
    /// 绑定已上传的贴图键；矩形区域由布局样式（`w`/`h` 等）决定。
    pub fn image_texture(mut self, key: impl Into<String>) -> Self {
        self.image_texture = Some(key.into());
        self
    }

    pub fn set_attr(&mut self, key: impl Into<String>, value: impl Into<String>) -> &mut Self {
        self.attrs.insert(key.into(), value.into());
        self
    }

    pub fn get_attr(&self, key: &str) -> Option<&String> {
        self.attrs.get(key)
    }

    pub fn template_hash(mut self, hash: impl Into<String>) -> Self {
        self.template_hash = Some(hash.into());
        self
    }

    pub fn template_id(mut self, id: impl Into<String>) -> Self {
        self.template_id = Some(id.into());
        self
    }

    pub fn cursor_kind(mut self, c: CursorKind) -> Self {
        self.cursor = Some(c.into());
        self
    }

    /// CSS [`cursor`](https://developer.mozilla.org/zh-CN/docs/Web/CSS/Reference/Properties/cursor)。
    /// `inherit` / `unset` 清除本节点声明（沿用祖先）。
    pub fn cursor(mut self, value: impl AsRef<str>) -> Self {
        self.cursor = CursorValue::parse_css(value.as_ref());
        self
    }

    pub fn cursor_text(self) -> Self {
        self.cursor_kind(CursorKind::Text)
    }
    pub fn cursor_not_allowed(self) -> Self {
        self.cursor_kind(CursorKind::NotAllowed)
    }
    pub fn min_w(self, w: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::MinWidth, w)
    }
    pub fn min_h(self, h: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::MinHeight, h)
    }
    pub fn max_w(self, w: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::MaxWidth, w)
    }
    pub fn max_h(self, h: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::MaxHeight, h)
    }
    pub fn on_click(mut self, handler: impl FnMut() + 'static) -> Self {
        self.on_click = Some(std::rc::Rc::new(std::cell::RefCell::new(handler)));
        self
    }

    /// DOM [`mousedown`](https://developer.mozilla.org/zh-CN/docs/Web/API/Element/mousedown_event).
    pub fn on_mousedown(
        mut self,
        handler: impl FnMut(&crate::app::pointer_events::PointerEvent) + 'static,
    ) -> Self {
        self.on_mousedown = Some(std::rc::Rc::new(std::cell::RefCell::new(handler)));
        self
    }

    /// DOM [`mouseup`](https://developer.mozilla.org/zh-CN/docs/Web/API/Element/mouseup_event).
    pub fn on_mouseup(
        mut self,
        handler: impl FnMut(&crate::app::pointer_events::PointerEvent) + 'static,
    ) -> Self {
        self.on_mouseup = Some(std::rc::Rc::new(std::cell::RefCell::new(handler)));
        self
    }

    /// Compatibility alias of [`Self::on_mousedown`].
    pub fn on_pointer_down(
        self,
        handler: impl FnMut(&crate::app::pointer_events::PointerEvent) + 'static,
    ) -> Self {
        self.on_mousedown(handler)
    }

    /// Compatibility alias of [`Self::on_mouseup`].
    pub fn on_pointer_up(
        self,
        handler: impl FnMut(&crate::app::pointer_events::PointerEvent) + 'static,
    ) -> Self {
        self.on_mouseup(handler)
    }

    pub fn on_context_menu(
        mut self,
        handler: impl FnMut(&crate::app::pointer_events::PointerEvent) + 'static,
    ) -> Self {
        self.on_context_menu = Some(std::rc::Rc::new(std::cell::RefCell::new(handler)));
        self
    }

    pub fn on_double_click(
        mut self,
        handler: impl FnMut(&crate::app::pointer_events::PointerEvent) + 'static,
    ) -> Self {
        self.on_double_click = Some(std::rc::Rc::new(std::cell::RefCell::new(handler)));
        self
    }

    /// DOM `dblclick` — alias of [`Self::on_double_click`].
    pub fn on_dblclick(
        self,
        handler: impl FnMut(&crate::app::pointer_events::PointerEvent) + 'static,
    ) -> Self {
        self.on_double_click(handler)
    }

    /// DOM [`mousemove`](https://developer.mozilla.org/zh-CN/docs/Web/API/Element/mousemove_event).
    pub fn on_mousemove(
        mut self,
        handler: impl FnMut(&crate::app::pointer_events::PointerEvent) + 'static,
    ) -> Self {
        self.on_mousemove = Some(std::rc::Rc::new(std::cell::RefCell::new(handler)));
        self
    }

    /// DOM [`mouseover`](https://developer.mozilla.org/zh-CN/docs/Web/API/Element/mouseover_event).
    pub fn on_mouseover(
        mut self,
        handler: impl FnMut(&crate::app::pointer_events::PointerEvent) + 'static,
    ) -> Self {
        self.on_mouseover = Some(std::rc::Rc::new(std::cell::RefCell::new(handler)));
        self
    }

    /// DOM [`mouseout`](https://developer.mozilla.org/zh-CN/docs/Web/API/Element/mouseout_event).
    pub fn on_mouseout(
        mut self,
        handler: impl FnMut(&crate::app::pointer_events::PointerEvent) + 'static,
    ) -> Self {
        self.on_mouseout = Some(std::rc::Rc::new(std::cell::RefCell::new(handler)));
        self
    }

    /// 节点从 UI 树移除且钩子 `Rc` 归零时调用一次。
    pub fn on_unmount(mut self, f: impl FnOnce() + 'static) -> Self {
        self.on_unmount = Some(UnmountHook::new(f));
        self
    }

    pub fn on_unmount_hook(mut self, hook: std::rc::Rc<UnmountHook>) -> Self {
        self.on_unmount = Some(hook);
        self
    }

    /// Attach component-owned reactive synchronization without subscribing the root UI effect.
    pub fn reactive_sync(
        mut self,
        callback: impl Fn(&mut Element, &mut LayoutTree) -> LayoutStatus + 'static,
    ) -> Self {
        self.reactive_sync = Some(ReactiveSync::new(callback));
        self
    }

    /// `rui_pointer_events`：命中策略（对齐 RN / 实用 CSS 子集）。
    ///
    /// - `auto`（默认）：自身与子节点都可命中
    /// - `none`：自身与子树均不命中（埋层页面等）
    /// - `box-none`：自身不作为目标，仍命中子节点（portal 壳、装饰层）
    pub fn pointer_events(mut self, mode: impl AsRef<str>) -> Self {
        match PointerEvents::parse(mode.as_ref()) {
            PointerEvents::Auto => {
                self.attrs.remove(ATTR_POINTER_EVENTS);
            }
            other => {
                self.set_attr(ATTR_POINTER_EVENTS, other.as_str());
            }
        }
        self
    }

    /// [`PointerEvents::None`] — 整树不命中。
    pub fn pointer_events_none(mut self) -> Self {
        self.set_attr(ATTR_POINTER_EVENTS, PointerEvents::None.as_str());
        self
    }

    /// [`PointerEvents::BoxNone`] — 自身穿透，子节点仍可命中。
    pub fn pointer_events_box_none(mut self) -> Self {
        self.set_attr(ATTR_POINTER_EVENTS, PointerEvents::BoxNone.as_str());
        self
    }

    pub fn pointer_events_mode(&self) -> PointerEvents {
        self.attrs
            .get(ATTR_POINTER_EVENTS)
            .map(|s| PointerEvents::parse(s))
            .unwrap_or(PointerEvents::Auto)
    }

    /// `true` unless [`PointerEvents::None`]（兼容旧调用）。
    pub fn pointer_events_enabled(&self) -> bool {
        self.pointer_events_mode() != PointerEvents::None
    }
    pub fn on_hover(mut self, handler: impl FnMut(bool) + 'static) -> Self {
        self.on_hover = Some(std::rc::Rc::new(std::cell::RefCell::new(handler)));
        self
    }
    pub fn on_key_down(mut self, handler: impl FnMut(KeyEvent) -> bool + 'static) -> Self {
        self.on_key_down = Some(std::rc::Rc::new(std::cell::RefCell::new(handler)));
        self
    }
    pub fn focus_id(mut self, id: impl Into<String>) -> Self {
        self.focus_id = Some(id.into());
        self
    }
    pub fn focusable(mut self, focusable: bool) -> Self {
        self.focusable = focusable;
        self
    }
    /// Enable shared focus-visible ring (theme `ring`, blur 3). Keyboard Tab only; see [`crate::style::focus_ring`].
    pub fn focus_ring(mut self, enabled: bool) -> Self {
        self.focus_ring = enabled;
        self
    }
    pub fn scroll_offset(mut self, x: f32, y: f32) -> Self {
        self.scroll_offset = [x.max(0.0), y.max(0.0)];
        self
    }
    pub fn gap(self, g: impl IntoCssDim) -> Self {
        let d = g.into_css_dim();
        self.set_css(UnitSlot::GapX, d.clone())
            .set_css(UnitSlot::GapY, d)
    }

    /// `column-gap` / Tailwind `gap-x-*`。
    pub fn gap_x(self, g: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::GapX, g)
    }

    /// `row-gap` / Tailwind `gap-y-*`。
    pub fn gap_y(self, g: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::GapY, g)
    }

    pub fn column_gap(self, g: impl IntoCssDim) -> Self {
        self.gap_x(g)
    }

    pub fn row_gap(self, g: impl IntoCssDim) -> Self {
        self.gap_y(g)
    }

    pub fn overflow(mut self, mode: impl AsRef<str>) -> Self {
        let o = Self::parse_overflow_mode(mode.as_ref());
        self.style.overflow.x = o;
        self.style.overflow.y = o;
        self
    }
    pub fn overflow_x(mut self, mode: impl AsRef<str>) -> Self {
        self.style.overflow.x = Self::parse_overflow_mode(mode.as_ref());
        self
    }
    pub fn overflow_y(mut self, mode: impl AsRef<str>) -> Self {
        self.style.overflow.y = Self::parse_overflow_mode(mode.as_ref());
        self
    }
    /// Static string, [`crate::core::reactivity::Signal`], or `|| String` compute.
    /// `text(sig.get())` is still a snapshot — pass `sig` or `|| sig.get().to_string()`.
    pub fn text(self, text: impl IntoNodeText) -> Self {
        text.apply_node_text(self)
    }
    pub fn font_family(mut self, family: impl Into<String>) -> Self {
        self.text_style.font_family = Some(family.into());
        self.attrs
            .insert("font_family".to_string(), "1".to_string());
        self
    }
    pub fn text_color(mut self, color: [u8; 4]) -> Self {
        self.text_style.color = color;
        self.attrs.insert("text_color".to_string(), "1".to_string());
        self
    }
    pub fn text_size(mut self, size: f32) -> Self {
        self.text_style.size = size;
        self.attrs.insert("text_size".to_string(), "1".to_string());
        self
    }
    pub fn font_weight(mut self, weight: impl IntoF32) -> Self {
        let w = weight.into_f32().round().clamp(100.0, 900.0) as u16;
        self.text_style.weight = w;
        self.attrs
            .insert("font_weight".to_string(), "1".to_string());
        self
    }
    pub fn underline(mut self, underline: bool) -> Self {
        self.text_style.underline = underline;
        // Mark explicit so `cascade_inheritable_text_style` does not wipe hover toggles
        // (e.g. Button `link` → `hover:underline`).
        self.attrs
            .insert("text_decoration_line".to_string(), "1".to_string());
        self
    }
    pub fn text_decoration(mut self, decoration: impl Into<String>) -> Self {
        let raw = decoration.into().to_lowercase();
        self.attrs
            .insert("text_decoration_line".to_string(), "1".to_string());
        self.attrs
            .insert("text_decoration_style".to_string(), "1".to_string());
        if raw.contains("none") {
            self.text_style.underline = false;
            self.text_style.overline = false;
            self.text_style.line_through = false;
            return self;
        }
        let has_line_token = raw.contains("underline")
            || raw.contains("overline")
            || raw.contains("line-through")
            || raw.contains("linethrough")
            || raw.contains("strikethrough");
        if has_line_token {
            self.text_style.underline = false;
            self.text_style.overline = false;
            self.text_style.line_through = false;
            if raw.contains("underline") {
                self.text_style.underline = true;
            }
            if raw.contains("overline") {
                self.text_style.overline = true;
            }
            if raw.contains("line-through")
                || raw.contains("linethrough")
                || raw.contains("strikethrough")
            {
                self.text_style.line_through = true;
            }
        }
        if raw.contains("double") {
            self.text_style.decoration_style = TextDecorationStyle::Double;
        } else if raw.contains("dotted") {
            self.text_style.decoration_style = TextDecorationStyle::Dotted;
        } else if raw.contains("dashed") {
            self.text_style.decoration_style = TextDecorationStyle::Dashed;
        } else if raw.contains("wavy") {
            self.text_style.decoration_style = TextDecorationStyle::Wavy;
        } else if raw.contains("solid") {
            self.text_style.decoration_style = TextDecorationStyle::Solid;
        }
        self
    }
    pub fn text_decoration_line(mut self, line: impl Into<String>) -> Self {
        let raw = line.into().to_lowercase();
        self.attrs
            .insert("text_decoration_line".to_string(), "1".to_string());
        if raw.contains("none") {
            self.text_style.underline = false;
            self.text_style.overline = false;
            self.text_style.line_through = false;
            return self;
        }
        if raw.contains("underline") {
            self.text_style.underline = true;
        }
        if raw.contains("overline") {
            self.text_style.overline = true;
        }
        if raw.contains("line-through")
            || raw.contains("linethrough")
            || raw.contains("strikethrough")
        {
            self.text_style.line_through = true;
        }
        self
    }
    pub fn text_decoration_style(mut self, style: impl Into<String>) -> Self {
        let s = style.into().trim().to_lowercase();
        self.attrs
            .insert("text_decoration_style".to_string(), "1".to_string());
        self.text_style.decoration_style = match s.as_str() {
            "double" => TextDecorationStyle::Double,
            "dotted" => TextDecorationStyle::Dotted,
            "dashed" => TextDecorationStyle::Dashed,
            "wavy" => TextDecorationStyle::Wavy,
            _ => TextDecorationStyle::Solid,
        };
        self
    }
    pub fn text_decoration_color(mut self, color: [u8; 4]) -> Self {
        self.text_style.decoration_color = Some(color);
        self.attrs
            .insert("text_decoration_color".to_string(), "1".to_string());
        self
    }
    pub fn text_decoration_thickness(mut self, thickness: impl IntoF32) -> Self {
        self.text_style.decoration_thickness = thickness.into_f32().max(0.0);
        self.attrs
            .insert("text_decoration_thickness".to_string(), "1".to_string());
        self
    }
    pub fn text_underline_offset(mut self, offset: impl IntoF32) -> Self {
        self.text_style.underline_offset = offset.into_f32();
        self.attrs
            .insert("text_underline_offset".to_string(), "1".to_string());
        self
    }
    pub fn text_shadow(mut self, spec: impl AsRef<str>) -> Self {
        let (c, offset, blur) = crate::style::parse_text_shadow(spec);
        self.text_style.shadow_color = c;
        self.text_style.shadow_offset = offset;
        self.text_style.shadow_blur = blur;
        self
    }
    pub fn text_shadow_color(mut self, color: [u8; 4]) -> Self {
        self.text_style.shadow_color = Some(color);
        self
    }
    pub fn text_shadow_blur(mut self, blur: impl IntoF32) -> Self {
        self.text_style.shadow_blur = blur.into_f32().max(0.0);
        self
    }
    pub fn text_shadow_offset_x(mut self, dx: impl IntoF32) -> Self {
        self.text_style.shadow_offset[0] = dx.into_f32();
        self
    }
    pub fn text_shadow_offset_y(mut self, dy: impl IntoF32) -> Self {
        self.text_style.shadow_offset[1] = dy.into_f32();
        self
    }
    pub fn text_center(mut self) -> Self {
        self.text_style.align = glyphon::cosmic_text::Align::Center;
        self.attrs
            .insert("text_align".to_string(), "center".to_string());
        self
    }
    pub fn text_left(mut self) -> Self {
        self.text_style.align = glyphon::cosmic_text::Align::Left;
        self.attrs
            .insert("text_align".to_string(), "left".to_string());
        self
    }
    pub fn text_right(mut self) -> Self {
        self.text_style.align = glyphon::cosmic_text::Align::Right;
        self.attrs
            .insert("text_align".to_string(), "right".to_string());
        self
    }
    pub fn text_line_height(mut self, line_height: impl IntoF32) -> Self {
        self.text_style.line_height_px = Some(line_height.into_f32().max(1.0));
        self.attrs
            .insert("line_height".to_string(), "1".to_string());
        self
    }
    pub fn text_leading(mut self, scale: impl IntoF32) -> Self {
        self.text_style.line_height_px = None;
        self.text_style.line_height_scale = Some(scale.into_f32().max(0.01));
        self.attrs.insert("leading".to_string(), "1".to_string());
        self
    }
    pub fn text_leading_none(mut self) -> Self {
        self.text_style.line_height_px = None;
        self.text_style.line_height_scale = Some(1.0);
        self.attrs.insert("leading".to_string(), "1".to_string());
        self
    }
    pub fn text_leading_tight(mut self) -> Self {
        self.text_style.line_height_px = None;
        self.text_style.line_height_scale = Some(1.25);
        self.attrs.insert("leading".to_string(), "1".to_string());
        self
    }
    pub fn text_leading_normal(mut self) -> Self {
        self.text_style.line_height_px = None;
        self.text_style.line_height_scale = Some(1.5);
        self.attrs.insert("leading".to_string(), "1".to_string());
        self
    }
    pub fn text_leading_relaxed(mut self) -> Self {
        self.text_style.line_height_px = None;
        self.text_style.line_height_scale = Some(1.625);
        self.attrs.insert("leading".to_string(), "1".to_string());
        self
    }
    /// Apply `text-overflow`: `clip` / `hidden` / `ellipsis` are nowrap + box `overflow: hidden`;
    /// `newline` restores wrapping (`white-space: normal`) and visible overflow.
    ///
    /// Default [`TextOverflow::Clip`] on a new node does **not** run this — wrapping stays
    /// allowed until clip/hidden/ellipsis is set explicitly (or via `truncate`).
    pub fn apply_text_overflow_mode(&mut self, overflow: TextOverflow) {
        use crate::style::WhiteSpace;
        self.text_style.overflow = overflow;
        self.attrs
            .insert("text_overflow".to_string(), overflow.as_str().to_string());
        match overflow {
            TextOverflow::Clip | TextOverflow::Hidden | TextOverflow::Ellipsis => {
                self.text_style.white_space = WhiteSpace::Nowrap;
                self.attrs
                    .insert("white_space".to_string(), "nowrap".to_string());
                self.style.overflow.x = taffy::style::Overflow::Hidden;
                self.style.overflow.y = taffy::style::Overflow::Hidden;
            }
            TextOverflow::Newline => {
                self.text_style.white_space = WhiteSpace::Normal;
                self.attrs
                    .insert("white_space".to_string(), "normal".to_string());
                self.style.overflow.x = taffy::style::Overflow::Visible;
                self.style.overflow.y = taffy::style::Overflow::Visible;
            }
        }
    }
    pub fn text_overflow(mut self, overflow: impl Into<TextOverflow>) -> Self {
        self.apply_text_overflow_mode(overflow.into());
        self
    }
    pub fn text_overflow_ellipsis(self) -> Self {
        self.text_overflow(TextOverflow::Ellipsis)
    }
    pub fn text_overflow_clip(self) -> Self {
        self.text_overflow(TextOverflow::Clip)
    }
    pub fn text_overflow_hidden(self) -> Self {
        self.text_overflow(TextOverflow::Hidden)
    }
    pub fn text_overflow_newline(self) -> Self {
        self.text_overflow(TextOverflow::Newline)
    }
    /// CSS / Tailwind `whitespace-nowrap`: caption stays one line (ShadCN Button).
    pub fn whitespace_nowrap(mut self) -> Self {
        self.text_style.white_space = crate::style::WhiteSpace::Nowrap;
        self.attrs
            .insert("white_space".to_string(), "nowrap".to_string());
        self
    }
    pub fn white_space(mut self, keyword: impl AsRef<str>) -> Self {
        let s = keyword.as_ref().trim().to_ascii_lowercase();
        self.text_style.white_space = match s.as_str() {
            "nowrap" | "no-wrap" | "pre" => crate::style::WhiteSpace::Nowrap,
            _ => crate::style::WhiteSpace::Normal,
        };
        self.attrs.insert("white_space".to_string(), s);
        self
    }
    pub fn selectable(mut self, selectable: bool) -> Self {
        self.text_style.selectable = selectable;
        if selectable {
            self.attrs
                .insert("text_selectable".to_string(), "1".to_string());
        } else {
            self.attrs.remove("text_selectable");
        }
        self
    }
    /// AccessKit semantic role (e.g. `"group"`, `"button"`, `"label"`).
    pub fn accesskit_role(mut self, role: impl Into<String>) -> Self {
        self.attrs.insert("accesskit_role".to_string(), role.into());
        self
    }
    /// Accessible name (preferred over `aria-label` / text / placeholder).
    pub fn accesskit_name(mut self, name: impl Into<String>) -> Self {
        self.attrs.insert("accesskit_name".to_string(), name.into());
        self
    }
    /// ARIA label (fallback name when `accesskit_name` is absent).
    pub fn aria_label(mut self, name: impl Into<String>) -> Self {
        self.attrs.insert("aria-label".to_string(), name.into());
        self
    }
    /// Stable AccessKit node key when the element has no `focus_id` (e.g. static Label).
    ///
    /// Pair with [`Self::aria_labelledby`] on a control. See `docs/a11y_smoke.md`.
    pub fn accesskit_id(mut self, id: impl Into<String>) -> Self {
        self.attrs.insert("accesskit_id".to_string(), id.into());
        self
    }
    /// Space-separated `accesskit_id` / `focus_id` keys of labeling nodes (`aria-labelledby`).
    pub fn aria_labelledby(mut self, ids: impl Into<String>) -> Self {
        self.attrs.insert("aria-labelledby".to_string(), ids.into());
        self
    }
    pub fn bg(mut self, color: [f32; 4]) -> Self {
        self.style.bg_color = color;
        self
    }
    pub fn border(mut self, color: [f32; 4], width: f32) -> Self {
        self.style.border_style = LineStyle::Solid;
        self.style.border_color = color;
        self.style.border_color_sides = None;
        self.style.set_uniform_border_width(width);
        self
    }
    pub fn border_style(mut self, style: impl AsRef<str>) -> Self {
        self.style.border_style = LineStyle::from_css_keyword(style.as_ref());
        self
    }
    pub fn border_color(mut self, color: [f32; 4]) -> Self {
        self.style.border_color = color;
        self.style.border_color_sides = None;
        self
    }
    pub fn border_width(mut self, width: impl IntoBorderWidths) -> Self {
        self.style
            .set_border_widths_trbl(width.into_border_widths());
        self
    }
    pub fn border_top_width(mut self, width: impl IntoF32) -> Self {
        self.style.set_border_side_width(0, width.into_f32());
        self
    }
    pub fn border_right_width(mut self, width: impl IntoF32) -> Self {
        self.style.set_border_side_width(1, width.into_f32());
        self
    }
    pub fn border_bottom_width(mut self, width: impl IntoF32) -> Self {
        self.style.set_border_side_width(2, width.into_f32());
        self
    }
    pub fn border_left_width(mut self, width: impl IntoF32) -> Self {
        self.style.set_border_side_width(3, width.into_f32());
        self
    }
    pub fn border_top_color(mut self, color: [f32; 4]) -> Self {
        self.style.set_border_side_color(0, color);
        self
    }
    pub fn border_right_color(mut self, color: [f32; 4]) -> Self {
        self.style.set_border_side_color(1, color);
        self
    }
    pub fn border_bottom_color(mut self, color: [f32; 4]) -> Self {
        self.style.set_border_side_color(2, color);
        self
    }
    pub fn border_left_color(mut self, color: [f32; 4]) -> Self {
        self.style.set_border_side_color(3, color);
        self
    }
    pub fn border_top(self, color: [f32; 4], width: impl IntoF32) -> Self {
        self.ensure_border_solid()
            .border_top_color(color)
            .border_top_width(width)
    }
    pub fn border_right(self, color: [f32; 4], width: impl IntoF32) -> Self {
        self.ensure_border_solid()
            .border_right_color(color)
            .border_right_width(width)
    }
    pub fn border_bottom(self, color: [f32; 4], width: impl IntoF32) -> Self {
        self.ensure_border_solid()
            .border_bottom_color(color)
            .border_bottom_width(width)
    }
    pub fn border_left(self, color: [f32; 4], width: impl IntoF32) -> Self {
        self.ensure_border_solid()
            .border_left_color(color)
            .border_left_width(width)
    }
    pub fn border_inline_start_width(self, width: impl IntoF32) -> Self {
        self.border_left_width(width)
    }
    pub fn border_inline_end_width(self, width: impl IntoF32) -> Self {
        self.border_right_width(width)
    }
    pub fn border_block_start_width(self, width: impl IntoF32) -> Self {
        self.border_top_width(width)
    }
    pub fn border_block_end_width(self, width: impl IntoF32) -> Self {
        self.border_bottom_width(width)
    }
    // Per-side style
    pub fn border_top_style(mut self, style: impl AsRef<str>) -> Self {
        self.style.set_border_side_style(0, LineStyle::from_css_keyword(style.as_ref()));
        self
    }
    pub fn border_right_style(mut self, style: impl AsRef<str>) -> Self {
        self.style.set_border_side_style(1, LineStyle::from_css_keyword(style.as_ref()));
        self
    }
    pub fn border_bottom_style(mut self, style: impl AsRef<str>) -> Self {
        self.style.set_border_side_style(2, LineStyle::from_css_keyword(style.as_ref()));
        self
    }
    pub fn border_left_style(mut self, style: impl AsRef<str>) -> Self {
        self.style.set_border_side_style(3, LineStyle::from_css_keyword(style.as_ref()));
        self
    }
    // Logical property style
    pub fn border_inline_start_style(self, style: impl AsRef<str>) -> Self {
        self.border_left_style(style)
    }
    pub fn border_inline_end_style(self, style: impl AsRef<str>) -> Self {
        self.border_right_style(style)
    }
    pub fn border_block_start_style(self, style: impl AsRef<str>) -> Self {
        self.border_top_style(style)
    }
    pub fn border_block_end_style(self, style: impl AsRef<str>) -> Self {
        self.border_bottom_style(style)
    }
    // Logical property color
    pub fn border_inline_start_color(self, color: [f32; 4]) -> Self {
        self.border_left_color(color)
    }
    pub fn border_inline_end_color(self, color: [f32; 4]) -> Self {
        self.border_right_color(color)
    }
    pub fn border_block_start_color(self, color: [f32; 4]) -> Self {
        self.border_top_color(color)
    }
    pub fn border_block_end_color(self, color: [f32; 4]) -> Self {
        self.border_bottom_color(color)
    }
    // Logical compound shortcuts
    pub fn border_inline(self, color: [f32; 4], width: impl IntoF32 + Copy) -> Self {
        self.ensure_border_solid()
            .border_left_color(color).border_right_color(color)
            .border_left_width(width).border_right_width(width)
    }
    pub fn border_block(self, color: [f32; 4], width: impl IntoF32 + Copy) -> Self {
        self.ensure_border_solid()
            .border_top_color(color).border_bottom_color(color)
            .border_top_width(width).border_bottom_width(width)
    }
    pub fn border_inline_start(self, color: [f32; 4], width: impl IntoF32) -> Self {
        self.border_left(color, width)
    }
    pub fn border_inline_end(self, color: [f32; 4], width: impl IntoF32) -> Self {
        self.border_right(color, width)
    }
    pub fn border_block_start(self, color: [f32; 4], width: impl IntoF32) -> Self {
        self.border_top(color, width)
    }
    pub fn border_block_end(self, color: [f32; 4], width: impl IntoF32) -> Self {
        self.border_bottom(color, width)
    }
    // border-radius longhand
    pub fn border_top_left_radius(mut self, r: f32) -> Self {
        self.style.radii[2] = r.max(0.0);
        self
    }
    pub fn border_top_right_radius(mut self, r: f32) -> Self {
        self.style.radii[0] = r.max(0.0);
        self
    }
    pub fn border_bottom_right_radius(mut self, r: f32) -> Self {
        self.style.radii[1] = r.max(0.0);
        self
    }
    pub fn border_bottom_left_radius(mut self, r: f32) -> Self {
        self.style.radii[3] = r.max(0.0);
        self
    }
    // border-shape / corner-shape
    pub fn border_shape(mut self, shape: crate::style::CornerShape) -> Self {
        self.style.corner_shape = shape;
        self.style.corner_shape_sides = None;
        self
    }
    pub fn corner_top_left_shape(mut self, shape: crate::style::CornerShape) -> Self {
        self.style.set_corner_side_shape(0, shape);
        self
    }
    pub fn corner_top_right_shape(mut self, shape: crate::style::CornerShape) -> Self {
        self.style.set_corner_side_shape(1, shape);
        self
    }
    pub fn corner_bottom_right_shape(mut self, shape: crate::style::CornerShape) -> Self {
        self.style.set_corner_side_shape(2, shape);
        self
    }
    pub fn corner_bottom_left_shape(mut self, shape: crate::style::CornerShape) -> Self {
        self.style.set_corner_side_shape(3, shape);
        self
    }
    // border-collapse / border-spacing
    pub fn border_collapse(mut self, collapse: bool) -> Self {
        self.style.border_collapse = collapse;
        self
    }
    pub fn border_spacing(mut self, h: f32, v: f32) -> Self {
        self.style.border_spacing = [h, v];
        self
    }
    // border-image
    pub fn border_image_source(mut self, src: impl Into<String>) -> Self {
        let s: String = src.into();
        if s.is_empty() {
            self.style.border_image_source = None;
        } else {
            let key = crate::app::pending_images::texture_key_for_src(&s);
            crate::app::pending_images::queue_image_from_src(&key, &s);
            self.style.border_image_source = Some(key);
        }
        self
    }
    pub fn border_image_slice(mut self, slice: [f32; 4]) -> Self {
        self.style.border_image_slice = slice;
        self
    }
    pub fn border_image_slice_fill(mut self, fill: bool) -> Self {
        self.style.border_image_slice_fill = fill;
        self
    }
    pub fn border_image_width(mut self, widths: [f32; 4]) -> Self {
        self.style.border_image_width = Some(widths);
        self
    }
    pub fn border_image_outset(mut self, outset: [f32; 4]) -> Self {
        self.style.border_image_outset = outset;
        self
    }
    pub fn border_image_repeat(mut self, repeat: [crate::style::BorderImageRepeat; 2]) -> Self {
        self.style.border_image_repeat = repeat;
        self
    }

    fn ensure_border_solid(mut self) -> Self {
        if self.style.border_style.is_invisible() {
            self.style.border_style = LineStyle::Solid;
        }
        self
    }

    pub fn outline(mut self, color: [f32; 4], width: f32) -> Self {
        self.style.outline_style = LineStyle::Solid;
        self.style.outline_color = color;
        self.style.outline_width = width;
        self
    }
    pub fn outline_style(mut self, style: impl AsRef<str>) -> Self {
        self.style.outline_style = LineStyle::from_css_keyword(style.as_ref());
        self
    }
    pub fn outline_color(mut self, color: [f32; 4]) -> Self {
        self.style.outline_color = color;
        self
    }
    pub fn outline_width(mut self, width: f32) -> Self {
        self.style.outline_width = width;
        self
    }
    pub fn outline_offset(mut self, offset: impl IntoF32) -> Self {
        self.style.outline_offset = offset.into_f32();
        self
    }
    /// Parse CSS `outline` shorthand: `<line-width> || <line-style> || <color>` (any order).
    pub fn apply_outline_shorthand(mut self, value: impl AsRef<str>) -> Self {
        let parts: Vec<&str> = value.as_ref().split_whitespace().collect();
        for token in &parts {
            let t = token.to_ascii_lowercase();
            if matches!(
                t.as_str(),
                "none" | "auto" | "dotted" | "dashed" | "solid" | "double"
                    | "groove" | "ridge" | "inset" | "outset"
            ) {
                self.style.outline_style = LineStyle::from_css_keyword(&t);
            } else if token.bytes().next().map_or(false, |b| b.is_ascii_digit() || b == b'.') {
                self.style.outline_width = (*token).into_f32();
            } else {
                self.style.outline_color = parse_color(token);
            }
        }
        if self.style.outline_style == LineStyle::None
            && self.style.outline_width > 0.0
            && parts.iter().any(|p| {
                let t = p.to_ascii_lowercase();
                !matches!(t.as_str(), "none")
            })
        {
            self.style.outline_style = LineStyle::Solid;
        }
        self
    }
    pub fn rounded(mut self, radius: impl IntoF32) -> Self {
        let r = radius.into_f32();
        self.style.radii = [r, r, r, r];
        self
    }
    pub fn rounded_full(mut self) -> Self {
        // Resolved later using actual layout width/height.
        self.style.radii = [-1.0, -1.0, -1.0, -1.0];
        self
    }
    pub fn shadow(mut self, color: [f32; 4], blur: f32) -> Self {
        self.style.shadow_color = color;
        self.style.shadow_blur = blur;
        self
    }
    pub fn box_shadow(mut self, shadow: impl AsRef<str>) -> Self {
        let list = crate::style::parse_box_shadows(shadow);
        self.style.set_shadows_from_parsed(&list);
        self
    }
    pub fn box_shadow_color(mut self, color: [f32; 4]) -> Self {
        self.style.shadow_color = color;
        self
    }
    pub fn box_shadow_blur(mut self, blur: f32) -> Self {
        self.style.shadow_blur = blur.max(0.0);
        self
    }

    /// Backdrop blur radius in logical px (`0` disables). See [`crate::style::Style::backdrop_blur`].
    pub fn backdrop_blur(mut self, blur: impl IntoF32) -> Self {
        self.style.backdrop_blur = blur.into_f32().max(0.0);
        self
    }
    /// Parse CSS `transition` shorthand, e.g. `"opacity 300ms ease-in-out 100ms"`.
    /// Multiple entries separated by commas.
    pub fn transition(mut self, value: impl AsRef<str>) -> Self {
        use crate::style::{TransitionEntry, TransitionProperty, TimingFunction};
        self.style.transition.clear();
        for segment in value.as_ref().split(',') {
            let parts: Vec<&str> = segment.split_whitespace().collect();
            if parts.is_empty() { continue; }
            let mut property = TransitionProperty::All;
            let mut duration_ms: f32 = 0.0;
            let mut delay_ms: f32 = 0.0;
            let mut timing = TimingFunction::Ease;
            let mut dur_set = false;
            for p in &parts {
                if let Some(ms) = parse_time_ms(p) {
                    if !dur_set { duration_ms = ms; dur_set = true; } else { delay_ms = ms; }
                } else if p.starts_with("cubic-bezier(") || p.contains("cubic-bezier") {
                    timing = TimingFunction::from_css(p);
                } else {
                    match *p {
                        "linear" | "ease" | "ease-in" | "ease-out" | "ease-in-out" => {
                            timing = TimingFunction::from_css(p);
                        }
                        _ => property = TransitionProperty::from_css(p),
                    }
                }
            }
            self.style.transition.push(TransitionEntry { property, duration_ms, delay_ms, timing });
        }
        self
    }

    pub fn transition_property(mut self, value: impl AsRef<str>) -> Self {
        use crate::style::TransitionProperty;
        let props: Vec<TransitionProperty> = value.as_ref()
            .split(',')
            .map(|s| TransitionProperty::from_css(s))
            .collect();
        if self.style.transition.is_empty() {
            for p in props {
                self.style.transition.push(crate::style::TransitionEntry {
                    property: p, duration_ms: 0.0, delay_ms: 0.0,
                    timing: crate::style::TimingFunction::Ease,
                });
            }
        } else {
            for (i, p) in props.into_iter().enumerate() {
                let idx: usize = i.min(self.style.transition.len() - 1);
                self.style.transition[idx].property = p;
            }
        }
        self
    }

    pub fn transition_duration(mut self, value: impl AsRef<str>) -> Self {
        let durations: Vec<f32> = value.as_ref().split(',').filter_map(|s| parse_time_ms(s.trim())).collect();
        for (i, d) in durations.into_iter().enumerate() {
            if i < self.style.transition.len() {
                self.style.transition[i].duration_ms = d;
            }
        }
        self
    }

    pub fn transition_delay(mut self, value: impl AsRef<str>) -> Self {
        let delays: Vec<f32> = value.as_ref().split(',').filter_map(|s| parse_time_ms(s.trim())).collect();
        for (i, d) in delays.into_iter().enumerate() {
            if i < self.style.transition.len() {
                self.style.transition[i].delay_ms = d;
            }
        }
        self
    }

    pub fn transition_timing_function(mut self, value: impl AsRef<str>) -> Self {
        use crate::style::TimingFunction;
        let funcs: Vec<TimingFunction> = value.as_ref().split(',').map(|s| TimingFunction::from_css(s.trim())).collect();
        for (i, f) in funcs.into_iter().enumerate() {
            if i < self.style.transition.len() {
                self.style.transition[i].timing = f;
            }
        }
        self
    }

    pub fn box_shadow_offset_x(mut self, dx: f32) -> Self {
        self.style.shadow_offset[0] = dx;
        self
    }
    pub fn box_shadow_offset_y(mut self, dy: f32) -> Self {
        self.style.shadow_offset[1] = dy;
        self
    }
    pub fn box_shadow_spread(mut self, spread: f32) -> Self {
        self.style.shadow_spread = spread;
        self
    }
    pub fn box_shadow_inset(mut self, inset: bool) -> Self {
        self.style.shadow_inset = inset;
        self
    }
    pub fn opacity(mut self, o: f32) -> Self {
        self.style.opacity = o.clamp(0.0, 1.0);
        self
    }
    pub fn translate(mut self, x: f32, y: f32) -> Self {
        self.style.translate = [x, y];
        self
    }
    /// Set translate as a fraction of own bounding box (CSS percentage translate).
    /// `translate_pct(0.5, 0.0)` = shift right by 50% of own width.
    pub fn translate_pct(mut self, x: f32, y: f32) -> Self {
        self.style.translate_pct = [x, y];
        self
    }
    pub fn w(self, width: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::Width, width)
    }
    pub fn inline_size(self, size: impl IntoCssDim) -> Self {
        self.w(size)
    }
    pub fn h(self, height: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::Height, height)
    }
    pub fn block_size(self, size: impl IntoCssDim) -> Self {
        self.h(size)
    }
    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }
    pub fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        self.children.extend(children);
        self
    }
    pub fn flex_col(mut self) -> Self {
        self.style.flex_direction = FlexDirection::Column;
        self
    }
    pub fn justify_center(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::CENTER);
        self
    }
    pub fn justify_start(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::FLEX_START);
        self
    }
    pub fn justify_end(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::FLEX_END);
        self
    }

    /// `justify-content`，与 CSS / Tailwind `justify-*` 一致（区别于 [`Self::flex_items`] / `align-items`）。
    pub fn justify(mut self, keyword: impl AsRef<str>) -> Self {
        let s = keyword.as_ref().trim().to_ascii_lowercase();
        self.style.justify_content = Some(match s.as_str() {
            "center" => JustifyContent::CENTER,
            "start" | "flex-start" => JustifyContent::FLEX_START,
            "end" | "flex-end" => JustifyContent::FLEX_END,
            "between" | "space-between" => JustifyContent::SPACE_BETWEEN,
            "around" | "space-around" => JustifyContent::SPACE_AROUND,
            "evenly" | "space-evenly" => JustifyContent::SPACE_EVENLY,
            _ => JustifyContent::FLEX_START,
        });
        self
    }

    /// `align-items`，与 RSX `items` / `align`、Tailwind `items-*` 一致。
    pub fn flex_items(mut self, keyword: impl AsRef<str>) -> Self {
        let s = keyword.as_ref().trim().to_ascii_lowercase();
        self.style.align_items = Some(match s.as_str() {
            "center" => AlignItems::CENTER,
            "start" | "flex-start" => AlignItems::FLEX_START,
            "end" | "flex-end" => AlignItems::FLEX_END,
            "stretch" => AlignItems::STRETCH,
            "baseline" => AlignItems::BASELINE,
            _ => AlignItems::STRETCH,
        });
        self
    }

    /// `flex-wrap`：`wrap` / `nowrap` / `wrap-reverse`。
    pub fn flex_wrap_kw(mut self, keyword: impl AsRef<str>) -> Self {
        let s = keyword.as_ref().trim().to_ascii_lowercase();
        self.style.flex_wrap = match s.as_str() {
            "wrap" => FlexWrap::Wrap,
            "wrap-reverse" | "wrap_reverse" => FlexWrap::WrapReverse,
            "nowrap" | "no-wrap" | "none" => FlexWrap::NoWrap,
            _ => FlexWrap::NoWrap,
        };
        self
    }

    pub fn items_center(mut self) -> Self {
        self.style.align_items = Some(AlignItems::CENTER);
        self
    }
    pub fn p(self, padding: impl IntoCssDim) -> Self {
        let d = padding.into_css_dim();
        self.set_css(UnitSlot::PadL, d.clone())
            .set_css(UnitSlot::PadR, d.clone())
            .set_css(UnitSlot::PadT, d.clone())
            .set_css(UnitSlot::PadB, d)
    }
    pub fn px(self, padding: impl IntoCssDim) -> Self {
        let d = padding.into_css_dim();
        self.set_css(UnitSlot::PadL, d.clone())
            .set_css(UnitSlot::PadR, d)
    }
    pub fn py(self, padding: impl IntoCssDim) -> Self {
        let d = padding.into_css_dim();
        self.set_css(UnitSlot::PadT, d.clone())
            .set_css(UnitSlot::PadB, d)
    }
    pub fn pt(self, padding: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::PadT, padding)
    }
    pub fn pr(self, padding: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::PadR, padding)
    }
    pub fn pb(self, padding: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::PadB, padding)
    }
    pub fn pl(self, padding: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::PadL, padding)
    }
    pub fn padding_inline(self, padding: impl IntoCssRectPair) -> Self {
        self.apply_rect_pair(UnitSlot::PadL, UnitSlot::PadR, padding)
    }
    pub fn padding_block(self, padding: impl IntoCssRectPair) -> Self {
        self.apply_rect_pair(UnitSlot::PadT, UnitSlot::PadB, padding)
    }
    pub fn padding_inline_start(self, padding: impl IntoCssDim) -> Self {
        self.pl(padding)
    }
    pub fn padding_inline_end(self, padding: impl IntoCssDim) -> Self {
        self.pr(padding)
    }
    pub fn padding_block_start(self, padding: impl IntoCssDim) -> Self {
        self.pt(padding)
    }
    pub fn padding_block_end(self, padding: impl IntoCssDim) -> Self {
        self.pb(padding)
    }
    pub fn padding_h(self, val: impl IntoCssDim) -> Self {
        self.px(val)
    }
    pub fn flex_row(mut self) -> Self {
        self.style.flex_direction = FlexDirection::Row;
        self
    }
    pub fn m(self, margin: impl IntoCssDim) -> Self {
        let d = margin.into_css_dim();
        self.set_css(UnitSlot::MarginL, d.clone())
            .set_css(UnitSlot::MarginR, d.clone())
            .set_css(UnitSlot::MarginT, d.clone())
            .set_css(UnitSlot::MarginB, d)
    }
    pub fn mx(self, margin: impl IntoCssDim) -> Self {
        let d = margin.into_css_dim();
        self.set_css(UnitSlot::MarginL, d.clone())
            .set_css(UnitSlot::MarginR, d)
    }
    pub fn my(self, margin: impl IntoCssDim) -> Self {
        let d = margin.into_css_dim();
        self.set_css(UnitSlot::MarginT, d.clone())
            .set_css(UnitSlot::MarginB, d)
    }
    pub fn mt(self, margin: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::MarginT, margin)
    }
    pub fn mr(self, margin: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::MarginR, margin)
    }
    pub fn mb(self, margin: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::MarginB, margin)
    }
    pub fn ml(self, margin: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::MarginL, margin)
    }
    pub fn margin_inline(self, margin: impl IntoCssRectPair) -> Self {
        self.apply_rect_pair(UnitSlot::MarginL, UnitSlot::MarginR, margin)
    }
    pub fn margin_block(self, margin: impl IntoCssRectPair) -> Self {
        self.apply_rect_pair(UnitSlot::MarginT, UnitSlot::MarginB, margin)
    }
    pub fn margin_inline_start(self, margin: impl IntoCssDim) -> Self {
        self.ml(margin)
    }
    pub fn margin_inline_end(self, margin: impl IntoCssDim) -> Self {
        self.mr(margin)
    }
    pub fn margin_block_start(self, margin: impl IntoCssDim) -> Self {
        self.mt(margin)
    }
    pub fn margin_block_end(self, margin: impl IntoCssDim) -> Self {
        self.mb(margin)
    }
    pub fn flex_grow(mut self, grow: impl IntoF32Raw) -> Self {
        self.style.flex_grow = grow.into_f32_raw();
        self
    }
    pub fn flex_shrink(mut self, shrink: impl IntoF32Raw) -> Self {
        self.style.flex_shrink = shrink.into_f32_raw();
        self
    }

    /// CSS `box-sizing: border-box`（默认）：`width`/`height` 含 padding + border。
    /// 与 Tailwind `box-border` 同义；从 ContentBox 切回时显式写出。
    pub fn border_box(mut self) -> Self {
        self.style.box_sizing = taffy::style::BoxSizing::BorderBox;
        self
    }

    /// CSS `box-sizing: content-box`：测宽为内容宽，padding 加在外侧。
    /// 文案叶若要自带水平 padding 并 `text-align:center`，必须用这个（默认 BorderBox
    /// 会把测宽当成边框宽，padding 吃掉内容区）。
    pub fn content_box(mut self) -> Self {
        self.style.box_sizing = taffy::style::BoxSizing::ContentBox;
        self
    }

    pub fn display_grid(mut self) -> Self {
        self.style.display = Display::Grid;
        self
    }

    pub fn display_flex(mut self) -> Self {
        self.style.display = Display::Flex;
        self
    }

    /// `flex-basis`（主轴初始尺寸）。
    pub fn flex_basis(self, basis: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::FlexBasis, basis)
    }

    /// `align-self`，与 Tailwind `self-*` 一致；`auto` / 未知关键字清除为继承父级 `align-items`。
    pub fn align_self_kw(mut self, keyword: impl AsRef<str>) -> Self {
        let s = keyword.as_ref().trim().to_ascii_lowercase();
        self.style.align_self = match s.as_str() {
            "" | "auto" => None,
            "center" => Some(AlignSelf::CENTER),
            "start" | "flex-start" => Some(AlignSelf::FLEX_START),
            "end" | "flex-end" => Some(AlignSelf::FLEX_END),
            "stretch" => Some(AlignSelf::STRETCH),
            "baseline" => Some(AlignSelf::BASELINE),
            _ => None,
        };
        self
    }

    /// `align-content`（多行 flex / grid 轴上分布）。
    pub fn align_content_kw(mut self, keyword: impl AsRef<str>) -> Self {
        let s = keyword.as_ref().trim().to_ascii_lowercase();
        self.style.align_content = Some(match s.as_str() {
            "start" | "flex-start" => AlignContent::FLEX_START,
            "end" | "flex-end" => AlignContent::FLEX_END,
            "center" => AlignContent::CENTER,
            "stretch" => AlignContent::STRETCH,
            "space-between" | "between" => AlignContent::SPACE_BETWEEN,
            "space-around" | "around" => AlignContent::SPACE_AROUND,
            "space-evenly" | "evenly" => AlignContent::SPACE_EVENLY,
            _ => AlignContent::STRETCH,
        });
        self
    }

    /// Grid 容器 `justify-items`。
    pub fn justify_items_kw(mut self, keyword: impl AsRef<str>) -> Self {
        let s = keyword.as_ref().trim().to_ascii_lowercase();
        self.style.justify_items = Some(match s.as_str() {
            "center" => AlignItems::CENTER,
            "start" | "flex-start" => AlignItems::FLEX_START,
            "end" | "flex-end" => AlignItems::FLEX_END,
            "stretch" => AlignItems::STRETCH,
            "baseline" => AlignItems::BASELINE,
            _ => AlignItems::STRETCH,
        });
        self
    }

    /// Grid 子项 `justify-self`；`auto` / 未知清除为继承父级 `justify-items`。
    pub fn justify_self_kw(mut self, keyword: impl AsRef<str>) -> Self {
        let s = keyword.as_ref().trim().to_ascii_lowercase();
        self.style.justify_self = match s.as_str() {
            "" | "auto" => None,
            "center" => Some(AlignSelf::CENTER),
            "start" | "flex-start" => Some(AlignSelf::FLEX_START),
            "end" | "flex-end" => Some(AlignSelf::FLEX_END),
            "stretch" => Some(AlignSelf::STRETCH),
            "baseline" => Some(AlignSelf::BASELINE),
            _ => None,
        };
        self
    }

    pub fn grid_template_columns(mut self, tracks: Vec<crate::layout::GridTrack>) -> Self {
        self.style.grid_template_columns = tracks;
        self
    }

    pub fn grid_template_rows(mut self, tracks: Vec<crate::layout::GridTrack>) -> Self {
        self.style.grid_template_rows = tracks;
        self
    }

    /// `grid-cols-{n}`：等分 `fr` 列，并启用 `display: grid`。
    pub fn grid_cols_equal(self, n: u16) -> Self {
        self.display_grid()
            .grid_template_columns(crate::layout::cols_equal_fr(n))
    }

    /// `grid-rows-{n}`：等分 `fr` 行，并启用 `display: grid`。
    pub fn grid_rows_equal(self, n: u16) -> Self {
        self.display_grid()
            .grid_template_rows(crate::layout::cols_equal_fr(n))
    }

    /// 展开宏打成的 `rui_fragment` 多子节点；否则保留 0/1 个根。
    pub fn unpack_fragment_children(children: Option<Element>) -> Vec<Element> {
        match children {
            Some(mut c) if c.attrs.get("rui_fragment").map(|s| s.as_str()) == Some("1") => {
                std::mem::take(&mut c.children)
            }
            Some(c) => vec![c],
            None => Vec::new(),
        }
    }

    pub fn grid_auto_flow(mut self, flow: GridAutoFlow) -> Self {
        self.style.grid_auto_flow = flow;
        self
    }

    pub fn grid_row(mut self, placement: Line<GridPlacement>) -> Self {
        self.style.grid_row = placement;
        self
    }

    pub fn grid_column(mut self, placement: Line<GridPlacement>) -> Self {
        self.style.grid_column = placement;
        self
    }

    /// `grid-row: <n>`（单条网格线起始）。
    pub fn grid_row_line(mut self, line_index: i16) -> Self {
        self.style.grid_row = line(line_index);
        self
    }

    /// `grid-column: <n>`。
    pub fn grid_column_line(mut self, line_index: i16) -> Self {
        self.style.grid_column = line(line_index);
        self
    }

    /// `grid-column: span <n>`。
    pub fn grid_column_span(mut self, n: u16) -> Self {
        self.style.grid_column = span(n);
        self
    }

    /// `grid-row: span <n>`。
    pub fn grid_row_span(mut self, n: u16) -> Self {
        self.style.grid_row = span(n);
        self
    }

    /// 列表项拖拽手柄：`focus_id` 供运行时识别；`list_id` 中勿含 `:`。
    pub fn drag_reorder_handle(mut self, list_id: impl AsRef<str>, index: usize) -> Self {
        self.focus_id = Some(crate::utils::drag_reorder::drag_item_focus_id(
            list_id.as_ref(),
            index,
        ));
        self.cursor = Some(CursorKind::Grab.into());
        self
    }

    pub fn absolute(mut self) -> Self {
        self.style.position = Position::Absolute;
        self.style.position_fixed = false;
        self.style.position_sticky = false;
        self
    }
    pub fn relative(mut self) -> Self {
        self.style.position = Position::Relative;
        self.style.position_fixed = false;
        self.style.position_sticky = false;
        self
    }
    /// CSS `position: static` — normal flow, inset properties ignored.
    pub fn static_position(mut self) -> Self {
        self.style.position = Position::Relative;
        self.style.position_fixed = false;
        self.style.position_sticky = false;
        self
    }

    pub fn fixed(mut self) -> Self {
        self.style.position = Position::Absolute;
        self.style.position_fixed = true;
        self.style.position_sticky = false;
        self
    }

    /// CSS `position: sticky` — relative layout + scroll-aware offset clamping.
    pub fn sticky(mut self) -> Self {
        self.style.position = Position::Relative;
        self.style.position_fixed = false;
        self.style.position_sticky = true;
        self
    }

    /// Generic CSS `position` setter: `"static"` | `"relative"` | `"absolute"` | `"fixed"` | `"sticky"`.
    pub fn position(self, value: impl AsRef<str>) -> Self {
        match value.as_ref().trim().to_ascii_lowercase().as_str() {
            "absolute" => self.absolute(),
            "fixed" => self.fixed(),
            "sticky" => self.sticky(),
            "static" => self.static_position(),
            _ => self.relative(),
        }
    }
    pub fn aspect_ratio(mut self, ratio: f32) -> Self {
        self.style.aspect_ratio = Some(ratio);
        self
    }
    pub fn aspect_ratio_opt(mut self, ratio: Option<f32>) -> Self {
        self.style.aspect_ratio = ratio;
        self
    }
    pub fn rotate(mut self, deg: f32) -> Self {
        self.style.rotate_deg = deg;
        self
    }
    /// CSS `scale` — uniform visual scale (does not affect layout).
    pub fn scale(mut self, scale: f32) -> Self {
        self.style.scale = [scale, scale];
        self
    }
    /// CSS `scale: x y` — per-axis visual scale (does not affect layout).
    pub fn scale_xy(mut self, x: f32, y: f32) -> Self {
        self.style.scale = [x, y];
        self
    }
    /// CSS `scale: x y z` — per-axis visual scale with Z (does not affect layout).
    pub fn scale_xyz(mut self, x: f32, y: f32, z: f32) -> Self {
        self.style.scale = [x, y];
        self.style.scale_z = z;
        self
    }
    /// CSS `transform` property — parse a CSS transform string.
    pub fn css_transform(mut self, s: &str) -> Self {
        self.style.transform = crate::style::parse_transform(s);
        self
    }
    /// Set the CSS `transform-origin` from a CSS string (e.g. `"left top"`, `"50% 50%"`).
    pub fn transform_origin(mut self, s: &str) -> Self {
        self.style.transform_origin = crate::style::parse_transform_origin(s);
        self
    }
    /// Set `transform-origin` by pixel offsets.
    pub fn transform_origin_xy(mut self, x: f32, y: f32) -> Self {
        use crate::layout::dimension::CssDim;
        use crate::style::transform::{TransformOriginX, TransformOriginY};
        self.style.transform_origin.x = TransformOriginX::Length(CssDim::Px(x));
        self.style.transform_origin.y = TransformOriginY::Length(CssDim::Px(y));
        self
    }
    /// CSS `transform-style`: `"flat"` or `"preserve-3d"`.
    pub fn transform_style(mut self, s: &str) -> Self {
        self.style.transform_style = crate::style::parse_transform_style(s);
        self
    }
    /// CSS `transform-box`.
    pub fn transform_box(mut self, s: &str) -> Self {
        self.style.transform_box = crate::style::parse_transform_box(s);
        self
    }
    /// Shorthand: apply `skew(xdeg, ydeg)` as a transform function.
    pub fn skew(mut self, x_deg: f32, y_deg: f32) -> Self {
        use crate::layout::dimension::CssAngle;
        use crate::style::TransformFunction;
        let func = TransformFunction::Skew(CssAngle::Deg(x_deg), Some(CssAngle::Deg(y_deg)));
        self.style.transform.get_or_insert_with(Vec::new).push(func);
        self
    }
    /// Shorthand: `skewX(deg)`.
    pub fn skew_x(mut self, deg: f32) -> Self {
        use crate::layout::dimension::CssAngle;
        use crate::style::TransformFunction;
        self.style.transform.get_or_insert_with(Vec::new).push(TransformFunction::SkewX(CssAngle::Deg(deg)));
        self
    }
    /// Shorthand: `skewY(deg)`.
    pub fn skew_y(mut self, deg: f32) -> Self {
        use crate::layout::dimension::CssAngle;
        use crate::style::TransformFunction;
        self.style.transform.get_or_insert_with(Vec::new).push(TransformFunction::SkewY(CssAngle::Deg(deg)));
        self
    }
    /// Shorthand: `perspective(px)`.
    pub fn perspective(mut self, px: f32) -> Self {
        use crate::layout::dimension::CssDim;
        use crate::style::TransformFunction;
        self.style.transform.get_or_insert_with(Vec::new).push(TransformFunction::Perspective(CssDim::Px(px)));
        self
    }
    /// Set a raw `matrix(a,b,c,d,e,f)` transform.
    pub fn matrix(mut self, a: f32, b: f32, c: f32, d: f32, e: f32, f: f32) -> Self {
        use crate::style::TransformFunction;
        self.style.transform.get_or_insert_with(Vec::new).push(TransformFunction::Matrix([a, b, c, d, e, f]));
        self
    }
    /// CSS `zoom` — magnification factor that **affects layout**.
    pub fn zoom(mut self, factor: f32) -> Self {
        self.style.zoom = factor.max(0.0);
        self
    }
    pub fn linear_gradient(mut self, from: [f32; 4], to: [f32; 4], angle_deg: f32) -> Self {
        self.style.bg_gradient = Some(crate::style::LinearGradient {
            from,
            to,
            angle_deg,
        });
        self
    }
    pub fn text_spans(mut self, spans: Vec<crate::style::TextSpan>) -> Self {
        let concat: String = spans.iter().map(|s| s.text.as_str()).collect();
        self.text_spans = Some(spans);
        self.text = Some(concat);
        self.style.flex_shrink = 0.0;
        self
    }

    /// Parse markdown-inspired inline markup into [`Self::text_spans`].
    pub fn markdown_inline(self, src: impl AsRef<str>) -> Self {
        self.text_spans(crate::text::parse_markdown_inline(src.as_ref()))
    }
    pub fn text_direction(mut self, dir: crate::style::TextDirection) -> Self {
        self.text_style.direction = dir;
        self
    }
    pub fn letter_spacing(mut self, spacing: f32) -> Self {
        self.text_style.letter_spacing = spacing;
        self
    }
    pub fn top(self, val: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::InsetT, val)
    }
    pub fn right(self, val: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::InsetR, val)
    }
    pub fn bottom(self, val: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::InsetB, val)
    }
    pub fn left(self, val: impl IntoCssDim) -> Self {
        self.set_css(UnitSlot::InsetL, val)
    }
    pub fn inset(self, val: impl IntoCssDim) -> Self {
        let d = val.into_css_dim();
        self.set_css(UnitSlot::InsetL, d.clone())
            .set_css(UnitSlot::InsetR, d.clone())
            .set_css(UnitSlot::InsetT, d.clone())
            .set_css(UnitSlot::InsetB, d)
    }
    pub fn inset_inline(self, val: impl IntoCssRectPair) -> Self {
        self.apply_rect_pair(UnitSlot::InsetL, UnitSlot::InsetR, val)
    }
    pub fn inset_block(self, val: impl IntoCssRectPair) -> Self {
        self.apply_rect_pair(UnitSlot::InsetT, UnitSlot::InsetB, val)
    }
    pub fn z_index(mut self, z: i32) -> Self {
        self.style.z_index = z;
        self
    }
    pub fn inset_rect(mut self, rect: Rect<LengthPercentageAuto>) -> Self {
        self.style.inset = rect;
        self
    }
}

/// Last-frame layout box is fully outside the window (plus pad for shadows).
/// Unlaid-out / zero-size / fixed / portal roots return false so we still walk them.
#[inline]
pub(crate) fn element_is_offscreen(
    el: &Element,
    scroll_acc: [f32; 2],
    viewport_w: f32,
    viewport_h: f32,
) -> bool {
    if el.style.position_fixed || el.template_id.as_deref() == Some("__rui_portal_layer") {
        return false;
    }
    let b = el.layout_bounds;
    if b[2] <= 0.0 || b[3] <= 0.0 || viewport_w <= 0.0 || viewport_h <= 0.0 {
        return false;
    }
    const PAD: f32 = 32.0;
    let vx = b[0] - scroll_acc[0] + el.sticky_visual_offset[0];
    let vy = b[1] - scroll_acc[1] + el.sticky_visual_offset[1];
    vx + b[2] < -PAD || vy + b[3] < -PAD || vx > viewport_w + PAD || vy > viewport_h + PAD
}

#[inline]
pub(crate) fn child_scroll_accumulation(el: &Element, scroll_acc: [f32; 2]) -> [f32; 2] {
    let mut next = scroll_acc;
    if el.style.overflow.x != taffy::style::Overflow::Visible {
        next[0] += el.scroll_offset[0];
    }
    if el.style.overflow.y != taffy::style::Overflow::Visible {
        next[1] += el.scroll_offset[1];
    }
    next
}

/// Run generic component-owned reactive synchronizers for the mounted tree.
pub(crate) fn sync_reactive_elements_subtree(
    element: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
    viewport_w: f32,
    viewport_h: f32,
    scroll_acc: [f32; 2],
) {
    if element_is_offscreen(element, scroll_acc, viewport_w, viewport_h) {
        return;
    }
    // Clone the Rc-backed descriptor so the callback may mutate its owning Element.
    if let Some(sync) = element.reactive_sync.clone() {
        *impact = impact.merge(sync.run(element, layout_tree));
    }
    let child_acc = child_scroll_accumulation(element, scroll_acc);
    for child in &mut element.children {
        sync_reactive_elements_subtree(
            child,
            layout_tree,
            impact,
            viewport_w,
            viewport_h,
            child_acc,
        );
    }
}

impl From<String> for Element {
    fn from(s: String) -> Self {
        Element::new().text(s)
    }
}

impl From<&str> for Element {
    fn from(s: &str) -> Self {
        Element::new().text(s)
    }
}

impl From<Vec<Element>> for Element {
    fn from(children: Vec<Element>) -> Self {
        let mut el = Element::new();
        for child in children {
            el = el.child(child);
        }
        el
    }
}

impl From<Option<Element>> for Element {
    fn from(opt: Option<Element>) -> Self {
        opt.unwrap_or_default()
    }
}

#[cfg(test)]
mod p1_builder_tests {
    use super::*;
    use crate::style::{TextDirection, TextSpan};

    #[test]
    fn text_spans_builder_sets_concat_text() {
        let el = Element::new().text_spans(vec![
            TextSpan::new("Hello").weight(700),
            TextSpan::new(" "),
            TextSpan::new("world").color([255, 0, 0, 255]),
        ]);
        assert_eq!(el.text.as_deref(), Some("Hello world"));
        assert_eq!(el.text_spans.as_ref().map(|s| s.len()), Some(3));
    }

    #[test]
    fn text_direction_builder() {
        let el = Element::new().text("hi").text_direction(TextDirection::Rtl);
        assert_eq!(el.text_style.direction, TextDirection::Rtl);
    }

    #[test]
    fn text_overflow_modes_set_whitespace_and_box_overflow() {
        let clip = Element::new().text("hi").text_overflow("clip");
        assert_eq!(clip.text_style.overflow, TextOverflow::Clip);
        assert_eq!(
            clip.text_style.white_space,
            crate::style::WhiteSpace::Nowrap
        );
        assert_eq!(clip.style.overflow.x, taffy::style::Overflow::Hidden);

        let hidden = Element::new().text("hi").text_overflow_hidden();
        assert_eq!(hidden.text_style.overflow, TextOverflow::Hidden);
        assert_eq!(
            hidden.text_style.white_space,
            crate::style::WhiteSpace::Nowrap
        );
        assert_eq!(hidden.style.overflow.x, taffy::style::Overflow::Hidden);

        let wrap = Element::new()
            .text("hi")
            .text_overflow_ellipsis()
            .text_overflow_newline();
        assert_eq!(wrap.text_style.overflow, TextOverflow::Newline);
        assert_eq!(
            wrap.text_style.white_space,
            crate::style::WhiteSpace::Normal
        );
        assert_eq!(wrap.style.overflow.x, taffy::style::Overflow::Visible);
    }

    #[test]
    fn fixed_sets_absolute_and_flag() {
        let el = Element::new().fixed();
        assert_eq!(el.style.position, Position::Absolute);
        assert!(el.style.position_fixed);
    }

    #[test]
    fn unpack_fragment_children_expands_rui_fragment() {
        let mut frag = Element::new();
        frag.set_attr("rui_fragment", "1");
        let frag = frag
            .child(Element::new().text("a"))
            .child(Element::new().text("b"));
        let kids = Element::unpack_fragment_children(Some(frag));
        assert_eq!(kids.len(), 2);
        assert_eq!(kids[0].text.as_deref(), Some("a"));
        assert_eq!(kids[1].text.as_deref(), Some("b"));
    }

    #[test]
    fn unpack_fragment_children_keeps_plain_element() {
        let el = Element::new().text("only");
        let kids = Element::unpack_fragment_children(Some(el));
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].text.as_deref(), Some("only"));
    }

    #[test]
    fn unpack_fragment_children_none_is_empty() {
        assert!(Element::unpack_fragment_children(None).is_empty());
    }

    #[test]
    fn into_element_list_flattens_rui_fragment() {
        let mut frag = Element::new();
        frag.set_attr("rui_fragment", "1");
        let frag = frag
            .child(Element::new().text("a"))
            .child(Element::new().text("b"));
        let kids = frag.into_element_list();
        assert_eq!(kids.len(), 2);
        assert_eq!(kids[0].text.as_deref(), Some("a"));
        assert_eq!(kids[1].text.as_deref(), Some("b"));
    }

    #[test]
    fn bind_element_ref_writes_template_id() {
        let slot = crate::core::reactivity::create_signal(None::<String>);
        let el = Element::new().template_id("tid:ref");
        bind_element_ref(&slot, &el);
        assert_eq!(slot.get().as_deref(), Some("tid:ref"));
    }

    #[test]
    fn into_dynamic_component_calls_closure() {
        let el = (|| Element::new().text("dyn")).into_dynamic_component();
        assert_eq!(el.text.as_deref(), Some("dyn"));
        let el2 = Element::new().text("plain").into_dynamic_component();
        assert_eq!(el2.text.as_deref(), Some("plain"));
    }
}
