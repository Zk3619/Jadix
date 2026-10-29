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

use crate::components::base::SlotChrome;
use crate::core::tree::{parse_css_aspect_ratio, Element, IntoAspectRatioValue};
use crate::primitives::r#box::r#box;
use crate::utils::take_packed;

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
    chrome: SlotChrome,
}

/// Builder entry (ADR-9 surface B).
pub fn aspect_ratio() -> AspectRatioBuilder {
    AspectRatioBuilder {
        ratio: 1.0,
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl AspectRatioBuilder {
    crate::impl_slot_chrome_methods!();

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
        let chrome = builder.chrome;
        chrome.apply(build_aspect_ratio(None, builder.ratio, builder.children))
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

crate::__component_contract! {
AspectRatioProps [base_builder];
/// ShadCN `AspectRatio`：固定宽高比容器。
#[allow(non_snake_case)]
pub fn AspectRatio(
identity [] id: String,
default [] ratio: AspectRatioValue,
base_builder [] base_builder: Option<
        Box<
            dyn FnOnce(
                crate::components::base::BaseComponentBuilder,
            ) -> crate::components::base::BaseComponentBuilder,
        >,
    >,
plain [] children: Option<Element>,
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
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/aspect_ratio.rs"]
mod tests;
