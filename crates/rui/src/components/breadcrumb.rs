//! # Breadcrumb
//!
//! ShadCN **base** 面包屑：对齐
//! `Reference/ShadCN-UI/.../bases/base/ui/breadcrumb.tsx`。
//!
//! ```text
//! Breadcrumb                 // nav · aria-label=breadcrumb
//! └── BreadcrumbList         // ol · flex wrap
//!     ├── BreadcrumbItem
//!     │   ├── BreadcrumbLink
//!     │   ├── BreadcrumbPage
//!     │   └── BreadcrumbEllipsis
//!     └── BreadcrumbSeparator
//! ```
//!
//! ## Usage (dual style · ADR-9)
//! ```ignore
//! // A · RSX / tags
//! <Breadcrumb>
//!     <BreadcrumbList>
//!         <BreadcrumbItem>
//!             <BreadcrumbLink on_click={|| nav_home()}>"Home"</BreadcrumbLink>
//!         </BreadcrumbItem>
//!         <BreadcrumbSeparator />
//!         <BreadcrumbItem>
//!             <BreadcrumbLink on_click={|| nav_docs()}>"Components"</BreadcrumbLink>
//!         </BreadcrumbItem>
//!         <BreadcrumbSeparator />
//!         <BreadcrumbItem>
//!             <BreadcrumbPage>"Breadcrumb"</BreadcrumbPage>
//!         </BreadcrumbItem>
//!     </BreadcrumbList>
//! </Breadcrumb>
//!
//! // B · Builder
//! Element::from(
//!     breadcrumb()
//!         .link("Home", || nav_home())
//!         .link("Components", || nav_docs())
//!         .page("Breadcrumb")
//! )
//! breadcrumb().ellipsis().page("Current")
//! ```
//!
//! ## Notes
//! - **Presentational**: empty `id` omits `template_id` (WP-2.8).
//! - `ui!` Multiple child nodes will pack fragments; List / Item will [`take_packed`] split horizontally.
//! - [`BreadcrumbLink`] / [`BreadcrumbPage`] share `text-sm` · `font-normal` (14px / 400); Link muted → foreground hover.
//! - Don't use `leading-none` for Link: keep default line height so glyphs aren't cramped;
//!   underline-offset paints below the box and relies on decoration clip expansion.
//! - [`BreadcrumbSeparator`] / [`BreadcrumbEllipsis`] 默认 Lucide chevron / more-horizontal。

use crate::components::base::{
    ensure_data_slot, packed_children, BaseComponentBuilder, SlotChrome,
};
use crate::components::button::button;
use crate::core::tree::{parse_text_color, Element, Handler};
use crate::impl_slot_chrome_methods;
use crate::primitives::r#box::hstack;
use crate::style::{parse_text_color as theme_fg, with_class_list};
use crate::utils::lucide::lucide_glyph;
use crate::utils::take_packed;
use crate::utils::text_node::{build_icon_text_node, build_text_node, TextNodeConfig};
use rui_macros::component;

const ATTR_SLOT: &str = "data-slot";
const SLOT_ROOT: &str = "breadcrumb";
const SLOT_LIST: &str = "breadcrumb-list";
const SLOT_ITEM: &str = "breadcrumb-item";
const SLOT_LINK: &str = "breadcrumb-link";
const SLOT_PAGE: &str = "breadcrumb-page";
const SLOT_SEPARATOR: &str = "breadcrumb-separator";
const SLOT_ELLIPSIS: &str = "breadcrumb-ellipsis";

/// ShadCN breadcrumb 文案：`text-sm` · `font-normal`（Link / Page 一致）。
const BREADCRUMB_TEXT_SIZE_PX: f32 = 14.0;
const BREADCRUMB_TEXT_WEIGHT: u16 = 400;

fn breadcrumb_caption_config(color: [u8; 4]) -> TextNodeConfig {
    TextNodeConfig {
        size_px: BREADCRUMB_TEXT_SIZE_PX,
        weight: BREADCRUMB_TEXT_WEIGHT,
        color,
        ..Default::default()
    }
}
fn extract_text_leaf(el: &Element) -> String {
    if let Some(t) = &el.text {
        if !t.is_empty() {
            return t.clone();
        }
    }
    for c in &el.children {
        let t = extract_text_leaf(c);
        if !t.is_empty() {
            return t;
        }
    }
    String::new()
}

fn lucide_icon(name: &str, size_px: f32) -> Element {
    let glyph = lucide_glyph(name).unwrap_or_else(|| "…".into());
    build_icon_text_node(glyph)
        .text_size(size_px)
        .text_color(parse_text_color("muted-foreground"))
        .pointer_events("none")
}

/// Stable `focus_id` when callers omit `id` — rebuild must not mint `btn-N` (hover signal loss).
fn breadcrumb_link_focus_id(id: &str, label: &str, disambiguator: Option<usize>) -> String {
    if !id.trim().is_empty() {
        return id.trim().to_string();
    }
    let label = label.trim();
    match (label.is_empty(), disambiguator) {
        (true, Some(idx)) => format!("breadcrumb-link:{idx}"),
        (true, None) => "breadcrumb-link".to_string(),
        (false, Some(idx)) => format!("breadcrumb-link:{label}:{idx}"),
        (false, None) => format!("breadcrumb-link:{label}"),
    }
}

fn build_breadcrumb_link(
    id: &str,
    label: &str,
    disambiguator: Option<usize>,
    mut on_click: Handler<()>,
    class: Option<String>,
) -> Element {
    let btn_id = breadcrumb_link_focus_id(id, label, disambiguator);
    let muted = parse_text_color("muted-foreground");
    let mut btn = button(&btn_id)
        .variant("link")
        .size("inline")
        .font_weight(BREADCRUMB_TEXT_WEIGHT)
        .text_color(muted)
        .on_click(move || on_click.call(()));
    if !label.is_empty() {
        btn = btn.text(label);
    }
    let mut el: Element = btn.into();
    el.set_attr(ATTR_SLOT, SLOT_LINK);
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        el = with_class_list(el, &c);
    }
    el
}

fn page_text_node(label: &str) -> Element {
    build_text_node(label, breadcrumb_caption_config(theme_fg("foreground")))
}

/// Remove extra delimiters at the end of the list (to prevent RSX from miswriting/flattening fragments).
fn strip_trailing_separators(mut kids: Vec<Element>) -> Vec<Element> {
    while matches!(
        kids.last()
            .and_then(|el| el.attrs.get(ATTR_SLOT).map(String::as_str)),
        Some(SLOT_SEPARATOR)
    ) {
        kids.pop();
    }
    kids
}

// ---------------------------------------------------------------------------
// RSX parts
// ---------------------------------------------------------------------------

/// Root: `nav` · `aria-label="breadcrumb"`.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Breadcrumb(
    id: String,
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut row = hstack().w("auto").min_w(0.0).items_center().gap("0");
    for k in kids {
        row = row.child(k);
    }
    let mut el: Element = row.into();
    el.set_attr(ATTR_SLOT, SLOT_ROOT);
    el = el.accesskit_role("navigation");
    let mut base = BaseComponentBuilder::new(el);
    if let Some(b) = base_builder {
        base = b(base);
    }
    let aria = base
        .label
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("breadcrumb");
    el = base.el.aria_label(aria);
    if !id.trim().is_empty() {
        el.template_id = Some(id);
    }
    el
}

/// 列表：`flex flex-wrap items-center gap-1.5`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn BreadcrumbList(class: Option<String>, children: Option<Element>) -> Element {
    let kids = strip_trailing_separators(children.map(take_packed).unwrap_or_default());
    let mut row = hstack()
        .gap("0.375rem")
        .items_center()
        .wrap("wrap")
        .w("auto")
        .min_w(0.0);
    for k in kids {
        row = row.child(k);
    }
    let mut el: Element = row.into();
    el.set_attr(ATTR_SLOT, SLOT_LIST);
    el.set_attr("role", "list");
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        el = with_class_list(el, &c);
    }
    el
}

/// Single item shell: `inline-flex items-center gap-1.5`.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn BreadcrumbItem(class: Option<String>, children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut row = hstack().gap("0.375rem").items_center().w("auto");
    for k in kids {
        row = row.child(k);
    }
    let mut el: Element = row.into();
    el.set_attr(ATTR_SLOT, SLOT_ITEM);
    el.set_attr("role", "listitem");
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        el = with_class_list(el, &c);
    }
    el
}

/// Clickable segment: Button `link` variant.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn BreadcrumbLink(
    id: String,
    class: Option<String>,
    #[prop(default = "Handler::default()")] mut on_click: Handler<()>,
    children: Option<Element>,
) -> Element {
    let label = children
        .as_ref()
        .map(extract_text_leaf)
        .filter(|s| !s.is_empty())
        .unwrap_or_default();
    build_breadcrumb_link(&id, &label, None, on_click, class)
}

/// 当前页：`aria-current="page"` · 不可导航。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn BreadcrumbPage(class: Option<String>, children: Option<Element>) -> Element {
    let label = children
        .as_ref()
        .map(extract_text_leaf)
        .filter(|s| !s.is_empty())
        .unwrap_or_default();
    let mut el: Element = hstack().items_center().into();
    el.set_attr(ATTR_SLOT, SLOT_PAGE);
    el.set_attr("role", "link");
    el.set_attr("aria-disabled", "true");
    el.set_attr("aria-current", "page");
    if let Some(c) = children {
        let kids = take_packed(c);
        if kids.len() == 1 && kids[0].text.as_ref().is_some_and(|t| !t.is_empty()) {
            el = el.child(page_text_node(&label));
        } else if kids.is_empty() && !label.is_empty() {
            el = el.child(page_text_node(&label));
        } else {
            for k in kids {
                if k.text.as_ref().is_some_and(|t| !t.is_empty()) && label == extract_text_leaf(&k)
                {
                    el = el.child(page_text_node(&label));
                } else {
                    el = el.child(k);
                }
            }
        }
    } else if !label.is_empty() {
        el = el.child(page_text_node(&label));
    }
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        el = with_class_list(el, &c);
    }
    el
}

/// Separator: Default chevron-right.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn BreadcrumbSeparator(class: Option<String>, children: Option<Element>) -> Element {
    let mut el: Element = hstack().items_center().justify_center().into();
    el.set_attr(ATTR_SLOT, SLOT_SEPARATOR);
    el.set_attr("role", "presentation");
    el.set_attr("aria-hidden", "true");
    if let Some(c) = children {
        let kids = take_packed(c);
        if kids.is_empty() {
            el = el.child(lucide_icon("chevron-right", 14.0));
        } else {
            for k in kids {
                el = el.child(k);
            }
        }
    } else {
        el = el.child(lucide_icon("chevron-right", 14.0));
    }
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        el = with_class_list(el, &c);
    }
    el
}

/// 折叠占位：more-horizontal + 屏幕阅读器 "More"。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn BreadcrumbEllipsis(class: Option<String>, children: Option<Element>) -> Element {
    let _ = children;
    let mut el: Element = hstack()
        .items_center()
        .justify_center()
        .w("2.25rem")
        .h("2.25rem")
        .into();
    el.set_attr(ATTR_SLOT, SLOT_ELLIPSIS);
    el.set_attr("role", "presentation");
    el.set_attr("aria-hidden", "true");
    el = el.child(lucide_icon("more-horizontal", 16.0));
    el = el.accesskit_name("More");
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        el = with_class_list(el, &c);
    }
    el
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

enum BreadcrumbBuilderPart {
    Child(Element),
    Link {
        label: String,
        on_click: Handler<()>,
        chrome: SlotChrome,
    },
    Page {
        label: String,
        chrome: SlotChrome,
    },
    Ellipsis {
        chrome: SlotChrome,
    },
}

pub struct BreadcrumbItemBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn breadcrumb_item() -> BreadcrumbItemBuilder {
    BreadcrumbItemBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl BreadcrumbItemBuilder {
    impl_slot_chrome_methods!();

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }
}

impl From<BreadcrumbItemBuilder> for Element {
    fn from(builder: BreadcrumbItemBuilder) -> Self {
        let el = BreadcrumbItem(BreadcrumbItemProps {
            class: None,
            children: packed_children(builder.children),
            base: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct BreadcrumbLinkBuilder {
    id: String,
    label: String,
    on_click: Handler<()>,
    chrome: SlotChrome,
}

pub fn breadcrumb_link() -> BreadcrumbLinkBuilder {
    BreadcrumbLinkBuilder {
        id: String::new(),
        label: String::new(),
        on_click: Handler::default(),
        chrome: SlotChrome::default(),
    }
}

impl BreadcrumbLinkBuilder {
    impl_slot_chrome_methods!();

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn text(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    pub fn on_click(mut self, f: impl Fn() + 'static) -> Self {
        self.on_click = Handler::new(move |_| f());
        self
    }
}

impl From<BreadcrumbLinkBuilder> for Element {
    fn from(builder: BreadcrumbLinkBuilder) -> Self {
        let el = build_breadcrumb_link(&builder.id, &builder.label, None, builder.on_click, None);
        builder.chrome.apply(el)
    }
}

pub struct BreadcrumbPageBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn breadcrumb_page() -> BreadcrumbPageBuilder {
    BreadcrumbPageBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl BreadcrumbPageBuilder {
    impl_slot_chrome_methods!();

    pub fn text(mut self, label: impl Into<String>) -> Self {
        self.children.push(Element::new().text(label.into()));
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }
}

impl From<BreadcrumbPageBuilder> for Element {
    fn from(builder: BreadcrumbPageBuilder) -> Self {
        let el = BreadcrumbPage(BreadcrumbPageProps {
            class: None,
            children: packed_children(builder.children),
            base: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct BreadcrumbSeparatorBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn breadcrumb_separator() -> BreadcrumbSeparatorBuilder {
    BreadcrumbSeparatorBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl BreadcrumbSeparatorBuilder {
    impl_slot_chrome_methods!();

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }
}

impl From<BreadcrumbSeparatorBuilder> for Element {
    fn from(builder: BreadcrumbSeparatorBuilder) -> Self {
        let el = BreadcrumbSeparator(BreadcrumbSeparatorProps {
            class: None,
            children: packed_children(builder.children),
            base: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct BreadcrumbEllipsisBuilder {
    chrome: SlotChrome,
}

pub fn breadcrumb_ellipsis() -> BreadcrumbEllipsisBuilder {
    BreadcrumbEllipsisBuilder {
        chrome: SlotChrome::default(),
    }
}

impl BreadcrumbEllipsisBuilder {
    impl_slot_chrome_methods!();
}

impl From<BreadcrumbEllipsisBuilder> for Element {
    fn from(builder: BreadcrumbEllipsisBuilder) -> Self {
        let el = BreadcrumbEllipsis(BreadcrumbEllipsisProps {
            class: None,
            children: None,
            base: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct BreadcrumbListBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn breadcrumb_list() -> BreadcrumbListBuilder {
    BreadcrumbListBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl BreadcrumbListBuilder {
    impl_slot_chrome_methods!();

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }
}

impl From<BreadcrumbListBuilder> for Element {
    fn from(builder: BreadcrumbListBuilder) -> Self {
        let el = BreadcrumbList(BreadcrumbListProps {
            class: None,
            children: packed_children(builder.children),
            base: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

/// Builder status (ADR-9 Side B).
pub struct BreadcrumbBuilder {
    id: String,
    chrome: SlotChrome,
    list_chrome: SlotChrome,
    item_chrome: SlotChrome,
    separator_chrome: SlotChrome,
    label: String,
    parts: Vec<BreadcrumbBuilderPart>,
}

/// Builder entry.
pub fn breadcrumb() -> BreadcrumbBuilder {
    BreadcrumbBuilder {
        id: String::new(),
        chrome: SlotChrome::default(),
        list_chrome: SlotChrome::default(),
        item_chrome: SlotChrome::default(),
        separator_chrome: SlotChrome::default(),
        label: String::new(),
        parts: Vec::new(),
    }
}

impl BreadcrumbBuilder {
    impl_slot_chrome_methods!();

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn aria_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.parts.push(BreadcrumbBuilderPart::Child(child.into()));
        self
    }

    pub fn link(mut self, label: impl Into<String>, on_click: impl Fn() + 'static) -> Self {
        self.parts.push(BreadcrumbBuilderPart::Link {
            label: label.into(),
            on_click: Handler::new(move |_| on_click()),
            chrome: SlotChrome::default(),
        });
        self
    }

    pub fn link_with(
        mut self,
        label: impl Into<String>,
        on_click: impl Fn() + 'static,
        f: impl FnOnce(BreadcrumbLinkBuilder) -> BreadcrumbLinkBuilder,
    ) -> Self {
        let b = f(breadcrumb_link().text(label).on_click(on_click));
        self.parts.push(BreadcrumbBuilderPart::Link {
            label: b.label,
            on_click: b.on_click,
            chrome: b.chrome,
        });
        self
    }

    pub fn page(mut self, label: impl Into<String>) -> Self {
        self.parts.push(BreadcrumbBuilderPart::Page {
            label: label.into(),
            chrome: SlotChrome::default(),
        });
        self
    }

    pub fn page_with(
        mut self,
        f: impl FnOnce(BreadcrumbPageBuilder) -> BreadcrumbPageBuilder,
    ) -> Self {
        self.parts
            .push(BreadcrumbBuilderPart::Child(f(breadcrumb_page()).into()));
        self
    }

    pub fn ellipsis(mut self) -> Self {
        self.parts.push(BreadcrumbBuilderPart::Ellipsis {
            chrome: SlotChrome::default(),
        });
        self
    }

    pub fn ellipsis_with(
        mut self,
        f: impl FnOnce(BreadcrumbEllipsisBuilder) -> BreadcrumbEllipsisBuilder,
    ) -> Self {
        self.parts.push(BreadcrumbBuilderPart::Ellipsis {
            chrome: f(breadcrumb_ellipsis()).chrome,
        });
        self
    }

    pub fn list_with(
        mut self,
        f: impl FnOnce(BreadcrumbListBuilder) -> BreadcrumbListBuilder,
    ) -> Self {
        self.list_chrome = f(breadcrumb_list()).chrome;
        self
    }
}

impl From<BreadcrumbBuilder> for Element {
    fn from(builder: BreadcrumbBuilder) -> Self {
        let mut list_kids = Vec::new();
        for (part_idx, part) in builder.parts.into_iter().enumerate() {
            if part_idx > 0 {
                list_kids.push(builder.separator_chrome.apply(BreadcrumbSeparator(
                    BreadcrumbSeparatorProps {
                        class: None,
                        children: None,
                        base: None,
                        ..Default::default()
                    },
                )));
            }
            match part {
                BreadcrumbBuilderPart::Child(el) => {
                    list_kids.push(ensure_data_slot(el, SLOT_ITEM, |inner| {
                        breadcrumb_item().child(inner).into()
                    }));
                }
                BreadcrumbBuilderPart::Link {
                    label,
                    on_click,
                    chrome,
                } => {
                    let link = chrome.apply(build_breadcrumb_link(
                        "",
                        &label,
                        Some(part_idx),
                        on_click,
                        None,
                    ));
                    list_kids.push(builder.item_chrome.apply(BreadcrumbItem(
                        BreadcrumbItemProps {
                            class: None,
                            children: Some(link),
                            base: None,
                            ..Default::default()
                        },
                    )));
                }
                BreadcrumbBuilderPart::Page { label, chrome } => {
                    let page = chrome.apply(BreadcrumbPage(BreadcrumbPageProps {
                        class: None,
                        children: Some(Element::new().text(label)),
                        base: None,
                        ..Default::default()
                    }));
                    list_kids.push(builder.item_chrome.apply(BreadcrumbItem(
                        BreadcrumbItemProps {
                            class: None,
                            children: Some(page),
                            base: None,
                            ..Default::default()
                        },
                    )));
                }
                BreadcrumbBuilderPart::Ellipsis { chrome } => {
                    let ellipsis = chrome.apply(BreadcrumbEllipsis(BreadcrumbEllipsisProps {
                        class: None,
                        children: None,
                        base: None,
                        ..Default::default()
                    }));
                    list_kids.push(builder.item_chrome.apply(BreadcrumbItem(
                        BreadcrumbItemProps {
                            class: None,
                            children: Some(ellipsis),
                            base: None,
                            ..Default::default()
                        },
                    )));
                }
            }
        }
        let list = builder
            .list_chrome
            .apply(BreadcrumbList(BreadcrumbListProps {
                class: None,
                children: packed_children(list_kids),
                base: None,
                ..Default::default()
            }));
        let mut el = Breadcrumb(BreadcrumbProps {
            id: builder.id,
            children: Some(list),
            base: None,
            base_builder: None,
            ..Default::default()
        });
        if !builder.label.trim().is_empty() {
            el = el.aria_label(builder.label.trim());
        }
        builder.chrome.apply(el)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_page_with_style() {
        let el: Element = breadcrumb()
            .link("Home", || ())
            .page_with(|p| p.style(|b| b.text_size(18.0)).text("Here"))
            .into();
        fn find_size(el: &Element, want: f32) -> bool {
            (el.text_style.size_resolved() - want).abs() < 0.01
                || el.children.iter().any(|c| find_size(c, want))
        }
        assert!(find_size(&el, 18.0));
    }

    #[test]
    fn breadcrumb_root_slot_and_aria() {
        let el = Breadcrumb(BreadcrumbProps {
            id: String::new(),
            children: None,
            base: None,
            base_builder: None,
            ..Default::default()
        });
        assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
        assert_eq!(
            el.attrs.get("aria-label").map(String::as_str),
            Some("breadcrumb")
        );
        assert!(el.template_id.is_none());
    }

    #[test]
    fn breadcrumb_page_aria_current() {
        let el = BreadcrumbPage(BreadcrumbPageProps {
            class: None,
            children: Some(Element::new().text("Current")),
            base: None,
            ..Default::default()
        });
        assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_PAGE));
        assert_eq!(
            el.attrs.get("aria-current").map(String::as_str),
            Some("page")
        );
    }

    #[test]
    fn breadcrumb_link_inline_button() {
        let el = build_breadcrumb_link("", "Home", None, Handler::default(), None);
        assert_eq!(
            el.focus_id.as_deref(),
            Some("breadcrumb-link:Home"),
            "empty id must use stable breadcrumb focus_id, not auto btn-N"
        );
        assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_LINK));
        assert!(el.button_hover_paint.is_some());
        assert_eq!(
            el.button_hover_paint.as_ref().map(|p| p.variant.as_str()),
            Some("link")
        );
        use crate::core::tree::PointerEvents;
        let caption = el
            .children
            .iter()
            .find(|c| c.text.is_some())
            .expect("caption");
        assert_eq!(caption.pointer_events_mode(), PointerEvents::None);
        assert_eq!(
            caption.text_style.underline_offset,
            Some(2.0),
            "link button default underline offset"
        );
    }

    #[test]
    fn breadcrumb_page_uses_foreground() {
        let el = BreadcrumbPage(BreadcrumbPageProps {
            class: None,
            children: Some(Element::new().text("Current")),
            base: None,
            ..Default::default()
        });
        let text = el
            .children
            .iter()
            .find(|c| c.text.is_some())
            .expect("caption");
        assert_eq!(text.text_style.color_resolved(), theme_fg("foreground"));
        assert_eq!(text.text_style.size_resolved(), BREADCRUMB_TEXT_SIZE_PX);
        assert_eq!(text.text_style.weight_resolved(), BREADCRUMB_TEXT_WEIGHT);
    }

    #[test]
    fn breadcrumb_link_and_page_typography_match() {
        let link = build_breadcrumb_link("", "Settings", None, Handler::default(), None);
        let page = BreadcrumbPage(BreadcrumbPageProps {
            class: None,
            children: Some(Element::new().text("Profile")),
            base: None,
            ..Default::default()
        });
        let link_text = link
            .children
            .iter()
            .find(|c| c.text.is_some())
            .expect("link caption");
        let page_text = page
            .children
            .iter()
            .find(|c| c.text.is_some())
            .expect("page caption");
        assert_eq!(
            link_text.text_style.size_resolved(),
            page_text.text_style.size_resolved()
        );
        assert_eq!(
            link_text.text_style.weight_resolved(),
            page_text.text_style.weight_resolved()
        );
        assert_eq!(
            link_text.text_style.line_height_scale,
            page_text.text_style.line_height_scale
        );
        assert_ne!(
            link_text.text_style.line_height_scale,
            Some(1.0),
            "breadcrumb must not use leading-none (keep readable line box with Page)"
        );
    }

    #[test]
    fn list_strips_trailing_separator() {
        let mut list_kids = vec![
            BreadcrumbItem(BreadcrumbItemProps {
                class: None,
                children: Some(BreadcrumbPage(BreadcrumbPageProps {
                    class: None,
                    children: Some(Element::new().text("Last")),
                    base: None,
                    ..Default::default()
                })),
                base: None,
                ..Default::default()
            }),
            BreadcrumbSeparator(BreadcrumbSeparatorProps {
                class: None,
                children: None,
                base: None,
                ..Default::default()
            }),
        ];
        list_kids = strip_trailing_separators(list_kids);
        assert_eq!(list_kids.len(), 1);
        assert_eq!(
            list_kids[0].attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_ITEM)
        );
    }

    #[test]
    fn builder_no_trailing_separator() {
        let el: Element = breadcrumb()
            .link("Home", || {})
            .link("Docs", || {})
            .page("Breadcrumb")
            .into();
        let list = el
            .children
            .iter()
            .find(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_LIST))
            .expect("list");
        assert_ne!(
            list.children
                .last()
                .and_then(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str())),
            Some(SLOT_SEPARATOR),
            "last trail segment must not be a separator"
        );
        let seps = list
            .children
            .iter()
            .filter(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_SEPARATOR))
            .count();
        assert_eq!(seps, 2, "Home / Docs / Breadcrumb → two separators");
    }

    #[test]
    fn builder_assembles_trail() {
        let el: Element = breadcrumb()
            .link("Home", || {})
            .link("Docs", || {})
            .page("Breadcrumb")
            .into();
        assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
        let list = el
            .children
            .iter()
            .find(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_LIST))
            .or_else(|| el.children.first())
            .expect("list");
        assert!(
            list.children
                .iter()
                .any(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_SEPARATOR)),
            "builder inserts separators between segments"
        );
    }

    #[test]
    fn builder_link_ids_are_stable() {
        let el: Element = breadcrumb()
            .link("Home", || {})
            .link("Docs", || {})
            .page("Breadcrumb")
            .into();
        let list = el
            .children
            .iter()
            .find(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_LIST))
            .expect("list");
        let link_ids: Vec<_> = list
            .children
            .iter()
            .filter_map(|item| {
                item.children.first().and_then(|link| {
                    (link.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_LINK))
                        .then(|| link.focus_id.clone())
                })
            })
            .collect();
        assert_eq!(
            link_ids,
            vec![
                Some("breadcrumb-link:Home:0".to_string()),
                Some("breadcrumb-link:Docs:1".to_string()),
            ]
        );
    }

    #[test]
    fn dual_surface_compiles_shape() {
        let _b: Element = breadcrumb().page("Only").into();
        let el = BreadcrumbList(BreadcrumbListProps {
            class: None,
            children: None,
            base: None,
            ..Default::default()
        });
        let _: Element = el;
    }
}
