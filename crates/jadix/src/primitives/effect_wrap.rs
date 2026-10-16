//! B16 subtree effect wrap: capture children → shader → textured quad.
//!
//! Hit-testing stays on the underlying Element tree; this only affects paint.

use crate::core::tree::{Element, IntoElementList};
use crate::renderer::{ATTR_EFFECT, ATTR_EFFECT_CAPTURE_CHILDREN, ATTR_EFFECT_STRENGTH};

pub struct EffectWrapBuilder {
    el: Element,
}

/// Wrap children with a post-process effect (`blur`, `glass`, `vhs`, `liquid`, …).
pub fn effect_wrap(kind: impl Into<String>) -> EffectWrapBuilder {
    let mut el = Element::new();
    el.set_attr(ATTR_EFFECT, kind.into());
    EffectWrapBuilder { el }
}

impl EffectWrapBuilder {
    pub fn strength(mut self, s: f32) -> Self {
        self.el
            .set_attr(ATTR_EFFECT_STRENGTH, format!("{:.3}", s.clamp(0.0, 1.0)));
        self
    }

    pub fn w(mut self, w: impl crate::core::tree::IntoCssDim) -> Self {
        self.el = self.el.w(w);
        self
    }

    pub fn h(mut self, h: impl crate::core::tree::IntoCssDim) -> Self {
        self.el = self.el.h(h);
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
}

impl From<EffectWrapBuilder> for Element {
    fn from(builder: EffectWrapBuilder) -> Self {
        let mut el = builder.el;
        el.set_attr(ATTR_EFFECT_CAPTURE_CHILDREN, "1");
        el
    }
}

impl IntoElementList for EffectWrapBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::layout_tree::LayoutTree;
    use crate::primitives::r#box::r#box;
    use crate::renderer::{take_capture_jobs, take_effect_jobs, EffectKind};

    #[test]
    fn wrap_queues_capture_and_effect() {
        let _ = take_capture_jobs();
        let _ = take_effect_jobs();
        let mut root = effect_wrap("blur")
            .strength(0.7)
            .w(120.0)
            .h(80.0)
            .child(r#box().w(60.0).h(40.0).bg([0.2, 0.5, 1.0, 1.0]))
            .into();
        root = Element::new().w(200.0).h(120.0).child(root);
        let mut tree = LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 200.0, 120.0, [-1.0, -1.0]);
        let caps = take_capture_jobs();
        assert_eq!(caps.len(), 1);
        assert!(!caps[0].instances.is_empty() || !caps[0].textured_draws.is_empty());
        let fxs = take_effect_jobs();
        assert_eq!(fxs.len(), 1);
        assert_eq!(fxs[0].kind, EffectKind::Blur);
        assert!((fxs[0].strength - 0.7).abs() < 1e-3);
    }
}
