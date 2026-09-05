//! # Tabs
//!
//! 对齐 solid-ui / Kobalte 与 ShadCN **Tabs**：
//!
//! ```text
//! Tabs                      // Root；horizontal=column / vertical=row
//! ├── TabsList              // default：bg-muted 圆角条（横向 w-full + wrap）；line：透明 + 底边指示
//! │   └── TabsTrigger       // 内容宽优先（ContentBox + flex_shrink=0）；选中：bg-background + shadow-sm
//! └── TabsContent           // 仅当前 value 挂载（宏走 tabs_content：0×0 + Display::None）
//! ```
//!
//! 另保留：
//! - [`tabs_headless`]：无头 index + ←/→（自绘标签行）
//! - [`tabs_list`]：`options` 便捷条（内部 ToggleGroup `segmented`）
//!
//! ## Usage
//! ```ignore
//! // 复合 RSX（推荐；宏走 tabs_scope）
//! <Tabs id="settings" value={tab} default_value="account">
//!     <TabsList>
//!         <TabsTrigger value="account">"Account"</TabsTrigger>
//!         <TabsTrigger value="password">"Password"</TabsTrigger>
//!     </TabsList>
//!     <TabsContent value="account">
//!         <Label>"Make changes to your account here."</Label>
//!     </TabsContent>
//!     <TabsContent value="password">
//!         <Label>"Change your password here."</Label>
//!     </TabsContent>
//! </Tabs>
//!
//! // line 变体 + 垂直
//! <Tabs orientation="vertical" default_value="a">
//!     <TabsList variant="line">
//!         <TabsTrigger value="a">"A"</TabsTrigger>
//!         <TabsTrigger value="b">"B"</TabsTrigger>
//!     </TabsList>
//!     <TabsContent value="a">"A panel"</TabsContent>
//!     <TabsContent value="b">"B panel"</TabsContent>
//! </Tabs>
//!
//! // 无头 index
//! Element::from(tabs_headless("hello_tabs", 3).selected_signal(tab_idx))
//!
//! // options 便捷条
//! Element::from(
//!     tabs_list("view")
//!         .value_signal(view)
//!         .options(vec![("list".into(), "List".into()), ("grid".into(), "Grid".into())]),
//! )
//! ```
//!
//! ## Notes
//! - 未传 `value` 时用内部信号 `{id}__tabs`；多实例请保证 `id` 唯一。
//! - Trigger `focus_id = "{tabs_id}:{value}"`；方向键只移焦点环，点击 / Enter / Space 改选中。
//! - 无头 `focus_id` 为 `tabstrip:{id}`（AccessKit TabList）。

use crate::app::reactive::{reactive_anchor, reactive_placeholder_sized, reactive_view};
use crate::components::base::SlotChrome;
use crate::components::toggle_group::toggle_group;
use crate::core::reactivity::{
    create_context, provide_context, use_context, use_signal, Context, Signal,
};
use crate::core::tree::{parse_color, parse_text_color, Element, LayoutStatus};
use crate::impl_simple_slot_builder;
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, vstack};
use crate::style::{is_variant_style_overridden, LineStyle, VariantStyleField, WhiteSpace};
use crate::utils::controlled::resolve_string_signal;
use crate::utils::roving::{attach_focus_roving, roving_edge, roving_move, FocusRovingAxis};
use crate::utils::take_packed;
use rui_macros::component;
use std::sync::OnceLock;
use taffy::style::{AlignItems, Display, FlexDirection, JustifyContent};

/// AccessKit / 焦点约定前缀（无头条带）。
pub const TABSTRIP_FOCUS_PREFIX: &str = "tabstrip:";

const ATTR_SLOT: &str = "data-slot";
const ATTR_VARIANT: &str = "data-variant";
const ATTR_ORIENTATION: &str = "data-orientation";
const ATTR_TABS_VALUE: &str = "rui_tabs_value";
const ATTR_LINE_INDICATOR: &str = "rui_tabs_line_indicator";
const SLOT_TABS: &str = "tabs";
const SLOT_LIST: &str = "tabs-list";
const SLOT_TRIGGER: &str = "tabs-trigger";
const SLOT_CONTENT: &str = "tabs-content";

const SHADOW_SM: ([f32; 4], f32) = ([0.0, 0.0, 0.0, 0.05], 2.0);

// ── Context ──────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabsOrientation {
    Horizontal,
    Vertical,
}

impl TabsOrientation {
    fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "vertical" => Self::Vertical,
            _ => Self::Horizontal,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
        }
    }
}

#[derive(Clone)]
pub struct TabsCtx {
    pub id: String,
    pub value: Signal<String>,
    pub disabled: bool,
    pub orientation: TabsOrientation,
}

fn tabs_context() -> Context<TabsCtx> {
    static CTX: OnceLock<Context<TabsCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<TabsCtx>)
}

pub fn use_tabs() -> Option<TabsCtx> {
    use_context(tabs_context())
}

fn require_tabs(part: &str) -> TabsCtx {
    use_tabs().unwrap_or_else(|| {
        panic!("{part} must be used within <Tabs> / tabs_scope");
    })
}

fn unpack_item_children(mut children: Vec<Element>) -> Vec<Element> {
    if children.len() == 1 && children[0].attrs.get("rui_fragment").map(|s| s.as_str()) == Some("1")
    {
        return std::mem::take(&mut children[0].children);
    }
    children
}

fn caption_text(el: &Element) -> Option<String> {
    crate::core::tree::first_trimmed_text(el)
}

fn is_tabs_trigger(el: &Element) -> bool {
    el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_TRIGGER)
}

fn stamp_list_variant(el: &mut Element, variant: &str) {
    crate::core::tree::walk_elements_mut(el, |node| {
        if is_tabs_trigger(node) {
            node.attrs
                .insert(ATTR_VARIANT.to_string(), variant.to_string());
        }
    });
}

fn list_variant_of(el: &Element) -> &str {
    match el
        .attrs
        .get(ATTR_VARIANT)
        .map(|s| s.as_str().trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("line") => "line",
        _ => "default",
    }
}

fn length_px(v: taffy::style::LengthPercentage) -> f32 {
    crate::layout::lp_length_px(v).unwrap_or(0.0)
}

fn set_padding_x(el: &mut Element, px: f32) {
    el.style.padding.left = taffy::style::LengthPercentage::length(px);
    el.style.padding.right = taffy::style::LengthPercentage::length(px);
}

fn set_padding_y(el: &mut Element, px: f32) {
    el.style.padding.top = taffy::style::LengthPercentage::length(px);
    el.style.padding.bottom = taffy::style::LengthPercentage::length(px);
}

// ── Trigger chrome ───────────────────────────────────────────────────────────

fn trigger_colors(selected: bool, disabled: bool, line: bool) -> ([f32; 4], [u8; 4]) {
    let mut bg = if selected && !line {
        parse_color("background")
    } else {
        parse_color("transparent")
    };
    let mut fg = if selected {
        parse_text_color("foreground")
    } else {
        parse_text_color("muted-foreground")
    };
    if disabled {
        bg[3] *= 0.5;
        fg[3] = (fg[3] as f32 * 0.5) as u8;
    }
    (bg, fg)
}

fn apply_trigger_chrome(el: &mut Element, selected: bool, disabled: bool) -> (bool, bool) {
    let line = list_variant_of(el) == "line";
    let vertical = el
        .attrs
        .get(ATTR_ORIENTATION)
        .map(|s| s.as_str() == "vertical")
        .unwrap_or(false);
    let (want_bg, want_fg) = trigger_colors(selected, disabled, line);
    let (want_shadow, want_blur) = if selected && !line {
        SHADOW_SM
    } else {
        ([0.0, 0.0, 0.0, 0.0], 0.0)
    };
    let mut paint = false;
    let mut layout = false;
    let lock_bg = is_variant_style_overridden(&el.attrs, VariantStyleField::Bg);
    let lock_shadow = is_variant_style_overridden(&el.attrs, VariantStyleField::Shadow);
    let lock_pad = is_variant_style_overridden(&el.attrs, VariantStyleField::Pad);
    let lock_fg = is_variant_style_overridden(&el.attrs, VariantStyleField::Fg);
    let lock_weight = is_variant_style_overridden(&el.attrs, VariantStyleField::Weight);
    let lock_leading = is_variant_style_overridden(&el.attrs, VariantStyleField::Leading);
    let lock_border = is_variant_style_overridden(&el.attrs, VariantStyleField::Border);

    if !lock_bg && el.style.bg_color != want_bg {
        el.style.bg_color = want_bg;
        paint = true;
    }
    let shadow_hit = (el.style.shadow_blur - want_blur).abs() > 0.01
        || (el.style.shadow_color[3] - want_shadow[3]).abs() > 0.001;
    if !lock_shadow && shadow_hit {
        el.style.shadow_color = want_shadow;
        el.style.shadow_blur = want_blur;
        el.style.shadow_offset = [0.0, 0.0];
        el.style.shadow_spread = 0.0;
        paint = true;
    }
    // line 用绘制属性，不用 border（四边均匀）。
    if !lock_border && (el.style.border_width != 0.0 || !el.style.border_style.is_invisible()) {
        el.style.border_width = 0.0;
        el.style.border_color = parse_color("transparent");
        el.style.border_style = LineStyle::None;
        layout = true;
    }

    // 单节点 ContentBox：padding 在测宽外侧；文字在完整边框盒上 Center（勿再 paint inset，
    // 否则原点已缩进却仍按边框宽居中 → 严重偏右）。竖直靠 prepare_text 的 dy 居中。
    // 不嵌 Button；勿用「外壳+缩叶子叶」——测宽叶左对齐会相对白底偏右。
    // 芯片优先内容宽：禁止 flex 均分/挤压（否则窄 TabsList 里字被 padding 吃光）。
    const PAD_X: f32 = 12.0;
    const PAD_Y: f32 = 8.0;

    if el.style.box_sizing != taffy::style::BoxSizing::ContentBox {
        el.style.box_sizing = taffy::style::BoxSizing::ContentBox;
        layout = true;
    }
    if (el.style.flex_shrink - 0.0).abs() > f32::EPSILON {
        el.style.flex_shrink = 0.0;
        layout = true;
    }
    if (el.style.flex_grow - 0.0).abs() > f32::EPSILON {
        el.style.flex_grow = 0.0;
        layout = true;
    }
    if !el.style.size.width.is_auto() {
        el.style.size.width = taffy::style::Dimension::auto();
        layout = true;
    }
    if el.style.flex_direction != FlexDirection::Row {
        el.style.flex_direction = FlexDirection::Row;
        layout = true;
    }
    if el.style.justify_content != Some(JustifyContent::CENTER) {
        el.style.justify_content = Some(JustifyContent::CENTER);
        layout = true;
    }
    if el.style.align_items != Some(AlignItems::CENTER) {
        el.style.align_items = Some(AlignItems::CENTER);
        layout = true;
    }
    if !lock_pad
        && (length_px(el.style.padding.left) != PAD_X
            || length_px(el.style.padding.right) != PAD_X
            || length_px(el.style.padding.top) != PAD_Y
            || length_px(el.style.padding.bottom) != PAD_Y)
    {
        set_padding_x(el, PAD_X);
        set_padding_y(el, PAD_Y);
        layout = true;
    }

    if el.text.is_none() {
        if let Some(ix) = el.children.iter().position(|c| c.text.is_some()) {
            let mut caption = el.children.remove(ix);
            if let Some(t) = caption.text.take() {
                el.text = Some(t);
                el.text_style = std::mem::take(&mut caption.text_style);
                layout = true;
            }
        }
    }

    let want_ind = if selected && line {
        if vertical {
            "v"
        } else {
            "h"
        }
    } else {
        ""
    };
    if el.attrs.get(ATTR_LINE_INDICATOR).map(|s| s.as_str()) != Some(want_ind) {
        if want_ind.is_empty() {
            el.attrs.remove(ATTR_LINE_INDICATOR);
        } else {
            el.attrs
                .insert(ATTR_LINE_INDICATOR.to_string(), want_ind.to_string());
        }
        paint = true;
    }

    if el.text.is_some() {
        if !lock_fg && el.text_style.color_resolved() != want_fg {
            el.text_style.color = Some(want_fg);
            paint = true;
        }
        if !lock_weight && el.text_style.weight_resolved() != 500 {
            el.text_style.weight = Some(500);
            layout = true;
        }
        if el.text_style.align_resolved() != crate::style::TextAlign::Center {
            el.text_style.align = Some(crate::style::TextAlign::Center);
            el.attrs
                .insert("text_align".to_string(), "center".to_string());
            paint = true;
        }
        if !lock_leading
            && (el.text_style.line_height_scale != Some(1.0)
                || el.text_style.line_height_px.is_some())
        {
            el.text_style.line_height_px = None;
            el.text_style.line_height_scale = Some(1.0);
            el.attrs.insert("leading".to_string(), "1".to_string());
            layout = true;
        }
        if el.text_style.white_space != WhiteSpace::Nowrap {
            el.text_style.white_space = WhiteSpace::Nowrap;
            el.attrs
                .insert("white_space".to_string(), "nowrap".to_string());
            layout = true;
        }
        if el.text_style.size.is_none() {
            el.text_style.size = Some(14.0);
            layout = true;
        }
    }
    (paint, layout)
}

fn build_trigger(ctx: &TabsCtx, value: &str, label: String, disabled: bool) -> Element {
    let selected = ctx.value.peek() == value;
    let (bg, fg) = trigger_colors(selected, disabled, false);
    let mut chip: Element = hstack()
        .w("auto")
        .px("0.75rem")
        .py("0.5rem")
        .items_center()
        .justify_center()
        .rounded("0.125rem")
        .bg(bg)
        .flex_shrink(0.0)
        .flex_grow(0.0)
        .into();
    chip = chip
        .content_box()
        .text(label)
        .text_size(14.0)
        .font_weight(500.0)
        .text_leading_none()
        .text_color(fg)
        .text_center()
        .whitespace_nowrap();
    if selected {
        chip = chip.shadow(SHADOW_SM.0, SHADOW_SM.1);
    }
    chip.attrs
        .insert(ATTR_SLOT.to_string(), SLOT_TRIGGER.to_string());
    chip.attrs
        .insert(ATTR_TABS_VALUE.to_string(), value.to_string());
    chip.attrs.insert(
        ATTR_ORIENTATION.to_string(),
        ctx.orientation.as_str().to_string(),
    );
    chip.attrs
        .insert(ATTR_VARIANT.to_string(), "default".to_string());

    let focus_id = format!("{}:{value}", ctx.id);
    let sel = ctx.value;
    let v = value.to_string();
    let fid_click = focus_id.clone();
    let disabled = disabled || ctx.disabled;
    let mut el: Element = interactive(chip)
        .focus_id(focus_id.clone())
        .focusable(!disabled)
        .focus_ring(!disabled)
        .on_click(move || {
            if !disabled {
                use_signal("__rui_button_focus_id", String::new).set(fid_click.clone());
                sel.set(v.clone());
            }
        })
        .into();
    el = el.accesskit_role("tab");
    if disabled {
        el = el.opacity(0.5).cursor_not_allowed();
    } else {
        el = el.cursor_kind(crate::core::tree::CursorKind::Pointer);
    }
    el.template_id = Some(focus_id);
    el.radio_selected_sync = Some(ctx.value);
    el
}

// ── Scope ────────────────────────────────────────────────────────────────────

/// 提供 Tabs 上下文并构建子树（宏 `<Tabs>` 无 `count` 时）。
pub fn tabs_scope(
    id: impl Into<String>,
    value: Option<Signal<String>>,
    default_value: impl Into<String>,
    orientation: impl Into<String>,
    disabled: bool,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = crate::utils::resolve_instance_id(id, "tabs");
    let default_value = default_value.into();
    let orientation = TabsOrientation::parse(&orientation.into());
    let value = resolve_string_signal(value, &id, "tabs", default_value);
    provide_context(
        tabs_context(),
        TabsCtx {
            id: id.clone(),
            value,
            disabled,
            orientation,
        },
    );
    let kids = take_packed(body());
    let mut root: Element = match orientation {
        TabsOrientation::Vertical => hstack()
            .gap("0.5rem")
            .items("start")
            .w("100%")
            .min_w(0.0)
            .children(kids)
            .into(),
        TabsOrientation::Horizontal => vstack()
            .gap("0.5rem")
            .w("100%")
            .min_w(0.0)
            .children(kids)
            .into(),
    };
    root.attrs
        .insert(ATTR_SLOT.to_string(), SLOT_TABS.to_string());
    root.attrs.insert(
        ATTR_ORIENTATION.to_string(),
        orientation.as_str().to_string(),
    );
    root.template_id = Some(id);
    root
}

pub struct TabsBuilder {
    id: String,
    value: Option<Signal<String>>,
    default_value: String,
    orientation: String,
    disabled: bool,
    chrome: SlotChrome,
}

/// Composite Tabs entry (parity with `<Tabs>` / [`tabs_scope`]).
pub fn tabs(id: impl Into<String>) -> TabsBuilder {
    TabsBuilder {
        id: id.into(),
        value: None,
        default_value: String::new(),
        orientation: "horizontal".into(),
        disabled: false,
        chrome: SlotChrome::default(),
    }
}

impl TabsBuilder {
    crate::impl_slot_chrome_methods!();

    pub fn value_signal(mut self, s: Signal<String>) -> Self {
        self.value = Some(s);
        self
    }

    pub fn default_value(mut self, v: impl Into<String>) -> Self {
        self.default_value = v.into();
        self
    }

    pub fn orientation(mut self, o: impl Into<String>) -> Self {
        self.orientation = o.into();
        self
    }

    pub fn disabled(mut self, v: bool) -> Self {
        self.disabled = v;
        self
    }

    /// Build List / Trigger / Content subtrees inside the context.
    pub fn child(self, body: impl FnOnce() -> Element) -> Element {
        self.chrome.apply(tabs_scope(
            self.id,
            self.value,
            self.default_value,
            self.orientation,
            self.disabled,
            body,
        ))
    }
}

/// 宏 `<TabsContent>`：仅在选中时挂载子树。
///
/// 不用 `show_reactive`（其占位默认 `100%×100%`，隐藏分支是裸 `Element::new()`）：
/// 多个 Content 并排在带 `gap` 的 Tabs 列里时，未选中槽若仍参与 flex / 残留叶测高，
/// 会在芯片条与当前面板文案之间挤出一整块空白。这里用 `0×0` 壳 + `Display::None`，
/// 与非宏 [`TabsContent`] 的显隐策略对齐。
///
/// 选中分支必须把 `body()` 包进壳层：`reactive_anchor` 会把 fragment 的 `template_id`
/// 改写成 `{tabs}:content:{value}`。若直接返回 DatePicker / ColorPicker 等
/// [`crate::app::reactive::reactive_component_live`] 根节点，其 `__view` id 被覆盖后
/// 内部 effect 在树上找不到锚点，面板永远空着（芯片正常、内容丢失）。
pub fn tabs_content(value: impl Into<String>, body: impl Fn() -> Element + 'static) -> Element {
    let ctx = require_tabs("TabsContent");
    let value = value.into();
    let selected = ctx.value;
    let want = value.clone();
    let anchor = format!("{}:content:{value}", ctx.id);
    let effect_anchor = anchor.clone();
    reactive_anchor(effect_anchor, move || {
        if selected.get() == want {
            let mut wrap: Element = vstack().w("100%").min_w(0.0).child(body()).into();
            wrap.attrs
                .insert(ATTR_SLOT.to_string(), SLOT_CONTENT.to_string());
            wrap.attrs.insert(ATTR_TABS_VALUE.to_string(), want.clone());
            wrap = wrap.accesskit_role("tabpanel");
            wrap
        } else {
            let mut hidden = Element::new();
            hidden.style.display = Display::None;
            hidden
                .attrs
                .insert(ATTR_SLOT.to_string(), SLOT_CONTENT.to_string());
            hidden
                .attrs
                .insert(ATTR_TABS_VALUE.to_string(), want.clone());
            hidden
        }
    });
    let mut panel = reactive_placeholder_sized(anchor, 0.0, 0.0);
    panel
        .attrs
        .insert(ATTR_SLOT.to_string(), SLOT_CONTENT.to_string());
    panel.attrs.insert(ATTR_TABS_VALUE.to_string(), value);
    panel = panel.accesskit_role("tabpanel");
    panel
}

// ── RSX parts ────────────────────────────────────────────────────────────────

/// solid-ui / ShadCN `TabsList`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn TabsList(
    id: String,
    variant: String,
    value: Option<Signal<String>>,
    default_value: String,
    options: Option<Vec<(String, String)>>,
    children: Option<Element>,
) -> Element {
    if let Some(opts) = options {
        let mut b = tabs_list(if id.is_empty() { "tabs" } else { id.as_str() })
            .default_value(default_value)
            .options(opts);
        if let Some(sig) = value {
            b = b.value_signal(sig);
        }
        return b.into();
    }

    let ctx = use_tabs();
    let variant = if variant.trim().is_empty() {
        "default".to_string()
    } else {
        variant.trim().to_ascii_lowercase()
    };
    let line = variant == "line";
    let vertical = ctx
        .as_ref()
        .map(|c| c.orientation == TabsOrientation::Vertical)
        .unwrap_or(false);
    let mut kids = children.map(take_packed).unwrap_or_default();
    for child in &mut kids {
        stamp_list_variant(child, &variant);
    }

    for child in &mut kids {
        if is_tabs_trigger(child) {
            let on = child
                .radio_selected_sync
                .map(|s| {
                    s.peek()
                        == child
                            .attrs
                            .get(ATTR_TABS_VALUE)
                            .map(|v| v.as_str())
                            .unwrap_or("")
                })
                .unwrap_or(false);
            apply_trigger_chrome(child, on, false);
        }
    }

    let mut list: Element = if vertical {
        let mut col = vstack().items("stretch");
        col = if line {
            col.gap("0.25rem")
        } else {
            col.gap(0.0)
        };
        col.w("auto")
            .flex_shrink(0.0)
            .align_self_kw("start")
            .children(kids)
            .into()
    } else {
        // 横向：条带吃满父宽并允许折行；芯片自身 w-auto + flex_shrink=0（内容宽优先），
        // 避免 w(auto)+nowrap 撑出 Card 后被 ScrollArea 裁掉后半段标签。
        let mut row = hstack().items_center().wrap("wrap");
        row = if line {
            row.gap("0.25rem")
        } else {
            // 同行仍连体（column gap 0）；折行后略留行距，避免两行芯片贴死。
            row.column_gap(0.0).row_gap("0.25rem")
        };
        row.w("100%").min_w(0.0).max_w("100%").children(kids).into()
    };

    if line {
        list = list.bg(parse_color("transparent"));
    } else {
        // h-fit：高度随芯片（含折行）；list padding 让选中白底四边略缩。
        list = list
            .h("auto")
            .rounded("0.375rem")
            .bg(parse_color("muted"))
            .p("0.375rem");
    }
    list.attrs
        .insert(ATTR_SLOT.to_string(), SLOT_LIST.to_string());
    list.attrs.insert(ATTR_VARIANT.to_string(), variant.clone());
    list.set_attr("rui_tabs_list", "1");
    list = list.accesskit_role("tablist");
    if let Some(ctx) = ctx.as_ref() {
        list.template_id = Some(format!("{}:list", ctx.id));
        let axis = if vertical {
            FocusRovingAxis::Vertical
        } else {
            FocusRovingAxis::Horizontal
        };
        attach_focus_roving(&mut list, axis);
    } else if !id.is_empty() {
        list.template_id = Some(format!("{id}:list"));
    }
    list
}

/// solid-ui / ShadCN `TabsTrigger`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn TabsTrigger(value: String, disabled: bool, children: Option<Element>) -> Element {
    let ctx = require_tabs("TabsTrigger");
    let kids = children
        .map(|c| unpack_item_children(vec![c]))
        .unwrap_or_default();
    let label = kids
        .iter()
        .find_map(caption_text)
        .unwrap_or_else(|| value.clone());
    build_trigger(&ctx, &value, label, disabled)
}

pub struct TabsTriggerBuilder {
    value: String,
    disabled: bool,
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn tabs_trigger(value: impl Into<String>) -> TabsTriggerBuilder {
    TabsTriggerBuilder {
        value: value.into(),
        disabled: false,
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl TabsTriggerBuilder {
    crate::impl_slot_chrome_methods!();

    pub fn disabled(mut self, v: bool) -> Self {
        self.disabled = v;
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children
            .extend(crate::utils::take_packed(child.into()));
        self
    }

    pub fn text(mut self, content: impl Into<String>) -> Self {
        self.children.push(Element::new().text(content.into()));
        self
    }
}

impl From<TabsTriggerBuilder> for Element {
    fn from(builder: TabsTriggerBuilder) -> Self {
        let el = TabsTrigger(TabsTriggerProps {
            value: Some(builder.value),
            disabled: Some(builder.disabled),
            children: crate::components::base::packed_children(builder.children),
            base: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

impl crate::core::tree::IntoElementList for TabsTriggerBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

/// 非宏路径：子节点已构建时用 display 显隐（RSX 走 [`tabs_content`]）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn TabsContent(value: String, children: Option<Element>) -> Element {
    let ctx = require_tabs("TabsContent");
    let mut panel = children.unwrap_or_else(Element::new);
    if panel.attrs.get("rui_fragment").map(|s| s.as_str()) == Some("1") {
        let kids = std::mem::take(&mut panel.children);
        panel = vstack().w("100%").min_w(0.0).children(kids).into();
    }
    panel
        .attrs
        .insert(ATTR_SLOT.to_string(), SLOT_CONTENT.to_string());
    panel
        .attrs
        .insert(ATTR_TABS_VALUE.to_string(), value.clone());
    panel = panel.accesskit_role("tabpanel");
    panel.template_id = Some(format!("{}:content:{value}", ctx.id));
    let selected = ctx.value;
    let want = value;
    panel = panel.reactive_sync(move |el, layout_tree| {
        let show = selected.peek() == want;
        let want_d = if show { Display::Flex } else { Display::None };
        if el.style.display != want_d {
            el.style.display = want_d;
            if el.layout_node.is_some() {
                layout_tree.update_taffy_node_style(el);
            }
            crate::app::damage::push_frame_paint_damage(el.layout_bounds);
            return LayoutStatus::LayoutChanged;
        }
        LayoutStatus::Clean
    });
    panel
}

// ── Headless TabList ─────────────────────────────────────────────────────────

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
        let id = crate::utils::resolve_instance_id(id, "tabs");
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

/// 无头 `<Tabs count={n}>`（复合写法由宏走 [`tabs_scope`]）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Tabs(id: String, count: usize, children: Option<Element>) -> Element {
    let mut builder = tabs_headless(id, count.max(1));
    if let Some(c) = children {
        builder = builder.child(c);
    }
    builder.into()
}

pub fn tabs_headless_root(id: impl Into<String>, tab_count: usize) -> Element {
    tabs_headless(id, tab_count).into()
}

// ── options 便捷条（ToggleGroup joined）──────────────────────────────────────

/// 可见 TabsList 样式壳：单选连体条带（ToggleGroup `segmented` + outline）。
pub struct TabsListBuilder {
    id: String,
    options: Vec<(String, String)>,
    value: Option<Signal<String>>,
    default_value: String,
    chrome: SlotChrome,
}

pub fn tabs_list(id: impl Into<String>) -> TabsListBuilder {
    TabsListBuilder {
        id: id.into(),
        options: Vec::new(),
        value: None,
        default_value: String::new(),
        chrome: SlotChrome::default(),
    }
}

impl TabsListBuilder {
    crate::impl_slot_chrome_methods!();

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
            chrome,
        } = builder;
        let default_value = if default_value.is_empty() {
            options.first().map(|(v, _)| v.clone()).unwrap_or_default()
        } else {
            default_value
        };
        let mut b = toggle_group(&id)
            .multiple(false)
            .required(true)
            .default_value(default_value)
            .options(options)
            .variant("outline")
            .segmented(true);
        if let Some(sig) = value {
            b = b.value_signal(sig);
        }
        let mut el: Element = b.into();
        el.set_attr("rui_tabs_list", "1");
        chrome.apply(el)
    }
}

impl_simple_slot_builder!(TabsListSlotBuilder, tabs_list_slot, TabsList, TabsListProps);

// ── Per-frame trigger sync ───────────────────────────────────────────────────

pub(crate) fn sync_tabs_trigger_subtree(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
) {
    crate::core::tree::walk_elements_mut(el, |el| {
        if is_tabs_trigger(el) {
            // Heal frames where radio sync mistakenly nested a Radio Indicator under the chip
            // (shared `radio_selected_sync` without `rui_radio_value`). That turns the text leaf
            // into a flex parent sized to the 16px dot → labels pile at the TabsList origin.
            let before = el.children.len();
            el.children.retain(|c| {
                c.attrs.get("data-slot").map(|s| s.as_str()) != Some("radio-indicator")
            });
            if el.children.len() != before {
                *impact = impact.merge(LayoutStatus::StructureChanged);
            }
            if let Some(sel) = el.radio_selected_sync {
                let item_value = el
                    .attrs
                    .get(ATTR_TABS_VALUE)
                    .map(|s| s.as_str())
                    .unwrap_or("");
                let on = sel.peek() == item_value;
                let disabled = el
                    .attrs
                    .get("disabled")
                    .map(|s| s.as_str() == "true")
                    .unwrap_or(false);
                let (paint, lay) = apply_trigger_chrome(el, on, disabled);
                if paint || lay {
                    if lay {
                        if el.layout_node.is_some() {
                            layout_tree.mark_element_layout_dirty(el);
                            layout_tree.update_taffy_node_style(el);
                        }
                        *impact = impact.merge(LayoutStatus::LayoutChanged);
                    } else {
                        if el.layout_node.is_some() {
                            layout_tree.update_taffy_node_style(el);
                        }
                        *impact = impact.merge(LayoutStatus::StyleChanged);
                    }
                    crate::app::damage::push_frame_paint_damage(el.layout_bounds);
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_scope_wires_trigger_for_and_selected() {
        let value = use_signal("tabs_test_value", || "account".to_string());
        let el = tabs_scope(
            "settings",
            Some(value),
            "account",
            "horizontal",
            false,
            || {
                let list = TabsList(TabsListProps {
                    id: String::new(),
                    variant: None,
                    value: None,
                    default_value: None,
                    options: None,
                    children: Some({
                        let mut frag = Element::new();
                        frag.set_attr("rui_fragment", "1");
                        frag.children(vec![
                            TabsTrigger(TabsTriggerProps {
                                value: Some("account".into()),
                                disabled: None,
                                children: Some(Element::new().text("Account")),
                                base: None,
                            }),
                            TabsTrigger(TabsTriggerProps {
                                value: Some("password".into()),
                                disabled: None,
                                children: Some(Element::new().text("Password")),
                                base: None,
                            }),
                        ])
                    }),
                    base: None,
                });
                let mut root = Element::new();
                root.set_attr("rui_fragment", "1");
                root.children(vec![list])
            },
        );
        assert_eq!(el.attrs.get(ATTR_SLOT).map(|s| s.as_str()), Some(SLOT_TABS));
        let list = el
            .children
            .iter()
            .find(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_LIST))
            .expect("list");
        let triggers: Vec<_> = list
            .children
            .iter()
            .filter(|c| is_tabs_trigger(c))
            .collect();
        assert_eq!(triggers.len(), 2);
        assert_eq!(triggers[0].focus_id.as_deref(), Some("settings:account"));
        assert_eq!(
            triggers[0].attrs.get(ATTR_TABS_VALUE).map(|s| s.as_str()),
            Some("account")
        );
        assert!(triggers[0].on_click.is_some());
        assert_eq!(
            triggers[0].style.box_sizing,
            taffy::style::BoxSizing::ContentBox
        );
        assert_eq!(triggers[0].text.as_deref(), Some("Account"));
        assert!(
            triggers[0].children.iter().all(|c| c.text.is_none()),
            "label on ContentBox trigger — not a shrink-wrapped caption child"
        );
    }

    #[test]
    fn tabs_trigger_text_paints_in_content_box_centered() {
        let value = use_signal("tabs_center_paint_value", || "list".to_string());
        let mut el = tabs_scope(
            "center-paint",
            Some(value),
            "list",
            "horizontal",
            false,
            || {
                let list = TabsList(TabsListProps {
                    id: String::new(),
                    variant: None,
                    value: None,
                    default_value: None,
                    options: None,
                    children: Some({
                        let mut frag = Element::new();
                        frag.set_attr("rui_fragment", "1");
                        frag.children(vec![TabsTrigger(TabsTriggerProps {
                            value: Some("list".into()),
                            disabled: None,
                            children: Some(Element::new().text("List")),
                            base: None,
                        })])
                    }),
                    base: None,
                });
                let mut root = Element::new();
                root.set_attr("rui_fragment", "1");
                root.children(vec![list])
            },
        );
        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut el, 640.0, 200.0, [-1.0, -1.0]);
        let list = el
            .children
            .iter()
            .find(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_LIST))
            .expect("list");
        let trigger = list
            .children
            .iter()
            .find(|c| is_tabs_trigger(c))
            .expect("trigger");
        let tb = trigger.layout_bounds;
        let pad_l = length_px(trigger.style.padding.left);
        let pad_r = length_px(trigger.style.padding.right);
        let pad_t = length_px(trigger.style.padding.top);
        let pad_b = length_px(trigger.style.padding.bottom);
        assert_eq!(pad_l, pad_r);
        assert_eq!(pad_t, pad_b);
        assert!(pad_l > 0.0 && pad_t > 0.0);
        assert_eq!(
            trigger.style.box_sizing,
            taffy::style::BoxSizing::ContentBox
        );
        assert_eq!(
            trigger.text_style.align_resolved(),
            crate::style::TextAlign::Center
        );
        let tn = result
            .text_nodes
            .iter()
            .find(|n| n.text == "List")
            .expect("List text node");
        // Full border box (no paint inset): Center on Wb with equal pad ≡ content-box center.
        assert!((tn.x - tb[0]).abs() < 1.0);
        assert!((tn.y - tb[1]).abs() < 1.0);
        assert!((tn.width - tb[2]).abs() < 1.5);
        assert!((tn.height - tb[3]).abs() < 1.5);
        let chip_cx = tb[0] + tb[2] * 0.5;
        let chip_cy = tb[1] + tb[3] * 0.5;
        let text_cx = tn.x + tn.width * 0.5;
        let text_cy = tn.y + tn.height * 0.5;
        assert!(
            (text_cx - chip_cx).abs() < 1.5,
            "text border-box center matches chip center x"
        );
        assert!(
            (text_cy - chip_cy).abs() < 1.5,
            "text border-box center matches chip center y"
        );
        assert!(
            tn.height > trigger.text_style.size_resolved() + pad_t + pad_b - 1.0,
            "chip taller than bare glyphs so prepare_text dy can vertical-center"
        );
    }

    #[test]
    fn tabs_list_options_builder_still_works() {
        let el: Element = tabs_list("gal-tabs")
            .default_value("list")
            .options(vec![
                ("list".into(), "List".into()),
                ("grid".into(), "Grid".into()),
            ])
            .into();
        assert_eq!(el.attrs.get("rui_tabs_list").map(|s| s.as_str()), Some("1"));
        assert!(el.children.len() >= 2);
    }

    #[test]
    fn builder_trigger_style() {
        use crate::core::reactivity::create_root;
        let (_o, el): (_, Element) = create_root(|| {
            tabs("tab-chrome").default_value("a").child(|| {
                tabs_list_slot()
                    .child(
                        tabs_trigger("a")
                            .style(|b| b.text_size(20.0))
                            .text("Account"),
                    )
                    .into()
            })
        });
        fn find_size(el: &Element, want: f32) -> bool {
            el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
                || (el.text_style.size_resolved() - want).abs() < 0.01
                || el.children.iter().any(|c| find_size(c, want))
        }
        assert!(
            find_size(&el, 20.0),
            "tabs_trigger .style(|b| b.text_size(20)) must land on the trigger tree"
        );
    }

    #[test]
    fn tabs_content_hidden_slot_is_display_none_zero_shell() {
        let value = use_signal("tabs_hidden_slot_value", || "list".to_string());
        let el = tabs_scope(
            "hidden-slot",
            Some(value),
            "list",
            "horizontal",
            false,
            || {
                let list = TabsList(TabsListProps {
                    id: String::new(),
                    variant: None,
                    value: None,
                    default_value: None,
                    options: None,
                    children: Some({
                        let mut frag = Element::new();
                        frag.set_attr("rui_fragment", "1");
                        frag.children(vec![
                            TabsTrigger(TabsTriggerProps {
                                value: Some("list".into()),
                                disabled: None,
                                children: Some(Element::new().text("List")),
                                base: None,
                            }),
                            TabsTrigger(TabsTriggerProps {
                                value: Some("grid".into()),
                                disabled: None,
                                children: Some(Element::new().text("Grid")),
                                base: None,
                            }),
                        ])
                    }),
                    base: None,
                });
                let list_panel = tabs_content("list", || Element::new().text("List panel"));
                let grid_panel = tabs_content("grid", || Element::new().text("Grid panel"));
                let mut root = Element::new();
                root.set_attr("rui_fragment", "1");
                root.children(vec![list, list_panel, grid_panel])
            },
        );
        let contents: Vec<_> = el
            .children
            .iter()
            .filter(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_CONTENT))
            .collect();
        assert_eq!(contents.len(), 2);
        for panel in &contents {
            // 初始壳：0×0，避免未跑 effect 前就按 100% 撑开列高。
            assert_eq!(panel.style.size.width, taffy::style::Dimension::length(0.0));
            assert_eq!(
                panel.style.size.height,
                taffy::style::Dimension::length(0.0)
            );
        }
    }

    #[test]
    fn tabs_chips_prefer_intrinsic_content_width_in_narrow_parent() {
        let value = use_signal("tabs_intrinsic_w_value", || "sketch".to_string());
        let labels = [
            "Sketch", "Chrome", "GitHub", "Twitter", "Material", "Block", "Circle", "Slider",
            "Compact",
        ];
        let scope = tabs_scope(
            "intrinsic-w",
            Some(value),
            "sketch",
            "horizontal",
            false,
            || {
                let triggers: Vec<_> = labels
                    .iter()
                    .map(|lab| {
                        TabsTrigger(TabsTriggerProps {
                            value: Some(lab.to_lowercase()),
                            disabled: None,
                            children: Some(Element::new().text(*lab)),
                            base: None,
                        })
                    })
                    .collect();
                let list = TabsList(TabsListProps {
                    id: String::new(),
                    variant: None,
                    value: None,
                    default_value: None,
                    options: None,
                    children: Some({
                        let mut frag = Element::new();
                        frag.set_attr("rui_fragment", "1");
                        frag.children(triggers)
                    }),
                    base: None,
                });
                let mut root = Element::new();
                root.set_attr("rui_fragment", "1");
                root.children(vec![list])
            },
        );
        // Gallery MyCard 形状：窄列 + min_w(0) + padding。
        let mut root = Element::new()
            .w(360.0)
            .h(240.0)
            .child(Element::new().w("100%").min_w(0.0).p(16.0).child(scope));
        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 360.0, 240.0, [-1.0, -1.0]);

        fn collect_triggers<'a>(el: &'a Element, out: &mut Vec<&'a Element>) {
            if is_tabs_trigger(el) {
                out.push(el);
            }
            for c in &el.children {
                collect_triggers(c, out);
            }
        }
        fn find_list<'a>(el: &'a Element) -> Option<&'a Element> {
            if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_LIST) {
                return Some(el);
            }
            el.children.iter().find_map(find_list)
        }
        let mut triggers = Vec::new();
        collect_triggers(&root, &mut triggers);
        assert_eq!(triggers.len(), labels.len());

        let list = find_list(&root).expect("tabs list");
        let list_w = list.layout_bounds[2];
        assert!(
            list_w <= 360.0 - 16.0 * 2.0 + 1.0,
            "TabsList must fit parent content (got {list_w})"
        );
        assert_eq!(list.style.flex_wrap, taffy::style::FlexWrap::Wrap);

        let mut ys: Vec<i32> = triggers
            .iter()
            .map(|t| t.layout_bounds[1].round() as i32)
            .collect();
        ys.sort_unstable();
        ys.dedup();
        assert!(
            ys.len() >= 2,
            "nine content-sized chips in ~328px must wrap to ≥2 rows, ys={ys:?}"
        );

        for (t, lab) in triggers.iter().zip(labels.iter()) {
            let w = t.layout_bounds[2];
            eprintln!(
                "chip '{lab}' x={:.1} y={:.1} w={w:.1} h={:.1}",
                t.layout_bounds[0], t.layout_bounds[1], t.layout_bounds[3]
            );
            assert!(
                (t.style.flex_shrink - 0.0).abs() < f32::EPSILON,
                "{lab}: flex_shrink must stay 0 so content width wins"
            );
            assert!(
                w >= 40.0,
                "{lab}: chip crushed to {w} — should size to label+padding"
            );
            let tn = result
                .text_nodes
                .iter()
                .find(|n| n.text == *lab)
                .unwrap_or_else(|| panic!("missing text node for {lab}"));
            assert!(
                tn.width >= 20.0,
                "{lab}: painted text width lost ({})",
                tn.width
            );
            // Chip right edge must stay inside the list (no clip / 内容丢失).
            let chip_r = t.layout_bounds[0] + t.layout_bounds[2];
            let list_r = list.layout_bounds[0] + list.layout_bounds[2];
            assert!(
                chip_r <= list_r + 1.0,
                "{lab}: chip overflows list (chip_r={chip_r} list_r={list_r})"
            );
        }

        // Gallery symptom: all labels stacked at the same x — must not share one origin.
        let mut xs: Vec<i32> = triggers
            .iter()
            .map(|t| t.layout_bounds[0].round() as i32)
            .collect();
        xs.sort_unstable();
        let unique_x = {
            let mut u = xs.clone();
            u.dedup();
            u
        };
        assert!(
            unique_x.len() >= 3,
            "chips must spread horizontally (or wrap); got xs={xs:?} — gallery overlap bug"
        );
        for i in 1..triggers.len() {
            let a = triggers[i - 1].layout_bounds;
            let b = triggers[i].layout_bounds;
            let same_row = (a[1] - b[1]).abs() < 1.0;
            if same_row {
                assert!(
                    b[0] + 0.5 >= a[0] + a[2],
                    "same-row chips must not overlap: '{}'@x={}w={} vs '{}'@x={}w={}",
                    labels[i - 1],
                    a[0],
                    a[2],
                    labels[i],
                    b[0],
                    b[2]
                );
            }
        }
    }

    #[test]
    fn radio_sync_must_not_nest_indicator_under_tabs_trigger() {
        // TabsTrigger reuses `radio_selected_sync` for the selected value signal but is not a
        // Radio control (no `rui_radio_value`). Global radio sync must not inject the size-4
        // Indicator — that destroys the text leaf and piles chip labels at x≈0 in gallery.
        let value = use_signal("tabs_radio_sync_guard", || "date".to_string());
        let mut el = tabs_scope(
            "radio-guard",
            Some(value),
            "date",
            "horizontal",
            false,
            || {
                let list = TabsList(TabsListProps {
                    id: String::new(),
                    variant: None,
                    value: None,
                    default_value: None,
                    options: None,
                    children: Some({
                        let mut frag = Element::new();
                        frag.set_attr("rui_fragment", "1");
                        frag.children(vec![
                            TabsTrigger(TabsTriggerProps {
                                value: Some("date".into()),
                                disabled: None,
                                children: Some(Element::new().text("Date")),
                                base: None,
                            }),
                            TabsTrigger(TabsTriggerProps {
                                value: Some("datetime".into()),
                                disabled: None,
                                children: Some(Element::new().text("DateTime")),
                                base: None,
                            }),
                            TabsTrigger(TabsTriggerProps {
                                value: Some("time".into()),
                                disabled: None,
                                children: Some(Element::new().text("Time")),
                                base: None,
                            }),
                        ])
                    }),
                    base: None,
                });
                let mut root = Element::new();
                root.set_attr("rui_fragment", "1");
                root.children(vec![list])
            },
        );

        let mut layout_tree = LayoutTree::new();
        let mut impact = LayoutStatus::Clean;
        crate::components::radio_group::sync_radio_checked_subtree(
            &mut el,
            &mut layout_tree,
            &mut impact,
        );
        sync_tabs_trigger_subtree(&mut el, &mut layout_tree, &mut impact);

        fn collect_triggers<'a>(node: &'a Element, out: &mut Vec<&'a Element>) {
            if is_tabs_trigger(node) {
                out.push(node);
            }
            for c in &node.children {
                collect_triggers(c, out);
            }
        }
        let mut triggers = Vec::new();
        collect_triggers(&el, &mut triggers);
        assert_eq!(triggers.len(), 3);
        for t in &triggers {
            assert!(
                t.children.iter().all(|c| {
                    c.attrs.get("data-slot").map(|s| s.as_str()) != Some("radio-indicator")
                }),
                "TabsTrigger must stay a text leaf — radio Indicator would crush content width"
            );
            assert!(
                t.text.as_deref().is_some_and(|s| !s.is_empty()),
                "chip label must remain on the trigger element"
            );
        }

        let mut root = Element::new().w(480.0).h(80.0).child(el);
        let _ = layout_tree.build_and_layout(&mut root, 480.0, 80.0, [-1.0, -1.0]);
        let mut laid = Vec::new();
        collect_triggers(&root, &mut laid);
        let mut xs: Vec<f32> = laid.iter().map(|t| t.layout_bounds[0]).collect();
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!(
            (xs[2] - xs[0]).abs() > 40.0,
            "after radio+tabs sync, chips must still spread (xs={xs:?})"
        );
        for w in laid.windows(2) {
            let a = w[0].layout_bounds;
            let b = w[1].layout_bounds;
            if (a[1] - b[1]).abs() < 1.0 {
                assert!(
                    b[0] + 0.5 >= a[0] + a[2].min(8.0),
                    "chips must not pile: {:?} vs {:?}",
                    a,
                    b
                );
            }
        }
    }

    /// Gallery pickers/color-picker: chips OK but panel empty when tabs_content stole
    /// nested `reactive_component_live` `template_id` (reconcile then miss forever).
    #[test]
    fn tabs_content_preserves_nested_reactive_component_anchor() {
        use std::cell::RefCell;
        use std::rc::Rc;

        use crate::app::reactive::{reactive_component_live, reset_registered_anchors};
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::core::reactivity::{create_effect, create_root};

        reset_registered_anchors();
        clear_test_reconcile_root();

        let root: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(Rc::clone(&root));

        let (handle, ()) = create_root(|| {
            let root_e = Rc::clone(&root);
            create_effect(move || {
                let el = tabs_scope("tc-nest", None, "date", "horizontal", false, || {
                    tabs_content("date", || {
                        reactive_component_live("tc-nest-picker__view", (), |_| {
                            Element::new().text("Pick a date")
                        })
                    })
                });
                *root_e.borrow_mut() = el;
            });
        });

        fn find_tid(el: &Element, want: &str) -> bool {
            if el.template_id.as_deref() == Some(want) {
                return true;
            }
            el.children.iter().any(|c| find_tid(c, want))
        }
        fn find_text(el: &Element, want: &str) -> bool {
            if el.text.as_deref() == Some(want) {
                return true;
            }
            el.children.iter().any(|c| find_text(c, want))
        }

        {
            let tree = root.borrow();
            assert!(
                find_tid(&tree, "tc-nest:content:date"),
                "tabs content slot must remain the outer shell"
            );
            assert!(
                find_tid(&tree, "tc-nest-picker__view"),
                "nested reactive_component template_id must not be overwritten by tabs_content"
            );
            assert!(
                find_text(&tree, "Pick a date"),
                "nested component body must mount (panel must not stay empty)"
            );
        }

        handle.dispose();
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    #[test]
    fn tabs_sync_strips_stray_radio_indicator_from_trigger() {
        let value = use_signal("tabs_strip_indicator", || "a".to_string());
        let mut el = tabs_scope("strip-ind", Some(value), "a", "horizontal", false, || {
            TabsTrigger(TabsTriggerProps {
                value: Some("a".into()),
                disabled: None,
                children: Some(Element::new().text("Alpha")),
                base: None,
            })
        });
        fn find_trigger_mut(el: &mut Element) -> Option<&mut Element> {
            if is_tabs_trigger(el) {
                return Some(el);
            }
            for c in &mut el.children {
                if let Some(t) = find_trigger_mut(c) {
                    return Some(t);
                }
            }
            None
        }
        let trigger = find_trigger_mut(&mut el).expect("trigger");
        let mut ind = Element::new().w(16.0).h(16.0);
        ind.attrs
            .insert("data-slot".into(), "radio-indicator".into());
        trigger.children.insert(0, ind);
        assert_eq!(trigger.children.len(), 1);

        let mut layout_tree = LayoutTree::new();
        let mut impact = LayoutStatus::Clean;
        sync_tabs_trigger_subtree(&mut el, &mut layout_tree, &mut impact);
        assert!(matches!(
            impact,
            LayoutStatus::StructureChanged | LayoutStatus::LayoutChanged
        ));
        let trigger = find_trigger_mut(&mut el).expect("trigger");
        assert!(
            trigger.children.is_empty(),
            "tabs sync must remove stray radio Indicator so text leaf measure returns"
        );
    }

    #[test]
    fn trigger_chrome_respects_author_padding_bg_and_size() {
        use crate::style::{mark_variant_style_override, VariantStyleField};
        let mut el = Element::new();
        el.text = Some("Account".into());
        el.text_style.size = Some(18.0);
        el.style.bg_color = [1.0, 0.0, 0.0, 1.0];
        mark_variant_style_override(&mut el, VariantStyleField::Bg);
        el.style.padding.left = taffy::style::LengthPercentage::length(20.0);
        el.style.padding.right = taffy::style::LengthPercentage::length(20.0);
        el.style.padding.top = taffy::style::LengthPercentage::length(20.0);
        el.style.padding.bottom = taffy::style::LengthPercentage::length(20.0);
        mark_variant_style_override(&mut el, VariantStyleField::Pad);
        apply_trigger_chrome(&mut el, true, false);
        assert_eq!(el.style.bg_color, [1.0, 0.0, 0.0, 1.0]);
        assert!((length_px(el.style.padding.left) - 20.0).abs() < 0.01);
        assert_eq!(el.text_style.size, Some(18.0));
    }
}
