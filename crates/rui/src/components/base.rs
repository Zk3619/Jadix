use crate::core::tree::{Element, IntoDimension, IntoF32};
use taffy::prelude::Dimension;

pub struct BaseComponentBuilder {
    pub(crate) el: Element,
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
        self.explicit_text_size_px = Some(size.into_f32());
        self
    }

    pub fn font_weight(mut self, weight: impl IntoF32) -> Self {
        let w = weight.into_f32().round().clamp(100.0, 900.0) as u16;
        self.explicit_text_weight = Some(w);
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
    pub fn absolute(mut self) -> Self {
        self.el = self.el.absolute();
        self
    }
    pub fn relative(mut self) -> Self {
        self.el = self.el.relative();
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
}
