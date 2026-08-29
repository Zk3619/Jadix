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
//! ## 用法（双写法 · ADR-9）
//! ```ignore
//! // A · RSX / 标签
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
//! ## 注意
//! - **展示型**：空 `id` 不写 `template_id`（WP-2.8）。
//! - `ui!` 多子节点会包 fragment；List / Item 会 [`take_packed`] 拆开横排。
//! - [`BreadcrumbLink`] / [`BreadcrumbPage`] 共用 `text-sm` · `font-normal`（14px / 400）；Link muted → foreground hover。
//! - 勿对 Link 使用 `leading-none`：`text-sm` 默认行高才容得下统一 2px 下划线 offset。
//! - [`BreadcrumbSeparator`] / [`BreadcrumbEllipsis`] 默认 Lucide chevron / more-horizontal。

use crate::components::base::BaseComponentBuilder;
use crate::components::button::button;
use crate::core::tree::{parse_text_color, Element, Handler};
use crate::primitives::r#box::hstack;
use crate::style::{parse_text_color as theme_fg, with_class_list};
use crate::utils::lucide::lucide_glyph;
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

/// `ui!` 多子节点 fragment / 匿名 VStack → 展平为 List / Item 子项。
fn take_packed(mut el: Element) -> Vec<Element> {
    if el.attrs.get("rui_fragment").map(|s| s.as_str()) == Some("1") {
        return std::mem::take(&mut el.children);
    }
    if el.on_click.is_some()
        || el.focus_id.is_some()
        || el.template_id.is_some()
        || el.attrs.contains_key(ATTR_SLOT)
    {
        return vec![el];
    }
    let bare = el.text.as_ref().is_none_or(|t| t.is_empty());
    if bare && el.children.len() > 1 {
        std::mem::take(&mut el.children)
    } else {
        vec![el]
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

fn build_breadcrumb_link(
    id: &str,
    label: &str,
    mut on_click: Handler<()>,
    class: Option<String>,
) -> Element {
    let btn_id = if id.trim().is_empty() { "" } else { id };
    let muted = parse_text_color("muted-foreground");
    let mut btn = button(btn_id)
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

/// 去掉列表末尾多余的分隔符（RSX 误写 / 展平 fragment 时防御）。
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

/// 根：`nav` · `aria-label="breadcrumb"`。
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

/// 单项壳：`inline-flex items-center gap-1.5`。
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

/// 可点击段：Button `link` 变体。
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
    build_breadcrumb_link(&id, &label, on_click, class)
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

/// 分隔符：默认 chevron-right。
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
    Link {
        label: String,
        on_click: Handler<()>,
    },
    Page(String),
    Ellipsis,
}

/// Builder 状态（ADR-9 面 B）。
pub struct BreadcrumbBuilder {
    id: String,
    class: String,
    label: String,
    parts: Vec<BreadcrumbBuilderPart>,
}

/// Builder 入口。
pub fn breadcrumb() -> BreadcrumbBuilder {
    BreadcrumbBuilder {
        id: String::new(),
        class: String::new(),
        label: String::new(),
        parts: Vec::new(),
    }
}

impl BreadcrumbBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.class = class.into();
        self
    }

    pub fn aria_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    pub fn link(mut self, label: impl Into<String>, on_click: impl Fn() + 'static) -> Self {
        self.parts.push(BreadcrumbBuilderPart::Link {
            label: label.into(),
            on_click: Handler::new(move |_| on_click()),
        });
        self
    }

    pub fn page(mut self, label: impl Into<String>) -> Self {
        self.parts.push(BreadcrumbBuilderPart::Page(label.into()));
        self
    }

    pub fn ellipsis(mut self) -> Self {
        self.parts.push(BreadcrumbBuilderPart::Ellipsis);
        self
    }
}

impl From<BreadcrumbBuilder> for Element {
    fn from(builder: BreadcrumbBuilder) -> Self {
        let mut list_kids = Vec::new();
        for (i, part) in builder.parts.into_iter().enumerate() {
            if i > 0 {
                list_kids.push(BreadcrumbSeparator(BreadcrumbSeparatorProps {
                    class: None,
                    children: None,
                    base: None,
                    ..Default::default()
                }));
            }
            match part {
                BreadcrumbBuilderPart::Link { label, on_click } => {
                    list_kids.push(BreadcrumbItem(BreadcrumbItemProps {
                        class: None,
                        children: Some(build_breadcrumb_link("", &label, on_click, None)),
                        base: None,
                        ..Default::default()
                    }));
                }
                BreadcrumbBuilderPart::Page(label) => {
                    list_kids.push(BreadcrumbItem(BreadcrumbItemProps {
                        class: None,
                        children: Some(BreadcrumbPage(BreadcrumbPageProps {
                            class: None,
                            children: Some(Element::new().text(label)),
                            base: None,
                            ..Default::default()
                        })),
                        base: None,
                        ..Default::default()
                    }));
                }
                BreadcrumbBuilderPart::Ellipsis => {
                    list_kids.push(BreadcrumbItem(BreadcrumbItemProps {
                        class: None,
                        children: Some(BreadcrumbEllipsis(BreadcrumbEllipsisProps {
                            class: None,
                            children: None,
                            base: None,
                            ..Default::default()
                        })),
                        base: None,
                        ..Default::default()
                    }));
                }
            }
        }
        let list_children = if list_kids.is_empty() {
            None
        } else {
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            Some(frag.children(list_kids))
        };
        let list = BreadcrumbList(BreadcrumbListProps {
            class: None,
            children: list_children,
            base: None,
            ..Default::default()
        });
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
        if !builder.class.trim().is_empty() {
            el = with_class_list(el, builder.class.trim());
        }
        el
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let el = build_breadcrumb_link("", "Home", Handler::default(), None);
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
        assert!(
            caption.attrs.contains_key("text_underline_offset"),
            "link button caption should carry default underline offset"
        );
        assert_eq!(
            caption.text_style.underline_offset, 2.0,
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
        assert_eq!(text.text_style.color, theme_fg("foreground"));
        assert_eq!(text.text_style.size, BREADCRUMB_TEXT_SIZE_PX);
        assert_eq!(text.text_style.weight, BREADCRUMB_TEXT_WEIGHT);
    }

    #[test]
    fn breadcrumb_link_and_page_typography_match() {
        let link = build_breadcrumb_link("", "Settings", Handler::default(), None);
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
        assert_eq!(link_text.text_style.size, page_text.text_style.size);
        assert_eq!(link_text.text_style.weight, page_text.text_style.weight);
        assert_eq!(
            link_text.text_style.line_height_scale,
            page_text.text_style.line_height_scale
        );
        assert_ne!(
            link_text.text_style.line_height_scale,
            Some(1.0),
            "breadcrumb must not use leading-none — tight line box makes 2px underline hug text"
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
