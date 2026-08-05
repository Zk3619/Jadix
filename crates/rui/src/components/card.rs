//! # Card
//!
//! 带边框、圆角与语义背景色的垂直卡片容器。间距取自 [`crate::tokens::spacing`]
//!（默认 `p=SPACE_4`、`gap=SPACE_2`、圆角 8px）。
//!
//! ## 用来做什么
//! 把一组相关内容（标题、说明、控件）包成一块视觉单元，无独立 Header/Title 槽。
//!
//! ## 用法
//! ```ignore
//! <Card>
//!   <Label text_size="0.875rem">"标题"</Label>
//!   <Label>"正文说明…"</Label>
//! </Card>
//! ```
//!
//! ## 注意
//! 需要标题时在 `children` 内自行放 [`crate::components::Label`]；更复杂卡片可在应用层自建组合。

use crate::core::tree::{parse_color, Element};
use crate::primitives::vstack;
use crate::tokens;
use rui_macros::component;

/// 卡片容器。见模块文档。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Card(children: Option<Element>) -> Element {
    let child = children.unwrap_or_else(Element::new);
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
