//! # Card
//!
//! 对齐 solid-ui / shadcn **复合 Card**：
//!
//! ```text
//! Card
//! ├── CardHeader
//! │   ├── CardTitle
//! │   ├── CardDescription
//! │   └── CardAction          // 可选；Header 右侧动作区（shadcn v4）
//! ├── CardContent
//! └── CardFooter
//! ```
//!
//! Root container: `bg-card`, borders, rounded corners, light shadow, and `py` + interval `gap` (shadcn v4).
//! 分区只加水平 `px`；纯子节点也能吃到根上的垂直内边距。
//!
//! `<Card>` / `<CardHeader>` in `ui!` are expanded by general PascalCase → `*Props` (with
//! [`DialogHeader`](crate::components::DialogHeader) is the same, no dedicated scope is required).
//!
//! ## Usage
//! ```ignore
//! ui! {
//!     <Card>
//!         <CardHeader>
//!             <CardTitle>"Notifications"</CardTitle>
//!             <CardDescription>"You have 3 unread messages."</CardDescription>
//!             <CardAction>
//!                 <Button variant="link" size="sm">"Sign Up"</Button>
//!             </CardAction>
//!         </CardHeader>
//!         <CardContent>
//!             <Label>"Card body…"</Label>
//!         </CardContent>
//!         <CardFooter>
//!             <Button w="100%">"Mark all as read"</Button>
//!         </CardFooter>
//!     </Card>
//! }
//! ```

use crate::core::tree::{parse_color, Element};
use crate::primitives::r#box::{hstack, vstack};
use crate::primitives::text::text;
use crate::style::parse_text_color;
use crate::tokens;
use crate::utils::take_packed;
use rui_macros::component;

const ATTR_CARD_SLOT: &str = "rui_card_slot";
const SLOT_ACTION: &str = "action";
fn is_card_action(el: &Element) -> bool {
    el.attrs
        .get(ATTR_CARD_SLOT)
        .map(|s| s.as_str() == SLOT_ACTION)
        .unwrap_or(false)
}

/// 卡片根容器（边框 / 圆角 / `bg-card` / 轻阴影）。见模块文档。
///
/// 默认 `w/max_w: 100%` + `min_w: 0`，避免 flex 子项因内容固有宽撑破父级。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Card(children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    Element::from(
        vstack()
            .w("100%")
            .max_w("100%")
            .min_w(0.0)
            .gap(tokens::SPACE_6)
            .py(tokens::SPACE_6)
            .bg(parse_color("card"))
            .border_style("solid")
            .border_color(parse_color("border"))
            .border_width(1.0)
            .rounded(tokens::RADIUS_LG)
            .shadow([0.0, 0.0, 0.0, 0.08], 4.0)
            .children(kids),
    )
}

/// 顶栏：标题 / 说明；若含 [`CardAction`] 则右侧并排放置。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn CardHeader(children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let (actions, rest): (Vec<_>, Vec<_>) = kids.into_iter().partition(|c| is_card_action(c));

    if actions.is_empty() {
        return Element::from(
            vstack()
                .w("100%")
                .max_w("100%")
                .min_w(0.0)
                .gap("0.375rem")
                .px(tokens::SPACE_6)
                .children(rest),
        );
    }

    // Don't use `w(100%)` when side by side with Action: percentage width + side button will make the Header burst out of the parent.
    let title_block: Element = vstack()
        .gap("0.375rem")
        .flex_grow(1.0)
        .flex_shrink(1.0)
        .min_w(0.0)
        .children(rest)
        .into();

    Element::from(
        hstack()
            .w("100%")
            .max_w("100%")
            .min_w(0.0)
            .items("start")
            .justify("between")
            .gap(tokens::SPACE_4)
            .px(tokens::SPACE_6)
            .child(title_block)
            .children(actions),
    )
}

/// 标题（`text-lg` / semibold / card-foreground）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn CardTitle(children: Option<Element>) -> Element {
    let fg = parse_text_color("card-foreground");
    match children {
        Some(c) => c
            .text_size(18.0)
            .font_weight(600.0)
            .text_color(fg)
            .min_w(0.0),
        None => Element::from(text("").size(18.0).weight(600.0).color(fg)).min_w(0.0),
    }
}

/// 说明文案（`text-sm` / muted-foreground）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn CardDescription(children: Option<Element>) -> Element {
    let muted = parse_text_color("muted-foreground");
    match children {
        Some(c) => c
            .text_size(14.0)
            .font_weight(400.0)
            .text_color(muted)
            .min_w(0.0),
        None => Element::from(text("").size(14.0).weight(400.0).color(muted)).min_w(0.0),
    }
}

/// Header 右侧动作区（对齐 shadcn `CardAction`）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn CardAction(children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = hstack()
        .items_center()
        .gap(tokens::SPACE_2)
        .flex_shrink(0.0)
        .children(kids)
        .into();
    el.set_attr(ATTR_CARD_SLOT, SLOT_ACTION);
    el
}

/// Main content area (horizontal `px`; vertical spacing is taken by the `gap` / `py` of the root [`Card`]).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn CardContent(children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    Element::from(
        vstack()
            .w("100%")
            .max_w("100%")
            .min_w(0.0)
            .gap(tokens::SPACE_4)
            .px(tokens::SPACE_6)
            .children(kids),
    )
}

/// 底栏（横向；默认左对齐，可用 `class` / `justify` 覆盖）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn CardFooter(children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    Element::from(
        hstack()
            .w("100%")
            .max_w("100%")
            .min_w(0.0)
            .items_center()
            .gap(tokens::SPACE_2)
            .px(tokens::SPACE_6)
            .children(kids),
    )
}
