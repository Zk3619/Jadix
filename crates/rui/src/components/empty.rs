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

use crate::components::base::BaseComponentBuilder;
use crate::core::tree::{Element, IntoF32, parse_color, parse_text_color};
use crate::primitives::r#box::{hstack, vstack};
use crate::style::with_class_list;
use crate::tokens;
use crate::utils::take_packed;
use crate::utils::text_node::{TextNodeConfig, build_text_node};
use rui_macros::component;

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

/// Root empty-state column (`flex-1` · centered · `text-center`).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Empty(
    id: String,
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
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

/// Header cluster: media + title + description (`max-w-sm` · centered).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn EmptyHeader(
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
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

/// Media slot for icon / image / avatar (`variant`: `default` | `icon`).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn EmptyMedia(
    variant: EmptyMediaVariant,
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el = build_empty_media(variant, kids);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

/// Title line (`font-semibold` / heading-sized).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn EmptyTitle(
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
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

/// Muted description copy (`text-sm` · `text-muted-foreground`).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn EmptyDescription(
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
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

/// Action / secondary content region (`max-w-sm` · centered column).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn EmptyContent(
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
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

// ---------------------------------------------------------------------------
// Builder (ADR-9 surface B)
// ---------------------------------------------------------------------------

pub struct EmptyHeaderBuilder {
    children: Vec<Element>,
}

pub fn empty_header() -> EmptyHeaderBuilder {
    EmptyHeaderBuilder {
        children: Vec::new(),
    }
}

impl EmptyHeaderBuilder {
    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }

    pub fn media(mut self, child: impl Into<Element>) -> Self {
        self.children.push(
            EmptyMedia(EmptyMediaProps {
                variant: Some(EmptyMediaVariant::Default),
                children: Some(child.into()),
                base: None,
                base_builder: None,
                ..Default::default()
            }),
        );
        self
    }

    pub fn media_icon(mut self, child: impl Into<Element>) -> Self {
        self.children.push(
            EmptyMedia(EmptyMediaProps {
                variant: Some(EmptyMediaVariant::Icon),
                children: Some(child.into()),
                base: None,
                base_builder: None,
                ..Default::default()
            }),
        );
        self
    }

    pub fn title(mut self, text: impl Into<String>) -> Self {
        self.children.push(
            EmptyTitle(EmptyTitleProps {
                children: Some(Element::new().text(text.into())),
                base: None,
                base_builder: None,
                ..Default::default()
            }),
        );
        self
    }

    pub fn description(mut self, text: impl Into<String>) -> Self {
        self.children.push(
            EmptyDescription(EmptyDescriptionProps {
                children: Some(Element::new().text(text.into())),
                base: None,
                base_builder: None,
                ..Default::default()
            }),
        );
        self
    }
}

impl From<EmptyHeaderBuilder> for Element {
    fn from(builder: EmptyHeaderBuilder) -> Self {
        EmptyHeader(EmptyHeaderProps {
            children: if builder.children.is_empty() {
                None
            } else {
                let mut frag = Element::new();
                frag.set_attr("rui_fragment", "1");
                Some(frag.children(builder.children))
            },
            base: None,
            base_builder: None,
            ..Default::default()
        })
    }
}

pub struct EmptyContentBuilder {
    children: Vec<Element>,
}

pub fn empty_content() -> EmptyContentBuilder {
    EmptyContentBuilder {
        children: Vec::new(),
    }
}

impl EmptyContentBuilder {
    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }
}

impl From<EmptyContentBuilder> for Element {
    fn from(builder: EmptyContentBuilder) -> Self {
        EmptyContent(EmptyContentProps {
            children: if builder.children.is_empty() {
                None
            } else {
                let mut frag = Element::new();
                frag.set_attr("rui_fragment", "1");
                Some(frag.children(builder.children))
            },
            base: None,
            base_builder: None,
            ..Default::default()
        })
    }
}

pub struct EmptyBuilder {
    id: String,
    class: String,
    children: Vec<Element>,
}

/// Builder entry for [`Empty`].
pub fn empty() -> EmptyBuilder {
    EmptyBuilder {
        id: String::new(),
        class: String::new(),
        children: Vec::new(),
    }
}

impl EmptyBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn class(mut self, classes: impl Into<String>) -> Self {
        self.class = classes.into();
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

    pub fn content(mut self, content: impl Into<Element>) -> Self {
        self.children.push(content.into());
        self
    }

    pub fn content_child(mut self, child: impl Into<Element>) -> Self {
        self.children
            .push(empty_content().child(child).into());
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
        let children = if builder.children.is_empty() {
            None
        } else {
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            Some(frag.children(builder.children))
        };
        let mut el = Empty(EmptyProps {
            id: builder.id,
            children,
            base: None,
            base_builder: None,
            ..Default::default()
        });
        if !builder.class.trim().is_empty() {
            el = with_class_list(el, builder.class.trim());
        }
        el
    }
}

pub struct EmptyMediaBuilder {
    variant: EmptyMediaVariant,
    children: Vec<Element>,
}

pub fn empty_media() -> EmptyMediaBuilder {
    EmptyMediaBuilder {
        variant: EmptyMediaVariant::Default,
        children: Vec::new(),
    }
}

impl EmptyMediaBuilder {
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
        let children = if builder.children.is_empty() {
            None
        } else {
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            Some(frag.children(builder.children))
        };
        EmptyMedia(EmptyMediaProps {
            variant: Some(builder.variant),
            children,
            base: None,
            base_builder: None,
            ..Default::default()
        })
    }
}

#[cfg(test)]
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
        assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_MEDIA));
        assert_eq!(
            el.attrs.get(ATTR_VARIANT).map(String::as_str),
            Some("icon")
        );
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
        let _b: Element = empty()
            .header(empty_header().title("X"))
            .into();
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
