//! # Navbar
//!
//! 顶栏布局壳（对齐站点 Header 习惯）：左侧 brand / 中间导览 / 右侧 actions。
//! 链接导览请嵌 [`crate::components::navigation_menu`]；应用命令见
//! [`crate::components::menubar`]；页面栈见 [`crate::app::navigator::navigation_host`]。
//!
//! ```text
//! Navbar
//! ├── NavbarBrand
//! ├── NavbarCenter          // 常嵌 NavigationMenu
//! └── NavbarActions
//! ```
//!
//! **身份（WP-2.8）**：展示型壳，**不要求**用户 `id`；空 / 省略 → 不写 `template_id`（I-1）。
//!
//! `ui!` 中 `<Navbar>` / `<NavbarBrand>` 等由通用 PascalCase → `*Props` 展开（与
//! [`Card`](crate::components::Card) 相同，无需专用 scope）。
//!
//! ## 用法（双写法）
//! ```ignore
//! // A. 复合 RSX（推荐）
//! ui! {
//!     <Navbar>
//!         <NavbarBrand><Label font_weight="600">"RUI"</Label></NavbarBrand>
//!         <NavbarCenter>
//!             <NavigationMenu id="main">…</NavigationMenu>
//!         </NavbarCenter>
//!         <NavbarActions>
//!             <Button variant="outline" size="sm">"Action"</Button>
//!         </NavbarActions>
//!     </Navbar>
//! }
//!
//! // B. Builder（默认与 RSX 同为 start；展示层可 `.center_justify("center")`）
//! navbar()
//!     .brand(label("").text("RUI"))
//!     .center_child(navigation_menu("main").link("docs", "Docs", || {}))
//!     .action(button("").variant("outline").size("sm").text("Action"))
//! ```
//!
//! ## 样式
//! - 根：`h-14` · 全宽 · `px-4` · `items-center` · `min-w-0` · `overflow-x: auto`（窄屏横滑，避免重叠）
//! - Brand / Actions：`shrink-0` · `items-center`
//! - Center：默认 `justify-start`（贴 Brand）；可用 `justify` / Builder `.center_justify(...)` 覆盖

use rui_macros::component;

use crate::core::tree::Element;
use crate::primitives::r#box::hstack;
use crate::style::with_class_list;
use crate::tokens;

const ATTR_SLOT: &str = "data-slot";
const SLOT_NAVBAR: &str = "navbar";
const SLOT_BRAND: &str = "navbar-brand";
const SLOT_CENTER: &str = "navbar-center";
const SLOT_ACTIONS: &str = "navbar-actions";

/// `ui!` 多子节点会先打成匿名 VStack / fragment；再拆开接到自己的布局上。
fn take_packed_children(mut el: Element) -> Vec<Element> {
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

fn paint_navbar_root(mut bar: Element, id: &str, class: Option<String>) -> Element {
    bar = bar
        .bg(tokens::background())
        .border_style("solid")
        .border_width(1.0)
        .border_color(tokens::border())
        .overflow_x("auto")
        .overflow_y("hidden");
    bar.set_attr(ATTR_SLOT, SLOT_NAVBAR);
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        bar = with_class_list(bar, &c);
    }
    // Display shell: never write Some("") (I-1).
    if !id.trim().is_empty() {
        bar.template_id = Some(id.to_string());
    }
    bar
}

/// 顶栏根容器：水平排列 Brand / Center / Actions（或任意子节点）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Navbar(id: String, class: Option<String>, children: Option<Element>) -> Element {
    let kids = children.map(take_packed_children).unwrap_or_default();
    let mut bar = hstack()
        .w("100%")
        .min_w(0.0)
        .h("3.5rem")
        .px("1rem")
        .items_center()
        .gap("1rem");
    for k in kids {
        bar = bar.child(k);
    }
    paint_navbar_root(bar.into(), &id, class)
}

/// 左侧品牌区（logo / 产品名）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn NavbarBrand(class: Option<String>, children: Option<Element>) -> Element {
    let kids = children.map(take_packed_children).unwrap_or_default();
    let mut row = hstack()
        .gap("0.75rem")
        .items_center()
        .h("100%")
        .flex_shrink(0.0);
    for k in kids {
        row = row.child(k);
    }
    let mut el: Element = row.into();
    el.set_attr(ATTR_SLOT, SLOT_BRAND);
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        el = with_class_list(el, &c);
    }
    el
}

/// 中间导览区（常嵌 [`crate::components::NavigationMenu`]）。
///
/// 默认 `justify-start`（贴 Brand）。展示层可用 `justify="center"` 或 Builder
/// [`.center_justify`](NavbarBuilder::center_justify) 覆盖。
/// `min-w-0` + `overflow-x: auto`：窄屏横滑，不叠到 Actions。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn NavbarCenter(
    class: Option<String>,
    /// 主轴对齐：`start`（默认）/ `center` / `end` / `between` …
    justify: Option<String>,
    children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed_children).unwrap_or_default();
    let justify = justify
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("start");
    let mut row = hstack()
        .gap("0.5rem")
        .items_center()
        .justify(justify)
        .flex_grow(1.0)
        .flex_shrink(1.0)
        .min_w(0.0)
        .h("100%")
        .overflow_x("auto")
        .overflow_y("hidden");
    for k in kids {
        row = row.child(k);
    }
    let mut el: Element = row.into();
    el.set_attr(ATTR_SLOT, SLOT_CENTER);
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        el = with_class_list(el, &c);
    }
    el
}

/// 右侧动作区（按钮 / 头像等）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn NavbarActions(class: Option<String>, children: Option<Element>) -> Element {
    let kids = children.map(take_packed_children).unwrap_or_default();
    let mut row = hstack()
        .gap("0.5rem")
        .items_center()
        .h("100%")
        .flex_shrink(0.0);
    for k in kids {
        row = row.child(k);
    }
    let mut el: Element = row.into();
    el.set_attr(ATTR_SLOT, SLOT_ACTIONS);
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        el = with_class_list(el, &c);
    }
    el
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

pub struct NavbarBuilder {
    id: String,
    class: Option<String>,
    brand: Option<Element>,
    center: Vec<Element>,
    /// 中栏主轴对齐；默认与 RSX 相同（`start`），由调用方覆盖。
    center_justify: Option<String>,
    actions: Vec<Element>,
}

/// 展示型入口：默认无用户 id（WP-2.8）。需要热重载槽时再 `.id(...)`。
pub fn navbar() -> NavbarBuilder {
    NavbarBuilder {
        id: String::new(),
        class: None,
        brand: None,
        center: Vec::new(),
        center_justify: None,
        actions: Vec::new(),
    }
}

impl NavbarBuilder {
    /// 可选槽位 / 测试选择器；空串等同未设置。
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }
    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.class = Some(class.into());
        self
    }
    pub fn brand(mut self, el: impl Into<Element>) -> Self {
        self.brand = Some(el.into());
        self
    }
    pub fn center_child(mut self, el: impl Into<Element>) -> Self {
        self.center.push(el.into());
        self
    }
    /// 覆盖中栏主轴对齐（默认 `start`，与 RSX `<NavbarCenter>` 一致）。
    pub fn center_justify(mut self, kw: impl Into<String>) -> Self {
        self.center_justify = Some(kw.into());
        self
    }
    pub fn action(mut self, el: impl Into<Element>) -> Self {
        self.actions.push(el.into());
        self
    }
}

impl From<NavbarBuilder> for Element {
    fn from(builder: NavbarBuilder) -> Self {
        let NavbarBuilder {
            id,
            class,
            brand,
            center,
            center_justify,
            actions,
        } = builder;
        let mut kids = Vec::new();
        if let Some(b) = brand {
            kids.push(NavbarBrand(NavbarBrandProps {
                class: None,
                children: Some(b),
                base: None,
            }));
        }
        if !center.is_empty() {
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            kids.push(NavbarCenter(NavbarCenterProps {
                class: None,
                justify: center_justify,
                children: Some(frag.children(center)),
                base: None,
            }));
        }
        if !actions.is_empty() {
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            kids.push(NavbarActions(NavbarActionsProps {
                class: None,
                children: Some(frag.children(actions)),
                base: None,
            }));
        }
        let mut frag = Element::new();
        frag.set_attr("rui_fragment", "1");
        Navbar(NavbarProps {
            id,
            class,
            children: Some(frag.children(kids)),
            base: None,
        })
    }
}

impl crate::core::tree::IntoElementList for NavbarBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navbar_without_id_writes_no_template_id() {
        let el: Element = navbar().into();
        assert!(el.template_id.is_none());
        assert_eq!(
            el.attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_NAVBAR)
        );
    }

    #[test]
    fn navbar_empty_id_writes_no_template_id() {
        let el: Element = navbar().id("").into();
        assert!(el.template_id.is_none());
        assert_ne!(el.template_id.as_deref(), Some(""));
    }

    #[test]
    fn navbar_explicit_id_sets_template_id() {
        let el: Element = navbar().id("nav-main").into();
        assert_eq!(el.template_id.as_deref(), Some("nav-main"));
    }

    #[test]
    fn navbar_rsx_parts_expose_slots() {
        let el = Navbar(NavbarProps {
            id: String::new(),
            class: None,
            children: Some({
                let mut frag = Element::new();
                frag.set_attr("rui_fragment", "1");
                frag.children(vec![
                    NavbarBrand(NavbarBrandProps {
                        class: None,
                        children: Some(Element::new().text("Brand")),
                        base: None,
                    }),
                    NavbarCenter(NavbarCenterProps {
                        class: None,
                        justify: None,
                        children: Some(Element::new().text("Center")),
                        base: None,
                    }),
                    NavbarActions(NavbarActionsProps {
                        class: None,
                        children: Some(Element::new().text("Go")),
                        base: None,
                    }),
                ])
            }),
            base: None,
        });
        assert_eq!(
            el.attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_NAVBAR)
        );
        assert_eq!(el.children.len(), 3);
        assert_eq!(
            el.children[0].attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_BRAND)
        );
        assert_eq!(
            el.children[1].attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_CENTER)
        );
        assert_eq!(
            el.children[2].attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_ACTIONS)
        );
    }

    #[test]
    fn navbar_builder_defaults_mid_slot_to_start() {
        let el: Element = navbar()
            .brand(Element::new().text("B"))
            .center_child(Element::new().text("C"))
            .action(Element::new().text("A"))
            .into();
        let center = el
            .children
            .iter()
            .find(|c| c.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_CENTER))
            .expect("center slot");
        assert_eq!(
            center.style.justify_content,
            Some(taffy::style::JustifyContent::FLEX_START)
        );
    }

    #[test]
    fn navbar_builder_center_justify_override() {
        let el: Element = navbar()
            .center_child(Element::new().text("C"))
            .center_justify("center")
            .into();
        let center = &el.children[0];
        assert_eq!(
            center.style.justify_content,
            Some(taffy::style::JustifyContent::CENTER)
        );
    }

    #[test]
    fn navbar_rsx_center_defaults_to_start() {
        let el = NavbarCenter(NavbarCenterProps {
            class: None,
            justify: None,
            children: Some(Element::new().text("C")),
            base: None,
        });
        assert_eq!(
            el.style.justify_content,
            Some(taffy::style::JustifyContent::FLEX_START)
        );
    }
}
