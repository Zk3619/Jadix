use taffy::prelude::*;
use winit::event::KeyEvent;


use crate::core::reactivity::Signal;

pub use crate::layout::dimension::{
    IntoDimension, IntoF32, IntoF32Raw, IntoLengthPercentage, CSS_ROOT_FONT_SIZE_PX,
};
pub use crate::layout::layout_tree::{LayoutProfileMs, LayoutTree, RenderResult, TextLayoutInfo};
pub use crate::style::Style;
pub use crate::style::{parse_color, parse_text_color, LineStyle, TextDecorationStyle, TextStyle};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorKind {
    Default,
    Pointer,
    Text,
    NotAllowed,
}

/// [`Element::button_hover_paint`]：用于无整树重建的按钮 hover / focus 环同步（见 `components/button.rs`）。
#[derive(Debug, Clone)]
pub struct ButtonHoverPaint {
    pub variant: String,
    pub disabled: bool,
    /// `"spring"`（欠阻尼弹簧）或 `"cubic"`（`cubic-bezier` 时长型过渡）。
    pub hover_easing: String,
}

/// [`Element::input_paint_sync`]：每帧按信号 `peek` 刷新 Input 边框、阴影、文本与光标（避免根 `create_effect` 订阅焦点与按键）。
#[derive(Debug, Clone)]
pub struct InputPaintSync {
    pub base_id: String,
    pub placeholder: String,
    pub invalid: bool,
    pub disabled: bool,
    pub text_size_px: f32,
    pub font_weight: u16,
    pub line_height_px: Option<f32>,
    pub leading_scale: Option<f32>,
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

pub struct Element {
    pub style: crate::style::Style,
    pub children: Vec<Element>,
    pub text: Option<String>,
    /// 仅参与 glyph 排版（`set_text` / 宽度估计），不计入 `text` 的字节长度；用于减轻纯中文 IME 下 cosmic-text 行高随拼音 bbox 抖动。
    pub text_shape_suffix: Option<String>,
    pub text_style: TextStyle,
    pub layout_bounds: [f32; 4],
    pub on_click: Option<std::rc::Rc<std::cell::RefCell<dyn FnMut()>>>,
    pub on_hover: Option<std::rc::Rc<std::cell::RefCell<dyn FnMut(bool)>>>,
    /// Keyboard on focused node. Return `true` if the event is consumed.
    pub on_key_down: Option<std::rc::Rc<std::cell::RefCell<dyn FnMut(KeyEvent) -> bool>>>,
    pub focus_id: Option<String>,
    pub focusable: bool,
    pub cursor: Option<CursorKind>,
    pub scroll_offset: [f32; 2],
    /// True if this node or any descendant is a ScrollArea viewport (`scroll:` focus id).
    /// Used to prune subtrees in the ViewportsOnly scrollbar prepass (see
    /// [`scroll_viewport_subtree_count`] for nested viewports).
    pub contains_scroll_viewport: bool,
    /// Count of `scroll:` viewport nodes in this subtree (including self). Nested ScrollAreas need
    /// scrollbar geometry prepass to recurse past ancestor viewports when this is > 1 on a viewport.
    pub scroll_viewport_subtree_count: u32,
    pub template_hash: Option<String>,
    pub template_id: Option<String>,
    /// 若存在，则每帧由 `sync_button_hover_paint_subtree` 根据 `{focus_id}_hover` 与全局焦点信号刷新样式（避免 `hover.get()` / `focused.get()` 订阅根 effect）。
    pub button_hover_paint: Option<ButtonHoverPaint>,
    /// 若存在，则每帧由 [`crate::components::input::sync_input_paint_subtree`] 刷新（`peek` 全部输入相关信号）。
    pub input_paint_sync: Option<InputPaintSync>,
    /// Switch track: [`crate::components::switch::sync_switch_checked_subtree`] reads `checked` and updates track bg + thumb alignment.
    pub switch_checked_sync: Option<Signal<bool>>,
    /// Checkbox root: [`crate::components::checkbox::sync_checkbox_checked_subtree`] updates fill + checkmark color.
    pub checkbox_sync: Option<Signal<bool>>,
    /// GPU 贴图资源键（见 [`crate::queue_image_rgba8`]）；布局收集时生成纹理四边形实例。
    pub image_texture: Option<String>,
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
    fn into_element_list(self) -> Vec<Element> {
        vec![self]
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

impl Element {
    pub(crate) fn into_margin_auto(val: impl IntoDimension) -> LengthPercentageAuto {
        match val.into_dimension() {
            Dimension::Length(v) => LengthPercentageAuto::Length(v),
            Dimension::Percent(v) => LengthPercentageAuto::Percent(v),
            Dimension::Auto => LengthPercentageAuto::Auto,
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
            text_shape_suffix: None,
            text_style: TextStyle::default(),
            layout_bounds: [0.0, 0.0, 0.0, 0.0],
            on_click: None,
            on_hover: None,
            on_key_down: None,
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
            image_texture: None,
            layout_node: None,
            attrs: std::collections::HashMap::new(),
        }
    }

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
        self.cursor = Some(c);
        self
    }

    pub fn cursor_text(self) -> Self {
        self.cursor_kind(CursorKind::Text)
    }
    pub fn cursor_not_allowed(self) -> Self {
        self.cursor_kind(CursorKind::NotAllowed)
    }
    pub fn min_w(mut self, w: impl IntoDimension) -> Self {
        self.style.min_size.width = w.into_dimension();
        self
    }
    pub fn min_h(mut self, h: impl IntoDimension) -> Self {
        self.style.min_size.height = h.into_dimension();
        self
    }
    pub fn max_w(mut self, w: impl IntoDimension) -> Self {
        self.style.max_size.width = w.into_dimension();
        self
    }
    pub fn max_h(mut self, h: impl IntoDimension) -> Self {
        self.style.max_size.height = h.into_dimension();
        self
    }
    pub fn on_click(mut self, handler: impl FnMut() + 'static) -> Self {
        self.on_click = Some(std::rc::Rc::new(std::cell::RefCell::new(handler)));
        self
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
    pub fn scroll_offset(mut self, x: f32, y: f32) -> Self {
        self.scroll_offset = [x.max(0.0), y.max(0.0)];
        self
    }
    fn gap_lp(g: impl IntoDimension) -> taffy::style::LengthPercentage {
        let dim = g.into_dimension();
        match dim {
            Dimension::Length(v) => taffy::style::LengthPercentage::Length(v),
            Dimension::Percent(v) => taffy::style::LengthPercentage::Percent(v),
            Dimension::Auto => taffy::style::LengthPercentage::Percent(0.0),
        }
    }

    pub fn gap(mut self, g: impl IntoDimension) -> Self {
        let lp = Self::gap_lp(g);
        self.style.gap = taffy::geometry::Size {
            width: lp.clone(),
            height: lp,
        };
        self
    }

    /// `column-gap` / Tailwind `gap-x-*`。
    pub fn gap_x(mut self, g: impl IntoDimension) -> Self {
        self.style.gap.width = Self::gap_lp(g);
        self
    }

    /// `row-gap` / Tailwind `gap-y-*`。
    pub fn gap_y(mut self, g: impl IntoDimension) -> Self {
        self.style.gap.height = Self::gap_lp(g);
        self
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
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        // 与 CSS 中文本节点类似：不在 flex 行内被 shrink 压窄，避免盒宽小于实际 glyph 宽而裁字。
        self.style.flex_shrink = 0.0;
        self
    }
    pub fn font_family(mut self, family: impl Into<String>) -> Self {
        self.text_style.font_family = Some(family.into());
        self.attrs.insert("font_family".to_string(), "1".to_string());
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
        self.attrs.insert("font_weight".to_string(), "1".to_string());
        self
    }
    pub fn underline(mut self, underline: bool) -> Self {
        self.text_style.underline = underline;
        self
    }
    pub fn text_decoration(mut self, decoration: impl Into<String>) -> Self {
        let raw = decoration.into().to_lowercase();
        self.attrs.insert("text_decoration_line".to_string(), "1".to_string());
        self.attrs.insert("text_decoration_style".to_string(), "1".to_string());
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
            if raw.contains("line-through") || raw.contains("linethrough") || raw.contains("strikethrough") {
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
        self.attrs.insert("text_decoration_line".to_string(), "1".to_string());
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
        if raw.contains("line-through") || raw.contains("linethrough") || raw.contains("strikethrough") {
            self.text_style.line_through = true;
        }
        self
    }
    pub fn text_decoration_style(mut self, style: impl Into<String>) -> Self {
        let s = style.into().trim().to_lowercase();
        self.attrs.insert("text_decoration_style".to_string(), "1".to_string());
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
        self.attrs.insert("text_decoration_color".to_string(), "1".to_string());
        self
    }
    pub fn text_decoration_thickness(mut self, thickness: impl IntoF32) -> Self {
        self.text_style.decoration_thickness = thickness.into_f32().max(0.0);
        self.attrs.insert("text_decoration_thickness".to_string(), "1".to_string());
        self
    }
    pub fn text_underline_offset(mut self, offset: impl IntoF32) -> Self {
        self.text_style.underline_offset = offset.into_f32();
        self.attrs.insert("text_underline_offset".to_string(), "1".to_string());
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
        self.attrs.insert("text_align".to_string(), "center".to_string());
        self
    }
    pub fn text_left(mut self) -> Self {
        self.text_style.align = glyphon::cosmic_text::Align::Left;
        self.attrs.insert("text_align".to_string(), "left".to_string());
        self
    }
    pub fn text_right(mut self) -> Self {
        self.text_style.align = glyphon::cosmic_text::Align::Right;
        self.attrs.insert("text_align".to_string(), "right".to_string());
        self
    }
    pub fn text_line_height(mut self, line_height: impl IntoF32) -> Self {
        self.text_style.line_height_px = Some(line_height.into_f32().max(1.0));
        self.attrs.insert("line_height".to_string(), "1".to_string());
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
    pub fn bg(mut self, color: [f32; 4]) -> Self {
        self.style.bg_color = color;
        self
    }
    pub fn border(mut self, color: [f32; 4], width: f32) -> Self {
        self.style.border_style = LineStyle::Solid;
        self.style.border_color = color;
        self.style.border_width = width;
        self
    }
    pub fn border_style(mut self, style: impl AsRef<str>) -> Self {
        self.style.border_style = LineStyle::from_css_keyword(style.as_ref());
        self
    }
    pub fn border_color(mut self, color: [f32; 4]) -> Self {
        self.style.border_color = color;
        self
    }
    pub fn border_width(mut self, width: f32) -> Self {
        self.style.border_width = width;
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
        let s = crate::style::parse_box_shadow(shadow);
        self.style.shadow_color = s.color;
        self.style.shadow_offset = s.offset;
        self.style.shadow_blur = s.blur;
        self.style.shadow_spread = s.spread;
        self.style.shadow_inset = s.inset;
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
    pub fn w(mut self, width: impl IntoDimension) -> Self {
        self.style.size.width = width.into_dimension();
        self
    }
    pub fn inline_size(self, size: impl IntoDimension) -> Self {
        self.w(size)
    }
    pub fn h(mut self, height: impl IntoDimension) -> Self {
        self.style.size.height = height.into_dimension();
        self
    }
    pub fn block_size(self, size: impl IntoDimension) -> Self {
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
        self.style.justify_content = Some(JustifyContent::Center);
        self
    }
    pub fn justify_start(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::FlexStart);
        self
    }
    pub fn justify_end(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::FlexEnd);
        self
    }

    /// `justify-content`，与 CSS / Tailwind `justify-*` 一致（区别于 [`Self::flex_items`] / `align-items`）。
    pub fn justify(mut self, keyword: impl AsRef<str>) -> Self {
        let s = keyword.as_ref().trim().to_ascii_lowercase();
        self.style.justify_content = Some(match s.as_str() {
            "center" => JustifyContent::Center,
            "start" | "flex-start" => JustifyContent::FlexStart,
            "end" | "flex-end" => JustifyContent::FlexEnd,
            "between" | "space-between" => JustifyContent::SpaceBetween,
            "around" | "space-around" => JustifyContent::SpaceAround,
            "evenly" | "space-evenly" => JustifyContent::SpaceEvenly,
            _ => JustifyContent::FlexStart,
        });
        self
    }

    /// `align-items`，与 RSX `items` / `align`、Tailwind `items-*` 一致。
    pub fn flex_items(mut self, keyword: impl AsRef<str>) -> Self {
        let s = keyword.as_ref().trim().to_ascii_lowercase();
        self.style.align_items = Some(match s.as_str() {
            "center" => AlignItems::Center,
            "start" | "flex-start" => AlignItems::FlexStart,
            "end" | "flex-end" => AlignItems::FlexEnd,
            "stretch" => AlignItems::Stretch,
            "baseline" => AlignItems::Baseline,
            _ => AlignItems::Stretch,
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
        self.style.align_items = Some(AlignItems::Center);
        self
    }
    pub fn p(mut self, padding: impl IntoLengthPercentage) -> Self {
        let p = padding.into_length_percentage();
        self.style.padding = Rect {
            left: p,
            right: p,
            top: p,
            bottom: p,
        };
        self
    }
    pub fn px(mut self, padding: impl IntoLengthPercentage) -> Self {
        let p = padding.into_length_percentage();
        self.style.padding.left = p;
        self.style.padding.right = p;
        self
    }
    pub fn py(mut self, padding: impl IntoLengthPercentage) -> Self {
        let p = padding.into_length_percentage();
        self.style.padding.top = p;
        self.style.padding.bottom = p;
        self
    }
    pub fn pt(mut self, padding: impl IntoLengthPercentage) -> Self {
        self.style.padding.top = padding.into_length_percentage();
        self
    }
    pub fn pr(mut self, padding: impl IntoLengthPercentage) -> Self {
        self.style.padding.right = padding.into_length_percentage();
        self
    }
    pub fn pb(mut self, padding: impl IntoLengthPercentage) -> Self {
        self.style.padding.bottom = padding.into_length_percentage();
        self
    }
    pub fn pl(mut self, padding: impl IntoLengthPercentage) -> Self {
        self.style.padding.left = padding.into_length_percentage();
        self
    }
    pub fn padding_inline(self, padding: impl IntoLengthPercentage) -> Self {
        self.px(padding)
    }
    pub fn padding_block(self, padding: impl IntoLengthPercentage) -> Self {
        self.py(padding)
    }
    pub fn padding_inline_start(self, padding: impl IntoLengthPercentage) -> Self {
        self.pl(padding)
    }
    pub fn padding_inline_end(self, padding: impl IntoLengthPercentage) -> Self {
        self.pr(padding)
    }
    pub fn padding_block_start(self, padding: impl IntoLengthPercentage) -> Self {
        self.pt(padding)
    }
    pub fn padding_block_end(self, padding: impl IntoLengthPercentage) -> Self {
        self.pb(padding)
    }
    pub fn padding_h(mut self, val: impl IntoLengthPercentage) -> Self {
        let p = val.into_length_percentage();
        self.style.padding.left = p;
        self.style.padding.right = p;
        self
    }
    pub fn flex_row(mut self) -> Self {
        self.style.flex_direction = FlexDirection::Row;
        self
    }
    pub fn m(mut self, margin: impl IntoDimension) -> Self {
        let m = Self::into_margin_auto(margin);
        self.style.margin = Rect {
            left: m,
            right: m,
            top: m,
            bottom: m,
        };
        self
    }
    pub fn mx(mut self, margin: impl IntoDimension) -> Self {
        let m = Self::into_margin_auto(margin);
        self.style.margin.left = m;
        self.style.margin.right = m;
        self
    }
    pub fn my(mut self, margin: impl IntoDimension) -> Self {
        let m = Self::into_margin_auto(margin);
        self.style.margin.top = m;
        self.style.margin.bottom = m;
        self
    }
    pub fn mt(mut self, margin: impl IntoDimension) -> Self {
        self.style.margin.top = Self::into_margin_auto(margin);
        self
    }
    pub fn mr(mut self, margin: impl IntoDimension) -> Self {
        self.style.margin.right = Self::into_margin_auto(margin);
        self
    }
    pub fn mb(mut self, margin: impl IntoDimension) -> Self {
        self.style.margin.bottom = Self::into_margin_auto(margin);
        self
    }
    pub fn ml(mut self, margin: impl IntoDimension) -> Self {
        self.style.margin.left = Self::into_margin_auto(margin);
        self
    }
    pub fn margin_inline(self, margin: impl IntoDimension) -> Self {
        self.mx(margin)
    }
    pub fn margin_block(self, margin: impl IntoDimension) -> Self {
        self.my(margin)
    }
    pub fn margin_inline_start(self, margin: impl IntoDimension) -> Self {
        self.ml(margin)
    }
    pub fn margin_inline_end(self, margin: impl IntoDimension) -> Self {
        self.mr(margin)
    }
    pub fn margin_block_start(self, margin: impl IntoDimension) -> Self {
        self.mt(margin)
    }
    pub fn margin_block_end(self, margin: impl IntoDimension) -> Self {
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

    pub fn display_grid(mut self) -> Self {
        self.style.display = Display::Grid;
        self
    }

    pub fn display_flex(mut self) -> Self {
        self.style.display = Display::Flex;
        self
    }

    /// `flex-basis`（主轴初始尺寸）。
    pub fn flex_basis(mut self, basis: impl IntoDimension) -> Self {
        self.style.flex_basis = basis.into_dimension();
        self
    }

    /// `align-self`，与 Tailwind `self-*` 一致；`auto` / 未知关键字清除为继承父级 `align-items`。
    pub fn align_self_kw(mut self, keyword: impl AsRef<str>) -> Self {
        let s = keyword.as_ref().trim().to_ascii_lowercase();
        self.style.align_self = match s.as_str() {
            "" | "auto" => None,
            "center" => Some(AlignSelf::Center),
            "start" | "flex-start" => Some(AlignSelf::FlexStart),
            "end" | "flex-end" => Some(AlignSelf::FlexEnd),
            "stretch" => Some(AlignSelf::Stretch),
            "baseline" => Some(AlignSelf::Baseline),
            _ => None,
        };
        self
    }

    /// `align-content`（多行 flex / grid 轴上分布）。
    pub fn align_content_kw(mut self, keyword: impl AsRef<str>) -> Self {
        let s = keyword.as_ref().trim().to_ascii_lowercase();
        self.style.align_content = Some(match s.as_str() {
            "start" | "flex-start" => AlignContent::FlexStart,
            "end" | "flex-end" => AlignContent::FlexEnd,
            "center" => AlignContent::Center,
            "stretch" => AlignContent::Stretch,
            "space-between" | "between" => AlignContent::SpaceBetween,
            "space-around" | "around" => AlignContent::SpaceAround,
            "space-evenly" | "evenly" => AlignContent::SpaceEvenly,
            _ => AlignContent::Stretch,
        });
        self
    }

    /// Grid 容器 `justify-items`。
    pub fn justify_items_kw(mut self, keyword: impl AsRef<str>) -> Self {
        let s = keyword.as_ref().trim().to_ascii_lowercase();
        self.style.justify_items = Some(match s.as_str() {
            "center" => AlignItems::Center,
            "start" | "flex-start" => AlignItems::FlexStart,
            "end" | "flex-end" => AlignItems::FlexEnd,
            "stretch" => AlignItems::Stretch,
            "baseline" => AlignItems::Baseline,
            _ => AlignItems::Stretch,
        });
        self
    }

    /// Grid 子项 `justify-self`；`auto` / 未知清除为继承父级 `justify-items`。
    pub fn justify_self_kw(mut self, keyword: impl AsRef<str>) -> Self {
        let s = keyword.as_ref().trim().to_ascii_lowercase();
        self.style.justify_self = match s.as_str() {
            "" | "auto" => None,
            "center" => Some(AlignSelf::Center),
            "start" | "flex-start" => Some(AlignSelf::FlexStart),
            "end" | "flex-end" => Some(AlignSelf::FlexEnd),
            "stretch" => Some(AlignSelf::Stretch),
            "baseline" => Some(AlignSelf::Baseline),
            _ => None,
        };
        self
    }

    pub fn grid_template_columns(mut self, tracks: Vec<TrackSizingFunction>) -> Self {
        self.style.grid_template_columns = tracks;
        self
    }

    pub fn grid_template_rows(mut self, tracks: Vec<TrackSizingFunction>) -> Self {
        self.style.grid_template_rows = tracks;
        self
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
        self.cursor = Some(CursorKind::Pointer);
        self
    }

    pub fn absolute(mut self) -> Self {
        self.style.position = Position::Absolute;
        self
    }
    pub fn relative(mut self) -> Self {
        self.style.position = Position::Relative;
        self
    }
    pub fn top(mut self, val: impl IntoDimension) -> Self {
        self.style.inset.top = Self::into_margin_auto(val);
        self
    }
    pub fn right(mut self, val: impl IntoDimension) -> Self {
        self.style.inset.right = Self::into_margin_auto(val);
        self
    }
    pub fn bottom(mut self, val: impl IntoDimension) -> Self {
        self.style.inset.bottom = Self::into_margin_auto(val);
        self
    }
    pub fn left(mut self, val: impl IntoDimension) -> Self {
        self.style.inset.left = Self::into_margin_auto(val);
        self
    }
    pub fn inset(mut self, val: impl IntoDimension) -> Self {
        let dim = Self::into_margin_auto(val);
        self.style.inset = Rect {
            left: dim,
            right: dim,
            top: dim,
            bottom: dim,
        };
        self
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
        match opt {
            Some(v) => v,
            None => Element::new(),
        }
    }
}
