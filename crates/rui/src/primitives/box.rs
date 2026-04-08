//! 布局原语：flex 容器、间距、背景、溢出等，最终落成一棵 [`Element`]。
//!
//! RSX：`VStack` / `HStack` / `Box` / `Rect` 由宏映射到 [`vstack`] / [`hstack`] / [`r#box`]。

use crate::core::tree::{Element, IntoDimension, IntoF32, IntoLengthPercentage};

pub struct BoxBuilder {
    pub(crate) el: Element,
}

impl BoxBuilder {
    pub fn new() -> Self {
        Self {
            el: Element::new(),
        }
    }

    pub fn w(mut self, w: impl IntoDimension) -> Self {
        self.el = self.el.w(w);
        self
    }

    pub fn h(mut self, h: impl IntoDimension) -> Self {
        self.el = self.el.h(h);
        self
    }

    pub fn min_w(mut self, w: impl IntoDimension) -> Self {
        self.el = self.el.min_w(w);
        self
    }

    pub fn min_h(mut self, h: impl IntoDimension) -> Self {
        self.el = self.el.min_h(h);
        self
    }

    pub fn max_w(mut self, w: impl IntoDimension) -> Self {
        self.el = self.el.max_w(w);
        self
    }

    pub fn max_h(mut self, h: impl IntoDimension) -> Self {
        self.el = self.el.max_h(h);
        self
    }

    pub fn p(mut self, p: impl IntoLengthPercentage) -> Self {
        self.el = self.el.p(p);
        self
    }

    pub fn px(mut self, p: impl IntoLengthPercentage) -> Self {
        self.el = self.el.px(p);
        self
    }

    pub fn py(mut self, p: impl IntoLengthPercentage) -> Self {
        self.el = self.el.py(p);
        self
    }

    pub fn pt(mut self, p: impl IntoLengthPercentage) -> Self {
        self.el = self.el.pt(p);
        self
    }

    pub fn pr(mut self, p: impl IntoLengthPercentage) -> Self {
        self.el = self.el.pr(p);
        self
    }

    pub fn pb(mut self, p: impl IntoLengthPercentage) -> Self {
        self.el = self.el.pb(p);
        self
    }

    pub fn pl(mut self, p: impl IntoLengthPercentage) -> Self {
        self.el = self.el.pl(p);
        self
    }

    pub fn padding_inline(mut self, p: impl IntoLengthPercentage) -> Self {
        self.el = self.el.padding_inline(p);
        self
    }

    pub fn padding_block(mut self, p: impl IntoLengthPercentage) -> Self {
        self.el = self.el.padding_block(p);
        self
    }

    pub fn padding_inline_start(mut self, p: impl IntoLengthPercentage) -> Self {
        self.el = self.el.padding_inline_start(p);
        self
    }

    pub fn padding_inline_end(mut self, p: impl IntoLengthPercentage) -> Self {
        self.el = self.el.padding_inline_end(p);
        self
    }

    pub fn padding_block_start(mut self, p: impl IntoLengthPercentage) -> Self {
        self.el = self.el.padding_block_start(p);
        self
    }

    pub fn padding_block_end(mut self, p: impl IntoLengthPercentage) -> Self {
        self.el = self.el.padding_block_end(p);
        self
    }

    pub fn m(mut self, m: impl IntoDimension) -> Self {
        self.el = self.el.m(m);
        self
    }

    pub fn mx(mut self, m: impl IntoDimension) -> Self {
        self.el = self.el.mx(m);
        self
    }

    pub fn my(mut self, m: impl IntoDimension) -> Self {
        self.el = self.el.my(m);
        self
    }

    pub fn mt(mut self, m: impl IntoDimension) -> Self {
        self.el = self.el.mt(m);
        self
    }

    pub fn mr(mut self, m: impl IntoDimension) -> Self {
        self.el = self.el.mr(m);
        self
    }

    pub fn mb(mut self, m: impl IntoDimension) -> Self {
        self.el = self.el.mb(m);
        self
    }

    pub fn ml(mut self, m: impl IntoDimension) -> Self {
        self.el = self.el.ml(m);
        self
    }

    pub fn margin_inline(mut self, m: impl IntoDimension) -> Self {
        self.el = self.el.margin_inline(m);
        self
    }

    pub fn margin_block(mut self, m: impl IntoDimension) -> Self {
        self.el = self.el.margin_block(m);
        self
    }

    pub fn margin_inline_start(mut self, m: impl IntoDimension) -> Self {
        self.el = self.el.margin_inline_start(m);
        self
    }

    pub fn margin_inline_end(mut self, m: impl IntoDimension) -> Self {
        self.el = self.el.margin_inline_end(m);
        self
    }

    pub fn margin_block_start(mut self, m: impl IntoDimension) -> Self {
        self.el = self.el.margin_block_start(m);
        self
    }

    pub fn margin_block_end(mut self, m: impl IntoDimension) -> Self {
        self.el = self.el.margin_block_end(m);
        self
    }

    pub fn inline_size(mut self, s: impl IntoDimension) -> Self {
        self.el = self.el.inline_size(s);
        self
    }

    pub fn block_size(mut self, s: impl IntoDimension) -> Self {
        self.el = self.el.block_size(s);
        self
    }

    pub fn bg(mut self, color: [f32; 4]) -> Self {
        self.el = self.el.bg(color);
        self
    }

    pub fn border(mut self, color: [f32; 4], width: f32) -> Self {
        self.el = self.el.border(color, width);
        self
    }
    pub fn border_style(mut self, style: impl AsRef<str>) -> Self {
        self.el = self.el.border_style(style);
        self
    }
    pub fn border_color(mut self, color: [f32; 4]) -> Self {
        self.el = self.el.border_color(color);
        self
    }
    pub fn border_width(mut self, width: f32) -> Self {
        self.el = self.el.border_width(width);
        self
    }

    pub fn outline(mut self, color: [f32; 4], width: f32) -> Self {
        self.el = self.el.outline(color, width);
        self
    }
    pub fn outline_style(mut self, style: impl AsRef<str>) -> Self {
        self.el = self.el.outline_style(style);
        self
    }
    pub fn outline_color(mut self, color: [f32; 4]) -> Self {
        self.el = self.el.outline_color(color);
        self
    }
    pub fn outline_width(mut self, width: f32) -> Self {
        self.el = self.el.outline_width(width);
        self
    }

    pub fn rounded(mut self, radius: impl IntoF32) -> Self {
        self.el = self.el.rounded(radius);
        self
    }

    pub fn gap(mut self, g: impl IntoDimension) -> Self {
        self.el = self.el.gap(g);
        self
    }
    pub fn items(mut self, align: impl Into<String>) -> Self {
        let s = align.into();
        self.el = self.el.flex_items(&s);
        self
    }
    pub fn wrap(mut self, w: impl Into<String>) -> Self {
        let s = w.into();
        self.el = self.el.flex_wrap_kw(&s);
        self
    }

    pub fn justify(mut self, keyword: impl Into<String>) -> Self {
        let s = keyword.into();
        self.el = self.el.justify(&s);
        self
    }


    pub fn flex_col(mut self) -> Self {
        self.el = self.el.flex_col();
        self
    }

    pub fn flex_row(mut self) -> Self {
        self.el = self.el.flex_row();
        self
    }

    pub fn flex_grow(mut self, grow: impl crate::core::tree::IntoF32Raw) -> Self {
        self.el = self.el.flex_grow(grow);
        self
    }

    pub fn flex_shrink(mut self, shrink: impl crate::core::tree::IntoF32Raw) -> Self {
        self.el = self.el.flex_shrink(shrink);
        self
    }

    pub fn justify_center(mut self) -> Self {
        self.el = self.el.justify_center();
        self
    }

    pub fn items_center(mut self) -> Self {
        self.el = self.el.items_center();
        self
    }

    pub fn absolute(mut self, abs: bool) -> Self {
        if abs { self.el = self.el.absolute(); }
        self
    }

    pub fn relative(mut self, rel: bool) -> Self {
        if rel { self.el = self.el.relative(); }
        self
    }

    pub fn shadow(mut self, color: [f32; 4], blur: f32) -> Self {
        self.el = self.el.shadow(color, blur);
        self
    }
    pub fn box_shadow(mut self, spec: impl AsRef<str>) -> Self {
        self.el = self.el.box_shadow(spec);
        self
    }
    pub fn box_shadow_color(mut self, color: [f32; 4]) -> Self {
        self.el = self.el.box_shadow_color(color);
        self
    }
    pub fn box_shadow_blur(mut self, blur: f32) -> Self {
        self.el = self.el.box_shadow_blur(blur);
        self
    }
    pub fn box_shadow_offset_x(mut self, dx: f32) -> Self {
        self.el = self.el.box_shadow_offset_x(dx);
        self
    }
    pub fn box_shadow_offset_y(mut self, dy: f32) -> Self {
        self.el = self.el.box_shadow_offset_y(dy);
        self
    }
    pub fn box_shadow_spread(mut self, spread: f32) -> Self {
        self.el = self.el.box_shadow_spread(spread);
        self
    }
    pub fn box_shadow_inset(mut self, inset: bool) -> Self {
        self.el = self.el.box_shadow_inset(inset);
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.el = self.el.child(child);
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        for c in children {
            self = self.child(c);
        }
        self
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.el = self.el.text(text);
        self
    }

    pub fn text_size(mut self, size: impl IntoF32) -> Self {
        self.el = self.el.text_size(size.into_f32());
        self
    }

    pub fn text_color(mut self, color: [u8; 4]) -> Self {
        self.el = self.el.text_color(color);
        self
    }

    pub fn font_weight(mut self, weight: impl IntoF32) -> Self {
        self.el = self.el.font_weight(weight);
        self
    }

    pub fn underline(mut self, underline: bool) -> Self {
        self.el = self.el.underline(underline);
        self
    }

    pub fn text_decoration(mut self, decoration: impl Into<String>) -> Self {
        self.el = self.el.text_decoration(decoration.into());
        self
    }

    pub fn text_decoration_line(mut self, line: impl Into<String>) -> Self {
        self.el = self.el.text_decoration_line(line.into());
        self
    }

    pub fn text_decoration_style(mut self, style: impl Into<String>) -> Self {
        self.el = self.el.text_decoration_style(style.into());
        self
    }

    pub fn text_decoration_color(mut self, color: [u8; 4]) -> Self {
        self.el = self.el.text_decoration_color(color);
        self
    }

    pub fn text_decoration_thickness(mut self, thickness: impl IntoF32) -> Self {
        self.el = self.el.text_decoration_thickness(thickness);
        self
    }

    pub fn text_underline_offset(mut self, offset: impl IntoF32) -> Self {
        self.el = self.el.text_underline_offset(offset);
        self
    }

    pub fn text_line_height(mut self, line_height: impl IntoF32) -> Self {
        self.el = self.el.text_line_height(line_height);
        self
    }

    pub fn text_leading(mut self, scale: impl IntoF32) -> Self {
        self.el = self.el.text_leading(scale);
        self
    }

    pub fn text_leading_none(mut self) -> Self {
        self.el = self.el.text_leading_none();
        self
    }

    pub fn text_leading_tight(mut self) -> Self {
        self.el = self.el.text_leading_tight();
        self
    }

    pub fn text_leading_normal(mut self) -> Self {
        self.el = self.el.text_leading_normal();
        self
    }

    pub fn text_leading_relaxed(mut self) -> Self {
        self.el = self.el.text_leading_relaxed();
        self
    }

    pub fn text_center(mut self) -> Self {
        self.el = self.el.text_center();
        self
    }
    pub fn text_align(mut self, align: impl Into<String>) -> Self {
        let s = align.into();
        if s.eq_ignore_ascii_case("left") || s.eq_ignore_ascii_case("start") {
            self.el = self.el.text_left();
        } else if s.eq_ignore_ascii_case("center") {
            self.el = self.el.text_center();
        } else if s.eq_ignore_ascii_case("right") || s.eq_ignore_ascii_case("end") {
            self.el = self.el.text_right();
        }
        self
    }

    pub fn text_shadow(mut self, spec: impl AsRef<str>) -> Self {
        self.el = self.el.text_shadow(spec);
        self
    }
    pub fn text_shadow_color(mut self, color: [u8; 4]) -> Self {
        self.el = self.el.text_shadow_color(color);
        self
    }
    pub fn text_shadow_blur(mut self, blur: f32) -> Self {
        self.el = self.el.text_shadow_blur(blur);
        self
    }
    pub fn text_shadow_offset_x(mut self, dx: f32) -> Self {
        self.el = self.el.text_shadow_offset_x(dx);
        self
    }
    pub fn text_shadow_offset_y(mut self, dy: f32) -> Self {
        self.el = self.el.text_shadow_offset_y(dy);
        self
    }

    pub fn overflow(mut self, mode: impl Into<String>) -> Self {
        self.el = self.el.overflow(mode.into());
        self
    }
    pub fn overflow_x(mut self, mode: impl Into<String>) -> Self {
        self.el = self.el.overflow_x(mode.into());
        self
    }
    pub fn overflow_y(mut self, mode: impl Into<String>) -> Self {
        self.el = self.el.overflow_y(mode.into());
        self
    }
}

impl From<BoxBuilder> for Element {
    fn from(builder: BoxBuilder) -> Self {
        builder.el
    }
}

impl crate::core::tree::IntoElementList for BoxBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

pub fn r#box() -> BoxBuilder {
    BoxBuilder::new()
}

pub fn hstack() -> BoxBuilder {
    BoxBuilder::new().flex_row()
}

pub fn vstack() -> BoxBuilder {
    // 与 CSS column flex 默认一致：交叉轴 stretch，子项（含文本）易获得父级宽度以便换行。
    BoxBuilder::new().flex_col().items("stretch")
}
