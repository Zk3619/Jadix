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

use crate::components::base::{packed_children, stamp_slot, SlotChrome};
use crate::core::tree::{parse_color, Element};
use crate::impl_slot_chrome_methods;
use crate::primitives::r#box::{hstack, vstack};
use crate::primitives::text::text;
use crate::style::parse_text_color;
use crate::tokens;
use crate::utils::take_packed;

const ATTR_SLOT: &str = "data-slot";
const SLOT_TITLE: &str = "card-title";
const SLOT_DESCRIPTION: &str = "card-description";
const ATTR_CARD_SLOT: &str = "jadix_card_slot";
const SLOT_ACTION: &str = "action";
fn is_card_action(el: &Element) -> bool {
    el.attrs
        .get(ATTR_CARD_SLOT)
        .map(|s| s.as_str() == SLOT_ACTION)
        .unwrap_or(false)
}

crate::__component_contract! {
CardProps [];
/// 卡片根容器（边框 / 圆角 / `bg-card` / 轻阴影）。见模块文档。
///
/// 默认 `w/max_w: 100%` + `min_w: 0`，避免 flex 子项因内容固有宽撑破父级。
#[allow(non_snake_case)]
pub fn Card(
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    stamp_slot(Element::from(
        vstack()
            .w("100%")
            .max_w("100%")
            .min_w(0.0)
            .flex_shrink(0.0)
            .gap(tokens::SPACE_6)
            .py(tokens::SPACE_6)
            .bg(parse_color("card"))
            .border_style("solid")
            .border_color(parse_color("border"))
            .border_width(1.0)
            .rounded(tokens::RADIUS_LG)
            .shadow(tokens::shadow_rgba(tokens::Elevation::Sm), 4.0)
            .children(kids),
    ), "card")
}
}

crate::__component_contract! {
CardHeaderProps [];
/// 顶栏：标题 / 说明；若含 [`CardAction`] 则右侧并排放置。
#[allow(non_snake_case)]
pub fn CardHeader(
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let (actions, rest): (Vec<_>, Vec<_>) = kids.into_iter().partition(|c| is_card_action(c));

    if actions.is_empty() {
        return stamp_slot(Element::from(
            vstack()
                .w("100%")
                .max_w("100%")
                .min_w(0.0)
                .gap("0.375rem")
                .px(tokens::SPACE_6)
                .children(rest),
        ), "card-header");
    }

    // Don't use `w(100%)` when side by side with Action: percentage width + side button will make the Header burst out of the parent.
    let title_block: Element = vstack()
        .gap("0.375rem")
        .flex_grow(1.0)
        .flex_shrink(1.0)
        .min_w(0.0)
        .children(rest)
        .into();

    stamp_slot(Element::from(
        hstack()
            .w("100%")
            .max_w("100%")
            .min_w(0.0)
            .items("start")
            .justify("between")
            .gap(tokens::SPACE_4)
            .px(tokens::SPACE_6)
            .wrap("wrap")
            .child(title_block)
            .children(actions),
    ), "card-header")
}
}

crate::__component_contract! {
CardTitleProps [];
/// 标题（`text-lg` / semibold / card-foreground）。
#[allow(non_snake_case)]
pub fn CardTitle(
plain [] children: Option<Element>,
) -> Element {
    let fg = parse_text_color("card-foreground");
    let body = match children {
        Some(c) => c
            .text_size(18.0)
            .font_weight(600.0)
            .text_color(fg)
            .min_w(0.0),
        None => Element::from(text("").size(18.0).weight(600.0).color(fg)).min_w(0.0),
    };
    let mut el: Element = vstack().w("100%").min_w(0.0).child(body).into();
    el.set_attr(ATTR_SLOT, SLOT_TITLE);
    el
}
}

crate::__component_contract! {
CardDescriptionProps [];
/// 说明文案（`text-sm` / muted-foreground）。
#[allow(non_snake_case)]
pub fn CardDescription(
plain [] children: Option<Element>,
) -> Element {
    let muted = parse_text_color("muted-foreground");
    let body = match children {
        Some(c) => c
            .text_size(14.0)
            .font_weight(400.0)
            .text_color(muted)
            .min_w(0.0),
        None => Element::from(text("").size(14.0).weight(400.0).color(muted)).min_w(0.0),
    };
    let mut el: Element = vstack().w("100%").min_w(0.0).child(body).into();
    el.set_attr(ATTR_SLOT, SLOT_DESCRIPTION);
    el
}
}

crate::__component_contract! {
CardActionProps [];
/// Header 右侧动作区（对齐 shadcn `CardAction`）。
#[allow(non_snake_case)]
pub fn CardAction(
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = hstack()
        .items_center()
        .min_w(0.0)
        .max_w("100%")
        .wrap("wrap")
        .gap(tokens::SPACE_2)
        .flex_shrink(0.0)
        .children(kids)
        .into();
    el.set_attr(ATTR_CARD_SLOT, SLOT_ACTION);
    el
}
}

crate::__component_contract! {
CardContentProps [];
/// Main content area (horizontal `px`; vertical spacing is taken by the `gap` / `py` of the root [`Card`]).
#[allow(non_snake_case)]
pub fn CardContent(
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    stamp_slot(Element::from(
        vstack()
            .w("100%")
            .max_w("100%")
            .min_w(0.0)
            .gap(tokens::SPACE_4)
            .px(tokens::SPACE_6)
            .children(kids),
    ), "card-content")
}
}

crate::__component_contract! {
CardFooterProps [];
/// 底栏（横向换行；默认左对齐，可用 `class` / `justify` 覆盖）。
#[allow(non_snake_case)]
pub fn CardFooter(
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    stamp_slot(Element::from(
        hstack()
            .w("100%")
            .max_w("100%")
            .min_w(0.0)
            .items_center()
            .wrap("wrap")
            .gap(tokens::SPACE_2)
            .px(tokens::SPACE_6)
            .children(kids),
    ), "card-footer")
}
}

// ---------------------------------------------------------------------------
// Builder (ADR-9 surface B)
// ---------------------------------------------------------------------------

pub struct CardTitleBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn card_title() -> CardTitleBuilder {
    CardTitleBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl CardTitleBuilder {
    impl_slot_chrome_methods!();

    pub fn text(mut self, content: impl Into<String>) -> Self {
        self.children.push(Element::new().text(content.into()));
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }
}

impl crate::core::tree::IntoElementList for CardTitleBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<CardTitleBuilder> for Element {
    fn from(builder: CardTitleBuilder) -> Self {
        let el = CardTitle(CardTitleProps {
            children: packed_children(builder.children),
            base: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct CardDescriptionBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn card_description() -> CardDescriptionBuilder {
    CardDescriptionBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl CardDescriptionBuilder {
    impl_slot_chrome_methods!();

    pub fn text(mut self, content: impl Into<String>) -> Self {
        self.children.push(Element::new().text(content.into()));
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }
}

impl crate::core::tree::IntoElementList for CardDescriptionBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<CardDescriptionBuilder> for Element {
    fn from(builder: CardDescriptionBuilder) -> Self {
        let el = CardDescription(CardDescriptionProps {
            children: packed_children(builder.children),
            base: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct CardActionBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn card_action() -> CardActionBuilder {
    CardActionBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl CardActionBuilder {
    impl_slot_chrome_methods!();

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }
}

impl crate::core::tree::IntoElementList for CardActionBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<CardActionBuilder> for Element {
    fn from(builder: CardActionBuilder) -> Self {
        let el = CardAction(CardActionProps {
            children: packed_children(builder.children),
            base: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct CardHeaderBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn card_header() -> CardHeaderBuilder {
    CardHeaderBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl CardHeaderBuilder {
    impl_slot_chrome_methods!();

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }

    pub fn title(mut self, text: impl Into<String>) -> Self {
        self.children.push(card_title().text(text).into());
        self
    }

    pub fn title_with(mut self, f: impl FnOnce(CardTitleBuilder) -> CardTitleBuilder) -> Self {
        self.children.push(f(card_title()).into());
        self
    }

    pub fn description(mut self, text: impl Into<String>) -> Self {
        self.children.push(card_description().text(text).into());
        self
    }

    pub fn description_with(
        mut self,
        f: impl FnOnce(CardDescriptionBuilder) -> CardDescriptionBuilder,
    ) -> Self {
        self.children.push(f(card_description()).into());
        self
    }

    pub fn action(mut self, child: impl Into<Element>) -> Self {
        let el = child.into();
        let el = if is_card_action(&el) {
            el
        } else {
            card_action().child(el).into()
        };
        self.children.push(el);
        self
    }

    pub fn action_with(mut self, f: impl FnOnce(CardActionBuilder) -> CardActionBuilder) -> Self {
        self.children.push(f(card_action()).into());
        self
    }
}

impl crate::core::tree::IntoElementList for CardHeaderBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<CardHeaderBuilder> for Element {
    fn from(builder: CardHeaderBuilder) -> Self {
        let el = CardHeader(CardHeaderProps {
            children: packed_children(builder.children),
            base: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct CardContentBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn card_content() -> CardContentBuilder {
    CardContentBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl CardContentBuilder {
    impl_slot_chrome_methods!();

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }
}

impl crate::core::tree::IntoElementList for CardContentBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<CardContentBuilder> for Element {
    fn from(builder: CardContentBuilder) -> Self {
        let el = CardContent(CardContentProps {
            children: packed_children(builder.children),
            base: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct CardFooterBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn card_footer() -> CardFooterBuilder {
    CardFooterBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl CardFooterBuilder {
    impl_slot_chrome_methods!();

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }
}

impl crate::core::tree::IntoElementList for CardFooterBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<CardFooterBuilder> for Element {
    fn from(builder: CardFooterBuilder) -> Self {
        let el = CardFooter(CardFooterProps {
            children: packed_children(builder.children),
            base: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct CardBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn card() -> CardBuilder {
    CardBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl CardBuilder {
    impl_slot_chrome_methods!();

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }

    pub fn header(mut self, header: impl Into<Element>) -> Self {
        self.children.push(header.into());
        self
    }

    pub fn header_with(mut self, f: impl FnOnce(CardHeaderBuilder) -> CardHeaderBuilder) -> Self {
        self.children.push(f(card_header()).into());
        self
    }

    pub fn content(mut self, child: impl Into<Element>) -> Self {
        self.children.push(card_content().child(child).into());
        self
    }

    pub fn content_with(
        mut self,
        f: impl FnOnce(CardContentBuilder) -> CardContentBuilder,
    ) -> Self {
        self.children.push(f(card_content()).into());
        self
    }

    pub fn footer(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }

    pub fn footer_with(mut self, f: impl FnOnce(CardFooterBuilder) -> CardFooterBuilder) -> Self {
        self.children.push(f(card_footer()).into());
        self
    }
}

impl crate::core::tree::IntoElementList for CardBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<CardBuilder> for Element {
    fn from(builder: CardBuilder) -> Self {
        let el = Card(CardProps {
            children: packed_children(builder.children),
            base: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/card.rs"]
mod tests;
