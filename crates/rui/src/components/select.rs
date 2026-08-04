use crate::components::button::button;
use crate::components::scroll_area::scroll_area;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::popup::{
    compute_anchored_placement, panel_block_clicks, register_last_popup_for_escape,
    underlay_close_on_click, PopupPlacement,
};
use crate::primitives::r#box::vstack;
use crate::tokens;
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
    children: Option<Element>,
) -> Element {
    let mut builder = select(&id)
        .default_value(default_value)
        .options(options)
        .placeholder(placeholder);

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

impl From<SelectBuilder> for Element {
    fn from(builder: SelectBuilder) -> Self {
        let open_key = format!("{}__select_open", builder.id);
        let val_key = format!("{}__select_value", builder.id);
        let place_key = format!("{}__select_place", builder.id);
        let open = use_signal(open_key.as_str(), || false);
        let default_initial = if !builder.default_value.is_empty() {
            builder.default_value.clone()
        } else {
            builder
                .options
                .first()
                .map(|(v, _)| v.clone())
                .unwrap_or_default()
        };
        let val = builder
            .value
            .unwrap_or_else(|| use_signal(val_key.as_str(), move || default_initial.clone()));
        let place = use_signal(place_key.as_str(), SelectPlace::default);

        register_last_popup_for_escape(open);

        let current = val.get();
        let label = builder
            .options
            .iter()
            .find(|(v, _)| v == &current)
            .map(|(_, l)| l.as_str())
            .filter(|_| !current.is_empty())
            .unwrap_or(builder.placeholder.as_str());

        let open_t = open;
        let trigger_id = format!("{}:trigger", builder.id);
        let trigger: Element = button(&trigger_id)
            .variant("outline")
            .w(builder.trigger_width.as_str())
            .on_click(move || open_t.set(!open_t.peek()))
            .text(label.to_string())
            .into();

        // 根节点仅包裹触发器：打开菜单时不改变文档流尺寸。
        let mut root: Element = vstack().gap("0").into();
        root = root.relative();
        root = root.child(trigger);
        root.set_attr("rui_select_root", builder.id.as_str());
        root.set_attr("rui_select_trigger", trigger_id.as_str());
        root.set_attr(
            "rui_select_match_trigger",
            if builder.match_trigger { "1" } else { "0" },
        );

        if open.get() {
            let p = place.peek();
            root = root.child(underlay_close_on_click(open));

            let mut rows: Element = vstack().gap("2px").p("4px").into();
            for (idx, (opt_v, opt_l)) in builder.options.into_iter().enumerate() {
                let v_sig = val;
                let o_sig = open;
                let ov = opt_v.clone();
                let bid = format!("{}:opt:{idx}", builder.id);
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

            let scroll_id = format!("{}:menu_scroll", builder.id);
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
            menu.style.z_index = 960;
            menu.set_attr("rui_portal", "1");
            menu.set_attr("rui_select_menu", builder.id.as_str());
            menu.set_attr("rui_select_trigger", trigger_id.as_str());
            menu.set_attr(
                "rui_select_match_trigger",
                if builder.match_trigger { "1" } else { "0" },
            );
            if let Some(mw) = &builder.menu_width {
                menu.set_attr("rui_select_menu_w", mw.as_str());
            }
            menu.set_attr("rui_select_max_h", builder.max_height.as_str());
            menu.set_attr(
                "rui_select_placement",
                match builder.placement {
                    PopupPlacement::BottomStart => "bottom-start",
                    PopupPlacement::BottomEnd => "bottom-end",
                    PopupPlacement::TopStart => "top-start",
                    PopupPlacement::TopEnd => "top-end",
                },
            );
            menu = panel_block_clicks(menu, 960);
            root = root.child(menu);
        }

        root.template_id = Some(builder.id);
        root
    }
}

fn parse_css_px(raw: &str, fallback: f32) -> f32 {
    let s = raw.trim();
    if let Some(n) = s.strip_suffix("rem") {
        return n.trim().parse::<f32>().unwrap_or(fallback / 16.0) * 16.0;
    }
    if let Some(n) = s.strip_suffix("px") {
        return n.trim().parse::<f32>().unwrap_or(fallback);
    }
    s.parse::<f32>().unwrap_or(fallback)
}

/// 布局后根据触发器位置更新 Select 浮层坐标（Portal 绝对定位）；几何有变时返回 `true`（需再跑一轮 layout）。
pub(crate) fn sync_select_floating_placement(
    root: &mut Element,
    viewport_w: f32,
    viewport_h: f32,
) -> bool {
    let trigger_bounds = collect_focus_bounds(root);
    let mut changed = false;
    apply_select_menus(root, &trigger_bounds, [viewport_w, viewport_h], &mut changed);
    changed
}

fn collect_focus_bounds(el: &Element) -> Vec<(String, [f32; 4])> {
    let mut out = Vec::new();
    collect_focus_bounds_rec(el, &mut out);
    out
}

fn collect_focus_bounds_rec(el: &Element, out: &mut Vec<(String, [f32; 4])>) {
    if let Some(fid) = &el.focus_id {
        out.push((fid.clone(), el.layout_bounds));
    }
    for c in &el.children {
        collect_focus_bounds_rec(c, out);
    }
}

fn apply_select_menus(
    el: &mut Element,
    triggers: &[(String, [f32; 4])],
    viewport: [f32; 2],
    changed: &mut bool,
) {
    if el.attrs.contains_key("rui_select_menu") {
        let id = el.attrs.get("rui_select_menu").cloned().unwrap_or_default();
        let trigger_fid = el
            .attrs
            .get("rui_select_trigger")
            .cloned()
            .unwrap_or_default();
        if let Some((_, tb)) = triggers.iter().find(|(f, _)| *f == trigger_fid) {
            let match_trigger = el
                .attrs
                .get("rui_select_match_trigger")
                .map(|s| s.as_str() != "0")
                .unwrap_or(true);
            let menu_w = if match_trigger {
                tb[2]
            } else if let Some(raw) = el.attrs.get("rui_select_menu_w") {
                parse_css_px(raw, tb[2])
            } else {
                tb[2]
            };
            let max_h_cap = el
                .attrs
                .get("rui_select_max_h")
                .map(|s| parse_css_px(s, 256.0))
                .unwrap_or(256.0);
            let preferred = match el.attrs.get("rui_select_placement").map(|s| s.as_str()) {
                Some("bottom-end") => PopupPlacement::BottomEnd,
                Some("top-start") => PopupPlacement::TopStart,
                Some("top-end") => PopupPlacement::TopEnd,
                _ => PopupPlacement::BottomStart,
            };
            let estimate_h = el.layout_bounds[3].max(32.0);
            let (x, y, max_h, _) =
                compute_anchored_placement(*tb, viewport, menu_w, estimate_h, preferred, 4.0);
            let max_h = max_h.min(max_h_cap).max(48.0);
            let w = menu_w.max(32.0);
            let next = SelectPlace { x, y, w, max_h };
            let place_key = format!("{id}__select_place");
            let place = use_signal(place_key.as_str(), SelectPlace::default);
            let cur = place.peek();
            let dirty = (cur.x - next.x).abs() > 0.5
                || (cur.y - next.y).abs() > 0.5
                || (cur.w - next.w).abs() > 0.5
                || (cur.max_h - next.max_h).abs() > 0.5;
            if dirty {
                place.set_silent(next);
                el.style.inset.left = crate::core::tree::Element::into_margin_auto(x);
                el.style.inset.top = crate::core::tree::Element::into_margin_auto(y);
                el.style.size.width = taffy::style::Dimension::Length(w);
                el.style.max_size.height = taffy::style::Dimension::Length(max_h);
                *changed = true;
            }
        }
    }
    for c in &mut el.children {
        apply_select_menus(c, triggers, viewport, changed);
    }
}

