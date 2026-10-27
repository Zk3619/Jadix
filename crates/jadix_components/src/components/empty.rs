//! # Empty
//!
//! ShadCN-style empty state shell. Aligns with
//! `Reference/ShadCN-UI/.../bases/base/ui/empty.tsx`.
//!
//! ```text
//! Empty                         // flex · centered column · text-center
//! ├── EmptyHeader               // max-w-sm · items-center
//! │   ├── EmptyMedia            // icon | default media slot
//! │   ├── EmptyTitle
//! │   └── EmptyDescription
//! └── EmptyContent              // actions / secondary copy
//! ```
//!
//! ## Usage (dual style · ADR-9)
//! ```ignore
//! // A · RSX / tags
//! <Empty>
//!     <EmptyHeader>
//!         <EmptyMedia variant="icon">
//!             <Icon name="inbox" />
//!         </EmptyMedia>
//!         <EmptyTitle>"No Projects Yet"</EmptyTitle>
//!         <EmptyDescription>"You have not created a project yet."</EmptyDescription>
//!     </EmptyHeader>
//!     <EmptyContent>
//!         <Button>"Create Project"</Button>
//!     </EmptyContent>
//! </Empty>
//!
//! // B · Builder
//! Element::from(
//!     empty()
//!         .header(
//!             empty_header()
//!                 .media_icon(icon_child)
//!                 .title("No Projects Yet")
//!                 .description("You have not created a project yet.")
//!         )
//!         .content_child(button("").text("Create Project"))
//! )
//! ```
//!
//! ## Notes
//! - **Presentational**: empty `id` omits `template_id` (WP-2.8).
//! - `<Empty>` / `EmptyHeader` / `EmptyMedia` / … use the generic PascalCase → `*Props`
//!   path (same as Card / Breadcrumb; no special `ui!` scope).
//! - `EmptyMedia variant="icon"` applies muted rounded icon chrome; `default` is transparent.
//! - 展示型空状态：无受控状态；标题/说明为子节点或 Builder 文案 helper。

use crate::components::base::{packed_children, BaseComponentBuilder, SlotChrome};
use crate::core::tree::{parse_color, parse_text_color, Element, IntoF32};
use crate::impl_slot_chrome_methods;
use crate::primitives::r#box::{hstack, vstack};
use crate::tokens;
use crate::utils::take_packed;
use crate::utils::text_node::{build_text_node, TextNodeConfig};

const ATTR_SLOT: &str = "data-slot";
const ATTR_VARIANT: &str = "data-variant";
const SLOT_ROOT: &str = "empty";
const SLOT_HEADER: &str = "empty-header";
const SLOT_MEDIA: &str = "empty-icon";
const SLOT_TITLE: &str = "empty-title";
const SLOT_DESCRIPTION: &str = "empty-description";
const SLOT_CONTENT: &str = "empty-content";

const MAX_W_SM: &str = "24rem";
const TITLE_SIZE_PX: f32 = 16.0;
const TITLE_WEIGHT: u16 = 600;
const DESCRIPTION_SIZE_PX: f32 = 14.0;
const DESCRIPTION_WEIGHT: u16 = 400;
const MEDIA_ICON_BOX: &str = "2.5rem";
const MEDIA_ICON_GLYPH: &str = "1.5rem";

/// `EmptyMedia` chrome: `default` (transparent) | `icon` (muted rounded box).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EmptyMediaVariant {
    #[default]
    Default,
    Icon,
}

impl EmptyMediaVariant {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Icon => "icon",
        }
    }
}

impl From<&str> for EmptyMediaVariant {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or(Self::Default)
    }
}

impl From<String> for EmptyMediaVariant {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl std::str::FromStr for EmptyMediaVariant {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "icon" => Self::Icon,
            _ => Self::Default,
        })
    }
}

fn title_text_config() -> TextNodeConfig {
    TextNodeConfig {
        size_px: TITLE_SIZE_PX,
        weight: TITLE_WEIGHT,
        color: parse_text_color("foreground"),
        text_align: crate::style::TextAlign::Center,
        ..Default::default()
    }
}

fn description_text_config() -> TextNodeConfig {
    TextNodeConfig {
        size_px: DESCRIPTION_SIZE_PX,
        weight: DESCRIPTION_WEIGHT,
        color: parse_text_color("muted-foreground"),
        text_align: crate::style::TextAlign::Center,
        ..Default::default()
    }
}

fn caption_from_children(children: Option<Element>, cfg: &TextNodeConfig) -> Vec<Element> {
    let Some(c) = children else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for mut child in take_packed(c) {
        if let Some(text) = child.text.take() {
            out.push(build_text_node(text, cfg.clone()));
        } else {
            child.text_style.color = Some(cfg.color);
            child.text_style.size = Some(cfg.size_px);
            child.text_style.weight = Some(cfg.weight);
            out.push(child);
        }
    }
    out
}

fn paint_media_children(children: Vec<Element>, variant: EmptyMediaVariant) -> Vec<Element> {
    let fg = parse_text_color("foreground");
    children
        .into_iter()
        .map(|mut child| {
            child = child.pointer_events_none();
            if variant == EmptyMediaVariant::Icon
                && child.text_style.font_family.as_deref() == Some("lucide")
                && child.text_style.size_resolved() <= 1.0
            {
                child = child.text_size(IntoF32::into_f32(MEDIA_ICON_GLYPH));
            }
            if child.text.is_some() {
                child.text_style.color = Some(fg);
            }
            child
        })
        .collect()
}

fn build_empty_media(variant: EmptyMediaVariant, children: Vec<Element>) -> Element {
    let kids = paint_media_children(children, variant);
    let mut shell = hstack()
        .items_center()
        .justify_center()
        .flex_shrink(0.0)
        .mb(tokens::SPACE_2);
    if variant == EmptyMediaVariant::Icon {
        shell = shell
            .w(MEDIA_ICON_BOX)
            .h(MEDIA_ICON_BOX)
            .min_w(MEDIA_ICON_BOX)
            .min_h(MEDIA_ICON_BOX)
            .bg(parse_color("muted"))
            .rounded(tokens::RADIUS_LG);
    }
    let mut el: Element = shell.children(kids).into();
    el.set_attr(ATTR_SLOT, SLOT_MEDIA);
    el.set_attr(ATTR_VARIANT, variant.as_str());
    el
}

// ---------------------------------------------------------------------------
// RSX parts
// ---------------------------------------------------------------------------

crate::__component_contract! {
EmptyProps [base_builder];
/// Root empty-state column (`flex-1` · centered · `text-center`).
#[allow(non_snake_case)]
pub fn Empty(
identity [] id: String,
base_builder [] base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = vstack()
        .w("100%")
        .min_w(0.0)
        .flex_grow(1.0)
        .items_center()
        .justify_center()
        .gap(tokens::SPACE_4)
        .children(kids)
        .into();
    el = el.text_center();
    el.set_attr(ATTR_SLOT, SLOT_ROOT);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    if !id.trim().is_empty() {
        el.template_id = Some(id);
    }
    el
}
}

crate::__component_contract! {
EmptyHeaderProps [base_builder];
/// Header cluster: media + title + description (`max-w-sm` · centered).
#[allow(non_snake_case)]
pub fn EmptyHeader(
base_builder [] base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = vstack()
        .w("100%")
        .max_w(MAX_W_SM)
        .min_w(0.0)
        .items_center()
        .gap(tokens::SPACE_2)
        .children(kids)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_HEADER);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}
}

crate::__component_contract! {
EmptyMediaProps [base_builder];
/// Media slot for icon / image / avatar (`variant`: `default` | `icon`).
#[allow(non_snake_case)]
pub fn EmptyMedia(
default [] variant: EmptyMediaVariant,
base_builder [] base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el = build_empty_media(variant, kids);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}
}

crate::__component_contract! {
EmptyTitleProps [base_builder];
/// Title line (`font-semibold` / heading-sized).
#[allow(non_snake_case)]
pub fn EmptyTitle(
base_builder [] base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
plain [] children: Option<Element>,
) -> Element {
    let cfg = title_text_config();
    let kids = caption_from_children(children, &cfg);
    let mut el: Element = vstack()
        .w("100%")
        .min_w(0.0)
        .items_center()
        .children(kids)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_TITLE);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}
}

crate::__component_contract! {
EmptyDescriptionProps [base_builder];
/// Muted description copy (`text-sm` · `text-muted-foreground`).
#[allow(non_snake_case)]
pub fn EmptyDescription(
base_builder [] base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
plain [] children: Option<Element>,
) -> Element {
    let cfg = description_text_config();
    let kids = caption_from_children(children, &cfg);
    let mut el: Element = vstack()
        .w("100%")
        .min_w(0.0)
        .items_center()
        .children(kids)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_DESCRIPTION);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}
}

crate::__component_contract! {
EmptyContentProps [base_builder];
/// Action / secondary content region (`max-w-sm` · centered column).
#[allow(non_snake_case)]
pub fn EmptyContent(
base_builder [] base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = vstack()
        .w("100%")
        .max_w(MAX_W_SM)
        .min_w(0.0)
        .items_center()
        .gap(tokens::SPACE_2)
        .children(kids)
        .into();
    el = el.text_center();
    el.set_attr(ATTR_SLOT, SLOT_CONTENT);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}
}

// ---------------------------------------------------------------------------
// Builder (ADR-9 surface B)
// ---------------------------------------------------------------------------

pub struct EmptyHeaderBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn empty_header() -> EmptyHeaderBuilder {
    EmptyHeaderBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl EmptyHeaderBuilder {
    impl_slot_chrome_methods!();

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }

    pub fn media(mut self, child: impl Into<Element>) -> Self {
        self.children.push(empty_media().child(child).into());
        self
    }

    pub fn media_icon(mut self, child: impl Into<Element>) -> Self {
        self.children.push(empty_media().icon().child(child).into());
        self
    }

    pub fn media_with(mut self, f: impl FnOnce(EmptyMediaBuilder) -> EmptyMediaBuilder) -> Self {
        self.children.push(f(empty_media()).into());
        self
    }

    pub fn title(mut self, text: impl Into<String>) -> Self {
        self.children.push(empty_title().text(text).into());
        self
    }

    pub fn title_with(mut self, f: impl FnOnce(EmptyTitleBuilder) -> EmptyTitleBuilder) -> Self {
        self.children.push(f(empty_title()).into());
        self
    }

    pub fn description(mut self, text: impl Into<String>) -> Self {
        self.children.push(empty_description().text(text).into());
        self
    }

    pub fn description_with(
        mut self,
        f: impl FnOnce(EmptyDescriptionBuilder) -> EmptyDescriptionBuilder,
    ) -> Self {
        self.children.push(f(empty_description()).into());
        self
    }
}

impl From<EmptyHeaderBuilder> for Element {
    fn from(builder: EmptyHeaderBuilder) -> Self {
        let el = EmptyHeader(EmptyHeaderProps {
            children: packed_children(builder.children),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct EmptyTitleBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn empty_title() -> EmptyTitleBuilder {
    EmptyTitleBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl EmptyTitleBuilder {
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

impl From<EmptyTitleBuilder> for Element {
    fn from(builder: EmptyTitleBuilder) -> Self {
        let el = EmptyTitle(EmptyTitleProps {
            children: packed_children(builder.children),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct EmptyDescriptionBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn empty_description() -> EmptyDescriptionBuilder {
    EmptyDescriptionBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl EmptyDescriptionBuilder {
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

impl From<EmptyDescriptionBuilder> for Element {
    fn from(builder: EmptyDescriptionBuilder) -> Self {
        let el = EmptyDescription(EmptyDescriptionProps {
            children: packed_children(builder.children),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct EmptyContentBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn empty_content() -> EmptyContentBuilder {
    EmptyContentBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl EmptyContentBuilder {
    impl_slot_chrome_methods!();

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }
}

impl From<EmptyContentBuilder> for Element {
    fn from(builder: EmptyContentBuilder) -> Self {
        let el = EmptyContent(EmptyContentProps {
            children: packed_children(builder.children),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct EmptyBuilder {
    id: String,
    chrome: SlotChrome,
    children: Vec<Element>,
}

/// Builder entry for [`Empty`].
pub fn empty() -> EmptyBuilder {
    EmptyBuilder {
        id: String::new(),
        chrome: SlotChrome::default(),
        children: Vec::new(),
    }
}

impl EmptyBuilder {
    impl_slot_chrome_methods!();

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }

    pub fn header(mut self, header: impl Into<Element>) -> Self {
        self.children.push(header.into());
        self
    }

    pub fn header_with(mut self, f: impl FnOnce(EmptyHeaderBuilder) -> EmptyHeaderBuilder) -> Self {
        self.children.push(f(empty_header()).into());
        self
    }

    pub fn content(mut self, content: impl Into<Element>) -> Self {
        self.children.push(content.into());
        self
    }

    pub fn content_with(
        mut self,
        f: impl FnOnce(EmptyContentBuilder) -> EmptyContentBuilder,
    ) -> Self {
        self.children.push(f(empty_content()).into());
        self
    }

    pub fn content_child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(empty_content().child(child).into());
        self
    }
}

impl crate::core::tree::IntoElementList for EmptyBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<EmptyBuilder> for Element {
    fn from(builder: EmptyBuilder) -> Self {
        let el = Empty(EmptyProps {
            id: builder.id,
            children: packed_children(builder.children),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct EmptyMediaBuilder {
    variant: EmptyMediaVariant,
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn empty_media() -> EmptyMediaBuilder {
    EmptyMediaBuilder {
        variant: EmptyMediaVariant::Default,
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl EmptyMediaBuilder {
    impl_slot_chrome_methods!();

    pub fn variant(mut self, variant: impl Into<EmptyMediaVariant>) -> Self {
        self.variant = variant.into();
        self
    }

    pub fn icon(mut self) -> Self {
        self.variant = EmptyMediaVariant::Icon;
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }
}

impl From<EmptyMediaBuilder> for Element {
    fn from(builder: EmptyMediaBuilder) -> Self {
        let el = EmptyMedia(EmptyMediaProps {
            variant: Some(builder.variant),
            children: packed_children(builder.children),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

#[cfg(all(test, feature = "product-tests"))]
mod tests {
    use super::*;

    #[test]
    fn empty_root_slot_and_layout() {
        let el = Empty(EmptyProps {
            id: String::new(),
            children: None,
            base: None,
            base_builder: None,
            ..Default::default()
        });
        assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
        assert!(el.template_id.is_none());
    }

    #[test]
    fn empty_media_icon_variant_chrome() {
        let el = EmptyMedia(EmptyMediaProps {
            variant: Some(EmptyMediaVariant::Icon),
            children: Some(Element::new().text("inbox")),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        assert_eq!(
            el.attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_MEDIA)
        );
        assert_eq!(el.attrs.get(ATTR_VARIANT).map(String::as_str), Some("icon"));
        assert_eq!(el.style.bg_color, parse_color("muted"));
    }

    #[test]
    fn empty_title_and_description_slots() {
        let title = EmptyTitle(EmptyTitleProps {
            children: Some(Element::new().text("Title")),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        let desc = EmptyDescription(EmptyDescriptionProps {
            children: Some(Element::new().text("Desc")),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        assert_eq!(
            title.attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_TITLE)
        );
        assert_eq!(
            desc.attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_DESCRIPTION)
        );
    }

    #[test]
    fn builder_assembles_header_and_content() {
        let el: Element = empty()
            .header(
                empty_header()
                    .title("No data")
                    .description("Try creating one."),
            )
            .content_child(Element::new().text("Action"))
            .into();
        assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
        assert_eq!(el.children.len(), 2);
        assert_eq!(
            el.children[0].attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_HEADER)
        );
        assert_eq!(
            el.children[1].attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_CONTENT)
        );
    }

    #[test]
    fn dual_surface_compiles_shape() {
        let _b: Element = empty().header(empty_header().title("X")).into();
        let el = Empty(EmptyProps {
            id: String::new(),
            children: None,
            base: None,
            base_builder: None,
            ..Default::default()
        });
        let _: Element = el;
    }
}
