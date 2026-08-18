//! # Slider
//!
//! 对齐 solid-ui / Kobalte **Slider** 与 ShadCN Aria 一体滑块：
//! Track `h-2` / `bg-secondary`，Fill `bg-primary`，Thumb `size-5` / `border-primary` / `bg-background`。
//!
//! ```text
//! Slider（相对定位根）
//! ├── Track（绝对定位轨道）
//! │   └── Fill（宽度随 value）
//! └── ThumbRow（flex 定位拇指）
//!     ├── spacer
//!     ├── Thumb
//!     └── spacer
//! ```
//!
//! ## 用法
//! ```ignore
//! <Slider id="volume" value={level} default_value={0.4} />
//! <Slider id="vol2" defaultValue={0.5} step={0.01} />
//! { Element::from(slider("volume").value_signal(level).step(0.05)) }
//! ```
//!
//! ## 注意
//! - 默认值域 `0..=1`；可用 `min` / `max` / `step` 扩展。
//! - 点击 / 拖拽按指针 x **连续**设值（不按 `step` 吸附）；键盘 ←/→（或 ↑/↓）按 `step`（默认 0.05）调节。
//! - 悬停 / 拖拽时拇指阴影加深（~150ms cubic），静止为紧凑接触阴影。
//! - 值变化经 [`ReactiveSync`](crate::core::tree::ReactiveSync) 原位同步，不订阅根 UI effect。

use crate::app::pointer_events::{EventPhase, PointerEvent};
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::{parse_color, Element, LayoutStatus};
use crate::primitives::interactive::{interactive, mark_hover_paint_dirty};
use crate::primitives::r#box::{hstack, r#box};
use crate::style::use_ui_transition_cubic;
use rui_macros::component;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use taffy::prelude::Dimension;
use winit::event::KeyEvent;
use winit::keyboard::{Key, NamedKey};

const ATTR_SLOT: &str = "data-slot";
const SLOT_FILL: &str = "slider-fill";
const SLOT_TRACK: &str = "slider-track";
const SLOT_THUMB: &str = "slider-thumb";
const SLOT_LEFT: &str = "slider-left";
const SLOT_RIGHT: &str = "slider-right";

/// solid-ui Track `h-2`；Thumb `size-5`。
const TRACK_H: f32 = 8.0;
const THUMB_PX: f32 = 20.0;
const TRACK_TOP: f32 = (THUMB_PX - TRACK_H) * 0.5;
const BORDER_W: f32 = 2.0;

static NEXT_SLIDER_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Clone)]
struct SliderDrag {
    value: Signal<f32>,
    min: f32,
    max: f32,
    bounds: Rc<Cell<[f32; 4]>>,
}

/// Rest / hover 拇指阴影：`(color, blur)`（无纵向偏移）。
const THUMB_SHADOW_REST: ([f32; 4], f32) = ([0.0, 0.0, 0.0, 0.18], 2.5);
const THUMB_SHADOW_HOVER: ([f32; 4], f32) = ([0.0, 0.0, 0.0, 0.42], 5.0);

fn slider_shadow_anim_key(id: &str) -> String {
    format!("rui:slider_thumb_shadow:{id}")
}

thread_local! {
    static SLIDER_DRAG: RefCell<Option<SliderDrag>> = const { RefCell::new(None) };
}

fn begin_slider_drag(drag: SliderDrag) {
    SLIDER_DRAG.with(|d| *d.borrow_mut() = Some(drag));
}

fn end_slider_drag() {
    SLIDER_DRAG.with(|d| *d.borrow_mut() = None);
    crate::app::release_pointer_capture();
}

pub(crate) fn is_slider_dragging() -> bool {
    SLIDER_DRAG.with(|d| d.borrow().is_some())
}

/// CursorMoved：若正在拖拽滑块，按 x 更新值。
///
/// 调用方须在**已释放** `AppState` 的 `RefCell` 借用后再调用：`Signal::set` 会同步
/// flush `bind_text` 等 effect，后者会 `try_update_element_by_template_id` → 再借同一 RefCell。
pub(crate) fn tick_slider_drag(x: f32) -> bool {
    SLIDER_DRAG.with(|d| {
        let Some(drag) = d.borrow().clone() else {
            return false;
        };
        let v = value_from_pointer_x(x, drag.bounds.get(), drag.min, drag.max);
        if (drag.value.peek() - v).abs() > 1e-5 {
            drag.value.set(v);
        }
        true
    })
}

pub(crate) fn clear_slider_drag_if_any() {
    let had = SLIDER_DRAG.with(|d| d.borrow().is_some());
    if had {
        end_slider_drag();
    }
}

fn quantize(v: f32, min: f32, max: f32, step: f32) -> f32 {
    let v = v.clamp(min, max);
    if step <= 1e-8 {
        return v;
    }
    let n = ((v - min) / step).round();
    (min + n * step).clamp(min, max)
}

/// 指针映射：连续值，不按 `step` 量化（步进仅键盘）。
fn value_from_pointer_x(x: f32, bounds: [f32; 4], min: f32, max: f32) -> f32 {
    let pad = THUMB_PX * 0.5;
    let inner = (bounds[2] - THUMB_PX).max(1.0);
    let t = ((x - bounds[0] - pad) / inner).clamp(0.0, 1.0);
    (min + t * (max - min)).clamp(min, max)
}

fn thumb_shadow_elevated(hovered: bool, dragging: bool) -> bool {
    hovered || dragging
}

fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// 按 `t∈[0,1]` 插值 rest→hover 阴影；返回是否写回了样式。
fn apply_thumb_shadow_t(thumb: &mut Element, t: f32) -> bool {
    let t = t.clamp(0.0, 1.0);
    let (c0, b0) = THUMB_SHADOW_REST;
    let (c1, b1) = THUMB_SHADOW_HOVER;
    let color = [
        lerp_f32(c0[0], c1[0], t),
        lerp_f32(c0[1], c1[1], t),
        lerp_f32(c0[2], c1[2], t),
        lerp_f32(c0[3], c1[3], t),
    ];
    let blur = lerp_f32(b0, b1, t);
    let color_ok = (thumb.style.shadow_color[0] - color[0]).abs() <= 0.001
        && (thumb.style.shadow_color[1] - color[1]).abs() <= 0.001
        && (thumb.style.shadow_color[2] - color[2]).abs() <= 0.001
        && (thumb.style.shadow_color[3] - color[3]).abs() <= 0.001;
    let blur_ok = (thumb.style.shadow_blur - blur).abs() <= 0.01;
    let offset_ok =
        thumb.style.shadow_offset[0].abs() <= 0.01 && thumb.style.shadow_offset[1].abs() <= 0.01;
    if color_ok && blur_ok && offset_ok {
        return false;
    }
    thumb.style.shadow_color = color;
    thumb.style.shadow_blur = blur;
    thumb.style.shadow_offset = [0.0, 0.0];
    true
}

fn push_slider_thumb_paint_damage(root: &Element) {
    let b = root.layout_bounds;
    if b[2] <= 0.0 || b[3] <= 0.0 {
        return;
    }
    // 拇指阴影余量（紧凑阴影，pad 不必很大）
    let pad = 8.0;
    crate::app::damage::push_frame_paint_damage([
        b[0] - pad,
        b[1] - pad,
        b[2] + pad * 2.0,
        b[3] + pad * 2.0,
    ]);
}

fn norm01(v: f32, min: f32, max: f32) -> f32 {
    let span = (max - min).max(1e-8);
    ((v - min) / span).clamp(0.0, 1.0)
}

fn track_bg() -> [f32; 4] {
    parse_color("secondary")
}

fn fill_bg() -> [f32; 4] {
    parse_color("primary")
}

fn thumb_bg() -> [f32; 4] {
    parse_color("background")
}

fn thumb_border() -> [f32; 4] {
    parse_color("primary")
}

fn has_slot(el: &Element, slot: &str) -> bool {
    el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(slot)
        || el.children.iter().any(|c| has_slot(c, slot))
}

fn find_slot_deep_mut<'a>(el: &'a mut Element, slot: &str) -> Option<&'a mut Element> {
    if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(slot) {
        return Some(el);
    }
    for i in 0..el.children.len() {
        if has_slot(&el.children[i], slot) {
            if el.children[i].attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(slot) {
                return Some(&mut el.children[i]);
            }
            return find_slot_deep_mut(&mut el.children[i], slot);
        }
    }
    None
}

pub struct SliderBuilder {
    id: String,
    value: Option<Signal<f32>>,
    default_value: f32,
    min: f32,
    max: f32,
    step: f32,
    disabled: bool,
    /// When set, the track shows this `image_texture` and omits the primary fill.
    track_texture: Option<String>,
    track_texture_fn: Option<Rc<dyn Fn() -> String>>,
    /// Live thumb fill (color-picker preview). Called from `reactive_sync`.
    thumb_fill: Option<Rc<dyn Fn() -> [f32; 4]>>,
    /// Extra work during the existing slider paint sync (no extra tree subscribe).
    on_sync: Option<Rc<dyn Fn()>>,
    children: Vec<Element>,
}

/// RSX `<Slider>`。数值类 props 用 `Option`，省略时走 builder 默认（`default≈0.35`、`max=1`、`step=0.05`）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Slider(
    id: String,
    value: Option<Signal<f32>>,
    default_value: Option<f32>,
    min: Option<f32>,
    max: Option<f32>,
    step: Option<f32>,
    disabled: bool,
    on_click: crate::core::tree::Handler,
    children: Option<Element>,
) -> Element {
    let mut builder = slider(&id).disabled(disabled);
    if let Some(v) = default_value {
        builder = builder.default_value(v);
    }
    if let Some(v) = min {
        builder = builder.min(v);
    }
    if let Some(v) = max {
        builder = builder.max(v);
    }
    if let Some(v) = step {
        builder = builder.step(v);
    }
    if let Some(sig) = value {
        builder = builder.value_signal(sig);
    }
    if let Some(c) = children {
        builder = builder.child(c);
    }

    let mut el: Element = builder.into();
    if !disabled {
        let mut handler = on_click;
        let mut old_on_click = el.on_click.clone();
        el = el.on_click(move || {
            if let Some(ref mut rc) = old_on_click {
                rc.borrow_mut()();
            }
            (handler.0)(());
        });
    }
    el
}

pub fn slider(id: impl Into<String>) -> SliderBuilder {
    let id = id.into();
    let id = if id.trim().is_empty() {
        let n = NEXT_SLIDER_ID.fetch_add(1, Ordering::Relaxed);
        format!("__rui_slider_rt_{n}")
    } else {
        id
    };
    SliderBuilder {
        id,
        value: None,
        default_value: 0.35,
        min: 0.0,
        max: 1.0,
        step: 0.05,
        disabled: false,
        track_texture: None,
        track_texture_fn: None,
        thumb_fill: None,
        on_sync: None,
        children: Vec::new(),
    }
}

impl SliderBuilder {
    pub fn value_signal(mut self, s: Signal<f32>) -> Self {
        self.value = Some(s);
        self
    }

    pub fn default_value(mut self, v: f32) -> Self {
        self.default_value = v;
        self
    }

    pub fn min(mut self, v: f32) -> Self {
        self.min = v;
        self
    }

    pub fn max(mut self, v: f32) -> Self {
        self.max = v;
        self
    }

    pub fn step(mut self, v: f32) -> Self {
        self.step = v.max(0.0);
        self
    }

    pub fn disabled(mut self, v: bool) -> Self {
        self.disabled = v;
        self
    }

    /// Rainbow / alpha ramps: paint `key` on the track and hide the primary fill.
    pub fn track_texture(mut self, key: impl Into<String>) -> Self {
        self.track_texture = Some(key.into());
        self
    }

    pub fn track_texture_fn(mut self, f: impl Fn() -> String + 'static) -> Self {
        self.track_texture_fn = Some(Rc::new(f));
        self
    }

    /// Thumb background, refreshed every paint sync (does not subscribe the parent tree).
    pub fn thumb_fill(mut self, f: impl Fn() -> [f32; 4] + 'static) -> Self {
        self.thumb_fill = Some(Rc::new(f));
        self
    }

    pub fn on_sync(mut self, f: impl Fn() + 'static) -> Self {
        self.on_sync = Some(Rc::new(f));
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

impl From<SliderBuilder> for Element {
    fn from(builder: SliderBuilder) -> Self {
        let id = builder.id;
        let min = builder.min;
        let max = if builder.max <= builder.min {
            builder.min + 1.0
        } else {
            builder.max
        };
        let step = if builder.step <= 0.0 {
            (max - min) * 0.05
        } else {
            builder.step
        };
        let default_value = quantize(builder.default_value, min, max, step);
        let scope = format!("{id}__slider");
        let val = builder
            .value
            .unwrap_or_else(|| use_signal(scope.as_str(), || default_value));
        let disabled = builder.disabled;
        let bounds = Rc::new(Cell::new([0.0_f32; 4]));

        let v0 = val.peek().clamp(min, max);
        let t0 = norm01(v0, min, max);
        let left_g = t0.max(0.0001);
        let right_g = (1.0 - t0).max(0.0001);

        let textured = builder
            .track_texture
            .clone()
            .or_else(|| builder.track_texture_fn.as_ref().map(|f| f()));
        let track_texture_fn = builder.track_texture_fn.clone();

        let fill = if textured.is_none() {
            let mut fill: Element = r#box()
                .h("100%")
                .w(format!("{:.2}%", (t0 * 100.0).clamp(0.0, 100.0)))
                .rounded("9999px")
                .bg(fill_bg())
                .into();
            fill.attrs
                .insert(ATTR_SLOT.to_string(), SLOT_FILL.to_string());
            Some(fill)
        } else {
            None
        };

        let (rest_c, rest_blur) = THUMB_SHADOW_REST;
        let thumb_fill = builder.thumb_fill.clone();
        let on_sync = builder.on_sync.clone();
        let thumb0 = thumb_fill.as_ref().map(|f| f()).unwrap_or_else(thumb_bg);
        let mut thumb: Element = r#box()
            .w(THUMB_PX)
            .h(THUMB_PX)
            .min_w(THUMB_PX)
            .min_h(THUMB_PX)
            .rounded("9999px")
            .bg(thumb0)
            .border(thumb_border(), BORDER_W)
            .shadow(rest_c, rest_blur)
            .flex_shrink(0.0)
            .into();
        thumb
            .attrs
            .insert(ATTR_SLOT.to_string(), SLOT_THUMB.to_string());

        let hovered = Rc::new(Cell::new(false));
        let _ = use_ui_transition_cubic(&slider_shadow_anim_key(&id), 0.0);

        let mut left: Element = r#box().flex_grow(left_g).h("100%").into();
        left.attrs
            .insert(ATTR_SLOT.to_string(), SLOT_LEFT.to_string());
        let mut right: Element = r#box().flex_grow(right_g).h("100%").into();
        right
            .attrs
            .insert(ATTR_SLOT.to_string(), SLOT_RIGHT.to_string());

        let row: Element = hstack()
            .w("100%")
            .h("100%")
            .items_center()
            .child(left)
            .child(thumb)
            .child(right)
            .into();

        // Track 在下层且不抢命中：指针落在拇指行 / 根上，再 bubble 到根 handler。
        let mut track: Element = r#box()
            .absolute(true)
            .left(0.0)
            .right(0.0)
            .top(TRACK_TOP)
            .h(TRACK_H)
            .rounded("9999px")
            .overflow("hidden")
            .bg(if textured.is_some() {
                // Ramp textures punch transparent corners; keep an opaque bed.
                crate::tokens::popover()
            } else {
                track_bg()
            })
            .pointer_events("none")
            .into();
        track
            .attrs
            .insert(ATTR_SLOT.to_string(), SLOT_TRACK.to_string());
        if let Some(key) = textured {
            track.image_texture = Some(key);
        }
        if let Some(fill) = fill {
            track = track.child(fill);
        }

        let mut root: Element = r#box()
            .relative(true)
            .w("100%")
            .max_w("100%")
            .min_w(0.0)
            .h(THUMB_PX)
            .child(track)
            .child(row)
            .children(builder.children)
            .into();

        let val_k = val;
        let bounds_pd = bounds.clone();
        let focus_id = id.clone();
        root = root.on_pointer_down(move |e: &PointerEvent| {
            if disabled {
                return;
            }
            // 命中叶子（拇指 / spacer）时根只收到 Capture→Bubble；勿只认 Target。
            if e.phase == EventPhase::Capture {
                return;
            }
            use_signal("__rui_button_focus_id", String::new).set(focus_id.clone());
            use_signal("__rui_focus_visible", || false).set(false);
            let b = bounds_pd.get();
            let v = value_from_pointer_x(e.x, b, min, max);
            val_k.set(v);
            begin_slider_drag(SliderDrag {
                value: val_k,
                min,
                max,
                bounds: bounds_pd.clone(),
            });
            mark_hover_paint_dirty();
            crate::app::set_pointer_capture(focus_id.clone());
            e.stop_propagation();
        });
        root = root.on_pointer_up(move |_e: &PointerEvent| {
            clear_slider_drag_if_any();
            mark_hover_paint_dirty();
        });

        let val_key = val;
        let hover_for_cb = hovered.clone();
        let mut el: Element = interactive(root)
            .focus_id(id.clone())
            .focusable(!disabled)
            .focus_ring(!disabled)
            .on_hover(move |h| {
                if disabled {
                    return;
                }
                if hover_for_cb.get() != h {
                    hover_for_cb.set(h);
                    mark_hover_paint_dirty();
                }
            })
            .on_key_down(move |ev: KeyEvent| {
                if disabled {
                    return false;
                }
                let cur = val_key.peek();
                match ev.logical_key {
                    Key::Named(NamedKey::ArrowLeft) | Key::Named(NamedKey::ArrowDown) => {
                        val_key.set(quantize(cur - step, min, max, step));
                        true
                    }
                    Key::Named(NamedKey::ArrowRight) | Key::Named(NamedKey::ArrowUp) => {
                        val_key.set(quantize(cur + step, min, max, step));
                        true
                    }
                    Key::Named(NamedKey::Home) => {
                        val_key.set(min);
                        true
                    }
                    Key::Named(NamedKey::End) => {
                        val_key.set(max);
                        true
                    }
                    _ => false,
                }
            })
            .into();

        if disabled {
            el = el.opacity(0.5).cursor_not_allowed();
        } else {
            el = el.cursor_kind(crate::core::tree::CursorKind::EwResize);
        }

        let sync_value = val;
        let sync_bounds = bounds;
        let sync_min = min;
        let sync_max = max;
        let sync_hover = hovered;
        let sync_id = id.clone();
        el = el.reactive_sync(move |root, layout_tree| {
            if let Some(f) = &on_sync {
                f();
            }
            sync_bounds.set(root.layout_bounds);
            let value = sync_value.peek().clamp(sync_min, sync_max);
            let t = norm01(value, sync_min, sync_max);
            let want_left = t.max(0.0001);
            let want_right = (1.0 - t).max(0.0001);
            let want_frac = t.clamp(0.0, 1.0);
            let mut layout_changed = false;
            let mut paint_changed = false;

            if let Some(tf) = &track_texture_fn {
                let key = tf();
                if let Some(track) = find_slot_deep_mut(root, SLOT_TRACK) {
                    if track.image_texture.as_deref() != Some(key.as_str()) {
                        track.image_texture = Some(key);
                        paint_changed = true;
                    }
                }
            }

            if let Some(fill) = find_slot_deep_mut(root, SLOT_FILL) {
                let mismatch = match crate::layout::dim_kind(fill.style.size.width) {
                    crate::layout::DimKind::Percent(p) => (p - want_frac).abs() > 0.0005,
                    _ => true,
                };
                if mismatch {
                    fill.style.size.width = Dimension::percent(want_frac);
                    if fill.layout_node.is_some() {
                        layout_tree.update_taffy_node_style(fill);
                    }
                    layout_changed = true;
                }
            }

            if let Some(left) = find_slot_deep_mut(root, SLOT_LEFT) {
                if (left.style.flex_grow - want_left).abs() > f32::EPSILON {
                    left.style.flex_grow = want_left;
                    if left.layout_node.is_some() {
                        layout_tree.update_taffy_node_style(left);
                    }
                    layout_changed = true;
                }
            }
            if let Some(right) = find_slot_deep_mut(root, SLOT_RIGHT) {
                if (right.style.flex_grow - want_right).abs() > f32::EPSILON {
                    right.style.flex_grow = want_right;
                    if right.layout_node.is_some() {
                        layout_tree.update_taffy_node_style(right);
                    }
                    layout_changed = true;
                }
            }

            if let Some(thumb) = find_slot_deep_mut(root, SLOT_THUMB) {
                if let Some(fill) = &thumb_fill {
                    let c = fill();
                    if thumb.style.bg_color != c {
                        thumb.style.bg_color = c;
                        paint_changed = true;
                    }
                }
                let want = if thumb_shadow_elevated(sync_hover.get(), is_slider_dragging()) {
                    1.0
                } else {
                    0.0
                };
                let tween = use_ui_transition_cubic(&slider_shadow_anim_key(&sync_id), want);
                tween.set(want);
                let shadow_t = tween.peek().clamp(0.0, 1.0);
                if apply_thumb_shadow_t(thumb, shadow_t) {
                    paint_changed = true;
                }
            }

            if layout_changed || paint_changed {
                push_slider_thumb_paint_damage(root);
            }
            if layout_changed {
                LayoutStatus::LayoutChanged
            } else if paint_changed {
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        });

        el = el.accesskit_role("slider");
        el.template_id = Some(id);
        el
    }
}

impl crate::core::tree::IntoElementList for SliderBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::pointer_events::{dispatch_pointer, hit_path, PointerKind};
    use crate::app::test_support::sync_reactive_elements_subtree;
    use crate::core::tree::LayoutTree;

    #[test]
    fn track_texture_omits_primary_fill() {
        let el: Element = slider("tex").track_texture("rui_cp_hue_ramp").into();
        assert!(!has_slot(&el, SLOT_FILL));
        assert!(has_slot(&el, SLOT_THUMB));
        let track = &el.children[0];
        assert_eq!(track.image_texture.as_deref(), Some("rui_cp_hue_ramp"));
        assert!(
            track.style.bg_color[3] > 0.5,
            "textured track must keep an opaque bed, a={}",
            track.style.bg_color[3]
        );
    }

    #[test]
    fn pointer_x_maps_to_value_ends() {
        let b = [10.0, 0.0, 100.0 + THUMB_PX, 20.0];
        let lo = value_from_pointer_x(10.0 + THUMB_PX * 0.5, b, 0.0, 1.0);
        let hi = value_from_pointer_x(10.0 + THUMB_PX * 0.5 + 100.0, b, 0.0, 1.0);
        assert!(lo <= 0.02, "lo={lo}");
        assert!(hi >= 0.98, "hi={hi}");
    }

    #[test]
    fn pointer_drag_is_continuous_not_stepped() {
        let b = [0.0, 0.0, 100.0 + THUMB_PX, 20.0];
        // 中点附近：若按 0.25 步进会落到 0.25/0.5；连续应接近真实比例。
        let mid = value_from_pointer_x(THUMB_PX * 0.5 + 37.0, b, 0.0, 1.0);
        assert!((mid - 0.37).abs() < 0.02, "mid={mid}");
        assert!(
            (mid - 0.25).abs() > 0.05 && (mid - 0.5).abs() > 0.05,
            "must not snap to 0.25 step grid, got {mid}"
        );
    }

    #[test]
    fn pointer_down_on_thumb_row_sets_value_via_bubble() {
        clear_slider_drag_if_any();
        let value = use_signal("__rui_test_slider_ptr", || 0.2_f32);
        value.set_silent(0.2);
        let mut root: Element = slider("test-slider-ptr").value_signal(value).into();
        // 模拟已布局：根宽 120（含拇指），高 20
        root.layout_bounds = [0.0, 0.0, 120.0, 20.0];
        for c in &mut root.children {
            c.layout_bounds = root.layout_bounds;
            for g in &mut c.children {
                g.layout_bounds = [0.0, 0.0, 40.0, 20.0];
            }
        }
        let mut layout_tree = LayoutTree::new();
        let mut impact = LayoutStatus::Clean;
        sync_reactive_elements_subtree(&mut root, &mut layout_tree, &mut impact);

        // 点在右半：应抬高 value（叶子命中后 bubble 到根）
        let x = 100.0;
        let y = 10.0;
        let path = hit_path(&root, x, y);
        assert!(!path.is_empty(), "expected hit path");
        assert!(
            path.iter().any(|e| e.on_mousedown.is_some()),
            "root handler must be on path"
        );
        let mut pev = PointerEvent::new(x, y, winit::event::MouseButton::Left, 1);
        dispatch_pointer(&path, &mut pev, PointerKind::Down);
        assert!(
            value.peek() > 0.5,
            "click should raise value, got {}",
            value.peek()
        );
        assert!(tick_slider_drag(110.0), "drag should be active");
        clear_slider_drag_if_any();
    }
}
