//! # Pagination
//!
//! 对齐 ShadCN `pagination.tsx`：分页导航复合零件（非 Table 内建分页状态）。
//!
//! ```text
//! Pagination                 // nav · w-full · justify-center
//! └── PaginationContent      // 横排 items-center
//!     └── PaginationItem     // 单项壳
//!         ├── PaginationPrevious / PaginationNext
//!         ├── PaginationLink     // Button outline|ghost · size icon|default
//!         └── PaginationEllipsis
//! ```
//!
//! **身份（WP-2.8）**：展示型壳，**不要求**用户 `id`；空 / 省略 → 不写 `template_id`（I-1）。
//! 受控页码用 [`Signal`]；Builder 未传则按实例键建内部信号。
//!
//! `ui!` 中 `<Pagination>` / `<PaginationLink>` 等由通用 PascalCase → `*Props` 展开
//!（与 [`Navbar`](crate::components::Navbar) 相同，无需专用 scope）。
//!
//! ## 用法（双写法）
//! ```ignore
//! // A. 复合 RSX（推荐）
//! ui! {
//!     <Pagination>
//!         <PaginationContent>
//!             <PaginationItem>
//!                 <PaginationPrevious on_click={move || { /* page - 1 */ }} />
//!             </PaginationItem>
//!             <PaginationItem>
//!                 <PaginationLink is_active={true} on_click={|| {}}>"1"</PaginationLink>
//!             </PaginationItem>
//!             <PaginationItem><PaginationEllipsis /></PaginationItem>
//!             <PaginationItem>
//!                 <PaginationNext on_click={move || { /* page + 1 */ }} />
//!             </PaginationItem>
//!         </PaginationContent>
//!     </Pagination>
//! }
//!
//! // B. Builder（页码窗 + 省略号；page 为 1-based）
//! pagination("docs")
//!     .page_count(12)
//!     .page_signal(page)
//!     .sibling_count(1)
//! ```
//!
//! ## 样式（ShadCN）
//! - 根：`mx-auto flex w-full justify-center` · `role=navigation`
//! - Content：`flex items-center gap-1`
//! - Link：`isActive` → `outline`，否则 `ghost`；默认 `size=icon`
//! - Previous / Next：`size=default` + chevron + 文案

use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use rui_macros::component;

use crate::app::reactive::{reactive_anchor, reactive_placeholder_sized};
use crate::components::button::button;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::{parse_text_color, Element, Handler};
use crate::primitives::r#box::hstack;
use crate::style::with_class_list;
use crate::utils::lucide::lucide_glyph;
use crate::utils::text_node::{build_icon_text_node, build_text_node, TextNodeConfig};

const ATTR_SLOT: &str = "data-slot";
const SLOT_ROOT: &str = "pagination";
const SLOT_CONTENT: &str = "pagination-content";
const SLOT_ITEM: &str = "pagination-item";
const SLOT_LINK: &str = "pagination-link";
const SLOT_PREV: &str = "pagination-previous";
const SLOT_NEXT: &str = "pagination-next";
const SLOT_ELLIPSIS: &str = "pagination-ellipsis";

/// `ui!` 多子节点会先打成匿名 VStack / fragment；再拆开接到自己的布局上。
fn take_packed(mut el: Element) -> Vec<Element> {
    if el.attrs.get("rui_fragment").map(|s| s.as_str()) == Some("1") {
        return std::mem::take(&mut el.children);
    }
    let bare = el.text.as_ref().is_none_or(|t| t.is_empty());
    if bare && el.children.len() > 1 {
        std::mem::take(&mut el.children)
    } else {
        vec![el]
    }
}

fn lucide_icon(name: &str, size_px: f32) -> Element {
    let glyph = lucide_glyph(name).unwrap_or_else(|| "…".into());
    // 图标勿抢点击：命中应落到外层 Button / interactive。
    build_icon_text_node(glyph)
        .text_size(size_px)
        .text_color(parse_text_color("foreground"))
        .pointer_events("none")
}

fn caption_text(text: impl Into<String>) -> Element {
    build_text_node(
        text,
        TextNodeConfig {
            size_px: 14.0,
            weight: 500,
            color: parse_text_color("foreground"),
            leading_scale: Some(1.0),
            ..Default::default()
        },
    )
    .pointer_events("none")
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

// ---------------------------------------------------------------------------
// Page window (Builder)
// ---------------------------------------------------------------------------

/// 页码窗条目：数字或省略号。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaginationPage {
    Number(usize),
    Ellipsis,
}

/// 生成带省略号的页码窗（`page` / 返回值均为 **1-based**）。
///
/// `sibling_count`：当前页两侧各保留的页数（ShadCN 示例常见为 1）。
pub fn pagination_range(
    page: usize,
    page_count: usize,
    sibling_count: usize,
) -> Vec<PaginationPage> {
    let page_count = page_count.max(1);
    let page = page.clamp(1, page_count);
    let sibling = sibling_count;

    if page_count <= sibling * 2 + 5 {
        return (1..=page_count).map(PaginationPage::Number).collect();
    }

    let left = page.saturating_sub(sibling).max(1);
    let right = (page + sibling).min(page_count);

    let mut out = Vec::new();
    let show_left_ellipsis = left > 2;
    let show_right_ellipsis = right < page_count.saturating_sub(1);

    out.push(PaginationPage::Number(1));
    if show_left_ellipsis {
        out.push(PaginationPage::Ellipsis);
    } else if left == 2 {
        out.push(PaginationPage::Number(2));
    }

    let start = if show_left_ellipsis { left } else { left.max(2) };
    let end = if show_right_ellipsis {
        right
    } else {
        right.min(page_count.saturating_sub(1))
    };
    for n in start..=end {
        if n != 1 && n != page_count {
            out.push(PaginationPage::Number(n));
        }
    }

    if show_right_ellipsis {
        out.push(PaginationPage::Ellipsis);
    } else if right == page_count.saturating_sub(1) && page_count > 1 {
        out.push(PaginationPage::Number(page_count - 1));
    }

    if page_count > 1 {
        out.push(PaginationPage::Number(page_count));
    }

    // Dedup consecutive numbers (edge cases around ellipsis boundaries).
    let mut dedup = Vec::with_capacity(out.len());
    for item in out {
        match (dedup.last(), item) {
            (Some(PaginationPage::Number(a)), PaginationPage::Number(b)) if *a == b => {}
            _ => dedup.push(item),
        }
    }
    dedup
}

// ---------------------------------------------------------------------------
// RSX parts
// ---------------------------------------------------------------------------

/// 根：`nav` · 全宽居中。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Pagination(id: String, class: Option<String>, children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut row = hstack()
        .w("100%")
        .justify("center")
        .items_center()
        .gap("0");
    for k in kids {
        row = row.child(k);
    }
    let mut el: Element = row.into();
    el.set_attr(ATTR_SLOT, SLOT_ROOT);
    el = el.accesskit_role("navigation");
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        el = with_class_list(el, &c);
    }
    if !id.trim().is_empty() {
        el.template_id = Some(id);
    }
    el
}

/// 内容行：`flex items-center gap-1`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn PaginationContent(class: Option<String>, children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut row = hstack().gap("0.25rem").items_center().w("auto");
    for k in kids {
        row = row.child(k);
    }
    let mut el: Element = row.into();
    el.set_attr(ATTR_SLOT, SLOT_CONTENT);
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        el = with_class_list(el, &c);
    }
    el
}

/// 单项壳（对齐 `<li>`）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn PaginationItem(class: Option<String>, children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut row = hstack().gap("0").items_center().w("auto").flex_shrink(0.0);
    for k in kids {
        row = row.child(k);
    }
    let mut el: Element = row.into();
    el.set_attr(ATTR_SLOT, SLOT_ITEM);
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        el = with_class_list(el, &c);
    }
    el
}

/// 页码 / 导航链接：`is_active` → outline，否则 ghost；默认 `size=icon`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn PaginationLink(
    id: String,
    class: Option<String>,
    is_active: bool,
    size: String,
    disabled: bool,
    #[prop(default = "Handler::default()")] mut on_click: Handler<()>,
    children: Option<Element>,
) -> Element {
    let size = if size.trim().is_empty() {
        "icon"
    } else {
        size.trim()
    };
    let variant = if is_active { "outline" } else { "ghost" };
    let label = children
        .as_ref()
        .map(extract_text_leaf)
        .filter(|s| !s.is_empty())
        .unwrap_or_default();

    let mut btn = button(if id.trim().is_empty() { "" } else { &id })
        .variant(variant)
        .size(size)
        .disabled(disabled);
    if let Some(c) = children {
        let kids = take_packed(c);
        if kids.len() == 1 && kids[0].text.as_ref().is_some_and(|t| !t.is_empty()) {
            btn = btn.text(label);
        } else {
            for k in kids {
                btn = btn.child(k);
            }
        }
    } else if !label.is_empty() {
        btn = btn.text(label);
    }
    btn = btn.on_click(move || on_click.call(()));
    let mut el: Element = btn.into();
    el.set_attr(ATTR_SLOT, SLOT_LINK);
    if is_active {
        el.set_attr("data-active", "true");
        el.set_attr("aria-current", "page");
    }
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        el = with_class_list(el, &c);
    }
    el
}

/// 上一页：chevron-left + 文案（默认 `"Previous"`）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn PaginationPrevious(
    id: String,
    class: Option<String>,
    text: String,
    disabled: bool,
    #[prop(default = "Handler::default()")] mut on_click: Handler<()>,
    children: Option<Element>,
) -> Element {
    let text = if text.trim().is_empty() {
        "Previous".to_string()
    } else {
        text
    };
    let mut btn = button(if id.trim().is_empty() { "" } else { &id })
        .variant("ghost")
        .size("default")
        .disabled(disabled)
        .on_click(move || on_click.call(()))
        .child(lucide_icon("chevron-left", 14.0));
    if let Some(c) = children {
        for k in take_packed(c) {
            btn = btn.child(k);
        }
    } else {
        btn = btn.child(caption_text(text));
    }
    let mut el: Element = btn.into();
    el.set_attr(ATTR_SLOT, SLOT_PREV);
    el.set_attr("aria-label", "Go to previous page");
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        el = with_class_list(el, &c);
    }
    el
}

/// 下一页：文案（默认 `"Next"`）+ chevron-right。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn PaginationNext(
    id: String,
    class: Option<String>,
    text: String,
    disabled: bool,
    #[prop(default = "Handler::default()")] mut on_click: Handler<()>,
    children: Option<Element>,
) -> Element {
    let text = if text.trim().is_empty() {
        "Next".to_string()
    } else {
        text
    };
    let mut btn = button(if id.trim().is_empty() { "" } else { &id })
        .variant("ghost")
        .size("default")
        .disabled(disabled)
        .on_click(move || on_click.call(()));
    if let Some(c) = children {
        for k in take_packed(c) {
            btn = btn.child(k);
        }
    } else {
        btn = btn.child(caption_text(text));
    }
    btn = btn.child(lucide_icon("chevron-right", 14.0));
    let mut el: Element = btn.into();
    el.set_attr(ATTR_SLOT, SLOT_NEXT);
    el.set_attr("aria-label", "Go to next page");
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        el = with_class_list(el, &c);
    }
    el
}

/// 省略号：`more-horizontal`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn PaginationEllipsis(class: Option<String>) -> Element {
    let mut el: Element = hstack()
        .h("2.25rem")
        .w("2.25rem")
        .items_center()
        .justify("center")
        .flex_shrink(0.0)
        .child(lucide_icon("more-horizontal", 14.0))
        .into();
    el.set_attr(ATTR_SLOT, SLOT_ELLIPSIS);
    el.set_attr("aria-hidden", "true");
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        el = with_class_list(el, &c);
    }
    el
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

pub struct PaginationBuilder {
    id: String,
    class: Option<String>,
    page_count: usize,
    page: Option<Signal<usize>>,
    default_page: usize,
    sibling_count: usize,
    previous_text: String,
    next_text: String,
    on_page_change: Option<Rc<dyn Fn(usize)>>,
}

/// 便捷入口：按 `page_count` 生成 Prev / 页码窗 / Next（复合零件组装）。
///
/// 展示型默认无用户 id；需要稳定受控槽或热重载时再 `.id(...)`。
pub fn pagination(id: impl Into<String>) -> PaginationBuilder {
    PaginationBuilder {
        id: id.into(),
        class: None,
        page_count: 1,
        page: None,
        default_page: 1,
        sibling_count: 1,
        previous_text: "Previous".into(),
        next_text: "Next".into(),
        on_page_change: None,
    }
}

impl PaginationBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }
    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.class = Some(class.into());
        self
    }
    pub fn page_count(mut self, n: usize) -> Self {
        self.page_count = n.max(1);
        self
    }
    /// 当前页（**1-based**）信号。
    pub fn page_signal(mut self, s: Signal<usize>) -> Self {
        self.page = Some(s);
        self
    }
    pub fn default_page(mut self, page: usize) -> Self {
        self.default_page = page.max(1);
        self
    }
    pub fn sibling_count(mut self, n: usize) -> Self {
        self.sibling_count = n;
        self
    }
    pub fn previous_text(mut self, text: impl Into<String>) -> Self {
        self.previous_text = text.into();
        self
    }
    pub fn next_text(mut self, text: impl Into<String>) -> Self {
        self.next_text = text.into();
        self
    }
    pub fn on_page_change(mut self, f: impl Fn(usize) + 'static) -> Self {
        self.on_page_change = Some(Rc::new(f));
        self
    }
}

fn paint_builder_content(
    page: usize,
    page_count: usize,
    sibling_count: usize,
    previous_text: &str,
    next_text: &str,
    page_sig: Signal<usize>,
    on_page_change: Option<Rc<dyn Fn(usize)>>,
) -> Element {
    let page = page.clamp(1, page_count.max(1));
    let notify = Rc::new(move |n: usize| {
        page_sig.set(n);
        if let Some(ref cb) = on_page_change {
            cb(n);
        }
    });

    let mut items = Vec::new();

    let prev_disabled = page <= 1;
    let prev_notify = Rc::clone(&notify);
    items.push(PaginationItem(PaginationItemProps {
        class: None,
        children: Some(PaginationPrevious(PaginationPreviousProps {
            id: String::new(),
            class: None,
            text: Some(previous_text.to_string()),
            disabled: Some(prev_disabled),
            on_click: Some(Handler::new(move |_| {
                if page > 1 {
                    prev_notify(page - 1);
                }
            })),
            children: None,
            base: None,
        })),
        base: None,
    }));

    for entry in pagination_range(page, page_count, sibling_count) {
        match entry {
            PaginationPage::Ellipsis => {
                items.push(PaginationItem(PaginationItemProps {
                    class: None,
                    children: Some(PaginationEllipsis(PaginationEllipsisProps {
                        class: None,
                        children: None,
                        base: None,
                    })),
                    base: None,
                }));
            }
            PaginationPage::Number(n) => {
                let go = Rc::clone(&notify);
                items.push(PaginationItem(PaginationItemProps {
                    class: None,
                    children: Some(PaginationLink(PaginationLinkProps {
                        id: String::new(),
                        class: None,
                        is_active: Some(n == page),
                        size: Some("icon".into()),
                        disabled: Some(false),
                        on_click: Some(Handler::new(move |_| go(n))),
                        children: Some(Element::new().text(n.to_string())),
                        base: None,
                    })),
                    base: None,
                }));
            }
        }
    }

    let next_disabled = page >= page_count;
    let next_notify = Rc::clone(&notify);
    items.push(PaginationItem(PaginationItemProps {
        class: None,
        children: Some(PaginationNext(PaginationNextProps {
            id: String::new(),
            class: None,
            text: Some(next_text.to_string()),
            disabled: Some(next_disabled),
            on_click: Some(Handler::new(move |_| {
                if page < page_count {
                    next_notify(page + 1);
                }
            })),
            children: None,
            base: None,
        })),
        base: None,
    }));

    let mut frag = Element::new();
    frag.set_attr("rui_fragment", "1");
    PaginationContent(PaginationContentProps {
        class: None,
        children: Some(frag.children(items)),
        base: None,
    })
}

static NEXT_PAGINATION_ID: AtomicUsize = AtomicUsize::new(1);

impl From<PaginationBuilder> for Element {
    fn from(builder: PaginationBuilder) -> Self {
        let PaginationBuilder {
            id,
            class,
            page_count,
            page,
            default_page,
            sibling_count,
            previous_text,
            next_text,
            on_page_change,
        } = builder;
        let instance = if id.trim().is_empty() {
            format!(
                "pagination-{}",
                NEXT_PAGINATION_ID.fetch_add(1, Ordering::Relaxed)
            )
        } else {
            id.clone()
        };
        let page_sig = page.unwrap_or_else(|| {
            use_signal(format!("{instance}__page").as_str(), move || {
                default_page.clamp(1, page_count.max(1))
            })
        });
        let view_id = format!("{instance}__pagination_view");
        let prev_text = previous_text.clone();
        let next_text = next_text.clone();
        let class_k = class.clone();
        let id_k = id.clone();

        reactive_anchor(view_id.clone(), move || {
            let content = paint_builder_content(
                page_sig.get(),
                page_count,
                sibling_count,
                &prev_text,
                &next_text,
                page_sig,
                on_page_change.clone(),
            );
            Pagination(PaginationProps {
                id: id_k.clone(),
                class: class_k.clone(),
                children: Some(content),
                base: None,
            })
        });
        // hug 内容高度；勿用 reactive_view 默认 100%×100%（会撑满父级 / 盖住邻居）。
        reactive_placeholder_sized(view_id, "100%", "auto")
    }
}

impl crate::core::tree::IntoElementList for PaginationBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_small_lists_all_pages() {
        assert_eq!(
            pagination_range(1, 5, 1),
            (1..=5).map(PaginationPage::Number).collect::<Vec<_>>()
        );
    }

    #[test]
    fn range_inserts_ellipsis() {
        let r = pagination_range(5, 12, 1);
        assert!(r.contains(&PaginationPage::Ellipsis));
        assert_eq!(r.first(), Some(&PaginationPage::Number(1)));
        assert_eq!(r.last(), Some(&PaginationPage::Number(12)));
        assert!(r.contains(&PaginationPage::Number(5)));
    }

    #[test]
    fn rsx_root_exposes_slot() {
        let el = Pagination(PaginationProps {
            id: String::new(),
            class: None,
            children: Some(PaginationContent(PaginationContentProps {
                class: None,
                children: Some(PaginationItem(PaginationItemProps {
                    class: None,
                    children: Some(PaginationEllipsis(PaginationEllipsisProps {
                        class: None,
                        children: None,
                        base: None,
                    })),
                    base: None,
                })),
                base: None,
            })),
            base: None,
        });
        assert_eq!(
            el.attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_ROOT)
        );
        assert!(el.template_id.is_none());
    }

    #[test]
    fn link_active_sets_data_active() {
        let el = PaginationLink(PaginationLinkProps {
            id: String::new(),
            class: None,
            is_active: Some(true),
            size: Some("icon".into()),
            disabled: Some(false),
            on_click: Some(Handler::default()),
            children: Some(Element::new().text("3")),
            base: None,
        });
        assert_eq!(el.attrs.get("data-active").map(String::as_str), Some("true"));
        assert_eq!(
            el.attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_LINK)
        );
    }

    #[test]
    fn pagination_link_wires_on_click() {
        use std::cell::Cell;
        let hit = Rc::new(Cell::new(0usize));
        let hit2 = Rc::clone(&hit);
        let el = PaginationLink(PaginationLinkProps {
            id: "pg-link-click".into(),
            class: None,
            is_active: Some(false),
            size: Some("icon".into()),
            disabled: Some(false),
            on_click: Some(Handler::new(move |_| hit2.set(hit2.get() + 1))),
            children: Some(Element::new().text("2")),
            base: None,
        });
        assert!(el.on_click.is_some(), "link must expose on_click");
        el.on_click.as_ref().unwrap().borrow_mut()();
        assert_eq!(hit.get(), 1);
    }

    #[test]
    fn builder_content_wires_page_link_clicks() {
        let page = use_signal("pagination_builder_click_page", || 2usize);
        let el = paint_builder_content(2, 5, 1, "Prev", "Next", page, None);
        assert_eq!(
            el.attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_CONTENT)
        );
        let mut found_click = false;
        fn walk(el: &Element, found: &mut bool) {
            if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_LINK)
                && el.on_click.is_some()
            {
                *found = true;
            }
            for c in &el.children {
                walk(c, found);
            }
        }
        walk(&el, &mut found_click);
        assert!(found_click, "builder page links must wire on_click");
    }
}
