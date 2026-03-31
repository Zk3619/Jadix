use crate::core::tree::{Element, IntoDimension, IntoF32};
use taffy::prelude::Dimension;

pub struct BaseComponentBuilder {
    pub el: Element,
    pub(crate) label: Option<String>,
    pub(crate) has_custom_w: bool,
    pub(crate) has_custom_h: bool,
    pub(crate) has_custom_rounded: bool,
    pub(crate) custom_w_px: Option<f32>,
    pub(crate) custom_h_px: Option<f32>,
    pub(crate) explicit_text_size_px: Option<f32>,
    pub(crate) auto_text_size_px: Option<f32>,
    pub(crate) explicit_text_weight: Option<u16>,
}

impl BaseComponentBuilder {
    pub fn new(el: Element) -> Self {
        Self {
            el,
            label: None,
            has_custom_w: false,
            has_custom_h: false,
            has_custom_rounded: false,
            custom_w_px: None,
            custom_h_px: None,
            explicit_text_size_px: None,
            auto_text_size_px: None,
            explicit_text_weight: None,
        }
    }

    fn text_size_from_edge_px(edge_px: f32) -> f32 {
        (edge_px * 0.44).clamp(12.0, 14.0)
    }

    fn recompute_auto_text_size(&mut self) {
        let base = match (self.custom_w_px, self.custom_h_px) {
            (Some(w), Some(h)) => w.min(h),
            (Some(w), None) => w,
            (None, Some(h)) => h,
            (None, None) => return,
        };
        self.auto_text_size_px = Some(Self::text_size_from_edge_px(base));
    }

    pub fn w(mut self, w: impl IntoDimension) -> Self {
        let dim = w.into_dimension();
        if let Dimension::Length(px) = dim {
            self.custom_w_px = Some(px);
            self.recompute_auto_text_size();
        }
        self.has_custom_w = true;
        self.el = self.el.w(dim);
        self
    }

    pub fn h(mut self, h: impl IntoDimension) -> Self {
        let dim = h.into_dimension();
        if let Dimension::Length(px) = dim {
            self.custom_h_px = Some(px);
            self.recompute_auto_text_size();
        }
        self.has_custom_h = true;
        self.el = self.el.h(dim);
        self
    }

    pub fn p(mut self, p: impl IntoDimension) -> Self {
        self.el = self.el.p(p);
        self
    }

    pub fn rounded(mut self, radius: impl IntoF32) -> Self {
        self.has_custom_rounded = true;
        self.el = self.el.rounded(radius);
        self
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.label = Some(text.into());
        self
    }

    pub fn bg(mut self, bg: [f32; 4]) -> Self {
        self.el = self.el.bg(bg);
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

    pub fn text_size(mut self, size: impl IntoF32) -> Self {
        let px = size.into_f32();
        self.explicit_text_size_px = Some(px);
        // 与 `font_weight` 相同：RSX `text_size` 走 `base` 时必须写回根节点，否则仅 `explicit_*` 无消费者。
        self.el = self.el.text_size(px);
        self
    }

    pub fn font_weight(mut self, weight: impl IntoF32) -> Self {
        let w = weight.into_f32().round().clamp(100.0, 900.0) as u16;
        self.explicit_text_weight = Some(w);
        // RSX 将 `font_weight` 作用在 `base` 回调上；必须写回根节点的 `text_style`，否则
        // `resolved_text_weight` 从未被消费时字重会丢失（例如 `<Label font_weight="700">`）。
        self.el = self.el.font_weight(w as f32);
        self
    }

    pub fn resolved_text_size(&self, default_px: f32) -> f32 {
        self.explicit_text_size_px
            .or(self.auto_text_size_px)
            .unwrap_or(default_px)
    }

    pub fn resolved_text_weight(&self, default_weight: u16) -> u16 {
        self.explicit_text_weight.unwrap_or(default_weight)
    }
    pub fn flex_grow(mut self, grow: impl crate::core::tree::IntoF32Raw) -> Self {
        self.el = self.el.flex_grow(grow);
        self
    }
    pub fn flex_shrink(mut self, shrink: impl crate::core::tree::IntoF32Raw) -> Self {
        self.el = self.el.flex_shrink(shrink);
        self
    }


    pub fn top(mut self, val: impl IntoDimension) -> Self {
        self.el = self.el.top(val);
        self
    }
    pub fn right(mut self, val: impl IntoDimension) -> Self {
        self.el = self.el.right(val);
        self
    }
    pub fn relative(mut self, r: bool) -> Self {
        if r { self.el = self.el.relative(); }
        self
    }
    pub fn absolute(mut self, a: bool) -> Self {
        if a { self.el = self.el.absolute(); }
        self
    }
    pub fn bottom(mut self, val: impl IntoDimension) -> Self {
        self.el = self.el.bottom(val);
        self
    }
    pub fn left(mut self, val: impl IntoDimension) -> Self {
        self.el = self.el.left(val);
        self
    }
    pub fn inset(mut self, val: impl IntoDimension) -> Self {
        self.el = self.el.inset(val);
        self
    }
    pub fn z_index(mut self, z: i32) -> Self {
        self.el = self.el.z_index(z);
        self
    }

    // Logical spacing
    pub fn mx(mut self, val: impl IntoDimension) -> Self {
        self.el = self.el.mx(val);
        self
    }
    pub fn my(mut self, val: impl IntoDimension) -> Self {
        self.el = self.el.my(val);
        self
    }
    pub fn px(mut self, val: impl IntoDimension) -> Self {
        self.el = self.el.px(val);
        self
    }
    pub fn py(mut self, val: impl IntoDimension) -> Self {
        self.el = self.el.py(val);
        self
    }
    pub fn pt(mut self, val: impl IntoDimension) -> Self {
        self.el = self.el.pt(val);
        self
    }
    pub fn pr(mut self, val: impl IntoDimension) -> Self {
        self.el = self.el.pr(val);
        self
    }
    pub fn pb(mut self, val: impl IntoDimension) -> Self {
        self.el = self.el.pb(val);
        self
    }
    pub fn pl(mut self, val: impl IntoDimension) -> Self {
        self.el = self.el.pl(val);
        self
    }
    pub fn mt(mut self, val: impl IntoDimension) -> Self {
        self.el = self.el.mt(val);
        self
    }
    pub fn mr(mut self, val: impl IntoDimension) -> Self {
        self.el = self.el.mr(val);
        self
    }
    pub fn mb(mut self, val: impl IntoDimension) -> Self {
        self.el = self.el.mb(val);
        self
    }
    pub fn ml(mut self, val: impl IntoDimension) -> Self {
        self.el = self.el.ml(val);
        self
    }

    // Text enhancements
    pub fn leading(mut self, scale: impl IntoF32) -> Self {
        self.el = self.el.text_leading(scale);
        self
    }
    pub fn leading_none(mut self) -> Self {
        self.el = self.el.text_leading_none();
        self
    }
    pub fn leading_tight(mut self) -> Self {
        self.el = self.el.text_leading_tight();
        self
    }
    pub fn leading_normal(mut self) -> Self {
        self.el = self.el.text_leading_normal();
        self
    }
    pub fn leading_relaxed(mut self) -> Self {
        self.el = self.el.text_leading_relaxed();
        self
    }
    pub fn line_height(mut self, px: impl IntoF32) -> Self {
        self.el = self.el.text_line_height(px);
        self
    }
    pub fn text_color(mut self, color: [u8; 4]) -> Self {
        self.el = self.el.text_color(color);
        self
    }
    pub fn underline(mut self, u: bool) -> Self {
        self.el = self.el.underline(u);
        self
    }
    pub fn text_decoration(mut self, d: impl Into<String>) -> Self {
        self.el = self.el.text_decoration(d.into());
        self
    }
    pub fn text_decoration_line(mut self, line: impl Into<String>) -> Self {
        self.el = self.el.text_decoration_line(line.into());
        self
    }
    pub fn text_decoration_style(mut self, s: impl Into<String>) -> Self {
        self.el = self.el.text_decoration_style(s.into());
        self
    }
    pub fn text_decoration_color(mut self, color: [u8; 4]) -> Self {
        self.el = self.el.text_decoration_color(color);
        self
    }
    pub fn text_decoration_thickness(mut self, t: impl IntoF32) -> Self {
        self.el = self.el.text_decoration_thickness(t);
        self
    }
    pub fn text_underline_offset(mut self, o: impl IntoF32) -> Self {
        self.el = self.el.text_underline_offset(o);
        self
    }
    pub fn text_center(mut self) -> Self {
        self.el = self.el.text_center();
        self
    }


    // Layout
    pub fn items(mut self, align: impl Into<String>) -> Self {
        self.el.set_attr("items", align.into());
        self
    }
    pub fn wrap(mut self, w: impl Into<String>) -> Self {
        self.el.set_attr("wrap", w.into());
        self
    }
    pub fn gap(mut self, g: impl IntoDimension) -> Self {
        self.el = self.el.gap(g);
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

    pub fn scroll_behavior(mut self, b: impl Into<String>) -> Self {
        self.el.set_attr("scroll_behavior", b.into());
        self
    }
    pub fn scrollbar_inset(mut self, i: impl Into<String>) -> Self {
        self.el.set_attr("scrollbar_inset", i.into());
        self
    }
    pub fn scrollbar_inset_x(mut self, i: impl Into<String>) -> Self {
        self.el.set_attr("scrollbar_inset_x", i.into());
        self
    }
    pub fn scrollbar_inset_y(mut self, i: impl Into<String>) -> Self {
        self.el.set_attr("scrollbar_inset_y", i.into());
        self
    }
    pub fn scrollbar_visibility(mut self, v: impl Into<String>) -> Self {
        self.el.set_attr("scrollbar_visibility", v.into());
        self
    }
    pub fn scrollbar_y_side(mut self, s: impl Into<String>) -> Self {
        self.el.set_attr("scrollbar_y_side", s.into());
        self
    }
    pub fn scrollbar_x_side(mut self, s: impl Into<String>) -> Self {
        self.el.set_attr("scrollbar_x_side", s.into());
        self
    }
    pub fn overflow_x(mut self, o: impl Into<String>) -> Self {
        self.el.set_attr("overflow_x", o.into());
        self
    }
    pub fn overflow_y(mut self, o: impl Into<String>) -> Self {
        self.el.set_attr("overflow_y", o.into());
        self
    }
    pub fn scrollbar_variant(mut self, v: impl Into<String>) -> Self {
        self.el.set_attr("scrollbar_variant", v.into());
        self
    }
}
