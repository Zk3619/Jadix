//! # Skeleton
//!
//! ShadCN-style loading placeholder. Aligns with
//! `Reference/ShadCN-UI/.../bases/base/ui/skeleton.tsx` — `data-slot="skeleton"`
//! + pulse animation (`cn-skeleton animate-pulse` in v4 registry).
//!
//! Default chrome: `bg-accent` + `rounded-md` + `animate-pulse` (theme-aware muted block).
//! 默认样式：`bg-accent` + `rounded-md` + `animate-pulse`（主题感知灰块 + 呼吸动画）。
//!
//! ## Usage (dual style · ADR-9)
//! ```ignore
//! // A · RSX / tags
//! <Skeleton class="h-4 w-[250px]" />
//! <Skeleton class="h-12 w-12 rounded-full" />
//! <HStack gap="1rem" items="center">
//!     <Skeleton class="h-12 w-12 rounded-full" />
//!     <VStack gap="0.5rem">
//!         <Skeleton class="h-4 w-[250px]" />
//!         <Skeleton class="h-4 w-[200px]" />
//!     </VStack>
//! </HStack>
//!
//! // B · Builder
//! Element::from(skeleton().w("250px").h("1rem"))
//! Element::from(skeleton().w("3rem").h("3rem").class("rounded-full"))
//! ```
//!
//! ## Notes
//! - **Presentational**: empty `id` omits `template_id` (WP-2.8).
//! - `<Skeleton>` uses the generic PascalCase → `SkeletonProps` path (same as Badge / Kbd /
//!   AspectRatio; no special `ui!` scope).
//! - Size / shape come from RSX layout attrs (`w` / `h` / `rounded` / `class`) or builder
//!   helpers; the root is a leaf block (no children slot).
//! - 展示型占位：无受控状态；尺寸与圆角由 `w`/`h`/`rounded`/`class` 或 Builder 链式指定。

use crate::components::base::BaseComponentBuilder;
use crate::core::tree::{parse_color, Element, IntoCssDim, IntoF32};
use crate::primitives::r#box::r#box;
use crate::style::with_class_list;
use crate::tokens;

const ATTR_SLOT: &str = "data-slot";
const SLOT_SKELETON: &str = "skeleton";
const DEFAULT_PULSE_CLASS: &str = "animate-pulse";

fn build_skeleton_core() -> Element {
    let mut el: Element = r#box().into();
    el = el.bg(parse_color("accent")).rounded(tokens::RADIUS_MD);
    el = with_class_list(el, DEFAULT_PULSE_CLASS);
    el.set_attr(ATTR_SLOT, SLOT_SKELETON);
    el
}

pub struct SkeletonBuilder {
    id: String,
    el: Element,
}

/// Builder entry (ADR-9 surface B).
pub fn skeleton() -> SkeletonBuilder {
    SkeletonBuilder {
        id: String::new(),
        el: build_skeleton_core(),
    }
}

impl SkeletonBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn w(mut self, w: impl IntoCssDim) -> Self {
        self.el = self.el.w(w);
        self
    }

    pub fn h(mut self, h: impl IntoCssDim) -> Self {
        self.el = self.el.h(h);
        self
    }

    pub fn min_w(mut self, w: impl IntoCssDim) -> Self {
        self.el = self.el.min_w(w);
        self
    }

    pub fn min_h(mut self, h: impl IntoCssDim) -> Self {
        self.el = self.el.min_h(h);
        self
    }

    pub fn rounded(mut self, r: impl IntoF32) -> Self {
        self.el = self.el.rounded(r);
        self
    }

    pub fn class(mut self, classes: impl AsRef<str>) -> Self {
        self.el = with_class_list(self.el, classes.as_ref());
        self
    }

    pub fn style(
        mut self,
        f: impl Fn(BaseComponentBuilder) -> BaseComponentBuilder + 'static,
    ) -> Self {
        self.el = f(BaseComponentBuilder::new(self.el)).el;
        self
    }

    pub fn bg(mut self, token: impl crate::style::IntoPaintColor) -> Self {
        self.el = self.el.bg(token);
        self
    }
}

impl crate::core::tree::IntoElementList for SkeletonBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<SkeletonBuilder> for Element {
    fn from(builder: SkeletonBuilder) -> Self {
        let mut el = builder.el;
        if !builder.id.trim().is_empty() {
            el.template_id = Some(builder.id);
        }
        el
    }
}

crate::__component_contract! {
SkeletonProps [base_builder];
/// ShadCN `Skeleton`: animated loading placeholder block.
#[allow(non_snake_case)]
pub fn Skeleton(
identity [] id: String,
base_builder [] base_builder: Option<
        Box<
            dyn FnOnce(
                crate::components::base::BaseComponentBuilder,
            ) -> crate::components::base::BaseComponentBuilder,
        >,
    >,
plain [] children: Option<crate::core::tree::Element>,
) -> Element {
    let mut el = build_skeleton_core();
    if !id.trim().is_empty() {
        el.template_id = Some(id);
    }
    if let Some(b) = base_builder {
        let base = crate::components::base::BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}
}

#[cfg(all(test, feature = "product-tests"))]
mod tests {
    use super::*;

    #[test]
    fn skeleton_sets_slot_accent_and_pulse() {
        let el: Element = skeleton().into();
        assert_eq!(
            el.attrs.get(ATTR_SLOT).map(|s| s.as_str()),
            Some(SLOT_SKELETON)
        );
        assert_eq!(el.style.bg_color, parse_color("accent"));
        assert!(
            el.attrs
                .get("jadix_animate")
                .map(|s| s.as_str())
                .is_some_and(|v| v == "pulse")
                || el
                    .attrs
                    .get("jadix_utility_class")
                    .map(|s| s.contains("animate-pulse"))
                    .unwrap_or(false)
        );
    }

    #[test]
    fn skeleton_empty_id_does_not_write_empty_template() {
        let el: Element = skeleton().into();
        assert!(el.template_id.is_none() || el.template_id.as_deref() != Some(""));
    }

    #[test]
    fn builder_applies_size_and_class() {
        let el: Element = skeleton().w("250px").h("1rem").class("rounded-full").into();
        let bare: Element = skeleton().into();
        assert_ne!(el.style.size.width, bare.style.size.width);
        assert_ne!(el.style.size.height, bare.style.size.height);
    }

    #[test]
    fn dual_surface_compiles_shape() {
        let _b: Element = skeleton().w("100px").h("20px").into();
        let el = Skeleton(SkeletonProps {
            id: String::new(),
            base: None,
            ..Default::default()
        });
        let _: Element = el;
    }

    #[test]
    fn builder_style() {
        let el: Element = skeleton().style(|b| b.text_size(20.0)).into();
        fn find_size(el: &Element, want: f32) -> bool {
            el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
                || (el.text_style.size_resolved() - want).abs() < 0.01
                || el.children.iter().any(|c| find_size(c, want))
        }
        assert!(
            find_size(&el, 20.0),
            "skeleton .style(|b| b.text_size(20)) must land on the placeholder"
        );
    }
}
