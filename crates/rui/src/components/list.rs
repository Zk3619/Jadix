//! # List
//!
//! Semantic `<ul>` / `<li>` list shell. Complements [`Item`] / [`ItemGroup`] (rich row chrome):
//! use **List** for plain or lightly marked rows; nest **Item** inside **ListItem** for media /
//! title / actions.
//!
//! Aligns with ShadCN navigation-menu `ListItem` patterns and community list panels that stack
//! `ItemGroup` rows Ã¯Â¿Â½?here the outer shell carries `role="list"` / `role="listitem"`.
//!
//! ```text
//! List                          // ul ÃÂ· role=list ÃÂ· flex flex-col gap-1 text-sm
//! Ã¢ÂÂÃ¢ÂÂÃ¢ÂÂ ListHeader?               // section title (font-medium ÃÂ· foreground)
//! Ã¢ÂÂÃ¢ÂÂÃ¢ÂÂ ListDescription?          // muted helper under header
//! Ã¢ÂÂÃ¢ÂÂÃ¢ÂÂ ListItem                  // li ÃÂ· role=listitem ÃÂ· optional marker
//!     Ã¢ÂÂÃ¢ÂÂÃ¢ÂÂ plain text / link
//!     Ã¢ÂÂÃ¢ÂÂÃ¢ÂÂ Item?                 // rich row (reuse item.rs)
//! ```
//!
//! ## Usage (dual style ÃÂ· ADR-9)
//! ```ignore
//! // A ÃÂ· RSX / tags
//! <List marker="disc">
//!     <ListHeader>"Features"</ListHeader>
//!     <ListDescription>"Everything included in this plan."</ListDescription>
//!     <ListItem>"Unlimited projects"</ListItem>
//!     <ListItem marker="disc">"24/7 support"</ListItem>
//!     <ListItem>
//!         <Item variant="outline" size="sm">
//!             <ItemContent>
//!                 <ItemTitle>"Rich row"</ItemTitle>
//!                 <ItemDescription>"Item inside ListItem."</ItemDescription>
//!             </ItemContent>
//!         </Item>
//!     </ListItem>
//! </List>
//!
//! // B ÃÂ· Builder
//! Element::from(
//!     list()
//!         .header("Features")
//!         .description("Everything included in this plan.")
//!         .marker("disc")
//!         .text("Unlimited projects")
//!         .text("24/7 support")
//!         .item_row(
//!             item()
//!                 .variant("outline")
//!                 .title_description("Rich row", "Item inside ListItem.")
//!         )
//! )
//! ```
//!
//! ## Notes
//! - **Presentational**: empty `id` omits `template_id` (WP-2.8).
//! - `<List>` / `<ListItem>` use the generic PascalCase Ã¯Â¿Â½?`*Props` path (same as Empty / Item).
//! - `marker` on `<List>` sets the default for child `<ListItem>`; per-item `marker` overrides.
//! - Ã¥ÂÂÃ¨Â¡Â¨Ã¨Â¯Â­Ã¤Â¹ÂÃ§Â»ÂÃ¤Â»Â¶Ã¯Â¼Â`List` Ã¤Â¸ÂºÃ§ÂºÂµÃ¯Â¿Â½?`ul`Ã¯Â¼Â`ListItem` Ã¯Â¿Â½?`li`Ã¯Â¼ÂÃ¥Â¤ÂÃ¦ÂÂÃ¨Â¡ÂÃ¨Â¯Â·Ã¥ÂµÂÃ¯Â¿Â½?`Item`Ã¯Â¿Â½?

use std::sync::OnceLock;

use crate::components::base::BaseComponentBuilder;
use crate::components::item::{
    ItemBuilder, ItemContent, ItemContentProps, ItemDescription, ItemDescriptionProps, ItemTitle,
    ItemTitleProps,
};
use crate::core::reactivity::{create_context, provide_context, use_context, Context};
use crate::core::tree::{parse_text_color, Element};
use crate::primitives::r#box::{hstack, vstack};
use crate::style::with_class_list;
use crate::utils::take_packed;
use crate::utils::text_node::{build_text_node, TextNodeConfig};
use rui_macros::component;

const ATTR_SLOT: &str = "data-slot";
const ATTR_MARKER: &str = "data-marker";
const SLOT_ROOT: &str = "list";
const SLOT_HEADER: &str = "list-header";
const SLOT_DESCRIPTION: &str = "list-description";
const SLOT_ITEM: &str = "list-item";

const TEXT_SIZE_PX: f32 = 14.0;
const TEXT_WEIGHT: u16 = 400;
const HEADER_WEIGHT: u16 = 500;

// ---------------------------------------------------------------------------
// Context (default marker + decimal index)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct ListCtx {
    default_marker: ListMarker,
    next_index: usize,
}

fn list_context() -> Context<ListCtx> {
    static CTX: OnceLock<Context<ListCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<ListCtx>)
}

fn use_list() -> Option<ListCtx> {
    use_context(list_context())
}

// ---------------------------------------------------------------------------
// Marker
// ---------------------------------------------------------------------------

/// Bullet / index prefix for [`ListItem`]: `none` | `disc` | `decimal`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListMarker {
    #[default]
    None,
    Disc,
    Decimal,
}

impl ListMarker {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Disc => "disc",
            Self::Decimal => "decimal",
        }
    }

    fn glyph(self, _index: usize) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Disc => Some("Ã¢ÂÂ¢"),
            Self::Decimal => None, // rendered via format! in build_list_item
        }
    }
}

impl From<&str> for ListMarker {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or_default()
    }
}

impl From<String> for ListMarker {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl std::str::FromStr for ListMarker {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "disc" | "bullet" | "dot" => Self::Disc,
            "decimal" | "number" | "ordered" => Self::Decimal,
            _ => Self::None,
        })
    }
}

fn list_text_config(color: [u8; 4], weight: u16) -> TextNodeConfig {
    TextNodeConfig {
        size_px: TEXT_SIZE_PX,
        weight,
        color,
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

fn list_marker_from_attrs(attrs: &std::collections::HashMap<String, String>) -> ListMarker {
    attrs
        .get(ATTR_MARKER)
        .map(|s| ListMarker::from(s.as_str()))
        .unwrap_or_default()
}

fn is_list_item(el: &Element) -> bool {
    el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_ITEM)
}

/// Pull row content out of an already-painted [`ListItem`] shell (marker + body).
fn extract_list_item_content(mut child: Element) -> Vec<Element> {
    if !is_list_item(&child) {
        return std::mem::take(&mut child.children);
    }
    let marker = list_marker_from_attrs(&child.attrs);
    if marker == ListMarker::None {
        return std::mem::take(&mut child.children);
    }
    child
        .children
        .pop()
        .map(|mut body| std::mem::take(&mut body.children))
        .unwrap_or_default()
}

/// Re-number decimal markers for direct `ListItem` children when `List` sets `marker="decimal"`.
fn apply_list_decimal_indices(default_marker: ListMarker, kids: Vec<Element>) -> Vec<Element> {
    if default_marker != ListMarker::Decimal {
        return kids;
    }
    let mut index = 0usize;
    kids.into_iter()
        .map(|child| {
            if !is_list_item(&child) {
                return child;
            }
            let item_marker = list_marker_from_attrs(&child.attrs);
            let effective = if item_marker == ListMarker::None {
                default_marker
            } else {
                item_marker
            };
            if effective != ListMarker::Decimal {
                return child;
            }
            let class = child
                .attrs
                .get("class")
                .cloned()
                .or_else(|| child.attrs.get("className").cloned());
            let content = extract_list_item_content(child);
            let rebuilt = build_list_item_shell(ListMarker::Decimal, index, content, class);
            index += 1;
            rebuilt
        })
        .collect()
}

fn build_list_item_shell(
    marker: ListMarker,
    index: usize,
    kids: Vec<Element>,
    class: Option<String>,
) -> Element {
    let fg = parse_text_color("muted-foreground");
    let cfg = list_text_config(fg, TEXT_WEIGHT);

    let content: Vec<Element> = if kids.len() == 1 && kids[0].text.is_some() {
        caption_from_children(Some(kids.into_iter().next().unwrap()), &cfg)
    } else if kids.is_empty() {
        Vec::new()
    } else {
        kids
    };

    let mut row = hstack().w("100%").min_w(0.0).gap("0.5rem").items("start");

    match marker {
        ListMarker::None => {}
        ListMarker::Disc => {
            if let Some(glyph) = marker.glyph(index) {
                row = row.child(
                    build_text_node(glyph, list_text_config(fg, TEXT_WEIGHT))
                        .flex_shrink(0.0)
                        .w("auto"),
                );
            }
        }
        ListMarker::Decimal => {
            let label = format!("{}.", index + 1);
            row = row.child(
                build_text_node(label, list_text_config(fg, TEXT_WEIGHT))
                    .flex_shrink(0.0)
                    .w("auto")
                    .min_w("1.25rem"),
            );
        }
    }

    let mut body = hstack().flex_grow(1.0).min_w(0.0).items_center();
    for kid in content {
        body = body.child(kid);
    }

    let mut el: Element = row.child(body).into();
    el.set_attr(ATTR_SLOT, SLOT_ITEM);
    el.set_attr(ATTR_MARKER, marker.as_str());
    el.set_attr("role", "listitem");
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        el = with_class_list(el, &c);
    }
    el
}

// ---------------------------------------------------------------------------
// RSX parts
// ---------------------------------------------------------------------------

/// Vertical list container (`role="list"`).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn List(
    marker: Option<ListMarker>,
    class: Option<String>,
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let default_marker = marker.unwrap_or_default();
    provide_context(
        list_context(),
        ListCtx {
            default_marker,
            next_index: 0,
        },
    );
    let kids = apply_list_decimal_indices(
        default_marker,
        children.map(take_packed).unwrap_or_default(),
    );
    let mut el: Element = vstack()
        .w("100%")
        .min_w(0.0)
        .gap("0.25rem")
        .text_size(TEXT_SIZE_PX)
        .children(kids)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_ROOT);
    el.set_attr(ATTR_MARKER, default_marker.as_str());
    el.set_attr("role", "list");
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        el = with_class_list(el, &c);
    }
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

/// Section title above list rows.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ListHeader(
    class: Option<String>,
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let cfg = list_text_config(parse_text_color("foreground"), HEADER_WEIGHT);
    let kids = caption_from_children(children, &cfg);
    let mut el: Element = hstack()
        .w("100%")
        .min_w(0.0)
        .items_center()
        .children(kids)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_HEADER);
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        el = with_class_list(el, &c);
    }
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

/// Muted helper copy under [`ListHeader`].
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ListDescription(
    class: Option<String>,
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let cfg = list_text_config(parse_text_color("muted-foreground"), TEXT_WEIGHT);
    let kids = caption_from_children(children, &cfg);
    let mut el: Element = vstack().w("100%").min_w(0.0).gap(0.0).children(kids).into();
    el.set_attr(ATTR_SLOT, SLOT_DESCRIPTION);
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        el = with_class_list(el, &c);
    }
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

/// Single list row (`role="listitem"`).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ListItem(
    marker: Option<ListMarker>,
    class: Option<String>,
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let explicit = marker.filter(|m| *m != ListMarker::None);
    let list_ctx = use_list();
    let effective = explicit.unwrap_or_else(|| {
        list_ctx
            .map(|c| c.default_marker)
            .unwrap_or(ListMarker::None)
    });
    let index = list_ctx.map(|c| c.next_index).unwrap_or(0);
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el = build_list_item_shell(effective, index, kids, class);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el
}

// ---------------------------------------------------------------------------
// Builders (ADR-9 surface B)
// ---------------------------------------------------------------------------

pub struct ListBuilder {
    marker: ListMarker,
    children: Vec<Element>,
}

pub fn list() -> ListBuilder {
    ListBuilder {
        marker: ListMarker::None,
        children: Vec::new(),
    }
}

impl ListBuilder {
    pub fn marker(mut self, m: impl Into<ListMarker>) -> Self {
        self.marker = m.into();
        self
    }

    pub fn header(mut self, text: impl Into<String>) -> Self {
        self.children.push(ListHeader(ListHeaderProps {
            children: Some(Element::new().text(text.into())),
            class: None,
            base: None,
            base_builder: None,
            ..Default::default()
        }));
        self
    }

    pub fn description(mut self, text: impl Into<String>) -> Self {
        self.children.push(ListDescription(ListDescriptionProps {
            children: Some(Element::new().text(text.into())),
            class: None,
            base: None,
            base_builder: None,
            ..Default::default()
        }));
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }

    pub fn item(mut self, child: impl Into<Element>) -> Self {
        self.children.push(ListItem(ListItemProps {
            marker: None,
            children: Some(child.into()),
            class: None,
            base: None,
            base_builder: None,
            ..Default::default()
        }));
        self
    }

    /// Plain text row (inherits list `marker` when inside [`List`]).
    pub fn text(mut self, label: impl Into<String>) -> Self {
        self.children.push(ListItem(ListItemProps {
            marker: None,
            children: Some(Element::new().text(label.into())),
            class: None,
            base: None,
            base_builder: None,
            ..Default::default()
        }));
        self
    }

    /// Title + description row (lightweight; no Item chrome).
    pub fn text_row(self, title: impl Into<String>, description: impl Into<String>) -> Self {
        let mut frag = Element::new();
        frag.set_attr("rui_fragment", "1");
        self.item(ListItem(ListItemProps {
            marker: None,
            children: Some(frag.children(vec![ItemContent(ItemContentProps {
                children: {
                    let mut inner = Element::new();
                    inner.set_attr("rui_fragment", "1");
                    Some(inner.children(vec![
                        ItemTitle(ItemTitleProps {
                            children: Some(Element::new().text(title.into())),
                            base: None,
                            base_builder: None,
                            ..Default::default()
                        }),
                        ItemDescription(ItemDescriptionProps {
                            children: Some(Element::new().text(description.into())),
                            base: None,
                            base_builder: None,
                            ..Default::default()
                        }),
                    ]))
                },
                base: None,
                base_builder: None,
                ..Default::default()
            })])),
            class: None,
            base: None,
            base_builder: None,
            ..Default::default()
        }))
    }

    /// Rich row Ã¯Â¿Â½?wraps an [`Item`] builder inside [`ListItem`].
    pub fn item_row(self, row: ItemBuilder) -> Self {
        self.item(row)
    }
}

impl From<ListBuilder> for Element {
    fn from(builder: ListBuilder) -> Self {
        List(ListProps {
            marker: Some(builder.marker),
            children: if builder.children.is_empty() {
                None
            } else {
                let mut frag = Element::new();
                frag.set_attr("rui_fragment", "1");
                Some(frag.children(builder.children))
            },
            class: None,
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
    fn list_marker_parse() {
        assert_eq!(ListMarker::from("disc"), ListMarker::Disc);
        assert_eq!(ListMarker::from("decimal"), ListMarker::Decimal);
        assert_eq!(ListMarker::from("none"), ListMarker::None);
    }

    #[test]
    fn list_sets_role_and_slot() {
        let el = List(ListProps {
            marker: Some(ListMarker::Disc),
            children: None,
            class: None,
            base: None,
            base_builder: None,
            ..Default::default()
        });
        assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
        assert_eq!(el.attrs.get("role").map(String::as_str), Some("list"));
        assert_eq!(el.attrs.get(ATTR_MARKER).map(String::as_str), Some("disc"));
    }

    #[test]
    fn list_item_sets_listitem_role() {
        let el = ListItem(ListItemProps {
            marker: Some(ListMarker::Disc),
            children: Some(Element::new().text("Row")),
            class: None,
            base: None,
            base_builder: None,
            ..Default::default()
        });
        assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ITEM));
        assert_eq!(el.attrs.get("role").map(String::as_str), Some("listitem"));
        assert!(el
            .children
            .iter()
            .any(|c| c.text.as_deref() == Some("Ã¢ÂÂ¢")));
    }

    #[test]
    fn list_decimal_marker_renders_index() {
        let el = build_list_item_shell(
            ListMarker::Decimal,
            2,
            vec![Element::new().text("Third")],
            None,
        );
        assert!(el.children.iter().any(|c| c.text.as_deref() == Some("3.")));
    }

    #[test]
    fn list_decimal_reindex_strips_existing_marker() {
        let first = build_list_item_shell(
            ListMarker::Decimal,
            0,
            vec![Element::new().text("One")],
            None,
        );
        let kids = apply_list_decimal_indices(
            ListMarker::Decimal,
            vec![
                first,
                build_list_item_shell(
                    ListMarker::Decimal,
                    0,
                    vec![Element::new().text("Two")],
                    None,
                ),
            ],
        );
        assert_eq!(kids.len(), 2);
        let count_ones = kids[0]
            .children
            .iter()
            .filter(|c| c.text.as_deref() == Some("1."))
            .count();
        assert_eq!(count_ones, 1, "must not double-wrap decimal marker");
        let count_twos = kids[1]
            .children
            .iter()
            .filter(|c| c.text.as_deref() == Some("2."))
            .count();
        assert_eq!(count_twos, 1);
    }

    #[test]
    fn list_builder_dual_surface() {
        let _el: Element = list()
            .marker("disc")
            .header("Title")
            .description("Subtitle")
            .text("One")
            .text_row("Two", "Second line")
            .into();
    }
}
