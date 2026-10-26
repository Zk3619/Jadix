//! # RadioGroup
//!
//! 对齐 solid-ui / Kobalte **RadioGroup** 与 ShadCN Aria 圆形单选：
//! `size-4` 圆框 + `border-primary`，选中时中心 `size-2.5` 实心圆；项为横排控制面 + 文案。
//!
//! ```text
//! RadioGroup                 // grid gap-2；accesskit RadioGroup
//! └── RadioGroupItem         // flex items-center gap-2
//!     ├── ItemControl        // 圆形控制面（焦点 / toggled）
//!     │   └── ItemIndicator  // 选中实心点
//!     └── children / RadioGroupItemLabel
//! ```
//!
//! ## Usage
//! ```ignore
//! // 复合 RSX（推荐；宏走 radio_group_scope）
//! <RadioGroup id="tier" value={tier} default_value="free">
//!     <RadioGroupItem value="free">"Free"</RadioGroupItem>
//!     <RadioGroupItem value="pro">
//!         <RadioGroupItemLabel>"Pro"</RadioGroupItemLabel>
//!     </RadioGroupItem>
//!     <RadioGroupItem value="team" disabled=true>"Team"</RadioGroupItem>
//! </RadioGroup>
//!
//! // 旁侧 Label（Solid id / for）
//! <HStack gap="0.5rem" items="center">
//!     <RadioGroupItem value="a" />  // 须在 RadioGroup 内
//!     <Label for="tier:a">"Option A"</Label>
//! </HStack>
//!
//! // Builder + options（便捷）
//! { Element::from(
//!     radio_group("tier")
//!         .value_signal(tier)
//!         .default_value("free")
//!         .options(vec![
//!             ("free", "Free"),
//!             ("pro", "Pro"),
//!         ]),
//! ) }
//! ```
//!
//! ## Notes
//! - 未传 `value` 时用内部信号 `{id}__radio`；多实例请保证 `id` 唯一。
//! - 项 `focus_id = "{group_id}:{value}"`；选中态 `peek` + 每帧 style sync，不订阅根 effect。
//! - 兼容旧 prop 名 `selected`（宏映射到 `value`）。

use crate::components::base::{
    fill_default_text_size, fill_default_text_weight, SlotAuthor, SlotChrome,
};
use crate::components::label::{label, label_accesskit_id_for, ATTR_LABEL_FOR};
use crate::core::reactivity::{
    create_context, provide_context, use_context, use_signal, with_child_owner, Context, Signal,
};
use crate::core::tree::{parse_color, Element, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::style::{is_variant_style_overridden, LineStyle, VariantStyleField};
use crate::utils::controlled::resolve_string_signal;
use crate::utils::roving::{attach_focus_roving, FocusRovingAxis};
use crate::utils::take_fragment;
use crate::utils::take_packed;
use jadix_macros::component;
use std::sync::OnceLock;

const ATTR_INDICATOR: &str = "data-slot";
const SLOT_INDICATOR: &str = "radio-indicator";
const ATTR_RADIO_VALUE: &str = "jadix_radio_value";

/// solid-ui ItemControl：`size-4`；Indicator 实心点约 `size-2.5`。
const BOX_PX: f32 = 16.0;
const DOT_PX: f32 = 10.0;
const BORDER_W: f32 = 1.5;

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct RadioGroupCtx {
    pub id: String,
    pub value: Signal<String>,
    pub disabled: bool,
}

fn radio_group_context() -> Context<RadioGroupCtx> {
    static CTX: OnceLock<Context<RadioGroupCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<RadioGroupCtx>)
}

pub fn use_radio_group() -> Option<RadioGroupCtx> {
    use_context(radio_group_context())
}

fn require_radio_group(part: &str) -> RadioGroupCtx {
    use_radio_group().unwrap_or_else(|| {
        panic!("{part} must be used within <RadioGroup> / radio_group_scope");
    })
}

fn unpack_item_children(mut children: Vec<Element>) -> Vec<Element> {
    if children.len() == 1
        && children[0].attrs.get("jadix_fragment").map(|s| s.as_str()) == Some("1")
    {
        return std::mem::take(&mut children[0].children);
    }
    children
}

fn is_caption_text_leaf(el: &Element) -> bool {
    el.children.is_empty()
        && el
            .text
            .as_deref()
            .map(str::trim)
            .is_some_and(|s| !s.is_empty())
        && el.checkbox_sync.is_none()
        && el.switch_checked_sync.is_none()
        && el.radio_selected_sync.is_none()
        && el.on_click.is_none()
        && el.focus_id.as_deref().map_or(true, |s| s.is_empty())
        && el.attrs.get("accesskit_role").map(|s| s.as_str()) != Some("label")
}

fn is_label_element(el: &Element) -> bool {
    el.attrs.get("accesskit_role").map(|s| s.as_str()) == Some("label")
}

fn wire_radio_captions(control_id: &str, children: Vec<Element>) -> (Vec<Element>, Option<String>) {
    let mut captions = Vec::with_capacity(children.len());
    let mut labelled_by = None::<String>;
    for child in children {
        let caption = if is_caption_text_leaf(&child) {
            let text = child.text.clone().unwrap_or_default();
            let author = SlotAuthor::from_el(&child);
            let mut lbl: Element = label("").text(text).for_target(control_id).into();
            fill_default_text_size(&mut lbl, 14.0);
            fill_default_text_weight(&mut lbl, 500);
            author.apply(lbl)
        } else if is_label_element(&child) {
            let mut lbl = child;
            let existing_for = lbl
                .attrs
                .get(ATTR_LABEL_FOR)
                .map(|s| s.as_str().trim().to_string())
                .filter(|s| !s.is_empty());
            let for_target = existing_for.unwrap_or_else(|| control_id.to_string());
            lbl.attrs
                .insert(ATTR_LABEL_FOR.to_string(), for_target.clone());
            if lbl
                .attrs
                .get("accesskit_id")
                .map(|s| s.as_str())
                .is_none_or(|s| s.is_empty())
            {
                lbl.attrs.insert(
                    "accesskit_id".to_string(),
                    label_accesskit_id_for(&for_target),
                );
            }
            if lbl.cursor.is_none() {
                lbl.cursor = Some(crate::core::tree::CursorKind::Pointer.into());
            }
            lbl
        } else {
            child
        };
        if labelled_by.is_none() {
            if let Some(aid) = caption
                .attrs
                .get("accesskit_id")
                .map(|s| s.as_str())
                .filter(|s| !s.is_empty())
            {
                labelled_by = Some(aid.to_string());
            }
        }
        captions.push(caption);
    }
    (captions, labelled_by)
}

fn control_bg() -> [f32; 4] {
    parse_color("background")
}

fn control_border() -> [f32; 4] {
    parse_color("primary")
}

fn indicator_dot_color(on: bool) -> [f32; 4] {
    if on {
        parse_color("primary")
    } else {
        [0.0, 0.0, 0.0, 0.0]
    }
}

fn build_indicator(on: bool) -> Element {
    let mut dot: Element = r#box()
        .w(DOT_PX)
        .h(DOT_PX)
        .min_w(DOT_PX)
        .min_h(DOT_PX)
        .rounded_full()
        .bg(indicator_dot_color(on))
        .flex_shrink(0.0)
        .into();
    dot.attrs
        .insert(ATTR_INDICATOR.to_string(), SLOT_INDICATOR.to_string());
    dot
}

fn find_indicator_mut(el: &mut Element) -> Option<&mut Element> {
    el.children.iter_mut().find(|c| {
        c.attrs
            .get(ATTR_INDICATOR)
            .map(|s| s.as_str() == SLOT_INDICATOR)
            .unwrap_or(false)
    })
}

/// 构建单个 radio 项（复合 Item 与 `options` Builder 共用）。
fn build_radio_item(
    group_id: &str,
    selected: Signal<String>,
    item_value: &str,
    captions_raw: Vec<Element>,
    disabled: bool,
) -> Element {
    let focus_id = format!("{group_id}:{item_value}");
    let on = selected.peek() == item_value;
    let has_caption = !captions_raw.is_empty();

    let mut control: Element = hstack()
        .w(BOX_PX)
        .h(BOX_PX)
        .min_w(BOX_PX)
        .min_h(BOX_PX)
        .rounded_full()
        .items_center()
        .justify_center()
        .flex_shrink(0.0)
        .into();
    control = control
        .border(control_border(), BORDER_W)
        .bg(control_bg())
        .child(build_indicator(on));
    control.radio_selected_sync = Some(selected);
    crate::authoring::presentation::attach(
        &mut control,
        crate::authoring::presentation::Phase::State,
        |element, layout, _context| {
            let mut impact = LayoutStatus::Clean;
            sync_radio_checked_node(element, layout, &mut impact);
            impact
        },
    );
    control
        .attrs
        .insert(ATTR_RADIO_VALUE.to_string(), item_value.to_string());

    let sel = selected;
    let v = item_value.to_string();
    let fid_click = focus_id.clone();
    let mut control: Element = interactive(control)
        .focus_id(focus_id.clone())
        .focusable(!disabled)
        .focus_ring(!disabled)
        .on_click(move || {
            if !disabled {
                use_signal("__jadix_button_focus_id", String::new).set(fid_click.clone());
                sel.set(v.clone());
            }
        })
        .into();
    control = control.accesskit_role("radio");
    if disabled {
        if !has_caption {
            control = control.opacity(0.5);
        }
        control = control.cursor_not_allowed();
    } else {
        control = control.cursor_kind(crate::core::tree::CursorKind::Pointer);
    }
    control.template_id = Some(focus_id.clone());

    if !has_caption {
        return control;
    }

    let (captions, labelled_by) = wire_radio_captions(&focus_id, captions_raw);
    if let Some(aid) = labelled_by {
        if !control.attrs.contains_key("aria-labelledby")
            && !control.attrs.contains_key("accesskit_labelled_by")
        {
            control.attrs.insert("aria-labelledby".to_string(), aid);
        }
    }

    // solid-ui Item：`flex items-center space-x-2`
    let mut row: Element = hstack()
        .gap("0.5rem")
        .items_center()
        .child(control)
        .children(captions)
        .into();
    row.template_id = Some(focus_id);
    if disabled {
        row = row.opacity(0.5).cursor_not_allowed();
    }
    row
}

// ---------------------------------------------------------------------------
// Scope（宏 `<RadioGroup>` 无 options 时）
// ---------------------------------------------------------------------------

/// 提供 RadioGroup 上下文并构建子树（`grid gap-2`）。
pub fn radio_group_scope(
    id: impl Into<String>,
    value: Option<Signal<String>>,
    default_value: impl Into<String>,
    disabled: bool,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = crate::utils::resolve_instance_id(id, "radio");
    let default_value = default_value.into();
    let value = resolve_string_signal(value, &id, "radio", default_value);
    with_child_owner(move || {
        provide_context(
            radio_group_context(),
            RadioGroupCtx {
                id: id.clone(),
                value,
                disabled,
            },
        );
        let kids = take_fragment(body());
        let mut root: Element = vstack()
            .gap("0.5rem")
            .w("100%")
            .min_w(0.0)
            .children(kids)
            .into();
        root = root.accesskit_role("radiogroup");
        root.attrs.insert("accesskit_id".to_string(), id.clone());
        root.template_id = Some(id);
        attach_focus_roving(&mut root, FocusRovingAxis::Vertical);
        root
    })
}

// ---------------------------------------------------------------------------
// RSX parts
// ---------------------------------------------------------------------------

/// solid-ui `RadioGroupItem`：圆形控制面 + 可选文案。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn RadioGroupItem(value: String, disabled: bool, children: Option<Element>) -> Element {
    let ctx = require_radio_group("RadioGroupItem");
    let disabled = disabled || ctx.disabled;
    let captions = children
        .map(|c| unpack_item_children(vec![c]))
        .unwrap_or_default();
    build_radio_item(&ctx.id, ctx.value, &value, captions, disabled)
}

/// solid-ui `RadioGroupItemLabel`：`text-sm font-medium`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn RadioGroupItemLabel(children: Option<Element>) -> Element {
    let _ = require_radio_group("RadioGroupItemLabel");
    match children {
        Some(c) if is_caption_text_leaf(&c) || c.text.as_ref().is_some_and(|t| !t.is_empty()) => {
            let author = SlotAuthor::from_el(&c);
            let text = c.text.clone().filter(|s| !s.is_empty()).unwrap_or_else(|| {
                c.children
                    .iter()
                    .find_map(|ch| ch.text.clone().filter(|s| !s.is_empty()))
                    .unwrap_or_default()
            });
            let mut lbl: Element = label("").text(text).into();
            fill_default_text_size(&mut lbl, 14.0);
            fill_default_text_weight(&mut lbl, 500);
            author.apply(lbl)
        }
        Some(mut c) => {
            fill_default_text_size(&mut c, 14.0);
            fill_default_text_weight(&mut c, 500);
            if c.attrs.get("accesskit_role").is_none() {
                c = c.accesskit_role("label");
            }
            c
        }
        None => {
            let mut lbl: Element = label("").into();
            fill_default_text_size(&mut lbl, 14.0);
            fill_default_text_weight(&mut lbl, 500);
            lbl
        }
    }
}

// ---------------------------------------------------------------------------
// Builder + options 便捷路径
// ---------------------------------------------------------------------------

/// One radio for Builder `.options()` sugar. Default chrome; per-item class/style → [`radio_option`].
#[derive(Clone, Debug, Default)]
pub struct RadioOption {
    pub value: String,
    pub label: String,
    pub chrome: SlotChrome,
}

/// Sugar entry: `radio_group("tier").options([radio_option("pro", "Pro").class("ring-2")])`.
pub fn radio_option(value: impl Into<String>, label: impl Into<String>) -> RadioOption {
    RadioOption {
        value: value.into(),
        label: label.into(),
        chrome: SlotChrome::default(),
    }
}

impl RadioOption {
    crate::impl_slot_chrome_methods!();
}

impl From<(String, String)> for RadioOption {
    fn from((value, label): (String, String)) -> Self {
        Self {
            value,
            label,
            chrome: SlotChrome::default(),
        }
    }
}

impl From<(&str, &str)> for RadioOption {
    fn from((value, label): (&str, &str)) -> Self {
        Self {
            value: value.to_string(),
            label: label.to_string(),
            chrome: SlotChrome::default(),
        }
    }
}

pub struct RadioGroupBuilder {
    group_id: String,
    selected: Option<Signal<String>>,
    default_value: String,
    options: Vec<RadioOption>,
    disabled: bool,
    children: Vec<Element>,
    chrome: SlotChrome,
}

/// RSX `<RadioGroup options=…>`；无 `options` 时宏走 [`radio_group_scope`]。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn RadioGroup(
    id: String,
    value: Option<Signal<String>>,
    /// 旧名；与 `value` 二选一，`value` 优先。
    selected: Option<Signal<String>>,
    default_value: String,
    options: Option<Vec<(String, String)>>,
    disabled: bool,
    children: Option<Element>,
) -> Element {
    let sig = value.or(selected);
    if let Some(opts) = options {
        let mut b = radio_group(&id)
            .default_value(default_value)
            .disabled(disabled)
            .options(opts);
        if let Some(s) = sig {
            b = b.value_signal(s);
        }
        if let Some(c) = children {
            b = b.child(c);
        }
        return b.into();
    }
    // 无 options：若宏未走 scope（纯 Props），仍包一层 scope 以支持 children 内 Item。
    radio_group_scope(&id, sig, default_value, disabled, || {
        children.unwrap_or_else(Element::new)
    })
}

pub fn radio_group(group_id: impl Into<String>) -> RadioGroupBuilder {
    RadioGroupBuilder {
        group_id: group_id.into(),
        selected: None,
        default_value: String::new(),
        options: Vec::new(),
        disabled: false,
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl RadioGroupBuilder {
    crate::impl_slot_chrome_methods!();

    pub fn value_signal(mut self, s: Signal<String>) -> Self {
        self.selected = Some(s);
        self
    }

    /// 兼容旧 API。
    pub fn selected_signal(self, s: Signal<String>) -> Self {
        self.value_signal(s)
    }

    pub fn default_value(mut self, v: impl Into<String>) -> Self {
        self.default_value = v.into();
        self
    }

    /// Data-driven sugar: [`radio_option`] or `(value, label)` tuples.
    pub fn options(mut self, pairs: impl IntoIterator<Item = impl Into<RadioOption>>) -> Self {
        self.options = pairs.into_iter().map(Into::into).collect();
        self
    }

    pub fn disabled(mut self, v: bool) -> Self {
        self.disabled = v;
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        for c in children {
            self = self.child(c);
        }
        self
    }
}

impl From<RadioGroupBuilder> for Element {
    fn from(builder: RadioGroupBuilder) -> Self {
        let id = crate::utils::resolve_instance_id(builder.group_id, "radio");
        let default_value = builder.default_value;
        let disabled = builder.disabled;
        let options = builder.options;
        let extra = builder.children;
        let selected = resolve_string_signal(builder.selected, &id, "radio", default_value.clone());

        with_child_owner(move || {
            provide_context(
                radio_group_context(),
                RadioGroupCtx {
                    id: id.clone(),
                    value: selected,
                    disabled,
                },
            );

            let mut items = Vec::with_capacity(options.len() + extra.len());
            for opt in options {
                let caption = Element::new().text(opt.label);
                let el = build_radio_item(&id, selected, &opt.value, vec![caption], disabled);
                items.push(opt.chrome.apply(el));
            }
            items.extend(extra);

            let mut root: Element = vstack()
                .gap("0.5rem")
                .w("100%")
                .min_w(0.0)
                .children(items)
                .into();
            root = root.accesskit_role("radiogroup");
            root.attrs.insert("accesskit_id".to_string(), id.clone());
            root.template_id = Some(id);
            attach_focus_roving(&mut root, FocusRovingAxis::Vertical);
            builder.chrome.apply(root)
        })
    }
}

impl crate::core::tree::IntoElementList for RadioGroupBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

pub struct RadioGroupItemBuilder {
    value: String,
    disabled: bool,
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn radio_group_item(value: impl Into<String>) -> RadioGroupItemBuilder {
    RadioGroupItemBuilder {
        value: value.into(),
        disabled: false,
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl RadioGroupItemBuilder {
    crate::impl_slot_chrome_methods!();

    pub fn disabled(mut self, v: bool) -> Self {
        self.disabled = v;
        self
    }
    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }
    pub fn text(mut self, content: impl Into<String>) -> Self {
        self.children.push(Element::new().text(content.into()));
        self
    }
}

impl From<RadioGroupItemBuilder> for Element {
    fn from(builder: RadioGroupItemBuilder) -> Self {
        let el = RadioGroupItem(RadioGroupItemProps {
            value: Some(builder.value),
            disabled: Some(builder.disabled),
            children: crate::components::base::packed_children(builder.children),
            base: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

impl crate::core::tree::IntoElementList for RadioGroupItemBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

crate::impl_simple_slot_builder!(
    RadioGroupItemLabelBuilder,
    radio_group_item_label,
    RadioGroupItemLabel,
    RadioGroupItemLabelProps
);

// ---------------------------------------------------------------------------
// Per-frame sync
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) fn sync_radio_checked_subtree(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
) {
    crate::core::tree::walk_elements_mut(el, |element| {
        sync_radio_checked_node(element, layout_tree, impact)
    });
}

fn sync_radio_checked_node(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
) {
    if let Some(sel) = el.radio_selected_sync {
        if !sel.is_alive() {
            el.radio_selected_sync = None;
        } else if el.attrs.get(ATTR_RADIO_VALUE).is_some() {
            // TabsTrigger / 菜单外复用 `radio_selected_sync` 只表示选中值，没有
            // `jadix_radio_value`。若仍走本分支会插入 size-4 Indicator，毁掉文案叶测宽，
            // gallery TabsList 芯片会全部挤在左侧重叠（内容宽变成圆点宽）。
            let item_value = el
                .attrs
                .get(ATTR_RADIO_VALUE)
                .map(|s| s.as_str())
                .unwrap_or("");
            let on = sel.peek() == item_value;
            let want_border = control_border();
            let want_bg = control_bg();
            let want_dot = indicator_dot_color(on);

            let chrome_changed = el.style.bg_color != want_bg
                || el.style.border_color != want_border
                || el.style.border_width != BORDER_W
                || el.style.border_style != LineStyle::Solid;
            if chrome_changed {
                if !is_variant_style_overridden(&el.attrs, VariantStyleField::Bg) {
                    el.style.bg_color = want_bg;
                }
                if !is_variant_style_overridden(&el.attrs, VariantStyleField::Border) {
                    el.style.border_color = want_border;
                    el.style.border_width = BORDER_W;
                    el.style.border_style = LineStyle::Solid;
                }
            }

            let indicator_changed = if let Some(dot) = find_indicator_mut(el) {
                let hit = !is_variant_style_overridden(&dot.attrs, VariantStyleField::Bg)
                    && ((dot.style.bg_color[0] - want_dot[0]).abs() > 0.001
                        || (dot.style.bg_color[1] - want_dot[1]).abs() > 0.001
                        || (dot.style.bg_color[2] - want_dot[2]).abs() > 0.001
                        || (dot.style.bg_color[3] - want_dot[3]).abs() > 0.001);
                if hit {
                    dot.style.bg_color = want_dot;
                }
                hit
            } else {
                el.children.insert(0, build_indicator(on));
                true
            };

            if chrome_changed || indicator_changed {
                if el.layout_node.is_some() {
                    layout_tree.update_taffy_node_style(el);
                }
                if let Some(dot) = find_indicator_mut(el) {
                    if dot.layout_node.is_some() {
                        layout_tree.update_taffy_node_style(dot);
                    }
                }
                crate::app::damage::push_frame_paint_element(el, el.paint_scroll_acc);
                *impact = impact.merge(LayoutStatus::StyleChanged);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "a11y")]
    use crate::a11y::bridge::{node_id_for_key, tree_update_from_elements};
    #[cfg(feature = "a11y")]
    use accesskit::Role;

    #[test]
    fn options_builder_builds_circular_radios() {
        let el: Element = radio_group("tier")
            .default_value("free")
            .options(vec![("free", "Free"), ("pro", "Pro")])
            .into();
        assert_eq!(
            el.attrs.get("accesskit_role").map(|s| s.as_str()),
            Some("radiogroup")
        );
        let first = &el.children[0];
        let ctrl = first
            .children
            .iter()
            .find(|c| c.radio_selected_sync.is_some())
            .expect("control");
        assert_eq!(ctrl.focus_id.as_deref(), Some("tier:free"));
        assert_eq!(
            ctrl.attrs.get("accesskit_role").map(|s| s.as_str()),
            Some("radio")
        );
        let dot = ctrl
            .children
            .iter()
            .find(|c| {
                c.attrs
                    .get(ATTR_INDICATOR)
                    .map(|s| s.as_str() == SLOT_INDICATOR)
                    .unwrap_or(false)
            })
            .expect("indicator");
        // default free → on
        assert!(dot.style.bg_color[3] > 0.5);
    }

    #[test]
    fn compound_item_requires_scope() {
        let el = radio_group_scope("plan", None, "a", false, || {
            RadioGroupItem(RadioGroupItemProps {
                value: Some("a".into()),
                disabled: None,
                children: Some(Element::new().text("A")),
                base: None,
            })
        });
        assert_eq!(
            el.attrs.get("accesskit_role").map(|s| s.as_str()),
            Some("radiogroup")
        );
        let item = &el.children[0];
        let ctrl = item
            .children
            .iter()
            .find(|c| c.radio_selected_sync.is_some())
            .expect("control");
        assert_eq!(ctrl.focus_id.as_deref(), Some("plan:a"));
    }

    #[cfg(feature = "a11y")]
    #[test]
    fn radio_exposes_role_in_accesskit() {
        let el: Element = radio_group("a11y_rg")
            .default_value("x")
            .options(vec![("x", "X")])
            .into();
        let root = Element::new().child(el);
        let update = tree_update_from_elements(&root, None);
        let gid = node_id_for_key("a11y_rg");
        let rid = node_id_for_key("a11y_rg:x");
        let group = update
            .nodes
            .iter()
            .find(|(i, _)| *i == gid)
            .map(|(_, n)| n)
            .expect("group");
        let radio = update
            .nodes
            .iter()
            .find(|(i, _)| *i == rid)
            .map(|(_, n)| n)
            .expect("radio");
        assert_eq!(group.role(), Role::RadioGroup);
        assert_eq!(radio.role(), Role::RadioButton);
    }

    #[test]
    fn builder_item_style() {
        use crate::core::reactivity::create_root;
        let (_owner, el): (_, Element) = create_root(|| {
            radio_group_scope("rg-chrome", None, "", false, || {
                radio_group_item("a")
                    .style(|b| b.text_size(20.0))
                    .text("A")
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
            "radio_group_item .style(|b| b.text_size(20)) must land on the item"
        );
    }

    #[test]
    fn builder_option_style_reaches_item() {
        use crate::core::reactivity::create_root;
        let (_owner, el): (_, Element) = create_root(|| {
            radio_group("rg-opt-chrome")
                .options([radio_option("a", "A").style(|b| b.text_size(20.0))])
                .into()
        });
        fn find_size(el: &Element, want: f32) -> bool {
            el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
                || (el.text_style.size_resolved() - want).abs() < 0.01
                || el.children.iter().any(|c| find_size(c, want))
        }
        assert!(
            find_size(&el, 20.0),
            "radio_option .style must land on the assembled item"
        );
    }
}
