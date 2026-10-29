//! # Navbar
//!
//! 顶栏布局壳（对齐站点 Header 习惯）：左侧 brand / 中间导览 / 右侧 actions。
//! Please embed [`crate::components::navigation_menu`] for link navigation; see the application command
//! [`crate::components::menubar`]; see [`crate::app::navigator::navigation_host`] for the page stack.
//!
//! ```text
//! Navbar
//! ├── NavbarBrand
//! ├── NavbarCenter // often embedded NavigationMenu
//! └── NavbarActions
//! ```
//!
//! **Identity (WP-2.8)**: presentational shell; user `id` **not required**; empty/omitted → no `template_id` (I-1).
//!
//! `<Navbar>` / `<NavbarBrand>` in `ui!` are expanded by general PascalCase → `*Props` (with
//! [`Card`](crate::components::Card) is the same, no dedicated scope is required).
//!
//! ## Usage (dual style)
//! ```ignore
//! // A. Composite RSX (recommended)
//! ui! {
//!     <Navbar>
//!         <NavbarBrand><Label font_weight="600">"Jadix"</Label></NavbarBrand>
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
//!     .brand(label("").text("Jadix"))
//!     .center_child(navigation_menu("main").link("docs", "Docs", || {}))
//!     .action(button("").variant("outline").size("sm").text("Action"))
//! ```
//!
//! ## 样式
//! - 根：`h-14` · 全宽 · `px-4` · `items-center` · `min-w-0` · `overflow-x: auto`（窄屏横滑，避免重叠）
//! - Brand / Actions：`shrink-0` · `items-center`
//! - Center：默认 `justify-start`（贴 Brand）；可用 `justify` / Builder `.center_justify(...)` 覆盖

use crate::components::base::{data_slot, ensure_data_slot, packed_children, SlotChrome};
use crate::core::tree::Element;
use crate::impl_slot_chrome_methods;
use crate::primitives::r#box::hstack;
use crate::tokens;
use crate::utils::take_packed;

const ATTR_SLOT: &str = "data-slot";
const SLOT_NAVBAR: &str = "navbar";
const SLOT_BRAND: &str = "navbar-brand";
const SLOT_CENTER: &str = "navbar-center";
const SLOT_ACTIONS: &str = "navbar-actions";
fn paint_navbar_root(mut bar: Element, id: &str) -> Element {
    bar = bar
        .bg(tokens::background())
        .border_style("solid")
        .border_width(1.0)
        .border_color(tokens::border())
        .overflow_x("auto")
        .overflow_y("hidden");
    bar.set_attr(ATTR_SLOT, SLOT_NAVBAR);
    // Display shell: never write Some("") (I-1).
    if !id.trim().is_empty() {
        bar.template_id = Some(id.to_string());
    }
    bar
}

crate::__component_contract! {
NavbarProps [];
/// 顶栏根容器：水平排列 Brand / Center / Actions（或任意子节点）。
#[allow(non_snake_case)]
pub fn Navbar(
identity [] id: String,
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
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
    paint_navbar_root(bar.into(), &id)
}
}

crate::__component_contract! {
NavbarBrandProps [];
/// 左侧品牌区（logo / 产品名）。
#[allow(non_snake_case)]
pub fn NavbarBrand(
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
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
    el
}
}

crate::__component_contract! {
NavbarCenterProps [];
/// 中间导览区（常嵌 [`crate::components::NavigationMenu`]）。
///
/// 默认 `justify-start`（贴 Brand）。展示层可用 `justify="center"` 或 Builder
/// [`.center_justify`](NavbarBuilder::center_justify) 覆盖。
/// `min-w-0` + `overflow-x: auto`：窄屏横滑，不叠到 Actions。
#[allow(non_snake_case)]
pub fn NavbarCenter(
plain [] justify: Option<String>,
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
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
    el
}
}

crate::__component_contract! {
NavbarActionsProps [];
/// 右侧动作区（按钮 / 头像等）。
#[allow(non_snake_case)]
pub fn NavbarActions(
plain [] children: Option<Element>,
) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
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
    el
}
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

pub struct NavbarBrandBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn navbar_brand() -> NavbarBrandBuilder {
    NavbarBrandBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl NavbarBrandBuilder {
    impl_slot_chrome_methods!();

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }
}

impl crate::core::tree::IntoElementList for NavbarBrandBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<NavbarBrandBuilder> for Element {
    fn from(builder: NavbarBrandBuilder) -> Self {
        let el = NavbarBrand(NavbarBrandProps {
            children: packed_children(builder.children),
            base: None,
        });
        builder.chrome.apply(el)
    }
}

pub struct NavbarCenterBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
    justify: Option<String>,
}

pub fn navbar_center() -> NavbarCenterBuilder {
    NavbarCenterBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
        justify: None,
    }
}

impl NavbarCenterBuilder {
    impl_slot_chrome_methods!();

    pub fn justify(mut self, kw: impl Into<String>) -> Self {
        self.justify = Some(kw.into());
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }
}

impl crate::core::tree::IntoElementList for NavbarCenterBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<NavbarCenterBuilder> for Element {
    fn from(builder: NavbarCenterBuilder) -> Self {
        let el = NavbarCenter(NavbarCenterProps {
            justify: builder.justify,
            children: packed_children(builder.children),
            base: None,
        });
        builder.chrome.apply(el)
    }
}

pub struct NavbarActionsBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn navbar_actions() -> NavbarActionsBuilder {
    NavbarActionsBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl NavbarActionsBuilder {
    impl_slot_chrome_methods!();

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }
}

impl crate::core::tree::IntoElementList for NavbarActionsBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<NavbarActionsBuilder> for Element {
    fn from(builder: NavbarActionsBuilder) -> Self {
        let el = NavbarActions(NavbarActionsProps {
            children: packed_children(builder.children),
            base: None,
        });
        builder.chrome.apply(el)
    }
}

pub struct NavbarBuilder {
    id: String,
    chrome: SlotChrome,
    brand: Option<Element>,
    center: Vec<Element>,
    /// 中栏主轴对齐；默认与 RSX 相同（`start`），由调用方覆盖。
    center_justify: Option<String>,
    actions: Vec<Element>,
    children: Vec<Element>,
}

/// 展示型入口：默认无用户 id（WP-2.8）。需要热重载槽时再 `.id(...)`。
pub fn navbar() -> NavbarBuilder {
    NavbarBuilder {
        id: String::new(),
        chrome: SlotChrome::default(),
        brand: None,
        center: Vec::new(),
        center_justify: None,
        actions: Vec::new(),
        children: Vec::new(),
    }
}

impl NavbarBuilder {
    impl_slot_chrome_methods!();

    /// 可选槽位 / 测试选择器；空串等同未设置。
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn brand(mut self, el: impl Into<Element>) -> Self {
        let el = el.into();
        self.brand = Some(ensure_data_slot(el, SLOT_BRAND, |c| {
            navbar_brand().child(c).into()
        }));
        self
    }

    pub fn brand_with(mut self, f: impl FnOnce(NavbarBrandBuilder) -> NavbarBrandBuilder) -> Self {
        self.brand = Some(f(navbar_brand()).into());
        self
    }

    pub fn center_child(mut self, el: impl Into<Element>) -> Self {
        self.center.push(el.into());
        self
    }

    pub fn center_with(
        mut self,
        f: impl FnOnce(NavbarCenterBuilder) -> NavbarCenterBuilder,
    ) -> Self {
        self.center.push(f(navbar_center()).into());
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

    pub fn actions_with(
        mut self,
        f: impl FnOnce(NavbarActionsBuilder) -> NavbarActionsBuilder,
    ) -> Self {
        self.actions.push(f(navbar_actions()).into());
        self
    }

    pub fn child(mut self, el: impl Into<Element>) -> Self {
        self.children.push(el.into());
        self
    }
}

fn push_center_slots(
    kids: &mut Vec<Element>,
    center: Vec<Element>,
    center_justify: Option<String>,
) {
    if center.is_empty() {
        return;
    }
    if center.len() == 1 && data_slot(&center[0]) == Some(SLOT_CENTER) {
        kids.push(center.into_iter().next().expect("center slot"));
        return;
    }
    let mut unslotted = Vec::new();
    for c in center {
        if data_slot(&c) == Some(SLOT_CENTER) {
            kids.push(c);
        } else {
            unslotted.push(c);
        }
    }
    if !unslotted.is_empty() {
        let mut b = navbar_center();
        if let Some(j) = center_justify {
            b = b.justify(j);
        }
        for c in unslotted {
            b = b.child(c);
        }
        kids.push(b.into());
    }
}

fn push_action_slots(kids: &mut Vec<Element>, actions: Vec<Element>) {
    if actions.is_empty() {
        return;
    }
    if actions.len() == 1 && data_slot(&actions[0]) == Some(SLOT_ACTIONS) {
        kids.push(actions.into_iter().next().expect("actions slot"));
        return;
    }
    let mut unslotted = Vec::new();
    for a in actions {
        if data_slot(&a) == Some(SLOT_ACTIONS) {
            kids.push(a);
        } else {
            unslotted.push(a);
        }
    }
    if !unslotted.is_empty() {
        let mut b = navbar_actions();
        for a in unslotted {
            b = b.child(a);
        }
        kids.push(b.into());
    }
}

impl From<NavbarBuilder> for Element {
    fn from(builder: NavbarBuilder) -> Self {
        let NavbarBuilder {
            id,
            chrome,
            brand,
            center,
            center_justify,
            actions,
            children,
        } = builder;
        let mut kids = Vec::new();
        if let Some(b) = brand {
            kids.push(b);
        }
        push_center_slots(&mut kids, center, center_justify);
        push_action_slots(&mut kids, actions);
        kids.extend(children);
        let el = Navbar(NavbarProps {
            id,
            children: packed_children(kids),
            base: None,
        });
        chrome.apply(el)
    }
}

impl crate::core::tree::IntoElementList for NavbarBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/navbar.rs"]
mod tests;
