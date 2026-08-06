//! # Select
//!
//! ShadCN 式下拉选择：outline 触发器 + Portal 浮层选项菜单（锚点定位、点遮罩关闭、Esc LIFO）。
//!
//! ## 用法
//! ```ignore
//! <Select id="mode" value={mode} default_value="standard"
//!   options={vec![("standard".into(),"标准".into()), ("sci".into(),"科学".into())]}
//!   placeholder="模式" w="7rem" />
//! // 或 Element::from(select("mode").value_signal(mode).options(…).placement(…))
//! ```
//!
//! ## 注意
//! - 菜单开闭与选项列表由组件内 [`crate::app::reactive::reactive_component_live`] 边界管理
//!   （`open` / `value` 订阅 + props 刷新），无需手动 `<Reactive>`。
//! - 菜单 / underlay 走 `rui_portal` + [`crate::app::portal_overlay`] 栈分配 z；不占文档流。
//! - 选项为 `(value, label)`；受控用 `Signal<String>`。

use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode};
use crate::app::reactive::reactive_component_live;
use crate::components::button::button;
use crate::components::scroll_area::scroll_area;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::r#box::vstack;
use crate::tokens;
use crate::utils::popup::{panel_block_clicks, underlay_close_on_click, PopupPlacement};
use rui_macros::component;

pub struct SelectBuilder {
    id: String,
    value: Option<Signal<String>>,
    default_value: String,
    options: Vec<(String, String)>,
    placeholder: String,
    /// 触发器宽度；默认 `12rem`。
    trigger_width: String,
    /// 浮层宽度；`None` 表示与触发器同宽（`match_trigger`）。
    menu_width: Option<String>,
    /// 浮层最大高度；超出可滚动。
    max_height: String,
    /// 允许菜单宽覆盖/大于触发器（显式 `menu_width` 时生效）。
    match_trigger: bool,
    placement: PopupPlacement,
    children: Vec<Element>,
}

/// ShadCN-style select: outline trigger + floating overlay menu (Portal，不占文档流尺寸).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Select(
    id: String,
    value: Option<Signal<String>>,
    default_value: String,
    options: Vec<(String, String)>,
    placeholder: String,
    w: Option<String>,
    children: Option<Element>,
) -> Element {
    let mut builder = select(&id)
        .default_value(default_value)
        .options(options)
        .placeholder(placeholder);

    if let Some(width) = w {
        builder = builder.trigger_width(width);
    }

    if let Some(sig) = value {
        builder = builder.value_signal(sig);
    }

    if let Some(c) = children {
        builder = builder.child(c);
    }

    builder.into()
}

pub fn select(id: impl Into<String>) -> SelectBuilder {
    SelectBuilder {
        id: id.into(),
        value: None,
        default_value: String::new(),
        options: Vec::new(),
        placeholder: "Select…".into(),
        trigger_width: "12rem".into(),
        menu_width: None,
        max_height: "16rem".into(),
        match_trigger: true,
        placement: PopupPlacement::BottomStart,
        children: Vec::new(),
    }
}

impl SelectBuilder {
    pub fn value_signal(mut self, s: Signal<String>) -> Self {
        self.value = Some(s);
        self
    }

    pub fn default_value(mut self, v: impl Into<String>) -> Self {
        self.default_value = v.into();
        self
    }

    pub fn options(mut self, pairs: Vec<(String, String)>) -> Self {
        self.options = pairs;
        self
    }

    pub fn placeholder(mut self, p: impl Into<String>) -> Self {
        self.placeholder = p.into();
        self
    }

    pub fn trigger_width(mut self, w: impl Into<String>) -> Self {
        self.trigger_width = w.into();
        self
    }

    pub fn menu_width(mut self, w: impl Into<String>) -> Self {
        self.menu_width = Some(w.into());
        self.match_trigger = false;
        self
    }

    pub fn max_height(mut self, h: impl Into<String>) -> Self {
        self.max_height = h.into();
        self
    }

    pub fn match_trigger(mut self, yes: bool) -> Self {
        self.match_trigger = yes;
        self
    }

    pub fn placement(mut self, p: PopupPlacement) -> Self {
        self.placement = p;
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

#[derive(Clone, Copy, PartialEq)]
struct SelectPlace {
    x: f32,
    y: f32,
    w: f32,
    max_h: f32,
}

impl Default for SelectPlace {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            w: 192.0,
            max_h: 256.0,
        }
    }
}

#[derive(Clone, PartialEq)]
struct SelectRenderProps {
    id: String,
    default_value: String,
    options: Vec<(String, String)>,
    placeholder: String,
    trigger_width: String,
    menu_width: Option<String>,
    max_height: String,
    match_trigger: bool,
    placement: PopupPlacement,
}

fn build_select_view(
    props: SelectRenderProps,
    open: Signal<bool>,
    val: Signal<String>,
    place: Signal<SelectPlace>,
) -> Element {
    let SelectRenderProps {
        id,
        default_value: _,
        options,
        placeholder,
        trigger_width,
        menu_width,
        max_height,
        match_trigger,
        placement,
    } = props;

    let _ = open.get();
    let overlay_z = portal_sync(&id, open, OverlayKind::Menu, OverlayMode::Stacked);

    let current = val.get();
    let label = options
        .iter()
        .find(|(v, _)| v == &current)
        .map(|(_, l)| l.as_str())
        .filter(|_| !current.is_empty())
        .unwrap_or(placeholder.as_str());

    let open_t = open;
    let trigger_id = format!("{id}:trigger");
    let trigger: Element = button(&trigger_id)
        .variant("outline")
        .w(trigger_width.as_str())
        .on_click(move || open_t.set(!open_t.peek()))
        .text(label.to_string())
        .into();

    let mut view: Element = vstack().gap("0").into();
    view = view.relative();
    view = view.child(trigger);
    view.set_attr("rui_select_root", id.as_str());
    view.set_attr("rui_select_trigger", trigger_id.as_str());
    view.set_attr(
        "rui_select_match_trigger",
        if match_trigger { "1" } else { "0" },
    );

    if let Some(oz) = overlay_z {
        let p = place.peek();
        view = view.child(underlay_close_on_click(open, oz.underlay));

        let mut rows: Element = vstack().gap("2px").p("4px").into();
        for (idx, (opt_v, opt_l)) in options.into_iter().enumerate() {
            let v_sig = val;
            let o_sig = open;
            let ov = opt_v.clone();
            let bid = format!("{id}:opt:{idx}");
            let row: Element = button(&bid)
                .variant("ghost")
                .size("sm")
                .w("100%")
                .on_click(move || {
                    v_sig.set(ov.clone());
                    o_sig.set(false);
                })
                .text(opt_l)
                .into();
            rows = rows.child(row);
        }

        let scroll_id = format!("{id}:menu_scroll");
        let menu_w_s = format!("{}px", p.w.max(32.0));
        let menu_max_h_s = format!("{}px", p.max_h.max(48.0));
        let mut menu: Element = scroll_area(&scroll_id)
            .overflow_x("hidden")
            .overflow_y("auto")
            .scrollbar_visibility("auto")
            .w(menu_w_s.as_str())
            .h("auto")
            .child(rows)
            .into();
        menu = menu
            .max_h(menu_max_h_s.as_str())
            .bg(tokens::background())
            .rounded("0.375rem")
            .border_style("solid")
            .border_width(1.0)
            .border_color(tokens::border())
            .absolute()
            .left(p.x)
            .top(p.y);
        menu.set_attr("rui_floating", id.as_str());
        menu.set_attr("rui_floating_trigger", trigger_id.as_str());
        // Keep legacy attrs for one release of compatibility with sync_floating_placement.
        menu.set_attr("rui_select_menu", id.as_str());
        menu.set_attr("rui_select_trigger", trigger_id.as_str());
        menu.set_attr(
            "rui_floating_match_trigger",
            if match_trigger { "1" } else { "0" },
        );
        menu.set_attr(
            "rui_select_match_trigger",
            if match_trigger { "1" } else { "0" },
        );
        if let Some(mw) = &menu_width {
            menu.set_attr("rui_floating_w", mw.as_str());
            menu.set_attr("rui_select_menu_w", mw.as_str());
        }
        menu.set_attr("rui_floating_max_h", max_height.as_str());
        menu.set_attr("rui_select_max_h", max_height.as_str());
        let place_attr = match placement {
            PopupPlacement::BottomStart => "bottom-start",
            PopupPlacement::BottomEnd => "bottom-end",
            PopupPlacement::TopStart => "top-start",
            PopupPlacement::TopEnd => "top-end",
        };
        menu.set_attr("rui_floating_placement", place_attr);
        menu.set_attr("rui_select_placement", place_attr);
        menu = panel_block_clicks(menu, oz.panel);
        view = view.child(menu);
    }

    view
}

impl From<SelectBuilder> for Element {
    fn from(builder: SelectBuilder) -> Self {
        let SelectBuilder {
            id,
            value,
            default_value,
            options,
            placeholder,
            trigger_width,
            menu_width,
            max_height,
            match_trigger,
            placement,
            children,
        } = builder;

        let open_key = format!("{id}__select_open");
        let val_key = format!("{id}__select_value");
        let place_key = format!("{id}__select_place");
        let open = use_signal(open_key.as_str(), || false);
        let default_initial = if !default_value.is_empty() {
            default_value.clone()
        } else {
            options.first().map(|(v, _)| v.clone()).unwrap_or_default()
        };
        let val =
            value.unwrap_or_else(|| use_signal(val_key.as_str(), move || default_initial.clone()));
        let place = use_signal(place_key.as_str(), SelectPlace::default);
        let view_id = format!("{id}__select_view");

        let render_props = SelectRenderProps {
            id: id.clone(),
            default_value,
            options,
            placeholder,
            trigger_width,
            menu_width,
            max_height,
            match_trigger,
            placement,
        };

        let mut root = reactive_component_live(view_id, render_props, move |props| {
            build_select_view(props, open, val, place)
        });
        root = root.children(children);
        // Keep the reactive_component_live anchor id (`{id}__select_view`). Overwriting with
        // bare `id` made `try_reconcile_subtree` miss the placeholder, so the trigger
        // never mounted (calculator mode Select looked "missing").
        root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::create_root;

    #[test]
    fn select_keeps_reactive_view_anchor_id() {
        let (_owner, el) = create_root(|| {
            let el: Element = select("calc:mode")
                .default_value("standard")
                .options(vec![
                    ("standard".into(), "标准".into()),
                    ("scientific".into(), "科学".into()),
                ])
                .trigger_width("7rem")
                .into();
            el
        });
        assert_eq!(
            el.template_id.as_deref(),
            Some("calc:mode__select_view"),
            "Select root must keep the reactive_component_live anchor id"
        );
    }
}
