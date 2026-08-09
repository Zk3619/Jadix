//! # Tabs
//!
//! 对齐 solid-ui / Kobalte Tabs：
//! - [`tabs_headless`] / [`Tabs`]：无头 TabList（选中 index + ←/→），**不绘制**可见标签 UI
//! - [`tabs_list`] / [`TabsList`]：可见 TabsList 样式壳（`bg-muted` 连体条带），值用 `Signal<String>`；
//!   **始终选中一项**（`required`）；方向键只移动焦点环，Enter/Space/点击才改选中
//!
//! ## 用法
//! ```ignore
//! // 无头：自绘标签行 + 面板
//! <Tabs id="main" count={3}>…自绘…</Tabs>
//! Element::from(tabs_headless("hello_tabs", 3).selected_signal(tab_idx))
//!
//! // 可见条带（solid-ui TabsList / TabsTrigger 外观）
//! Element::from(
//!   tabs_list("view")
//!     .value_signal(view)
//!     .options(vec![("list".into(), "List".into()), ("grid".into(), "Grid".into())]),
//! )
//! // 面板：<Show when={view.get() == "list"}>…</Show>
//! ```
//!
//! ## 注意
//! - 无头选中 index 由组件内 [`crate::app::reactive::reactive_view`] 边界订阅，无需手动 `<Reactive>`。
//! - 无头 `focus_id` 为 `tabstrip:{id}`（AccessKit TabList）。

use crate::app::reactive::reactive_view;
use crate::components::toggle_group::toggle_group;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::interactive::interactive;
use crate::utils::roving::{roving_edge, roving_move};
use rui_macros::component;

/// AccessKit / 焦点约定前缀。
pub const TABSTRIP_FOCUS_PREFIX: &str = "tabstrip:";

// ── Headless TabList ──────────────────────────────────────────────────────────

pub struct TabsHeadlessBuilder {
    id: String,
    count: usize,
    selected: Option<Signal<usize>>,
    default_selected: usize,
    children: Vec<Element>,
}

pub fn tabs_headless(id: impl Into<String>, tab_count: usize) -> TabsHeadlessBuilder {
    TabsHeadlessBuilder {
        id: id.into(),
        count: tab_count.max(1),
        selected: None,
        default_selected: 0,
        children: Vec::new(),
    }
}

impl TabsHeadlessBuilder {
    pub fn selected_signal(mut self, s: Signal<usize>) -> Self {
        self.selected = Some(s);
        self
    }

    pub fn default_selected(mut self, i: usize) -> Self {
        self.default_selected = i;
        self
    }

    pub fn child(mut self, c: Element) -> Self {
        self.children.push(c);
        self
    }

    pub fn children(mut self, c: Vec<Element>) -> Self {
        self.children.extend(c);
        self
    }
}

impl From<TabsHeadlessBuilder> for Element {
    fn from(builder: TabsHeadlessBuilder) -> Self {
        let TabsHeadlessBuilder {
            id,
            count,
            selected,
            default_selected,
            children,
        } = builder;
        let scope = format!("{id}__tabs");
        let sel = selected.unwrap_or_else(|| {
            use_signal(scope.as_str(), || {
                default_selected.min(count.saturating_sub(1))
            })
        });
        let view_id = format!("{id}__tabs_view");
        let strip_id = id.clone();

        let strip = reactive_view(view_id, move || {
            let _ = sel.get();
            let sel_k = sel;
            let n = count;
            let focus_id = format!("{TABSTRIP_FOCUS_PREFIX}{strip_id}");
            let mut el: Element = interactive(Element::new())
                .focus_id(focus_id)
                .focusable(true)
                .focus_ring(true)
                .on_key_down(move |ev| {
                    use winit::event::ElementState;
                    use winit::keyboard::{Key, NamedKey};
                    if ev.state != ElementState::Pressed || ev.repeat {
                        return false;
                    }
                    let i = sel_k.get();
                    match ev.logical_key {
                        Key::Named(NamedKey::ArrowRight) => {
                            if let Some(next) = roving_move(n, i, 1) {
                                sel_k.set(next);
                            }
                            true
                        }
                        Key::Named(NamedKey::ArrowLeft) => {
                            if let Some(next) = roving_move(n, i, -1) {
                                sel_k.set(next);
                            }
                            true
                        }
                        Key::Named(NamedKey::Home) => {
                            if let Some(next) = roving_edge(n, false) {
                                sel_k.set(next);
                            }
                            true
                        }
                        Key::Named(NamedKey::End) => {
                            if let Some(next) = roving_edge(n, true) {
                                sel_k.set(next);
                            }
                            true
                        }
                        _ => false,
                    }
                })
                .into();
            el.template_id = Some(format!("{strip_id}:strip"));
            el
        });

        let mut root = Element::new().child(strip);
        root = root.children(children);
        root.template_id = Some(id);
        root
    }
}

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Tabs(id: String, count: usize, children: Option<Element>) -> Element {
    let mut builder = tabs_headless(id, count);
    if let Some(c) = children {
        builder = builder.child(c);
    }
    builder.into()
}

pub fn tabs_headless_root(id: impl Into<String>, tab_count: usize) -> Element {
    tabs_headless(id, tab_count).into()
}

// ── Styled TabsList（solid-ui TabsList / TabsTrigger）─────────────────────────

/// 可见 TabsList 样式壳：单选连体条带（ToggleGroup `joined` + outline）。
pub struct TabsListBuilder {
    id: String,
    options: Vec<(String, String)>,
    value: Option<Signal<String>>,
    default_value: String,
}

pub fn tabs_list(id: impl Into<String>) -> TabsListBuilder {
    TabsListBuilder {
        id: id.into(),
        options: Vec::new(),
        value: None,
        default_value: String::new(),
    }
}

impl TabsListBuilder {
    pub fn options(mut self, opts: Vec<(String, String)>) -> Self {
        self.options = opts;
        self
    }
    pub fn value_signal(mut self, s: Signal<String>) -> Self {
        self.value = Some(s);
        self
    }
    pub fn default_value(mut self, v: impl Into<String>) -> Self {
        self.default_value = v.into();
        self
    }
}

impl From<TabsListBuilder> for Element {
    fn from(builder: TabsListBuilder) -> Self {
        let TabsListBuilder {
            id,
            options,
            value,
            default_value,
        } = builder;
        // Tabs always keep one selected option; chips support ←/→ roving (via ToggleGroup).
        let default_value = if default_value.is_empty() {
            options
                .first()
                .map(|(v, _)| v.clone())
                .unwrap_or_default()
        } else {
            default_value
        };
        let mut b = toggle_group(&id)
            .multiple(false)
            .required(true)
            .default_value(default_value)
            .options(options)
            .variant("outline")
            .joined(true);
        if let Some(sig) = value {
            b = b.value_signal(sig);
        }
        let mut el: Element = b.into();
        el.set_attr("rui_tabs_list", "1");
        el
    }
}

/// RSX `<TabsList>`：solid-ui 风格可见标签条。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn TabsList(
    id: String,
    value: Option<Signal<String>>,
    default_value: String,
    options: Option<Vec<(String, String)>>,
    children: Option<Element>,
) -> Element {
    let _ = children;
    let mut b = tabs_list(id).default_value(default_value);
    if let Some(sig) = value {
        b = b.value_signal(sig);
    }
    if let Some(opts) = options {
        b = b.options(opts);
    }
    b.into()
}
