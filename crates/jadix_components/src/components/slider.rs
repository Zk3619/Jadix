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
//! ## Usage
//! ```ignore
//! <Slider id="volume" value={level} default_value={0.4} />
//! <Slider id="vol2" defaultValue={0.5} step={0.01} />
//! { Element::from(slider("volume").value_signal(level).step(0.05)) }
//! ```
//!
//! ## Notes
//! - 默认值域 `0..=1`；可用 `min` / `max` / `step` 扩展。
//! - 点击 / 拖拽按指针 x **连续**设值（不按 `step` 吸附）；键盘 ←/→（或 ↑/↓）按 `step`（默认 0.05）调节。
//! - 悬停 / 拖拽时拇指阴影加深（瞬时切换，避免 cubic tween 连帧 StyleChanged
//!   在 inline ColorPicker 双轨上卡顿），静止为紧凑接触阴影。
//! - 根四边留 `THUMB_SHADOW_PAD`（交叉轴加高/加宽），轨道内缩到 content box，避免光晕被裁切或被轨道盖住。
//! - 值变化经 [`ReactiveSync`](crate::core::tree::ReactiveSync) 原位同步，不订阅根 UI effect。

use crate::app::pointer_events::{EventPhase, PointerEvent};
use crate::components::base::{SlotChrome, SlotMap};
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::{parse_color, Element, LayoutStatus};
use crate::primitives::interactive::{interactive, mark_hover_paint_dirty};
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::style::{is_variant_style_overridden, VariantStyleField};
use crate::tokens;
use crate::utils::controlled::resolve_f32_signal;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use taffy::prelude::Dimension;
use winit::event::KeyEvent;
use winit::keyboard::{Key, NamedKey};

const ATTR_SLOT: &str = "data-slot";
const ATTR_ORIENTATION: &str = "data-orientation";
const SLOT_FILL: &str = "slider-fill";
const SLOT_TRACK: &str = "slider-track";
const SLOT_THUMB: &str = "slider-thumb";
const SLOT_BEFORE: &str = "slider-before";
const SLOT_AFTER: &str = "slider-after";

/// solid-ui Track `h-2`；Thumb `size-5`。
const TRACK_CROSS: f32 = 8.0;
const THUMB_PX: f32 = 20.0;
const TRACK_INSET: f32 = (THUMB_PX - TRACK_CROSS) * 0.5;
const BORDER_W: f32 = 2.0;
const THUMB_SHADOW_REST: ([f32; 4], f32) = (tokens::shadow_ink(0.18), 2.5);
const THUMB_SHADOW_HOVER: ([f32; 4], f32) = (tokens::shadow_ink(0.42), 5.0);
/// Omnidirectional pad so elevated thumb halo stays inside the root on **all**
/// sides (horizontal was clipping T/B when only `px` was applied). Track is
/// inset to the content box so its fill cannot paint over the end/cross halo.
const THUMB_SHADOW_PAD: f32 = THUMB_SHADOW_HOVER.1 * 2.0;
/// Cross-axis root size: thumb + halo pad on both sides (border-box).
const ROOT_CROSS: f32 = THUMB_PX + 2.0 * THUMB_SHADOW_PAD;
/// 纵向滑块默认高度（横向仍 `w-full` + 拇指高）。
const SLIDER_VERTICAL_DEFAULT_H: &str = "8rem";

static NEXT_SLIDER_ID: AtomicUsize = AtomicUsize::new(1);

/// Slider 主轴（对位 Tabs / Progress `orientation`）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SliderOrientation {
    #[default]
    Horizontal,
    Vertical,
}

impl SliderOrientation {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "vertical" | "col" | "column" => Self::Vertical,
            _ => Self::Horizontal,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
        }
    }

    pub fn is_vertical(self) -> bool {
        matches!(self, Self::Vertical)
    }
}

impl From<&str> for SliderOrientation {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for SliderOrientation {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

#[derive(Clone)]
struct SliderDrag {
    value: Signal<f32>,
    min: f32,
    max: f32,
    bounds: Rc<Cell<[f32; 4]>>,
    vertical: bool,
}

crate::authoring::window::window_local! {
    static SLIDER_DRAG: RefCell<Option<SliderDrag>> = const { RefCell::new(None) };
}

fn begin_slider_drag(id: impl Into<String>, drag: SliderDrag) {
    crate::utils::drag::pointer::begin(crate::utils::drag::pointer::DragKind::Slider, id, 0.0, 0.0);
    SLIDER_DRAG.with(|d| *d.borrow_mut() = Some(drag));
    crate::utils::drag::routing::register(
        module_path!(),
        crate::utils::drag::routing::DragHandlers::new(tick_slider_drag, clear_slider_drag_if_any)
            .scroll_policy(crate::utils::drag::routing::ScrollPolicy::Freeze),
    );
}

fn end_slider_drag() {
    crate::utils::drag::routing::unregister(module_path!());
    crate::utils::drag::pointer::clear();
    SLIDER_DRAG.with(|d| *d.borrow_mut() = None);
    crate::app::release_pointer_capture();
}

pub(crate) fn is_slider_dragging() -> bool {
    SLIDER_DRAG.with(|d| d.borrow().is_some())
}

/// CursorMoved：若正在拖拽滑块，按主轴坐标更新值。
///
/// 调用方须在**已释放** `AppState` 的 `RefCell` 借用后再调用：`Signal::set` 会同步
/// flush `bind_text` 等 effect，后者会 `try_update_element_by_template_id` → 再借同一 RefCell。
pub(crate) fn tick_slider_drag(x: f32, y: f32) -> bool {
    SLIDER_DRAG.with(|d| {
        let Some(drag) = d.borrow().clone() else {
            return false;
        };
        let v = if drag.vertical {
            value_from_pointer_y(y, drag.bounds.get(), drag.min, drag.max)
        } else {
            value_from_pointer_x(x, drag.bounds.get(), drag.min, drag.max)
        };
        if (drag.value.peek() - v).abs() > 1e-5 {
            drag.value.set(v);
        }
        true
    })
}

pub(crate) fn clear_slider_drag_if_any() {
    if is_slider_dragging() {
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
/// `bounds` 须为视口坐标（[`PointerEvent::current_bounds`] / hit-path visual box）。
/// Main-axis `THUMB_SHADOW_PAD` keeps the thumb halo inside the root; travel is
/// the content box between those pads.
fn value_from_pointer_x(x: f32, bounds: [f32; 4], min: f32, max: f32) -> f32 {
    let pad = THUMB_SHADOW_PAD + THUMB_PX * 0.5;
    let inner = (bounds[2] - 2.0 * THUMB_SHADOW_PAD - THUMB_PX).max(1.0);
    let t = ((x - bounds[0] - pad) / inner).clamp(0.0, 1.0);
    (min + t * (max - min)).clamp(min, max)
}

/// 纵向：自下而上增大（屏幕 y 向下）。
fn value_from_pointer_y(y: f32, bounds: [f32; 4], min: f32, max: f32) -> f32 {
    let pad = THUMB_SHADOW_PAD + THUMB_PX * 0.5;
    let inner = (bounds[3] - 2.0 * THUMB_SHADOW_PAD - THUMB_PX).max(1.0);
    let t = (1.0 - ((y - bounds[1] - pad) / inner)).clamp(0.0, 1.0);
    (min + t * (max - min)).clamp(min, max)
}

fn thumb_shadow_elevated(hovered: bool, dragging: bool) -> bool {
    hovered || dragging
}

/// Snap rest↔hover thumb shadow. A cubic tween kept `StyleChanged` + collect-hash
/// invalidation hot for ~150ms; switching between two full-width ColorPicker
/// tracks (inline hue/alpha) then hitched the forms page every frame.
fn apply_thumb_shadow(thumb: &mut Element, elevated: bool) -> bool {
    let (color, blur) = if elevated {
        THUMB_SHADOW_HOVER
    } else {
        THUMB_SHADOW_REST
    };
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

/// Paint dirty for a thumb chrome change.
///
/// **Page (inline ColorPicker):** key must be the thumb's Taffy id, not the slider
/// root. `node_present_dest_reset` matches `NodePaint` by key and expands the wipe
/// to that slot's full AABB — the root is `w:100%`, so root-keyed damage dest-resets
/// the whole hue/alpha strip and PartialLoad encode balloons (gallery logs: encode
/// 90–180ms while collect stays ~5ms). Floating ColorPicker is Overlay-keyed and
/// never hits that page path.
fn push_slider_thumb_paint_damage(root: &Element) {
    let scroll = root.paint_scroll_offset();
    let thumb = find_slot_deep(root, SLOT_THUMB);
    let thumb_box = thumb
        .map(|t| crate::core::tree::visual_paint_aabb(t, scroll))
        .filter(|b| b[2] > 0.0 && b[3] > 0.0);
    let b = thumb_box.unwrap_or_else(|| crate::core::tree::visual_paint_aabb(root, scroll));
    if b[2] <= 0.0 || b[3] <= 0.0 {
        return;
    }
    // 拇指阴影余量：与 SDF `ext` 对齐，避免 PartialLoad scissor 裁掉 halo
    // 后留下轨道像素盖住阴影。
    let pad = thumb
        .map(|t| {
            let border = t
                .style
                .resolved_border_widths()
                .into_iter()
                .fold(0.0, f32::max);
            crate::authoring::widgets::shadows::sdf_outset_shadow_pad(
                t.style.shadow_blur,
                t.style.shadow_spread,
                t.style.shadow_offset,
                border,
            )
        })
        .unwrap_or(THUMB_SHADOW_PAD)
        .max(THUMB_SHADOW_PAD);
    let bounds = [b[0] - pad, b[1] - pad, b[2] + pad * 2.0, b[3] + pad * 2.0];
    if crate::app::damage::paint_kind_for_element(root)
        == crate::app::damage::PaintDirtyKind::Overlay
    {
        let key = root
            .overlay_stamp()
            .filter(|key| *key != 0)
            .unwrap_or_else(|| crate::layout::layout_tree::overlay_portal_stamp_key(root));
        crate::app::damage::push_frame_overlay_damage(key, bounds);
    } else {
        // Prefer the thumb slot key so dest-reset cannot expand to the full track.
        let key_el = if thumb_box.is_some() {
            thumb.unwrap_or(root)
        } else {
            root
        };
        crate::app::damage::push_frame_paint_key(
            crate::app::damage::paint_element_key(key_el),
            bounds,
        );
    }
}

fn find_slot_deep<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
    crate::core::tree::find_by_attr(el, ATTR_SLOT, slot)
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

fn find_slot_deep_mut<'a>(el: &'a mut Element, slot: &str) -> Option<&'a mut Element> {
    crate::core::tree::find_by_attr_mut(el, ATTR_SLOT, slot)
}

pub struct SliderBuilder {
    id: String,
    value: Option<Signal<f32>>,
    default_value: f32,
    min: f32,
    max: f32,
    step: f32,
    orientation: SliderOrientation,
    disabled: bool,
    /// When set, the track shows this `image_texture` and omits the primary fill.
    track_texture: Option<String>,
    track_texture_fn: Option<Rc<dyn Fn() -> String>>,
    /// Live thumb fill (color-picker preview). Called from `reactive_sync`.
    thumb_fill: Option<Rc<dyn Fn() -> [f32; 4]>>,
    /// Extra work during the existing slider paint sync (no extra tree subscribe).
    on_sync: Option<Rc<dyn Fn()>>,
    children: Vec<Element>,
    chrome: SlotChrome,
    slots: SlotMap,
}

crate::__component_contract! {
SliderProps [];
/// RSX `<Slider>`。数值类 props 用 `Option`，省略时走 builder 默认（`default≈0.35`、`max=1`、`step=0.05`）。
#[allow(non_snake_case)]
pub fn Slider(
identity [] id: String,
plain [] value: Option<Signal<f32>>,
plain [] default_value: Option<f32>,
plain [] min: Option<f32>,
plain [] max: Option<f32>,
plain [] step: Option<f32>,
plain [] orientation: Option<String>,
default [] disabled: bool,
default [] on_click: crate::core::tree::Handler,
plain [] children: Option<Element>,
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
    if let Some(o) = orientation {
        builder = builder.orientation(o);
    }
    if let Some(sig) = value {
        builder = builder.value_signal(sig);
    }
    if let Some(c) = children {
        builder = builder.child(c);
    }

    let mut el: Element = builder.into();
    if !disabled {
        let notify = Rc::new(RefCell::new(on_click));
        let mut old_on_click = el.on_click.clone();
        let click_n = notify.clone();
        el = el.on_click(move || {
            if let Some(ref mut rc) = old_on_click {
                rc.borrow_mut()();
            }
            (click_n.borrow_mut().0)(());
        });
        // Slider drag uses `on_pointer_down` (= `on_mousedown`) and never emits
        // `click`. Chain the RSX `on_click` here so scrubbers can enter pause.
        let old_down = el.on_mousedown.clone();
        let down_n = notify;
        el = el.on_mousedown(move |e: &PointerEvent| {
            if let Some(ref rc) = old_down {
                rc.borrow_mut()(e);
            }
            (down_n.borrow_mut().0)(());
        });
    }
    el
}
}

pub fn slider(id: impl Into<String>) -> SliderBuilder {
    let id = id.into();
    let id = if id.trim().is_empty() {
        let n = NEXT_SLIDER_ID.fetch_add(1, Ordering::Relaxed);
        format!("__jadix_slider_rt_{n}")
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
        orientation: SliderOrientation::Horizontal,
        disabled: false,
        track_texture: None,
        track_texture_fn: None,
        thumb_fill: None,
        on_sync: None,
        children: Vec::new(),
        chrome: SlotChrome::default(),
        slots: SlotMap::default(),
    }
}

impl SliderBuilder {
    crate::impl_slot_chrome_methods!();
    crate::impl_slot_map_methods!();

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

    pub fn orientation(mut self, o: impl Into<SliderOrientation>) -> Self {
        self.orientation = o.into();
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
        let chrome = builder.chrome.clone();
        let slots = builder.slots.clone();
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
        let val = resolve_f32_signal(builder.value, &id, "slider", default_value);
        let disabled = builder.disabled;
        let vertical = builder.orientation.is_vertical();

        let v0 = val.peek().clamp(min, max);
        let t0 = norm01(v0, min, max);
        let before_g = if vertical {
            (1.0 - t0).max(0.0001)
        } else {
            t0.max(0.0001)
        };
        let after_g = if vertical {
            t0.max(0.0001)
        } else {
            (1.0 - t0).max(0.0001)
        };

        let textured = builder
            .track_texture
            .clone()
            .or_else(|| builder.track_texture_fn.as_ref().map(|f| f()));
        let track_texture_fn = builder.track_texture_fn.clone();

        let fill = if textured.is_none() {
            let fill: Element = if vertical {
                r#box()
                    .w("100%")
                    .h(format!("{:.2}%", (t0 * 100.0).clamp(0.0, 100.0)))
                    .rounded_full()
                    .bg(fill_bg())
                    .into()
            } else {
                r#box()
                    .h("100%")
                    .w(format!("{:.2}%", (t0 * 100.0).clamp(0.0, 100.0)))
                    .rounded_full()
                    .bg(fill_bg())
                    .into()
            };
            Some(slots.paint(SLOT_FILL, fill))
        } else {
            None
        };

        let (rest_c, rest_blur) = THUMB_SHADOW_REST;
        let thumb_fill = builder.thumb_fill.clone();
        let on_sync = builder.on_sync.clone();
        let thumb0 = thumb_fill.as_ref().map(|f| f()).unwrap_or_else(thumb_bg);
        let thumb: Element = r#box()
            .w(THUMB_PX)
            .h(THUMB_PX)
            .min_w(THUMB_PX)
            .min_h(THUMB_PX)
            .rounded_full()
            .bg(thumb0)
            .border(thumb_border(), BORDER_W)
            .shadow(rest_c, rest_blur)
            .flex_shrink(0.0)
            .into();
        let thumb = slots.paint(SLOT_THUMB, thumb);

        let hovered = Rc::new(Cell::new(false));

        let mut before: Element = if vertical {
            r#box().flex_grow(before_g).w("100%").into()
        } else {
            r#box().flex_grow(before_g).h("100%").into()
        };
        before
            .attrs
            .insert(ATTR_SLOT.to_string(), SLOT_BEFORE.to_string());
        let mut after: Element = if vertical {
            r#box().flex_grow(after_g).w("100%").into()
        } else {
            r#box().flex_grow(after_g).h("100%").into()
        };
        after
            .attrs
            .insert(ATTR_SLOT.to_string(), SLOT_AFTER.to_string());

        // Tree order (track then thumb_axis) keeps the halo above the ramp.
        // Do **not** assign z_index here: any non-zero z opens page_stack_bands and
        // PartialLoad dest-reset punches white holes over the track (ColorPicker).
        let thumb_axis: Element = if vertical {
            vstack()
                .w("100%")
                .h("100%")
                .items_center()
                .child(before)
                .child(thumb)
                .child(after)
                .into()
        } else {
            hstack()
                .w("100%")
                .h("100%")
                .items_center()
                .child(before)
                .child(thumb)
                .child(after)
                .into()
        };

        let mut track: Element = if vertical {
            let mut track_shell: Element = vstack()
                .w("100%")
                .h("100%")
                .justify("end")
                .rounded_full()
                .overflow("hidden")
                .pointer_events("none")
                .into();
            if let Some(fill) = fill {
                track_shell = track_shell.child(fill);
            }
            // Absolute insets are from the padding edge; offset by shadow pad so
            // the track sits in the content box (centered on the thumb).
            r#box()
                .absolute(true)
                .top(THUMB_SHADOW_PAD)
                .bottom(THUMB_SHADOW_PAD)
                .left(THUMB_SHADOW_PAD + TRACK_INSET)
                .w(TRACK_CROSS)
                .rounded_full()
                .overflow("hidden")
                .bg(if textured.is_some() {
                    crate::tokens::popover()
                } else {
                    track_bg()
                })
                .pointer_events("none")
                .child(track_shell)
                .into()
        } else {
            let mut track: Element = r#box()
                .absolute(true)
                .left(THUMB_SHADOW_PAD)
                .right(THUMB_SHADOW_PAD)
                .top(THUMB_SHADOW_PAD + TRACK_INSET)
                .h(TRACK_CROSS)
                .rounded_full()
                .overflow("hidden")
                .bg(if textured.is_some() {
                    crate::tokens::popover()
                } else {
                    track_bg()
                })
                .pointer_events("none")
                .into();
            if let Some(fill) = fill {
                track = track.child(fill);
            }
            track
        };
        if let Some(key) = textured {
            track.image_texture = Some(key);
        }
        let track = slots.paint(SLOT_TRACK, track);

        let mut root: Element = if vertical {
            r#box()
                .relative(true)
                .w(ROOT_CROSS)
                .h(SLIDER_VERTICAL_DEFAULT_H)
                .p(THUMB_SHADOW_PAD)
                .child(track)
                .child(thumb_axis)
                .children(builder.children)
                .into()
        } else {
            r#box()
                .relative(true)
                .w("100%")
                .max_w("100%")
                .min_w(0.0)
                .h(ROOT_CROSS)
                .p(THUMB_SHADOW_PAD)
                .child(track)
                .child(thumb_axis)
                .children(builder.children)
                .into()
        };
        root.set_attr(ATTR_ORIENTATION, builder.orientation.as_str());

        let val_k = val;
        let focus_id = id.clone();
        root = root.on_pointer_down(move |e: &PointerEvent| {
            if disabled {
                return;
            }
            if e.phase == EventPhase::Capture {
                return;
            }
            use_signal("__jadix_button_focus_id", String::new).set(focus_id.clone());
            use_signal("__jadix_focus_visible", || false).set(false);
            // Hit-path visual bounds (viewport space, scroll-adjusted) — not `layout_bounds`.
            let b = e.current_bounds;
            let v = if vertical {
                value_from_pointer_y(e.y, b, min, max)
            } else {
                value_from_pointer_x(e.x, b, min, max)
            };
            val_k.set(v);
            begin_slider_drag(
                focus_id.clone(),
                SliderDrag {
                    value: val_k,
                    min,
                    max,
                    bounds: Rc::new(Cell::new(b)),
                    vertical,
                },
            );
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
                let dec = match ev.logical_key {
                    Key::Named(NamedKey::ArrowLeft) | Key::Named(NamedKey::ArrowDown) => true,
                    _ => false,
                };
                let inc = match ev.logical_key {
                    Key::Named(NamedKey::ArrowRight) | Key::Named(NamedKey::ArrowUp) => true,
                    _ => false,
                };
                if dec {
                    val_key.set(quantize(cur - step, min, max, step));
                    true
                } else if inc {
                    val_key.set(quantize(cur + step, min, max, step));
                    true
                } else {
                    match ev.logical_key {
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
                }
            })
            .into();

        if disabled {
            el = el.opacity(0.5).cursor_not_allowed();
        } else if vertical {
            el = el.cursor_kind(crate::core::tree::CursorKind::NsResize);
        } else {
            el = el.cursor_kind(crate::core::tree::CursorKind::EwResize);
        }

        let sync_value = val;
        let sync_min = min;
        let sync_max = max;
        let sync_hover = hovered;
        let sync_id = id.clone();
        el = el.reactive_sync(move |root, layout_tree| {
            if let Some(f) = &on_sync {
                f();
            }
            let _ = root;
            let Some(value) = sync_value.try_peek() else {
                return LayoutStatus::Clean;
            };
            let value = value.clamp(sync_min, sync_max);
            let t = norm01(value, sync_min, sync_max);
            let want_before = if vertical {
                (1.0 - t).max(0.0001)
            } else {
                t.max(0.0001)
            };
            let want_after = if vertical {
                t.max(0.0001)
            } else {
                (1.0 - t).max(0.0001)
            };
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
                let mismatch = if vertical {
                    match crate::layout::dim_kind(fill.style.size.height) {
                        crate::layout::DimKind::Percent(p) => (p - want_frac).abs() > 0.0005,
                        _ => true,
                    }
                } else {
                    match crate::layout::dim_kind(fill.style.size.width) {
                        crate::layout::DimKind::Percent(p) => (p - want_frac).abs() > 0.0005,
                        _ => true,
                    }
                };
                if mismatch {
                    if vertical {
                        fill.style.size.height = Dimension::percent(want_frac);
                    } else {
                        fill.style.size.width = Dimension::percent(want_frac);
                    }
                    if fill.layout_node_id().is_some() {
                        layout_tree.update_taffy_node_style(fill);
                    }
                    layout_changed = true;
                }
            }

            if let Some(before) = find_slot_deep_mut(root, SLOT_BEFORE) {
                if (before.style.flex_grow - want_before).abs() > f32::EPSILON {
                    before.style.flex_grow = want_before;
                    if before.layout_node_id().is_some() {
                        layout_tree.update_taffy_node_style(before);
                    }
                    layout_changed = true;
                }
            }
            if let Some(after) = find_slot_deep_mut(root, SLOT_AFTER) {
                if (after.style.flex_grow - want_after).abs() > f32::EPSILON {
                    after.style.flex_grow = want_after;
                    if after.layout_node_id().is_some() {
                        layout_tree.update_taffy_node_style(after);
                    }
                    layout_changed = true;
                }
            }

            if let Some(thumb) = find_slot_deep_mut(root, SLOT_THUMB) {
                if let Some(fill) = &thumb_fill {
                    if !is_variant_style_overridden(&thumb.attrs, VariantStyleField::Bg) {
                        let c = fill();
                        if thumb.style.bg_color != c {
                            thumb.style.bg_color = c;
                            paint_changed = true;
                        }
                    }
                }
                let dragging = crate::utils::drag::pointer::peek().is_some_and(|session| {
                    session.kind == crate::utils::drag::pointer::DragKind::Slider
                        && session.id == sync_id
                });
                let elevated = !disabled && thumb_shadow_elevated(sync_hover.get(), dragging);
                if apply_thumb_shadow(thumb, elevated) {
                    paint_changed = true;
                }
            }

            if layout_changed || paint_changed {
                push_slider_thumb_paint_damage(root);
                if paint_changed && !layout_changed {
                    // Skip sync_reactive's full-root dest-reset (`push_frame_paint_element_change`).
                    // Under Overlay that shares the portal stamp, a full-width slider strip
                    // unions with its neighbor into the whole ColorPicker every paint frame.
                    root.set_attr("jadix_halo_dirty", "1");
                }
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
        chrome.apply(el)
    }
}

impl crate::core::tree::IntoElementList for SliderBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/slider.rs"]
mod tests;
