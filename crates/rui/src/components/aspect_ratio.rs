//! # AspectRatio
//!
//! ShadCN 风格宽高比容器：对齐
//! `Reference/ShadCN-UI/.../ui/aspect-ratio.tsx` — `relative` + CSS `aspect-ratio`
//!（[`crate::core::tree::parse_css_aspect_ratio`] / MDN）。
//!
//! ## Usage (dual style · ADR-9)
//! ```ignore
//! // A · RSX / tags
//! <AspectRatio ratio={16.0 / 9.0} w="100%">
//!     <Image src="photo.jpg" w="100%" h="100%" />
//! </AspectRatio>
//! <AspectRatio ratio="16/9" class="rounded-md overflow-hidden">
//!     <Image texture="demo" w="100%" h="100%" />
//! </AspectRatio>
//!
//! // B · Builder
//! Element::from(
//!     aspect_ratio()
//!         .ratio(16.0 / 9.0)
//!         .child(Image(ImageProps { src: "…".into(), ..Default::default() }))
//! )
//! // 或直接在 Image / Box 上写属性（宏 → BaseComponentBuilder）
//! <Image src="photo.jpg" w="12rem" aspect_ratio="16/9" />
//! <Box w="12rem" aspect-ratio="1 / 1" bg="muted" />
//! ```
//!
//! ## Notes
//! - **Presentational**: empty `id` omits `template_id` (WP-2.8).
//! - 容器设 `w`（或父级定宽）且另一轴为 auto 时，比例才会约束盒子（MDN）。
//! - 子节点宜 `w/h=100%`（或 absolute 铺满）以填满裁剪区；组件会把子节点铺满。

use crate::core::tree::{parse_css_aspect_ratio, Element, IntoAspectRatioValue};
use crate::primitives::r#box::r#box;
use crate::utils::take_packed;
use rui_macros::component;

const ATTR_SLOT: &str = "data-slot";
const SLOT_ASPECT_RATIO: &str = "aspect-ratio";

/// `ratio` 属性：数字或 CSS 字符串（`"16/9"`、`"auto"`…）。缺省为 `1`（正方形）。
#[derive(Clone, Debug)]
pub struct AspectRatioValue(pub Option<f32>);

impl Default for AspectRatioValue {
    fn default() -> Self {
        Self(Some(1.0))
    }
}

impl From<f32> for AspectRatioValue {
    fn from(v: f32) -> Self {
        Self(v.into_aspect_ratio_value())
    }
}

impl From<f64> for AspectRatioValue {
    fn from(v: f64) -> Self {
        Self((v as f32).into_aspect_ratio_value())
    }
}

impl From<i32> for AspectRatioValue {
    fn from(v: i32) -> Self {
        Self((v as f32).into_aspect_ratio_value())
    }
}

impl From<&str> for AspectRatioValue {
    fn from(s: &str) -> Self {
        Self(parse_css_aspect_ratio(s).or_else(|| {
            // 缺省 prop 空串 → 1:1；显式 "auto" 已由 parse 得 None。
            if s.trim().is_empty() {
                Some(1.0)
            } else {
                None
            }
        }))
    }
}

impl From<String> for AspectRatioValue {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}
pub struct AspectRatioBuilder {
    ratio: f32,
    children: Vec<Element>,
}

/// Builder entry (ADR-9 surface B).
pub fn aspect_ratio() -> AspectRatioBuilder {
    AspectRatioBuilder {
        ratio: 1.0,
        children: Vec::new(),
    }
}

impl AspectRatioBuilder {
    pub fn ratio(mut self, ratio: impl IntoAspectRatioValue) -> Self {
        if let Some(r) = ratio.into_aspect_ratio_value() {
            self.ratio = r;
        }
        self
    }

    pub fn child(mut self, c: Element) -> Self {
        self.children.push(c);
        self
    }

    pub fn children(mut self, c: Vec<Element>) -> Self {
        self.children.extend(c);
        self
    }
}

impl crate::core::tree::IntoElementList for AspectRatioBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<AspectRatioBuilder> for Element {
    fn from(builder: AspectRatioBuilder) -> Self {
        build_aspect_ratio(None, builder.ratio, builder.children)
    }
}

fn fill_child(child: Element) -> Element {
    child.absolute().inset(0.0).w("100%").h("100%")
}

fn build_aspect_ratio(id: Option<String>, ratio: f32, children: Vec<Element>) -> Element {
    let ratio = if ratio.is_finite() && ratio > 0.0 {
        ratio
    } else {
        1.0
    };
    let mut el: Element = r#box().w("100%").into();
    el = el.relative().overflow("hidden").aspect_ratio(ratio);
    if let Some(id) = id.filter(|s| !s.is_empty()) {
        el.template_id = Some(id);
    }
    el.set_attr(ATTR_SLOT, SLOT_ASPECT_RATIO);
    for c in children {
        el = el.child(fill_child(c));
    }
    el
}

/// ShadCN `AspectRatio`：固定宽高比容器。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AspectRatio(
    id: String,
    ratio: AspectRatioValue,
    base_builder: Option<
        Box<
            dyn FnOnce(
                crate::components::base::BaseComponentBuilder,
            ) -> crate::components::base::BaseComponentBuilder,
        >,
    >,
    children: Option<Element>,
) -> Element {
    let r = ratio.0.filter(|v| v.is_finite() && *v > 0.0).unwrap_or(1.0);
    let kids = match children {
        Some(c) => take_packed(c),
        None => Vec::new(),
    };
    let mut el = build_aspect_ratio(Some(id), r, kids);
    if let Some(b) = base_builder {
        let base = crate::components::base::BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tree::parse_css_aspect_ratio;

    #[test]
    fn parse_mdn_aspect_ratio_values() {
        assert_eq!(parse_css_aspect_ratio("auto"), None);
        assert!((parse_css_aspect_ratio("1 / 1").unwrap() - 1.0).abs() < f32::EPSILON);
        assert!((parse_css_aspect_ratio("16/9").unwrap() - 16.0 / 9.0).abs() < 0.001);
        assert!((parse_css_aspect_ratio("0.5").unwrap() - 0.5).abs() < f32::EPSILON);
        assert!((parse_css_aspect_ratio("auto 3/4").unwrap() - 0.75).abs() < f32::EPSILON);
        assert!((parse_css_aspect_ratio("9/6 auto").unwrap() - 1.5).abs() < f32::EPSILON);
    }

    #[test]
    fn builder_sets_slot_and_ratio() {
        let el: Element = aspect_ratio().ratio(16.0 / 9.0).into();
        assert_eq!(
            el.attrs.get(ATTR_SLOT).map(|s| s.as_str()),
            Some(SLOT_ASPECT_RATIO)
        );
        assert!((el.style.aspect_ratio.unwrap() - 16.0 / 9.0).abs() < 0.001);
    }

    #[test]
    fn dual_surface_compiles_shape() {
        let _b: Element = aspect_ratio().ratio("16/9").into();
        let el = AspectRatio(AspectRatioProps {
            id: String::new(),
            ratio: Some(AspectRatioValue::from("1/1")),
            children: None,
            base: None,
            ..Default::default()
        });
        let _: Element = el;
    }
}
