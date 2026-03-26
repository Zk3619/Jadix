use taffy::prelude::*;

pub use crate::layout::dimension::{
    IntoDimension, IntoF32, IntoF32Raw, IntoLengthPercentage, CSS_ROOT_FONT_SIZE_PX,
};
pub use crate::layout::layout_tree::{LayoutTree, RenderResult, TextLayoutInfo};
pub use crate::style::Style;
pub use crate::style::{parse_color, parse_text_color, TextDecorationStyle, TextStyle};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorKind {
    Default,
    Pointer,
    Text,
    NotAllowed,
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

pub struct Element {
    pub style: crate::style::Style,
    pub children: Vec<Element>,
    pub text: Option<String>,
    pub text_style: TextStyle,
    pub layout_bounds: [f32; 4],
    pub on_click: Option<std::rc::Rc<std::cell::RefCell<dyn FnMut()>>>,
    pub on_hover: Option<std::rc::Rc<std::cell::RefCell<dyn FnMut(bool)>>>,
    pub focus_id: Option<String>,
    pub focusable: bool,
    pub cursor: Option<CursorKind>,
    pub scroll_offset: [f32; 2],
    pub template_hash: Option<String>,
    pub template_id: Option<String>,
    pub(crate) layout_node: Option<taffy::prelude::NodeId>,
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
            text_style: TextStyle::default(),
            layout_bounds: [0.0, 0.0, 0.0, 0.0],
            on_click: None,
            on_hover: None,
            focus_id: None,
            focusable: false,
            cursor: None,
            scroll_offset: [0.0, 0.0],
            template_hash: None,
            template_id: None,
            layout_node: None,
        }
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
    pub fn gap(mut self, g: impl IntoF32) -> Self {
        let val = g.into_f32();
        self.style.gap = taffy::geometry::Size {
            width: taffy::style::LengthPercentage::Length(val),
            height: taffy::style::LengthPercentage::Length(val),
        };
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
        self
    }
    pub fn font_family(mut self, family: impl Into<String>) -> Self {
        self.text_style.font_family = Some(family.into());
        self
    }
    pub fn text_color(mut self, color: [u8; 4]) -> Self {
        self.text_style.color = color;
        self
    }
    pub fn text_size(mut self, size: f32) -> Self {
        self.text_style.size = size;
        self
    }
    pub fn font_weight(mut self, weight: impl IntoF32) -> Self {
        let w = weight.into_f32().round().clamp(100.0, 900.0) as u16;
        self.text_style.weight = w;
        self
    }
    pub fn underline(mut self, underline: bool) -> Self {
        self.text_style.underline = underline;
        self
    }
    pub fn text_decoration(mut self, decoration: impl Into<String>) -> Self {
        let raw = decoration.into().to_lowercase();
        if raw.contains("none") {
            self.text_style.underline = false;
            return self;
        }
        if raw.contains("underline") {
            self.text_style.underline = true;
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
    pub fn text_decoration_style(mut self, style: impl Into<String>) -> Self {
        let s = style.into().to_lowercase();
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
        self
    }
    pub fn text_decoration_thickness(mut self, thickness: impl IntoF32) -> Self {
        self.text_style.decoration_thickness = thickness.into_f32().max(0.0);
        self
    }
    pub fn text_underline_offset(mut self, offset: impl IntoF32) -> Self {
        self.text_style.underline_offset = offset.into_f32();
        self
    }
    pub fn text_shadow(mut self, color: [u8; 4], blur_radius: f32) -> Self {
        self.text_style.shadow_color = Some(color);
        self.text_style.shadow_blur = blur_radius;
        self
    }
    pub fn text_center(mut self) -> Self {
        self.text_style.align = glyphon::cosmic_text::Align::Center;
        self
    }
    pub fn text_line_height(mut self, line_height: impl IntoF32) -> Self {
        self.text_style.line_height_px = Some(line_height.into_f32().max(1.0));
        self
    }
    pub fn text_leading(mut self, scale: impl IntoF32) -> Self {
        self.text_style.line_height_px = None;
        self.text_style.line_height_scale = Some(scale.into_f32().max(0.01));
        self
    }
    pub fn text_leading_none(mut self) -> Self {
        self.text_style.line_height_px = None;
        self.text_style.line_height_scale = Some(1.0);
        self
    }
    pub fn text_leading_tight(mut self) -> Self {
        self.text_style.line_height_px = None;
        self.text_style.line_height_scale = Some(1.25);
        self
    }
    pub fn text_leading_normal(mut self) -> Self {
        self.text_style.line_height_px = None;
        self.text_style.line_height_scale = Some(1.5);
        self
    }
    pub fn text_leading_relaxed(mut self) -> Self {
        self.text_style.line_height_px = None;
        self.text_style.line_height_scale = Some(1.625);
        self
    }
    pub fn bg(mut self, color: [f32; 4]) -> Self {
        self.style.bg_color = color;
        self
    }
    pub fn border(mut self, color: [f32; 4], width: f32) -> Self {
        self.style.border_color = color;
        self.style.border_width = width;
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

impl IntoElementList for String {
    fn into_element_list(self) -> Vec<Element> {
        vec![Element::from(self)]
    }
}

impl IntoElementList for &str {
    fn into_element_list(self) -> Vec<Element> {
        vec![Element::from(self)]
    }
}
