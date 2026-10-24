//! # Switch
//!
//! 对齐 solid-ui / Kobalte **Switch** 控制面：
//! `h-6 w-11` 胶囊轨道、`size-5` 拇指、`data-[checked]:bg-primary` / 未选 `bg-input`，
//! 拇指 `translate-x-0` → `translate-x-5`（约 20px）带短过渡；支持受控 / 非受控与禁用。
//!
//! solid-ui 复合写法：
//! ```text
//! Switch
//! ├── SwitchControl   // 轨道（含 Input 焦点环）
//! │   └── SwitchThumb
//! └── SwitchLabel
//! ```
//! Jadix 根组件自动拼 Control + Thumb；文案可内嵌（自动横排 + Label `for`），
//! 或与旁侧 [`Label`](crate::components::Label) 用 Solid `id` / `for` 配对。
//!
//! ## Usage
//! ```ignore
//! // 内嵌文案
//! <Switch>"Airplane Mode"</Switch>
//! <Switch>
//!     <Label font_weight="500">"Airplane Mode"</Label>
//! </Switch>
//! // 与旁侧 Label 配对
//! <HStack gap="0.5rem" items="center">
//!     <Switch id="airplane" />
//!     <Label for="airplane">"Airplane Mode"</Label>
//! </HStack>
//! // 受控
//! <Switch id="demo" checked={on} />
//! <Switch id="demo2" defaultChecked=true />
//! // Builder
//! { Element::from(switch("demo").checked_signal(on_sig)) }
//! ```
//!
//! ## Notes
//! - 未传 `checked` 时走 [`crate::utils::toggle_state::resolve_bool_signal`]（Owner 作用域 `{id}__switch`）；多实例请保证 `id` 唯一或依赖自动 id。
//! - 状态用 `peek` + 每帧 style sync；拇指位移 / 轨道色走 CSS
//!   `transition-transform` / `transition-colors`（~150ms）。
//! - Focusable; Space/Enter toggle shares the button path; nested copy / Label `for` clicks forward to this control.

use crate::components::base::SlotChrome;
use crate::components::label::{label, label_accesskit_id_for, ATTR_LABEL_FOR};
use crate::core::reactivity::Signal;
use crate::core::tree::{parse_color, Element, IntoCssDim, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box};
use crate::style::{TimingFunction, TransitionEntry, TransitionProperty};
use crate::tokens;
use crate::utils::resolve_instance_id;
use crate::utils::toggle_state::{bool_on_click, resolve_bool_signal, SLOT_SWITCH};
use jadix_macros::component;

const ATTR_THUMB: &str = "data-slot";
const SLOT_THUMB: &str = "switch-thumb";

/// solid-ui Control：`h-6 w-11`（24×44）；Thumb：`size-5`（20）；行程 `translate-x-5`（20）。
const TRACK_W: f32 = 44.0;
const TRACK_H: f32 = 24.0;
const THUMB_PX: f32 = 20.0;
/// 轨道内边距 2px ×2 + 拇指 20 → 行程 20（对标 `data-[checked]:translate-x-5`）。
const THUMB_TRAVEL: f32 = 20.0;
const TRACK_PAD: f32 = 2.0;
const SWITCH_TRANSITION_MS: f32 = 150.0;

fn track_off_bg() -> [f32; 4] {
    parse_color("input")
}

fn track_on_bg() -> [f32; 4] {
    parse_color("primary")
}

fn thumb_bg() -> [f32; 4] {
    parse_color("background")
}

fn ensure_switch_track_css_transition(el: &mut Element) {
    let timing = TimingFunction::CubicBezier(0.4, 0.0, 0.2, 1.0);
    if let Some(entry) = el
        .style
        .transition
        .iter_mut()
        .find(|e| e.property == TransitionProperty::BackgroundColor)
    {
        entry.duration_ms = SWITCH_TRANSITION_MS;
        entry.timing = timing;
    } else {
        el.style.transition.push(TransitionEntry {
            property: TransitionProperty::BackgroundColor,
            duration_ms: SWITCH_TRANSITION_MS,
            delay_ms: 0.0,
            timing,
        });
    }
}

fn ensure_switch_thumb_css_transition(el: &mut Element) {
    let timing = TimingFunction::CubicBezier(0.4, 0.0, 0.2, 1.0);
    if let Some(entry) = el
        .style
        .transition
        .iter_mut()
        .find(|e| e.property == TransitionProperty::Transform)
    {
        entry.duration_ms = SWITCH_TRANSITION_MS;
        entry.timing = timing;
    } else {
        el.style.transition.push(TransitionEntry {
            property: TransitionProperty::Transform,
            duration_ms: SWITCH_TRANSITION_MS,
            delay_ms: 0.0,
            timing,
        });
    }
}

fn unpack_switch_children(mut children: Vec<Element>) -> Vec<Element> {
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

/// 把内嵌字符串 / Label 接到本 Switch：`for` + `accesskit_id` + 控件 `aria-labelledby`。
fn wire_switch_captions(
    control_id: &str,
    children: Vec<Element>,
) -> (Vec<Element>, Option<String>) {
    let mut captions = Vec::with_capacity(children.len());
    let mut labelled_by = None::<String>;
    for child in children {
        let caption = if is_caption_text_leaf(&child) {
            let text = child.text.clone().unwrap_or_default();
            label("")
                .text(text)
                .for_target(control_id)
                .text_size(14.0)
                .font_weight(500.0)
                .into()
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

fn build_thumb(translate_x: f32) -> Element {
    // solid-ui Thumb：`shadow-lg` ≈ 黑色半透明阴影；`shadow(color, blur)` 的 color 是 RGBA，
    // 切勿把 offset 写进 color（曾误写 `[0,1,0,α]` → 绿色描边）。
    let mut thumb: Element = r#box()
        .w(THUMB_PX)
        .h(THUMB_PX)
        .min_w(THUMB_PX)
        .min_h(THUMB_PX)
        .rounded_full()
        .bg(thumb_bg())
        .shadow(tokens::shadow_rgba(tokens::Elevation::Md), 4.0)
        .flex_shrink(0.0)
        .into();
    thumb = thumb.translate(translate_x, 0.0);
    ensure_switch_thumb_css_transition(&mut thumb);
    thumb
        .attrs
        .insert(ATTR_THUMB.to_string(), SLOT_THUMB.to_string());
    // 指针事件落在轨道 interactive 上（对标 solid-ui `pointer-events-none`）。
    thumb
}

fn find_thumb_mut(el: &mut Element) -> Option<&mut Element> {
    el.children.iter_mut().find(|c| {
        c.attrs
            .get(ATTR_THUMB)
            .map(|s| s.as_str() == SLOT_THUMB)
            .unwrap_or(false)
    })
}

fn push_switch_paint_damage(el: &Element) {
    crate::app::damage::push_frame_paint_element(el, el.paint_scroll_acc);
}

pub struct SwitchBuilder {
    id: String,
    checked: Option<Signal<bool>>,
    default_checked: bool,
    disabled: bool,
    children: Vec<Element>,
    chrome: SlotChrome,
}

/// RSX `<Switch>`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Switch(
    id: String,
    checked: Option<Signal<bool>>,
    default_checked: bool,
    disabled: bool,
    on_click: crate::core::tree::Handler,
    children: Option<Element>,
) -> Element {
    let mut builder = switch(&id)
        .default_checked(default_checked)
        .disabled(disabled);

    if let Some(sig) = checked {
        builder = builder.checked_signal(sig);
    }

    if let Some(c) = children {
        builder = builder.child(c);
    }

    let mut el: Element = builder.into();
    if !disabled {
        let mut handler = on_click;
        let target_is_root = el.switch_checked_sync.is_some();
        if target_is_root {
            let mut old_on_click = el.on_click.clone();
            el = el.on_click(move || {
                if let Some(ref mut rc) = old_on_click {
                    rc.borrow_mut()();
                }
                (handler.0)(());
            });
        } else if let Some(idx) = el
            .children
            .iter()
            .position(|c| c.switch_checked_sync.is_some())
        {
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

pub fn switch(id: impl Into<String>) -> SwitchBuilder {
    SwitchBuilder {
        id: id.into(),
        checked: None,
        default_checked: false,
        disabled: false,
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl SwitchBuilder {
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

    /// 尺寸固定（solid-ui `w-11 h-6`）；保留链式 API，调用为 no-op。
    pub fn w(self, w: impl IntoCssDim) -> Self {
        let _ = w;
        self
    }
}

impl From<SwitchBuilder> for Element {
    fn from(builder: SwitchBuilder) -> Self {
        let chrome = builder.chrome;
        let id = resolve_instance_id(builder.id, "switch");
        let sig = resolve_bool_signal(builder.checked, &id, SLOT_SWITCH, builder.default_checked);
        let disabled = builder.disabled;
        let captions_raw = unpack_switch_children(builder.children);
        let has_caption = !captions_raw.is_empty();

        // `peek`：不把 checked 订阅到根 effect；轨道/拇指由 `sync_switch_checked_subtree` 每帧对齐。
        let on = sig.peek();
        let tx0 = if on { THUMB_TRAVEL } else { 0.0 };
        let track_bg = if on { track_on_bg() } else { track_off_bg() };
        let thumb = build_thumb(tx0);

        // solid-ui Control: `inline-flex h-6 w-11 … rounded-full border-2 border-transparent`
        let mut track: Element = hstack()
            .w(TRACK_W)
            .h(TRACK_H)
            .min_w(TRACK_W)
            .min_h(TRACK_H)
            .rounded_full()
            .p(TRACK_PAD)
            .items_center()
            .flex_shrink(0.0)
            .bg(track_bg)
            .child(thumb)
            .into();
        ensure_switch_track_css_transition(&mut track);
        track.switch_checked_sync = Some(sig);

        let mut track: Element = interactive(track)
            .focus_id(id.clone())
            .focusable(!disabled)
            .focus_ring(!disabled)
            .on_click(bool_on_click(sig, disabled, id.clone()))
            .into();
        if disabled {
            if !has_caption {
                track = track.opacity(0.5);
            }
            track = track.cursor_not_allowed();
        } else {
            track = track.cursor_kind(crate::core::tree::CursorKind::Pointer);
        }
        track.template_id = Some(id.clone());
        if disabled && !has_caption { track.set_attr("disabled", "true"); }
        track = crate::components::base::stamp_slot(track, "switch");

        if !has_caption {
            return chrome.apply(track);
        }

        let (captions, labelled_by) = wire_switch_captions(&id, captions_raw);
        if let Some(aid) = labelled_by {
            if !track.attrs.contains_key("aria-labelledby")
                && !track.attrs.contains_key("accesskit_labelled_by")
            {
                track.attrs.insert("aria-labelledby".to_string(), aid);
            }
        }

        // 组合根：轨道 + 文案横排（solid-ui Root `flex items-center`）。焦点 / sync / a11y 仍在轨道上。
        let mut root: Element = hstack()
            .gap("0.5rem")
            .items_center()
            .child(track)
            .children(captions)
            .into();
        root.template_id = Some(id);
        if disabled {
            root = root.opacity(0.5).cursor_not_allowed();
        }
        if disabled { root.set_attr("disabled", "true"); }
        crate::components::base::paint_slot(root, "switch-group", &chrome)
    }
}

impl crate::core::tree::IntoElementList for SwitchBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

pub(crate) fn sync_switch_checked_subtree(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
) {
    crate::core::tree::walk_elements_mut(el, |el| {
        if let Some(sig) = el.switch_checked_sync {
            if !sig.is_alive() {
                // Same teardown window as checkbox: owner gone, node still in this frame.
                el.switch_checked_sync = None;
                return;
            }
            let on = sig.peek();
            let want_tx = if on { THUMB_TRAVEL } else { 0.0 };
            let want_bg = if on { track_on_bg() } else { track_off_bg() };

            ensure_switch_track_css_transition(el);

            let bg_changed = (el.style.bg_color[0] - want_bg[0]).abs() > 0.001
                || (el.style.bg_color[1] - want_bg[1]).abs() > 0.001
                || (el.style.bg_color[2] - want_bg[2]).abs() > 0.001
                || (el.style.bg_color[3] - want_bg[3]).abs() > 0.001;
            if bg_changed {
                el.style.bg_color = want_bg;
            }

            let thumb_changed = if let Some(thumb) = find_thumb_mut(el) {
                ensure_switch_thumb_css_transition(thumb);
                let want_thumb_bg = thumb_bg();
                let tx_hit = (thumb.style.translate[0] - want_tx).abs() > 0.01;
                let bg_hit = (thumb.style.bg_color[0] - want_thumb_bg[0]).abs() > 0.001
                    || (thumb.style.bg_color[1] - want_thumb_bg[1]).abs() > 0.001
                    || (thumb.style.bg_color[2] - want_thumb_bg[2]).abs() > 0.001
                    || (thumb.style.bg_color[3] - want_thumb_bg[3]).abs() > 0.001;
                if tx_hit {
                    thumb.style.translate[0] = want_tx;
                }
                if bg_hit {
                    thumb.style.bg_color = want_thumb_bg;
                }
                tx_hit || bg_hit
            } else {
                el.children.insert(0, build_thumb(want_tx));
                true
            };

            if bg_changed || thumb_changed {
                if el.layout_node.is_some() {
                    layout_tree.update_taffy_node_style(el);
                }
                if let Some(thumb) = find_thumb_mut(el) {
                    if thumb.layout_node.is_some() {
                        layout_tree.update_taffy_node_style(thumb);
                    }
                }
                // translate 不改布局几何，但仍需局部重绘；StyleChanged 避免 FullClear。
                push_switch_paint_damage(el);
                *impact = impact.merge(LayoutStatus::StyleChanged);
            } else if el.transition_snapshot.has_active_tweens()
                || find_thumb_mut(el).is_some_and(|t| t.transition_snapshot.has_active_tweens())
            {
                push_switch_paint_damage(el);
                *impact = impact.merge(LayoutStatus::StyleChanged);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checked_sync_preserves_track_damage_identity() {
        crate::core::reactivity::create_root(|| {
            let checked = crate::core::reactivity::use_signal("switch-damage-regression", || false);
            let mut root: Element = switch("damage-switch").checked_signal(checked).into();
            let mut tree = LayoutTree::new();
            tree.build_and_layout(&mut root, 240.0, 80.0, [-1.0; 2]);
            let bounds = root.layout_bounds;
            crate::app::damage::take_frame_paint_nodes();
            checked.set(true);
            let mut impact = LayoutStatus::Clean;
            sync_switch_checked_subtree(&mut root, &mut tree, &mut impact);
            let dirties = crate::app::damage::take_frame_paint_nodes();
            assert!(
                dirties
                    .iter()
                    .any(|dirty| dirty.key == crate::app::damage::paint_element_key(&root))
            );
            assert_eq!(root.layout_bounds, bounds);
            assert_eq!(impact, LayoutStatus::StyleChanged);
        });
    }

    #[cfg(feature = "a11y")]
    use crate::a11y::bridge::{node_id_for_key, tree_update_from_elements};
    use crate::components::label::{label, ATTR_LABEL_FOR};
    #[cfg(feature = "a11y")]
    use accesskit::Role;

    #[test]
    fn switch_string_child_builds_labelled_row() {
        let el: Element = switch("")
            .child(Element::new().text("Airplane Mode"))
            .into();
        let ctrl = el
            .children
            .iter()
            .find(|c| c.switch_checked_sync.is_some())
            .expect("control");
        let lbl = el
            .children
            .iter()
            .find(|c| c.attrs.get("accesskit_role").map(|s| s.as_str()) == Some("label"))
            .expect("caption label");
        assert_eq!(
            lbl.attrs.get(ATTR_LABEL_FOR).map(|s| s.as_str()),
            ctrl.focus_id.as_deref()
        );
        let aid = lbl
            .attrs
            .get("accesskit_id")
            .cloned()
            .expect("accesskit_id");
        assert_eq!(
            ctrl.attrs.get("aria-labelledby").map(|s| s.as_str()),
            Some(aid.as_str())
        );
        assert_eq!(lbl.text.as_deref(), Some("Airplane Mode"));
    }

    #[test]
    fn switch_nested_label_wires_for() {
        let el: Element = switch("air")
            .child(Element::from(label("").text("Airplane Mode")))
            .into();
        let ctrl = el
            .children
            .iter()
            .find(|c| c.switch_checked_sync.is_some())
            .expect("control");
        let lbl = el
            .children
            .iter()
            .find(|c| c.attrs.get("accesskit_role").map(|s| s.as_str()) == Some("label"))
            .expect("label");
        assert_eq!(
            lbl.attrs.get(ATTR_LABEL_FOR).map(|s| s.as_str()),
            Some("air")
        );
        assert_eq!(ctrl.focus_id.as_deref(), Some("air"));
    }

    #[test]
    fn switch_thumb_starts_off_at_origin() {
        let el: Element = switch("off").into();
        let thumb = el
            .children
            .iter()
            .find(|c| {
                c.attrs
                    .get(ATTR_THUMB)
                    .map(|s| s.as_str() == SLOT_THUMB)
                    .unwrap_or(false)
            })
            .expect("thumb");
        assert!((thumb.style.translate[0]).abs() < 0.01);
    }

    #[test]
    fn switch_thumb_starts_on_at_travel() {
        let el: Element = switch("on").default_checked(true).into();
        let thumb = el
            .children
            .iter()
            .find(|c| {
                c.attrs
                    .get(ATTR_THUMB)
                    .map(|s| s.as_str() == SLOT_THUMB)
                    .unwrap_or(false)
            })
            .expect("thumb");
        assert!((thumb.style.translate[0] - THUMB_TRAVEL).abs() < 0.01);
    }

    #[cfg(feature = "a11y")]
    #[test]
    fn switch_caption_exposes_labelled_by_in_accesskit() {
        let el: Element = switch("a11y_sw")
            .child(Element::new().text("Airplane Mode"))
            .into();
        let root = Element::new().child(el);
        let update = tree_update_from_elements(&root, None);
        let lid = node_id_for_key("lbl:for:a11y_sw");
        let cid = node_id_for_key("a11y_sw");
        let label = update
            .nodes
            .iter()
            .find(|(i, _)| *i == lid)
            .map(|(_, n)| n)
            .expect("label node");
        let sw = update
            .nodes
            .iter()
            .find(|(i, _)| *i == cid)
            .map(|(_, n)| n)
            .expect("switch");
        assert_eq!(label.role(), Role::Label);
        assert_eq!(sw.role(), Role::Switch);
        assert_eq!(sw.labelled_by(), &[lid]);
    }
}
