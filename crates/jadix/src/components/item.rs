//! # Item
//!
//! ShadCN-style list row shell. Aligns with
//! `Reference/ShadCN-UI/.../bases/base/ui/item.tsx` — media + title + description + actions.
//!
//! ```text
//! ItemGroup                       // role=list · flex column
//! ├── Item                        // variant: default | outline | muted; size: default | sm | xs
//! │   ├── ItemHeader?             // full-width top row
//! │   ├── ItemMedia?              // icon | image | default
//! │   ├── ItemContent
//! │   │   ├── ItemTitle
//! │   │   └── ItemDescription?
//! │   ├── ItemActions?            // trailing controls
//! │   └── ItemFooter?             // full-width bottom row
//! └── ItemSeparator
//! ```
//!
//! ## Usage (dual style · ADR-9)
//! ```ignore
//! // A · RSX / tags
//! <ItemGroup>
//!     <Item variant="outline" size="sm">
//!         <ItemMedia variant="icon">
//!             <Icon name="bell" size="1rem" />
//!         </ItemMedia>
//!         <ItemContent>
//!             <ItemTitle>"Notifications"</ItemTitle>
//!             <ItemDescription>"Manage how you receive alerts."</ItemDescription>
//!         </ItemContent>
//!         <ItemActions>
//!             <Button variant="outline" size="sm">"Edit"</Button>
//!         </ItemActions>
//!     </Item>
//!     <ItemSeparator />
//!     <Item variant="muted">
//!         <ItemContent>
//!             <ItemTitle>"Muted row"</ItemTitle>
//!         </ItemContent>
//!     </Item>
//! </ItemGroup>
//!
//! // B · Builder
//! Element::from(
//!     item_group()
//!         .child(
//!             item()
//!                 .variant("outline")
//!                 .media_icon(icon_child)
//!                 .title("Notifications")
//!                 .description("Manage how you receive alerts.")
//!                 .action(button("").text("Edit"))
//!         )
//! )
//! ```
//!
//! ## Notes
//! - **Presentational**: no scope / open state; `variant` / `size` support macro `bind_attr` (same as Alert).
//! - `<Item>` / compound tags use generic PascalCase → `*Props`; `ItemMedia variant` is also bindable.
//! - 列表行展示组件：`ItemGroup` 分组，`Item` 为 flex 行，子槽位对应 ShadCN 复合结构。

use std::sync::OnceLock;

use crate::components::base::{
    ensure_data_slot, packed_children, BaseComponentBuilder, SlotChrome,
};
use crate::components::separator::separator;
use crate::core::reactivity::{create_context, provide_context, use_context, Context};
use crate::core::tree::{parse_color, parse_text_color, Element, IntoF32, LayoutStatus};
use crate::impl_slot_chrome_methods;
use crate::primitives::r#box::{hstack, vstack};
use crate::tokens;
use crate::utils::take_packed;
use crate::utils::text_node::{build_text_node, TextNodeConfig};
use jadix_macros::component;

const ATTR_SLOT: &str = "data-slot";
const ATTR_VARIANT: &str = "data-variant";
const ATTR_SIZE: &str = "data-size";
const ATTR_ITEM_VARIANT: &str = "jadix_item_variant";
const ATTR_ITEM_SIZE: &str = "jadix_item_size";

const SLOT_GROUP: &str = "item-group";
const SLOT_ITEM: &str = "item";
const SLOT_SEPARATOR: &str = "item-separator";
const SLOT_MEDIA: &str = "item-media";
const SLOT_CONTENT: &str = "item-content";
const SLOT_TITLE: &str = "item-title";
const SLOT_DESCRIPTION: &str = "item-description";
const SLOT_ACTIONS: &str = "item-actions";
const SLOT_HEADER: &str = "item-header";
const SLOT_FOOTER: &str = "item-footer";

const TITLE_SIZE_PX: f32 = 14.0;
const TITLE_WEIGHT: u16 = 500;
const DESC_SIZE_PX: f32 = 14.0;
const DESC_WEIGHT: u16 = 400;
const ITEM_RADIUS: &str = "1rem";

// ---------------------------------------------------------------------------
// Variants
// ---------------------------------------------------------------------------

/// Item surface: `default` (transparent) | `outline` | `muted`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ItemVariant {
    #[default]
    Default,
    Outline,
    Muted,
}

impl ItemVariant {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Outline => "outline",
            Self::Muted => "muted",
        }
    }
}

impl From<&str> for ItemVariant {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or(Self::Default)
    }
}

impl From<String> for ItemVariant {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl std::str::FromStr for ItemVariant {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "outline" => Self::Outline,
            "muted" => Self::Muted,
            _ => Self::Default,
        })
    }
}

/// Item density: `default` | `sm` | `xs`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ItemSize {
    #[default]
    Default,
    Sm,
    Xs,
}

impl ItemSize {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Sm => "sm",
            Self::Xs => "xs",
        }
    }

    fn gap(self) -> &'static str {
        match self {
            Self::Default | Self::Sm => "0.875rem",
            Self::Xs => "0.5rem",
        }
    }

    fn px(self) -> &'static str {
        match self {
            Self::Default => "1rem",
            Self::Sm => "0.875rem",
            Self::Xs => "0.625rem",
        }
    }

    fn py(self) -> &'static str {
        match self {
            Self::Default => "0.875rem",
            Self::Sm => "0.75rem",
            Self::Xs => "0.5rem",
        }
    }

    fn image_box(self) -> &'static str {
        match self {
            Self::Default => "2.5rem",
            Self::Sm => "2rem",
            Self::Xs => "1.5rem",
        }
    }

    fn image_radius(self) -> &'static str {
        match self {
            Self::Default => ITEM_RADIUS,
            Self::Sm => tokens::RADIUS_LG,
            Self::Xs => tokens::RADIUS_MD,
        }
    }
}

impl From<&str> for ItemSize {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or(Self::Default)
    }
}

impl From<String> for ItemSize {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl std::str::FromStr for ItemSize {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "sm" | "small" => Self::Sm,
            "xs" | "xsmall" | "extra-small" => Self::Xs,
            _ => Self::Default,
        })
    }
}

/// `ItemMedia` chrome: `default` | `icon` | `image`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ItemMediaVariant {
    #[default]
    Default,
    Icon,
    Image,
}

impl ItemMediaVariant {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Icon => "icon",
            Self::Image => "image",
        }
    }
}

impl From<&str> for ItemMediaVariant {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or(Self::Default)
    }
}

impl From<String> for ItemMediaVariant {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl std::str::FromStr for ItemMediaVariant {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "icon" => Self::Icon,
            "image" | "img" | "avatar" => Self::Image,
            _ => Self::Default,
        })
    }
}

// ---------------------------------------------------------------------------
// Context (ItemMedia image sizing)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct ItemCtx {
    size: ItemSize,
}

fn item_context() -> Context<ItemCtx> {
    static CTX: OnceLock<Context<ItemCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<ItemCtx>)
}

fn use_item() -> Option<ItemCtx> {
    use_context(item_context())
}

fn item_variant_from_attrs(attrs: &std::collections::HashMap<String, String>) -> ItemVariant {
    attrs
        .get(ATTR_ITEM_VARIANT)
        .map(|s| ItemVariant::from(s.as_str()))
        .unwrap_or_default()
}

fn item_size_from_attrs(attrs: &std::collections::HashMap<String, String>) -> ItemSize {
    attrs
        .get(ATTR_ITEM_SIZE)
        .map(|s| ItemSize::from(s.as_str()))
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Paint
// ---------------------------------------------------------------------------

fn title_text_config() -> TextNodeConfig {
    TextNodeConfig {
        size_px: TITLE_SIZE_PX,
        weight: TITLE_WEIGHT,
        color: parse_text_color("foreground"),
        ..Default::default()
    }
}

fn description_text_config() -> TextNodeConfig {
    TextNodeConfig {
        size_px: DESC_SIZE_PX,
        weight: DESC_WEIGHT,
        color: parse_text_color("muted-foreground"),
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

/// Keep [`bind_attr`] / [`bind_text`] identity when rebuilding a painted shell.
fn preserve_binding_identity(from: &Element, to: &mut Element) {
    to.template_id = from.template_id.clone();
    to.focus_id = from.focus_id.clone();
    to.on_click = from.on_click.clone();
}

pub fn apply_item_paint(el: &mut Element, variant: ItemVariant, size: ItemSize) -> LayoutStatus {
    let prev_v = item_variant_from_attrs(&el.attrs);
    let prev_s = item_size_from_attrs(&el.attrs);
    if prev_v == variant && prev_s == size {
        return LayoutStatus::Clean;
    }

    let size_changed = prev_s != size;

    let mut painted = std::mem::take(el)
        .w("100%")
        .min_w(0.0)
        .items_center()
        .flex_wrap_kw("wrap")
        .gap(size.gap())
        .px(size.px())
        .py(size.py())
        .rounded(ITEM_RADIUS)
        .border_style("solid")
        .border_width(1.0)
        .text_size(TITLE_SIZE_PX);

    painted = match variant {
        ItemVariant::Default => painted
            .bg([0.0, 0.0, 0.0, 0.0])
            .border_color([0.0, 0.0, 0.0, 0.0]),
        ItemVariant::Outline => painted
            .bg(parse_color("background"))
            .border_color(parse_color("border")),
        ItemVariant::Muted => {
            let mut c = parse_color("muted");
            c[3] *= 0.5;
            painted.bg(c).border_color([0.0, 0.0, 0.0, 0.0])
        }
    };

    painted.set_attr(ATTR_ITEM_VARIANT, variant.as_str());
    painted.set_attr(ATTR_ITEM_SIZE, size.as_str());
    painted.set_attr(ATTR_SLOT, SLOT_ITEM);
    painted.set_attr(ATTR_VARIANT, variant.as_str());
    painted.set_attr(ATTR_SIZE, size.as_str());
    *el = painted;

    if size_changed {
        LayoutStatus::LayoutChanged
    } else {
        LayoutStatus::StyleChanged
    }
}

/// Macro bind: reactive `variant={sig.get()}` on `<Item>`.
pub fn apply_bound_item_variant(el: &mut Element, variant: impl AsRef<str>) -> LayoutStatus {
    let size = item_size_from_attrs(&el.attrs);
    apply_item_paint(el, ItemVariant::from(variant.as_ref()), size)
}

/// Macro bind: reactive `size={sig.get()}` on `<Item>`.
pub fn apply_bound_item_size(el: &mut Element, size: impl AsRef<str>) -> LayoutStatus {
    let variant = item_variant_from_attrs(&el.attrs);
    apply_item_paint(el, variant, ItemSize::from(size.as_ref()))
}

fn build_item_media(variant: ItemMediaVariant, children: Vec<Element>) -> Element {
    let item_size = use_item().map(|c| c.size).unwrap_or(ItemSize::Default);
    let mut shell = hstack()
        .items_center()
        .justify_center()
        .flex_shrink(0.0)
        .gap(tokens::SPACE_2);

    let kids: Vec<Element> = children
        .into_iter()
        .map(|mut child| {
            child = child.pointer_events_none();
            if variant == ItemMediaVariant::Icon
                && child.text_style.font_family.as_deref() == Some("lucide")
                && child.text_style.size_resolved() <= 1.0
            {
                child = child.text_size(IntoF32::into_f32("1rem"));
            }
            if child.text.is_some() {
                child.text_style.color = Some(parse_text_color("foreground"));
            }
            child
        })
        .collect();

    match variant {
        ItemMediaVariant::Default => {}
        ItemMediaVariant::Icon => {
            shell = shell
                .w("2rem")
                .h("2rem")
                .min_w("2rem")
                .min_h("2rem")
                .rounded(tokens::RADIUS_SM)
                .bg(parse_color("muted"));
        }
        ItemMediaVariant::Image => {
            let box_sz = item_size.image_box();
            shell = shell
                .w(box_sz)
                .h(box_sz)
                .min_w(box_sz)
                .min_h(box_sz)
                .rounded(item_size.image_radius())
                .overflow("hidden");
        }
    }

    let mut el: Element = shell.children(kids).into();
    el.set_attr(ATTR_SLOT, SLOT_MEDIA);
    el.set_attr(ATTR_VARIANT, variant.as_str());
    el
}

pub fn apply_bound_item_media_variant(el: &mut Element, variant: impl AsRef<str>) -> LayoutStatus {
    let v = ItemMediaVariant::from(variant.as_ref());
    let prev = el
        .attrs
        .get(ATTR_VARIANT)
        .map(|s| ItemMediaVariant::from(s.as_str()))
        .unwrap_or_default();
    if prev == v {
        return LayoutStatus::Clean;
    }
    let kids = std::mem::take(&mut el.children);
    let mut rebuilt = build_item_media(v, kids);
    preserve_binding_identity(el, &mut rebuilt);
    *el = rebuilt;
    LayoutStatus::StyleChanged
}

// ---------------------------------------------------------------------------
// RSX parts
// ---------------------------------------------------------------------------

/// Groups related items (`role="list"`).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ItemGroup(
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = vstack().w("100%").min_w(0.0).gap(0.0).children(kids).into();
    el.set_attr(ATTR_SLOT, SLOT_GROUP);
    el.set_attr("role", "list");
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

/// Horizontal rule between items.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ItemSeparator(
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
) -> Element {
    let mut el: Element = separator().into();
    el.set_attr(ATTR_SLOT, SLOT_SEPARATOR);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

/// Row shell (`variant` + `size`).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Item(
    variant: Option<ItemVariant>,
    size: Option<ItemSize>,
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let variant = variant.unwrap_or_default();
    let size = size.unwrap_or_default();
    provide_context(item_context(), ItemCtx { size });
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = hstack().children(kids).into();
    apply_item_paint(&mut el, variant, size);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

/// Media slot (icon / image / avatar).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ItemMedia(
    variant: Option<ItemMediaVariant>,
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let variant = variant.unwrap_or_default();
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el = build_item_media(variant, kids);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

/// Main text column (`flex-1`).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ItemContent(
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = vstack()
        .flex_grow(1.0)
        .flex_shrink(1.0)
        .min_w(0.0)
        .gap("0.125rem")
        .items("start")
        .children(kids)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_CONTENT);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

/// Primary label (single-line ellipsis).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ItemTitle(
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let cfg = title_text_config();
    let kids = caption_from_children(children, &cfg);
    let mut el: Element = hstack()
        .w("fit-content")
        .max_w("100%")
        .min_w(0.0)
        .items_center()
        .children(kids)
        .into();
    el = el.text_overflow_ellipsis().overflow_x("hidden");
    el.set_attr(ATTR_SLOT, SLOT_TITLE);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

/// Secondary copy (muted · up to two lines).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ItemDescription(
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let cfg = description_text_config();
    let kids = caption_from_children(children, &cfg);
    let mut el: Element = vstack()
        .w("100%")
        .min_w(0.0)
        .max_h("2.75rem")
        .overflow_y("hidden")
        .children(kids)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_DESCRIPTION);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

/// Trailing actions (buttons, badges, menus).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ItemActions(
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = hstack()
        .items_center()
        .gap(tokens::SPACE_2)
        .flex_shrink(0.0)
        .children(kids)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_ACTIONS);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

/// Full-width header row above the main item content.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ItemHeader(
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = hstack()
        .w("100%")
        .flex_basis("100%")
        .items_center()
        .justify("between")
        .flex_shrink(0.0)
        .children(kids)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_HEADER);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

/// Full-width footer row below the main item content.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ItemFooter(
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = hstack()
        .w("100%")
        .flex_basis("100%")
        .items_center()
        .justify("between")
        .flex_shrink(0.0)
        .children(kids)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_FOOTER);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

// ---------------------------------------------------------------------------
// Builders (ADR-9 surface B)
// ---------------------------------------------------------------------------

pub struct ItemSeparatorBuilder {
    chrome: SlotChrome,
}

pub fn item_separator() -> ItemSeparatorBuilder {
    ItemSeparatorBuilder {
        chrome: SlotChrome::default(),
    }
}

impl ItemSeparatorBuilder {
    impl_slot_chrome_methods!();
}

impl crate::core::tree::IntoElementList for ItemSeparatorBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<ItemSeparatorBuilder> for Element {
    fn from(builder: ItemSeparatorBuilder) -> Self {
        let el = ItemSeparator(ItemSeparatorProps {
            base: None,
            base_builder: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct ItemMediaBuilder {
    variant: ItemMediaVariant,
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn item_media() -> ItemMediaBuilder {
    ItemMediaBuilder {
        variant: ItemMediaVariant::Default,
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl ItemMediaBuilder {
    impl_slot_chrome_methods!();

    pub fn variant(mut self, variant: impl Into<ItemMediaVariant>) -> Self {
        self.variant = variant.into();
        self
    }

    pub fn icon(mut self) -> Self {
        self.variant = ItemMediaVariant::Icon;
        self
    }

    pub fn image(mut self) -> Self {
        self.variant = ItemMediaVariant::Image;
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }
}

impl crate::core::tree::IntoElementList for ItemMediaBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<ItemMediaBuilder> for Element {
    fn from(builder: ItemMediaBuilder) -> Self {
        let el = ItemMedia(ItemMediaProps {
            variant: Some(builder.variant),
            children: packed_children(builder.children),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct ItemContentBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn item_content() -> ItemContentBuilder {
    ItemContentBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl ItemContentBuilder {
    impl_slot_chrome_methods!();

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }
}

impl crate::core::tree::IntoElementList for ItemContentBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<ItemContentBuilder> for Element {
    fn from(builder: ItemContentBuilder) -> Self {
        let el = ItemContent(ItemContentProps {
            children: packed_children(builder.children),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct ItemTitleBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn item_title() -> ItemTitleBuilder {
    ItemTitleBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl ItemTitleBuilder {
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

impl crate::core::tree::IntoElementList for ItemTitleBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<ItemTitleBuilder> for Element {
    fn from(builder: ItemTitleBuilder) -> Self {
        let el = ItemTitle(ItemTitleProps {
            children: packed_children(builder.children),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct ItemDescriptionBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn item_description() -> ItemDescriptionBuilder {
    ItemDescriptionBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl ItemDescriptionBuilder {
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

impl crate::core::tree::IntoElementList for ItemDescriptionBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<ItemDescriptionBuilder> for Element {
    fn from(builder: ItemDescriptionBuilder) -> Self {
        let el = ItemDescription(ItemDescriptionProps {
            children: packed_children(builder.children),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct ItemActionsBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn item_actions() -> ItemActionsBuilder {
    ItemActionsBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl ItemActionsBuilder {
    impl_slot_chrome_methods!();

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }
}

impl crate::core::tree::IntoElementList for ItemActionsBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<ItemActionsBuilder> for Element {
    fn from(builder: ItemActionsBuilder) -> Self {
        let el = ItemActions(ItemActionsProps {
            children: packed_children(builder.children),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct ItemHeaderBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn item_header() -> ItemHeaderBuilder {
    ItemHeaderBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl ItemHeaderBuilder {
    impl_slot_chrome_methods!();

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }
}

impl crate::core::tree::IntoElementList for ItemHeaderBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<ItemHeaderBuilder> for Element {
    fn from(builder: ItemHeaderBuilder) -> Self {
        let el = ItemHeader(ItemHeaderProps {
            children: packed_children(builder.children),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct ItemFooterBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn item_footer() -> ItemFooterBuilder {
    ItemFooterBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl ItemFooterBuilder {
    impl_slot_chrome_methods!();

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }
}

impl crate::core::tree::IntoElementList for ItemFooterBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<ItemFooterBuilder> for Element {
    fn from(builder: ItemFooterBuilder) -> Self {
        let el = ItemFooter(ItemFooterProps {
            children: packed_children(builder.children),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct ItemGroupBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn item_group() -> ItemGroupBuilder {
    ItemGroupBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl ItemGroupBuilder {
    impl_slot_chrome_methods!();

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }

    pub fn separator(mut self) -> Self {
        self.children.push(item_separator().into());
        self
    }
}

impl crate::core::tree::IntoElementList for ItemGroupBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<ItemGroupBuilder> for Element {
    fn from(builder: ItemGroupBuilder) -> Self {
        let el = ItemGroup(ItemGroupProps {
            children: packed_children(builder.children),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct ItemBuilder {
    variant: ItemVariant,
    size: ItemSize,
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn item() -> ItemBuilder {
    ItemBuilder {
        variant: ItemVariant::Default,
        size: ItemSize::Default,
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl ItemBuilder {
    impl_slot_chrome_methods!();

    pub fn variant(mut self, v: impl Into<ItemVariant>) -> Self {
        self.variant = v.into();
        self
    }

    pub fn size(mut self, s: impl Into<ItemSize>) -> Self {
        self.size = s.into();
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }

    pub fn header(mut self, child: impl Into<Element>) -> Self {
        let el = child.into();
        let el = ensure_data_slot(el, SLOT_HEADER, |c| item_header().child(c).into());
        self.children.push(el);
        self
    }

    pub fn header_with(mut self, f: impl FnOnce(ItemHeaderBuilder) -> ItemHeaderBuilder) -> Self {
        self.children.push(f(item_header()).into());
        self
    }

    pub fn footer(mut self, child: impl Into<Element>) -> Self {
        let el = child.into();
        let el = ensure_data_slot(el, SLOT_FOOTER, |c| item_footer().child(c).into());
        self.children.push(el);
        self
    }

    pub fn footer_with(mut self, f: impl FnOnce(ItemFooterBuilder) -> ItemFooterBuilder) -> Self {
        self.children.push(f(item_footer()).into());
        self
    }

    pub fn media(mut self, child: impl Into<Element>) -> Self {
        self.children.push(item_media().child(child).into());
        self
    }

    pub fn media_icon(mut self, child: impl Into<Element>) -> Self {
        self.children.push(item_media().icon().child(child).into());
        self
    }

    pub fn media_image(mut self, child: impl Into<Element>) -> Self {
        self.children.push(item_media().image().child(child).into());
        self
    }

    pub fn media_with(mut self, f: impl FnOnce(ItemMediaBuilder) -> ItemMediaBuilder) -> Self {
        self.children.push(f(item_media()).into());
        self
    }

    pub fn content(mut self, child: impl Into<Element>) -> Self {
        let el = child.into();
        let el = ensure_data_slot(el, SLOT_CONTENT, |c| item_content().child(c).into());
        self.children.push(el);
        self
    }

    pub fn title_description(
        mut self,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        self.children.push(
            item_content()
                .child(item_title().text(title))
                .child(item_description().text(description))
                .into(),
        );
        self
    }

    pub fn title(mut self, text: impl Into<String>) -> Self {
        self.children
            .push(item_content().child(item_title().text(text)).into());
        self
    }

    pub fn title_with(mut self, f: impl FnOnce(ItemTitleBuilder) -> ItemTitleBuilder) -> Self {
        self.children
            .push(item_content().child(f(item_title())).into());
        self
    }

    pub fn description(mut self, text: impl Into<String>) -> Self {
        self.children
            .push(item_content().child(item_description().text(text)).into());
        self
    }

    pub fn description_with(
        mut self,
        f: impl FnOnce(ItemDescriptionBuilder) -> ItemDescriptionBuilder,
    ) -> Self {
        self.children
            .push(item_content().child(f(item_description())).into());
        self
    }

    pub fn action(mut self, child: impl Into<Element>) -> Self {
        self.children.push(item_actions().child(child).into());
        self
    }

    pub fn action_with(mut self, f: impl FnOnce(ItemActionsBuilder) -> ItemActionsBuilder) -> Self {
        self.children.push(f(item_actions()).into());
        self
    }
}

impl crate::core::tree::IntoElementList for ItemBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<ItemBuilder> for Element {
    fn from(builder: ItemBuilder) -> Self {
        let el = Item(ItemProps {
            variant: Some(builder.variant),
            size: Some(builder.size),
            children: packed_children(builder.children),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_variant_and_size_parse() {
        assert_eq!(ItemVariant::from("outline"), ItemVariant::Outline);
        assert_eq!(ItemSize::from("xs"), ItemSize::Xs);
        assert_eq!(ItemMediaVariant::from("image"), ItemMediaVariant::Image);
    }

    #[test]
    fn item_paint_sets_slots() {
        let mut el = Element::new();
        assert_eq!(
            apply_item_paint(&mut el, ItemVariant::Outline, ItemSize::Sm),
            LayoutStatus::LayoutChanged
        );
        assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ITEM));
        assert_eq!(
            el.attrs.get(ATTR_VARIANT).map(String::as_str),
            Some("outline")
        );
        assert_eq!(
            apply_item_paint(&mut el, ItemVariant::Outline, ItemSize::Sm),
            LayoutStatus::Clean
        );
        assert_eq!(
            apply_item_paint(&mut el, ItemVariant::Muted, ItemSize::Sm),
            LayoutStatus::StyleChanged
        );
    }

    #[test]
    fn item_group_and_title_slots() {
        let group = ItemGroup(ItemGroupProps {
            children: None,
            base: None,
            base_builder: None,
            ..Default::default()
        });
        assert_eq!(
            group.attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_GROUP)
        );
        let title = ItemTitle(ItemTitleProps {
            children: Some(Element::new().text("Hi")),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        assert_eq!(
            title.attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_TITLE)
        );
    }

    #[test]
    fn apply_bound_item_variant_preserves_template_id() {
        let mut el = Element::new();
        el.template_id = Some("item-bind-test".into());
        assert_eq!(
            apply_item_paint(&mut el, ItemVariant::Outline, ItemSize::Default),
            LayoutStatus::StyleChanged
        );
        assert_eq!(el.template_id.as_deref(), Some("item-bind-test"));
        assert_eq!(
            apply_bound_item_variant(&mut el, "muted"),
            LayoutStatus::StyleChanged
        );
        assert_eq!(
            el.attrs.get(ATTR_VARIANT).map(String::as_str),
            Some("muted")
        );
        assert_eq!(el.template_id.as_deref(), Some("item-bind-test"));
    }

    #[test]
    fn apply_item_paint_preserves_layout_node() {
        let node = taffy::prelude::NodeId::new(42);
        let mut el = Element::new();
        el.layout_node = Some(node);
        el.template_id = Some("item-layout-node".into());
        assert_eq!(
            apply_item_paint(&mut el, ItemVariant::Outline, ItemSize::Default),
            LayoutStatus::StyleChanged
        );
        assert_eq!(el.layout_node, Some(node));
        assert_eq!(
            apply_bound_item_variant(&mut el, "default"),
            LayoutStatus::StyleChanged
        );
        assert_eq!(el.layout_node, Some(node));
    }

    #[test]
    fn dual_surface_compiles_shape() {
        let _g: Element = item_group()
            .child(
                item()
                    .variant("muted")
                    .size("sm")
                    .title_description("T", "D"),
            )
            .into();
    }

    #[test]
    fn item_title_with_chrome_applies_text_size() {
        let el: Element = item()
            .class("w-full")
            .title_with(|t| t.style(|b| b.text_size(18.0)).text("Hi"))
            .into();
        let title = el
            .children
            .iter()
            .find(|c| c.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_CONTENT))
            .and_then(|content| {
                content
                    .children
                    .iter()
                    .find(|c| c.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_TITLE))
            })
            .expect("item title slot");
        assert_eq!(title.text_style.size, Some(18.0));
    }
}
