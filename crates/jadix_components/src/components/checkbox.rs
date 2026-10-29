//! # Checkbox
//!
//! 对齐 solid-ui / Kobalte **Checkbox** 控制面 + ShadCN 选中态色板：
//! `size-4`, `rounded-sm`, unselected `border-primary`, selected `bg-primary` + Lucide `check`.
//! Supports controlled/uncontrolled, `disabled`, `invalid` (`aria-invalid` look and feel).
//!
//! ## Usage
//! ```ignore
//! // 内嵌文案（推荐组合写法；自动横排 + Label `for` / aria-labelledby）
//! <Checkbox>"Accept terms and conditions"</Checkbox>
//! <Checkbox>
//!     <Label font_weight="500">"Accept terms"</Label>
//! </Checkbox>
//! // Pair with side Label (Solid `for`)
//! <HStack gap="0.5rem" items="center">
//!     <Checkbox id="terms" />
//!     <Label for="terms">"Accept terms and conditions"</Label>
//! </HStack>
//! // 描述文案放进子容器：
//! <HStack gap="0.5rem" items="start">
//!     <Checkbox id="terms-2" defaultChecked=true />
//!     <VStack gap="0.25rem">
//!         <Label for="terms-2" font_weight="500">"Accept terms"</Label>
//!         <Label color="muted-foreground">"You agree to our Terms…"</Label>
//!     </VStack>
//! </HStack>
//! // Builder
//! { Element::from(checkbox("demo").checked_signal(sig).invalid(true)) }
//! ```
//!
//! ## Notes
//! - 未传 `checked` 时走 [`crate::utils::toggle_state::resolve_bool_signal`]（Owner 作用域 `{id}__checkbox`）；多实例请保证 `id` 唯一或依赖自动 id。
//! - 状态用 `peek` + 每帧 style sync；Indicator 始终保留勾选字形，未选中仅字色透明。
//! - Focusable; Space/Enter toggle shares the button path; nested copy / Label `for` clicks forward to this control.
//! - With children, the whole Checkbox box (including gap/padding/description) activates the control.
//!   Its default width hugs content; explicit width/padding can make a selectable card.

use crate::components::base::SlotChrome;
use crate::components::label::{label, label_accesskit_id_for, ATTR_LABEL_FOR};
use crate::core::reactivity::Signal;
use crate::core::tree::{parse_color, parse_text_color, Element, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::hstack;
use crate::style::{is_dark_theme, LineStyle};
use crate::utils::lucide_glyph;
use crate::utils::resolve_instance_id;
use crate::utils::text_node::build_icon_text_node;
use crate::utils::toggle_state::{bool_on_click, resolve_bool_signal, SLOT_CHECKBOX};

const ATTR_INVALID: &str = "data-invalid";
const ATTR_INDICATOR: &str = "data-slot";
const SLOT_INDICATOR: &str = "checkbox-indicator";
/// The solid-ui Control is `size-4` (16px); the Indicator checkbox is slightly smaller than the box to facilitate visual centering.
const BOX_PX: f32 = 16.0;
const CHECK_PX: f32 = 12.0;
/// 未选边框略粗于 1px，小尺寸上更易辨认（仍接近 solid-ui `border`）。
const BORDER_W: f32 = 1.5;

fn unchecked_bg() -> [f32; 4] {
    // solid-ui / shadcn: shallow bottom; dark slightly uses input transparent overlay
    if is_dark_theme() {
        let mut c = parse_color("input");
        c[3] = 0.3;
        c
    } else {
        let mut c = parse_color("background");
        c[3] = 1.0;
        c
    }
}

fn unchecked_border(invalid: bool) -> [f32; 4] {
    // solid-ui: unchecked `border-primary` (clearer than border-input); invalid → destructive
    if invalid {
        parse_color("destructive")
    } else {
        parse_color("primary")
    }
}

fn checked_chrome() -> ([f32; 4], [f32; 4], [u8; 4]) {
    (
        parse_color("primary"),
        parse_color("primary"),
        parse_text_color("primary-foreground"),
    )
}

fn check_glyph() -> String {
    lucide_glyph("check").unwrap_or_else(|| "✓".into())
}

/// solid-ui `CheckboxPrimitive.Indicator`: Fixed Lucide `check` centered inside the box.
/// 未选中时仍保留字形，仅把字色 alpha 置 0——避免「空节点 0×0 → 选中后对号不进布局」。
fn build_indicator(fg: [u8; 4]) -> Element {
    let mut icon = build_icon_text_node(check_glyph())
        .w(CHECK_PX)
        .h(CHECK_PX)
        .min_w(CHECK_PX)
        .min_h(CHECK_PX)
        .text_size(CHECK_PX)
        .text_color(fg)
        .text_center()
        .flex_shrink(0.0);
    icon.attrs
        .insert(ATTR_INDICATOR.to_string(), SLOT_INDICATOR.to_string());
    icon.attrs
        .insert("text_align".to_string(), "center".to_string());
    icon
}

fn find_indicator_mut(el: &mut Element) -> Option<&mut Element> {
    el.children.iter_mut().find(|c| {
        c.attrs
            .get(ATTR_INDICATOR)
            .map(|s| s.as_str() == SLOT_INDICATOR)
            .unwrap_or(false)
    })
}

fn apply_indicator_state(indicator: &mut Element, fg: [u8; 4]) -> bool {
    let want = check_glyph();
    let want_align = crate::style::TextAlign::Center;
    let changed = indicator.text.as_deref() != Some(want.as_str())
        || indicator.text_style.color_resolved() != fg
        || (indicator.text_style.size_resolved() - CHECK_PX).abs() > f32::EPSILON
        || indicator.text_style.align_resolved() != want_align
        || indicator.text_style.font_family.as_deref() != Some("lucide");
    if changed {
        indicator.text = Some(want);
        indicator.text_style.font_family = Some("lucide".into());
        indicator.text_style.size = Some(CHECK_PX);
        indicator.text_style.color = Some(fg);
        indicator.text_style.align = Some(want_align);
        indicator
            .attrs
            .insert("text_align".to_string(), "center".to_string());
    }
    changed
}

fn indicator_fg(on: bool) -> [u8; 4] {
    if on {
        parse_text_color("primary-foreground")
    } else {
        [0, 0, 0, 0]
    }
}

fn unpack_checkbox_children(mut children: Vec<Element>) -> Vec<Element> {
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
        && el.focus_id.as_deref().is_none_or(|s| s.is_empty())
        && el.attrs.get("accesskit_role").map(|s| s.as_str()) != Some("label")
}

fn is_label_element(el: &Element) -> bool {
    el.attrs.get("accesskit_role").map(|s| s.as_str()) == Some("label")
}

/// 把内嵌字符串 / Label 接到本 Checkbox：`for` + `accesskit_id` + 控件 `aria-labelledby`。
fn wire_checkbox_captions(
    control_id: &str,
    children: Vec<Element>,
) -> (Vec<Element>, Option<String>) {
    let mut captions = Vec::with_capacity(children.len());
    let mut labelled_by = None::<String>;
    for child in children {
        let caption = if is_caption_text_leaf(&child) {
            let text = child.text.clone().unwrap_or_default();
            label("").text(text).for_target(control_id).into()
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

pub struct CheckboxBuilder {
    id: String,
    checked: Option<Signal<bool>>,
    default_checked: bool,
    disabled: bool,
    invalid: bool,
    children: Vec<Element>,
    chrome: SlotChrome,
}

crate::__component_contract! {
CheckboxProps [];
/// RSX `<Checkbox>`。
#[allow(non_snake_case)]
pub fn Checkbox(
identity [] id: String,
plain [] checked: Option<Signal<bool>>,
default [] default_checked: bool,
default [] disabled: bool,
default [] invalid: bool,
default [] on_click: crate::core::tree::Handler,
plain [] children: Option<Element>,
) -> Element {
    let mut builder = checkbox(&id)
        .default_checked(default_checked)
        .disabled(disabled)
        .invalid(invalid);

    if let Some(sig) = checked {
        builder = builder.checked_signal(sig);
    }

    if let Some(c) = children {
        builder = builder.child(c);
    }

    let mut el: Element = builder.into();
    if !disabled {
        let mut handler = on_click;
        let target_is_root = el.checkbox_sync.is_some();
        if target_is_root {
            let mut old_on_click = el.on_click.clone();
            el = el.on_click(move || {
                if let Some(ref mut rc) = old_on_click {
                    rc.borrow_mut()();
                }
                (handler.0)(());
            });
        } else if let Some(idx) = el.children.iter().position(|c| c.checkbox_sync.is_some()) {
            let mut ctrl = std::mem::take(&mut el.children[idx]);
            let mut old_on_click = ctrl.on_click.clone();
            ctrl = ctrl.on_click(move || {
                if let Some(ref mut rc) = old_on_click {
                    rc.borrow_mut()();
                }
                (handler.0)(());
            });
            el.children[idx] = ctrl;
        }
    }
    el
}
}

pub fn checkbox(id: impl Into<String>) -> CheckboxBuilder {
    CheckboxBuilder {
        id: id.into(),
        checked: None,
        default_checked: false,
        disabled: false,
        invalid: false,
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl CheckboxBuilder {
    crate::impl_slot_chrome_methods!();

    pub fn checked_signal(mut self, s: Signal<bool>) -> Self {
        self.checked = Some(s);
        self
    }

    pub fn default_checked(mut self, v: bool) -> Self {
        self.default_checked = v;
        self
    }

    pub fn disabled(mut self, v: bool) -> Self {
        self.disabled = v;
        self
    }

    pub fn invalid(mut self, v: bool) -> Self {
        self.invalid = v;
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

impl From<CheckboxBuilder> for Element {
    fn from(builder: CheckboxBuilder) -> Self {
        let chrome = builder.chrome;
        let id = resolve_instance_id(builder.id, "checkbox");
        let sig = resolve_bool_signal(builder.checked, &id, SLOT_CHECKBOX, builder.default_checked);
        let on = sig.peek();
        let disabled = builder.disabled;
        let invalid = builder.invalid;
        let captions_raw = unpack_checkbox_children(builder.children);
        let has_caption = !captions_raw.is_empty();

        let (bg, border) = if on {
            let (bg, border, _) = checked_chrome();
            (bg, border)
        } else {
            (unchecked_bg(), unchecked_border(invalid))
        };
        let fg = indicator_fg(on);

        // solid-ui Control: `size-4 shrink-0 rounded-sm border border-primary`
        // 必须走 `border()`：默认 `border_style=None`，只设 width/color 不会画出边框。
        let mut control: Element = hstack()
            .w(BOX_PX)
            .h(BOX_PX)
            .min_w(BOX_PX)
            .min_h(BOX_PX)
            .rounded("0.125rem")
            .items_center()
            .justify_center()
            .flex_shrink(0.0)
            .into();
        control = control.border(border, BORDER_W).bg(bg);
        control = control.child(build_indicator(fg));
        control.checkbox_sync = Some(sig);
        crate::authoring::presentation::attach(
            &mut control,
            crate::authoring::presentation::Phase::State,
            |element, layout, _context| {
                let mut impact = LayoutStatus::Clean;
                sync_checkbox_checked_node(element, layout, &mut impact);
                impact
            },
        );
        if invalid {
            control
                .attrs
                .insert(ATTR_INVALID.to_string(), "true".to_string());
        }

        let mut control: Element = interactive(control)
            .focus_id(id.clone())
            .focusable(!disabled)
            .focus_ring(!disabled)
            .on_click(bool_on_click(sig, disabled, id.clone()))
            .into();
        if disabled {
            // 有文案时透明度挂在组合根，避免控制面再叠一层半透明。
            if !has_caption {
                control = control.opacity(0.5);
            }
            control = control.cursor_not_allowed();
        } else {
            control = control.cursor_kind(crate::core::tree::CursorKind::Pointer);
        }
        control.template_id = Some(id.clone());
        if disabled && !has_caption {
            control.set_attr("disabled", "true");
        }
        control = crate::components::base::stamp_slot(control, "checkbox");

        if !has_caption {
            return chrome.apply(control);
        }

        let (captions, labelled_by) = wire_checkbox_captions(&id, captions_raw);
        if let Some(aid) = labelled_by {
            if !control.attrs.contains_key("aria-labelledby")
                && !control.attrs.contains_key("accesskit_labelled_by")
            {
                control.attrs.insert("aria-labelledby".to_string(), aid);
            }
        }

        // 组合根：控制面 + 文案横排。焦点 / sync / a11y CheckBox 仍在控制面上。
        let mut root: Element = hstack()
            .w("fit-content")
            .gap("0.5rem")
            .items_center()
            .child(control)
            .children(captions)
            .into();
        // Reuse control forwarding so padding, gap and nested descriptions
        // activate the current control callback once and give it keyboard focus.
        root.attrs.insert(
            crate::app::pointer_events::ATTR_ACTIVATE_FOR.to_string(),
            id.clone(),
        );
        root.template_id = Some(id);
        if disabled {
            root = root.opacity(0.5).cursor_not_allowed();
        } else {
            root = root.cursor_kind(crate::core::tree::CursorKind::Pointer);
        }
        if disabled {
            root.set_attr("disabled", "true");
        }
        crate::components::base::paint_slot(root, "checkbox-group", &chrome)
    }
}

impl crate::core::tree::IntoElementList for CheckboxBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[cfg(all(test, feature = "product-tests"))]
pub(crate) fn sync_checkbox_checked_subtree(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
) {
    let mut dirty_nodes = Vec::new();
    crate::core::tree::walk_elements_mut(el, |element| {
        let mut changed = LayoutStatus::Clean;
        sync_checkbox_checked_node(element, layout_tree, &mut changed);
        if changed != LayoutStatus::Clean {
            if let Some(node) = element.layout_node_id() {
                dirty_nodes.push(node);
            }
        }
        *impact = impact.merge(changed);
    });
    crate::authoring::widgets::collect::invalidate_collect_paint_hash_spines(el, &dirty_nodes);
}

fn sync_checkbox_checked_node(
    el: &mut Element,
    _layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
) {
    let mut dirty_nodes = Vec::new();
    if let Some(sig) = el.checkbox_sync {
        if !sig.is_alive() {
            // Menu Content 等作用域关闭后，树节点可能仍短暂留在本帧；勿 peek 已 dispose 信号。
            el.checkbox_sync = None;
        } else {
            let on = sig.peek();
            let invalid = el
                .attrs
                .get(ATTR_INVALID)
                .map(|s| s.as_str() == "true")
                .unwrap_or(false);
            let (want_bg, want_border) = if on {
                let (bg, border, _) = checked_chrome();
                (bg, border)
            } else {
                (unchecked_bg(), unchecked_border(invalid))
            };
            let want_fg = indicator_fg(on);

            let chrome_changed = el.style.bg_color != want_bg
                || el.style.border_color != want_border
                || el.style.border_width != BORDER_W
                || el.style.border_style != LineStyle::Solid;
            if chrome_changed {
                el.style.bg_color = want_bg;
                el.style.border_color = want_border;
                el.style.border_width = BORDER_W;
                el.style.border_style = LineStyle::Solid;
            }

            let indicator_changed = if let Some(indicator) = find_indicator_mut(el) {
                apply_indicator_state(indicator, want_fg)
            } else {
                el.children.insert(0, build_indicator(want_fg));
                *impact = impact.merge(LayoutStatus::StructureChanged);
                true
            };

            if chrome_changed || indicator_changed {
                if let Some(node) = el.layout_node_id() {
                    dirty_nodes.push(node);
                }
                crate::app::damage::push_frame_paint_element(el, el.paint_scroll_offset());
                *impact = impact.merge(LayoutStatus::StyleChanged);
            }
        }
    }
    crate::authoring::widgets::collect::invalidate_collect_paint_hash_spines(el, &dirty_nodes);
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/checkbox.rs"]
mod tests;
