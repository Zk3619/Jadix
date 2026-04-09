//! ShadCN 风格容器：边框 + 圆角 + 语义背景，间距取自 [`crate::tokens::spacing`]。

use crate::core::tree::{parse_color, Element};
use crate::primitives::vstack;
use crate::tokens;
use rui_macros::component;

/// 卡片容器（无标题槽；需要标题时在 `children` 内放 [`crate::components::Label`]）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Card(children: Option<Element>) -> Element {
    let child = children.unwrap_or_else(|| Element::new());
    Element::from(
        vstack()
            .w("100%")
            .p(tokens::SPACE_4)
            .gap(tokens::SPACE_2)
            .bg(parse_color("background"))
            .border_style("solid")
            .border_color(parse_color("border"))
            .border_width(0.125)
            .rounded(8.0)
            .child(child),
    )
}
