//! ColorPicker 纹理与面板 UI（产品层）。

use super::drag::*;
use super::model::*;
use super::*;

pub(super) fn sv_texture_key(id: &str, hue: f32) -> String {
    format!("rui_cp_sv:{id}:{:.0}", hue.rem_euclid(360.0))
}

pub(super) fn hue_texture_key() -> &'static str {
    "rui_cp_hue_ramp"
}

struct TexCpuEntry {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    /// Content fingerprint; when it changes, pixels are rebuilt and re-uploaded
    /// under the same key (avoids per-RGB key thrash on the alpha track).
    tag: u64,
}

thread_local! {
    /// CPU backup for ColorPicker ramps — TexturePool eviction can rehydrate via
    /// [`flush_color_picker_textures`] (same pattern as gradients / path rasters).
    pub(super) static TEX_CPU: RefCell<std::collections::HashMap<String, TexCpuEntry>> =
        RefCell::new(std::collections::HashMap::new());
    static TEX_NEEDED: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

/// Test/diagnostic: how many SV keys currently sit in the CPU cache.
#[cfg(test)]
pub(super) fn tex_sv_cpu_count() -> usize {
    TEX_CPU.with(|c| {
        c.borrow()
            .keys()
            .filter(|k| k.starts_with("rui_cp_sv:"))
            .count()
    })
}

/// Mid-column luma at first vs last row of an SV ramp (for orientation tests).
#[cfg(test)]
pub(super) fn sv_texture_mid_column_luma(id: &str, hue: f32) -> (i32, i32) {
    let key = ensure_sv_texture(id, hue);
    TEX_CPU.with(|c| {
        let map = c.borrow();
        let entry = map.get(&key).expect("sv cpu");
        let w = entry.width as usize;
        let h = entry.height as usize;
        let mid = w / 2;
        let top = mid * 4;
        let bot = ((h - 1) * w + mid) * 4;
        let top_luma =
            entry.rgba[top] as i32 + entry.rgba[top + 1] as i32 + entry.rgba[top + 2] as i32;
        let bot_luma =
            entry.rgba[bot] as i32 + entry.rgba[bot + 1] as i32 + entry.rgba[bot + 2] as i32;
        (top_luma, bot_luma)
    })
}

/// Rasterize once (immutable key → pixels), mark needed for this frame.
pub(super) fn tex_cached(key: String, build: impl FnOnce() -> (u32, u32, Vec<u8>)) -> String {
    TEX_CPU.with(|c| {
        let mut map = c.borrow_mut();
        if !map.contains_key(&key) {
            let (w, h, rgba) = build();
            map.insert(
                key.clone(),
                TexCpuEntry {
                    width: w,
                    height: h,
                    rgba,
                    tag: 0,
                },
            );
        }
    });
    TEX_NEEDED.with(|n| {
        n.borrow_mut().insert(key.clone());
    });
    key
}

/// Stable key with replaceable pixels: rebuild + queue when `tag` changes.
pub(super) fn tex_cached_tagged(
    key: String,
    tag: u64,
    build: impl FnOnce() -> (u32, u32, Vec<u8>),
) -> String {
    let mut queued = false;
    TEX_CPU.with(|c| {
        let mut map = c.borrow_mut();
        let rebuild = map.get(&key).map(|e| e.tag != tag).unwrap_or(true);
        if rebuild {
            let (w, h, rgba) = build();
            crate::queue_image_rgba8(key.clone(), w, h, rgba.clone());
            map.insert(
                key.clone(),
                TexCpuEntry {
                    width: w,
                    height: h,
                    rgba,
                    tag,
                },
            );
            queued = true;
        }
    });
    let _ = queued;
    TEX_NEEDED.with(|n| {
        n.borrow_mut().insert(key.clone());
    });
    key
}

/// Re-queue ColorPicker ramps missing from the GPU pool after eviction.
pub(crate) fn flush_color_picker_textures(mut has_gpu: impl FnMut(&str) -> bool) -> bool {
    let needed: Vec<String> = TEX_NEEDED
        .with(|n| std::mem::take(&mut *n.borrow_mut()))
        .into_iter()
        .collect();
    let mut any = false;
    TEX_CPU.with(|c| {
        let map = c.borrow();
        for key in needed {
            if has_gpu(&key) {
                continue;
            }
            if let Some(entry) = map.get(&key) {
                crate::app::pending_images::queue_image_rgba8(
                    &key,
                    entry.width,
                    entry.height,
                    entry.rgba.clone(),
                );
                any = true;
            }
        }
    });
    any
}

pub(super) fn apply_rounded_rect_alpha(rgba: &mut [u8], w: u32, h: u32, radius: f32) {
    let wf = w as f32;
    let hf = h as f32;
    let r = radius.min(wf * 0.5).min(hf * 0.5).max(0.0);
    if r < 0.5 {
        return;
    }
    for y in 0..h {
        for x in 0..w {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let qx = (px - wf * 0.5).abs() - wf * 0.5 + r;
            let qy = (py - hf * 0.5).abs() - hf * 0.5 + r;
            let outside = qx.max(0.0).hypot(qy.max(0.0));
            let inside = qx.max(qy).min(0.0);
            let sdf = outside + inside - r;
            let a = (0.75 - sdf).clamp(0.0, 1.0);
            let i = ((y * w + x) * 4) as usize;
            rgba[i + 3] = (a * 255.0).round() as u8;
        }
    }
}

pub(super) fn ensure_sv_texture(id: &str, hue: f32) -> String {
    let hue = hue.rem_euclid(360.0);
    tex_cached(sv_texture_key(id, hue), move || {
        let w = 192u32;
        let h = 144u32;
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            // `uv_for_subrect`: widget top → texture uv.y=0 (row 0). Pointer uses
            // `val = 1 - ty`, so row 0 must be value 1 (bright), last row value 0.
            let v = 1.0 - y as f32 / (h - 1).max(1) as f32;
            for x in 0..w {
                let s = x as f32 / (w - 1).max(1) as f32;
                let (r, g, b) = hsv_to_rgb(hue, s, v);
                let i = ((y * w + x) * 4) as usize;
                rgba[i] = r;
                rgba[i + 1] = g;
                rgba[i + 2] = b;
                rgba[i + 3] = 255;
            }
        }
        apply_rounded_rect_alpha(&mut rgba, w, h, h as f32 * (0.375 / 9.0));
        (w, h, rgba)
    })
}

pub(super) fn ensure_hue_texture() -> String {
    tex_cached(hue_texture_key().to_string(), || {
        let w = 180u32;
        let h = 8u32;
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for x in 0..w {
            let hue = x as f32 / (w - 1).max(1) as f32 * 360.0;
            let (r, g, b) = hsv_to_rgb(hue, 1.0, 1.0);
            for y in 0..h {
                let i = ((y * w + x) * 4) as usize;
                rgba[i] = r;
                rgba[i + 1] = g;
                rgba[i + 2] = b;
                rgba[i + 3] = 255;
            }
        }
        apply_rounded_rect_alpha(&mut rgba, w, h, h as f32 * 0.5);
        (w, h, rgba)
    })
}

pub(super) fn alpha_texture_key(id: &str) -> String {
    format!("rui_cp_alpha:{id}")
}

pub(super) fn ensure_alpha_texture(id: &str, c: Rgba) -> String {
    let tag = ((c.r as u64) << 16) | ((c.g as u64) << 8) | (c.b as u64);
    tex_cached_tagged(alpha_texture_key(id), tag, move || {
        let w = 180u32;
        let h = 8u32;
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        let cell = 4u32;
        for y in 0..h {
            for x in 0..w {
                let a = x as f32 / (w - 1).max(1) as f32;
                let checker = if ((x / cell) + (y / cell)) % 2 == 0 {
                    220.0
                } else {
                    160.0
                };
                let i = ((y * w + x) * 4) as usize;
                rgba[i] = (checker * (1.0 - a) + c.r as f32 * a).round() as u8;
                rgba[i + 1] = (checker * (1.0 - a) + c.g as f32 * a).round() as u8;
                rgba[i + 2] = (checker * (1.0 - a) + c.b as f32 * a).round() as u8;
                rgba[i + 3] = 255;
            }
        }
        apply_rounded_rect_alpha(&mut rgba, w, h, h as f32 * 0.5);
        (w, h, rgba)
    })
}

pub(super) fn trigger_label(text: &str) -> Element {
    build_text_node(
        text,
        TextNodeConfig {
            size_px: 0.875 * 16.0,
            weight: 400,
            color: parse_text_color("foreground"),
            ..Default::default()
        },
    )
}

pub(super) fn picker_trigger(id: &str, leading: Element, main: Element, disabled: bool) -> Element {
    let mut border = parse_color("input");
    let mut bg = parse_color("transparent");
    if disabled {
        border[3] *= 0.5;
        bg[3] *= 0.5;
    }
    let mut row: Element = hstack()
        .h("2.25rem")
        .w("100%")
        .min_w("0")
        .rounded("0.375rem")
        .px("0.75rem")
        .gap("0.5rem")
        .items_center()
        .bg(bg)
        .border(border, 1.0)
        .shadow(
            crate::tokens::shadow_rgba(crate::tokens::Elevation::Xs),
            1.0,
        )
        .into();
    row = row.child(leading).child(Element::from(
        hstack()
            .flex_grow(1.0)
            .min_w("0")
            .items_center()
            .child(main),
    ));
    let cursor = if disabled {
        row.cursor_not_allowed()
    } else {
        row.cursor_kind(CursorKind::Pointer)
    };
    let trigger_fid = trigger_focus_id(id);
    let mut out: Element = interactive(cursor)
        .focus_id(trigger_fid.as_str())
        .focusable(!disabled)
        .into();
    out.template_id = Some(trigger_fid);
    out.set_attr(ATTR_CP_SLOT, "color-picker-trigger");
    out
}

#[derive(Clone, PartialEq)]
pub(super) struct ColorPickerRenderProps {
    pub(super) id: String,
    pub(super) variant: ColorPickerVariant,
    pub(super) format: ColorFormat,
    pub(super) show_alpha: bool,
    pub(super) disabled: bool,
}

pub(super) const ATTR_CP_SLOT: &str = "data-slot";
pub(super) const SLOT_SV_TEX: &str = "cp-sv-tex";
pub(super) const SLOT_SV_KNOB: &str = "cp-sv-knob";

pub(super) fn find_tid_mut<'a>(el: &'a mut Element, tid: &str) -> Option<&'a mut Element> {
    crate::core::tree::find_in_tree_mut(el, |n| n.template_id.as_deref() == Some(tid))
}

pub(super) fn sync_trigger_preview(
    root: &mut Element,
    id: &str,
    c: Rgba,
    fmt: ColorFormat,
) -> crate::core::tree::LayoutStatus {
    let mut changed = crate::core::tree::LayoutStatus::Clean;
    let want_bg = c.to_f32();
    let swatch_id = format!("{id}__cp_trig_swatch");
    if let Some(sw) = find_tid_mut(root, &swatch_id) {
        if sw.style.bg_color != want_bg {
            sw.style.bg_color = want_bg;
            changed = crate::core::tree::LayoutStatus::StyleChanged;
        }
    }
    let label_id = format!("{id}__cp_trig_label");
    let want = format_color(c, fmt);
    if let Some(label) = find_tid_mut(root, &label_id) {
        if label.text.as_deref() != Some(want.as_str()) {
            label.text = Some(want);
            changed = crate::core::tree::LayoutStatus::StyleChanged;
        }
    }
    changed
}

pub(super) fn with_trigger_preview_sync(
    view: Element,
    id: String,
    rgba: Signal<Rgba>,
    fmt_sig: Signal<ColorFormat>,
) -> Element {
    view.reactive_sync(move |root, _| sync_trigger_preview(root, &id, rgba.peek(), fmt_sig.peek()))
}

pub(super) fn follow_hue_into_rgba(
    rgba: Signal<Rgba>,
    hue: Signal<f32>,
    sat: Signal<f32>,
    val: Signal<f32>,
    out: Signal<String>,
    fmt_sig: Signal<ColorFormat>,
) {
    create_effect(move || {
        let hue_v = hue.get();
        let s = sat.peek();
        let v = val.peek();
        let c = rgba.peek();
        let (r, g, b) = hsv_to_rgb(hue_v.rem_euclid(360.0), s, v);
        if r == c.r && g == c.g && b == c.b {
            return;
        }
        let next = Rgba { r, g, b, a: c.a };
        rgba.set(next);
        sync_string_from_rgba(out, next, fmt_sig.peek());
    });
}

pub(super) fn find_cp_slot_mut<'a>(el: &'a mut Element, slot: &str) -> Option<&'a mut Element> {
    crate::core::tree::find_by_attr_mut(el, ATTR_CP_SLOT, slot)
}

/// Half of the 0.75rem (12px) SV knob — paint offset so the *center* sits on (S, V).
pub(super) const SV_KNOB_HALF_PX: f32 = 6.0;

pub(super) fn sv_knob_frac(sat: f32, val: f32) -> (f32, f32) {
    (sat.clamp(0.0, 1.0), (1.0 - val).clamp(0.0, 1.0))
}

pub(super) fn inset_percent_mismatch(cur: LengthPercentageAuto, want: f32) -> bool {
    match crate::layout::lpa_kind(cur) {
        crate::layout::DimKind::Percent(p) => (p - want).abs() > 0.0005,
        _ => true,
    }
}

/// Place the knob with Taffy `%` insets so the first layout (before bounds exist) is
/// already on the current S/V. Pixel `translate` only centers the 12px handle.
pub(super) fn place_sv_knob(
    knob: &mut Element,
    layout_tree: Option<&mut LayoutTree>,
    c: Rgba,
    sat: f32,
    val: f32,
) -> LayoutStatus {
    let (left, top) = sv_knob_frac(sat, val);
    let want_t = [-SV_KNOB_HALF_PX, -SV_KNOB_HALF_PX];
    let mut status = LayoutStatus::Clean;
    if inset_percent_mismatch(knob.style.inset.left, left)
        || inset_percent_mismatch(knob.style.inset.top, top)
    {
        knob.style.inset.left = LengthPercentageAuto::percent(left);
        knob.style.inset.top = LengthPercentageAuto::percent(top);
        if let Some(lt) = layout_tree {
            if knob.layout_node.is_some() {
                lt.update_taffy_node_style(knob);
            }
        }
        status = LayoutStatus::LayoutChanged;
    }
    if knob.style.translate != want_t {
        knob.style.translate = want_t;
        status = status.merge(LayoutStatus::StyleChanged);
    }
    if knob.style.bg_color != c.to_f32() {
        knob.style.bg_color = c.to_f32();
        status = status.merge(LayoutStatus::StyleChanged);
    }
    status
}

pub(super) fn sync_sv_visuals(
    root: &mut Element,
    layout_tree: &mut LayoutTree,
    id: &str,
    c: Rgba,
    hue: f32,
    sat: f32,
    val: f32,
) -> LayoutStatus {
    let mut status = LayoutStatus::Clean;
    let key = ensure_sv_texture(id, hue);
    if let Some(tex) = find_cp_slot_mut(root, SLOT_SV_TEX) {
        if tex.image_texture.as_deref() != Some(key.as_str()) {
            tex.image_texture = Some(key);
            status = LayoutStatus::StyleChanged;
        }
    }
    if let Some(knob) = find_cp_slot_mut(root, SLOT_SV_KNOB) {
        status = status.merge(place_sv_knob(knob, Some(layout_tree), c, sat, val));
    }
    status
}

pub(super) fn apply_sv_from_rgb(
    parsed: Rgba,
    hue: Signal<f32>,
    sat: Signal<f32>,
    val: Signal<f32>,
) {
    let (h, s, v) = rgb_to_hsv(parsed.r, parsed.g, parsed.b);
    if s > 0.001 && v > 0.001 {
        hue.set(h);
    }
    sat.set(s);
    val.set(v);
}

pub(super) fn apply_parsed(
    parsed: Rgba,
    rgba: Signal<Rgba>,
    hue: Signal<f32>,
    sat: Signal<f32>,
    val: Signal<f32>,
    out: Signal<String>,
    fmt: ColorFormat,
) {
    rgba.set(parsed);
    apply_sv_from_rgb(parsed, hue, sat, val);
    sync_string_from_rgba(out, parsed, fmt);
}

pub(super) fn wire_pointer_drag(mut el: Element, fid: String, drag: ColorDrag) -> Element {
    let b_ref = drag.bounds.clone();
    el = el.on_pointer_down({
        let drag = drag.clone();
        let fid = fid.clone();
        move |e| {
            if e.phase == EventPhase::Capture {
                return;
            }
            let b = if e.current_bounds[2] > 0.0 {
                e.current_bounds
            } else {
                b_ref.get()
            };
            b_ref.set(b);
            apply_color_pointer(&drag, e.x, e.y);
            sync_string_from_rgba(drag.out, drag.rgba.peek(), drag.fmt_sig.peek());
            begin_color_drag(drag.clone());
            crate::app::set_pointer_capture(fid.clone());
            e.stop_propagation();
        }
    });
    el = el.cursor_kind(CursorKind::Pointer);
    el.reactive_sync(move |root, layout_tree| {
        // Do not overwrite drag bounds with `layout_bounds`: those are unscrolled
        // abs coords, while pointer `x/y` and `current_bounds` are window-space
        // (scroll already subtracted). Inline pickers inside a ScrollArea would
        // map every move to ty=0 → V=1 (knob glued to the top).
        if let Some(id) = drag.sv_id.as_deref() {
            sync_sv_visuals(
                root,
                layout_tree,
                id,
                drag.rgba.peek(),
                drag.hue.peek(),
                drag.sat.peek(),
                drag.val.peek(),
            )
        } else {
            LayoutStatus::Clean
        }
    })
}

pub(super) fn sv_area(
    id: &str,
    height: &str,
    rgba: Signal<Rgba>,
    hue: Signal<f32>,
    sat: Signal<f32>,
    val: Signal<f32>,
    fmt_sig: Signal<ColorFormat>,
    out: Signal<String>,
    show_alpha: bool,
    disabled: bool,
) -> Element {
    let c = rgba.peek();
    let hue_v = hue.peek();
    let sv_key = ensure_sv_texture(id, hue_v);

    let mut tex: Element = r#box().w("100%").h("100%").rounded("0.375rem").into();
    // Opaque fill under the ramp: rounded-texture alpha is 0 in the corners, otherwise
    // the page (not the popover) shows through those pixels.
    tex = tex
        .bg(tokens::popover())
        .overflow("hidden")
        .pointer_events("none");
    tex.image_texture = Some(sv_key);
    tex.attrs
        .insert(ATTR_CP_SLOT.to_string(), SLOT_SV_TEX.to_string());

    let mut knob: Element = r#box().w("0.75rem").h("0.75rem").rounded_full().into();
    knob = knob
        .absolute()
        .bg(c.to_f32())
        .border([1.0, 1.0, 1.0, 1.0], 2.0)
        .pointer_events("none");
    let _ = place_sv_knob(&mut knob, None, c, sat.peek(), val.peek());
    knob.attrs
        .insert(ATTR_CP_SLOT.to_string(), SLOT_SV_KNOB.to_string());

    let mut area: Element = r#box()
        .w("100%")
        .h(height)
        .rounded("0.375rem")
        .relative(true)
        .bg(tokens::popover())
        .border(tokens::border(), 1.0)
        .child(tex)
        .child(knob)
        .into();
    if disabled {
        return area;
    }
    let drag = ColorDrag {
        kind: ColorDragKind::Sv,
        bounds: Rc::new(Cell::new([0.0f32; 4])),
        rgba,
        hue,
        sat,
        val,
        show_alpha,
        out,
        fmt_sig,
        sv_id: Some(id.to_string()),
    };
    let fid = format!("{id}:sv");
    area = wire_pointer_drag(area, fid.clone(), drag);
    interactive(area)
        .focus_id(fid.as_str())
        .focusable(true)
        .focus_ring(true)
        .on_key_down({
            let rgba_k = rgba;
            let hue_k = hue;
            let sat_k = sat;
            let val_k = val;
            let out_k = out;
            let fmt_k = fmt_sig;
            move |ev| {
                let cur = rgba_k.peek();
                let mut s = sat_k.peek();
                let mut v = val_k.peek();
                let step = 0.02;
                match &ev.logical_key {
                    Key::Named(NamedKey::ArrowLeft) => s = (s - step).max(0.0),
                    Key::Named(NamedKey::ArrowRight) => s = (s + step).min(1.0),
                    Key::Named(NamedKey::ArrowUp) => v = (v + step).min(1.0),
                    Key::Named(NamedKey::ArrowDown) => v = (v - step).max(0.0),
                    Key::Named(NamedKey::Home) => s = 0.0,
                    Key::Named(NamedKey::End) => s = 1.0,
                    Key::Named(NamedKey::PageUp) => v = 1.0,
                    Key::Named(NamedKey::PageDown) => v = 0.0,
                    _ => return false,
                }
                sat_k.set(s);
                val_k.set(v);
                let (r, g, b) = hsv_to_rgb(hue_k.peek(), s, v);
                rgba_k.set(Rgba { r, g, b, a: cur.a });
                sync_string_from_rgba(out_k, rgba_k.peek(), fmt_k.peek());
                true
            }
        })
        .into()
}

pub(super) fn hue_slider(
    id: &str,
    _rgba: Signal<Rgba>,
    hue: Signal<f32>,
    _fmt_sig: Signal<ColorFormat>,
    _out: Signal<String>,
    disabled: bool,
) -> Element {
    slider(format!("{id}:hue"))
        .min(0.0)
        .max(360.0)
        .step(1.0)
        .value_signal(hue)
        .track_texture(ensure_hue_texture())
        // Re-mark needed each sync so TexturePool eviction rehydrates the ramp.
        .track_texture_fn(|| ensure_hue_texture())
        .disabled(disabled)
        .thumb_fill({
            let hue = hue;
            move || {
                let (r, g, b) = hsv_to_rgb(hue.peek().rem_euclid(360.0), 1.0, 1.0);
                [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
            }
        })
        .into()
}

pub(super) fn alpha_slider(
    id: &str,
    rgba: Signal<Rgba>,
    fmt_sig: Signal<ColorFormat>,
    out: Signal<String>,
    show_alpha: bool,
    disabled: bool,
) -> Element {
    let alpha = use_signal(format!("{id}__cp_alpha").as_str(), || {
        rgba.peek().a.clamp(0.0, 1.0)
    });
    create_effect({
        let rgba = rgba;
        let out = out;
        let fmt_sig = fmt_sig;
        let alpha = alpha;
        move || {
            let a = alpha.get();
            let c = rgba.peek();
            let want = if show_alpha { a.clamp(0.0, 1.0) } else { 1.0 };
            if (c.a - want).abs() <= 0.001 {
                return;
            }
            let mut next = c;
            next.a = want;
            rgba.set(next);
            sync_string_from_rgba(out, next, fmt_sig.peek());
        }
    });
    let alpha_id = id.to_string();
    slider(format!("{id}:alpha"))
        .min(0.0)
        .max(1.0)
        .step(0.01)
        .value_signal(alpha)
        .track_texture(ensure_alpha_texture(id, rgba.peek()))
        .track_texture_fn({
            let rgba = rgba;
            let alpha_id = alpha_id;
            move || ensure_alpha_texture(&alpha_id, rgba.peek())
        })
        .disabled(disabled)
        .thumb_fill({
            let rgba = rgba;
            move || rgba.peek().to_f32()
        })
        .on_sync({
            let alpha = alpha;
            move || {
                let c = rgba.peek();
                let want = if show_alpha { c.a.clamp(0.0, 1.0) } else { 1.0 };
                if (want - alpha.peek()).abs() > 0.001 {
                    alpha.set_silent(want);
                }
            }
        })
        .into()
}

pub(super) fn hsv_sliders(
    id: &str,
    variant: ColorPickerVariant,
    rgba: Signal<Rgba>,
    hue: Signal<f32>,
    sat: Signal<f32>,
    val: Signal<f32>,
    fmt_sig: Signal<ColorFormat>,
    out: Signal<String>,
    show_alpha: bool,
    disabled: bool,
) -> Element {
    let alpha_on = show_alpha && fmt_sig.peek().supports_alpha();
    let mut sliders = vstack()
        .gap("0.5rem")
        .w("100%")
        .child(sv_area(
            id,
            variant.area_height(),
            rgba,
            hue,
            sat,
            val,
            fmt_sig,
            out,
            show_alpha,
            disabled,
        ))
        .child(hue_slider(id, rgba, hue, fmt_sig, out, disabled));
    if alpha_on {
        sliders = sliders.child(alpha_slider(id, rgba, fmt_sig, out, show_alpha, disabled));
    }
    sliders.into()
}

pub(super) fn ensure_linear_ramp(key: String, from: [f32; 4], to: [f32; 4]) -> String {
    tex_cached(key, move || {
        let w = 180u32;
        let h = 8u32;
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for x in 0..w {
            let t = x as f32 / (w - 1).max(1) as f32;
            let r = ((from[0] + (to[0] - from[0]) * t) * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8;
            let g = ((from[1] + (to[1] - from[1]) * t) * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8;
            let b = ((from[2] + (to[2] - from[2]) * t) * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8;
            for y in 0..h {
                let i = ((y * w + x) * 4) as usize;
                rgba[i] = r;
                rgba[i + 1] = g;
                rgba[i + 2] = b;
                rgba[i + 3] = 255;
            }
        }
        apply_rounded_rect_alpha(&mut rgba, w, h, h as f32 * 0.5);
        (w, h, rgba)
    })
}

pub(super) fn channel_byte(c: Rgba, part: &str) -> u8 {
    match part {
        "r" => c.r,
        "g" => c.g,
        _ => c.b,
    }
}

pub(super) fn write_channel_byte(mut c: Rgba, part: &str, v: u8) -> Rgba {
    match part {
        "r" => c.r = v,
        "g" => c.g = v,
        _ => c.b = v,
    }
    c
}

pub(super) fn channel_track(
    id: &str,
    part: &str,
    from: [f32; 4],
    to: [f32; 4],
    rgba: Signal<Rgba>,
    hue: Signal<f32>,
    sat: Signal<f32>,
    val: Signal<f32>,
    fmt_sig: Signal<ColorFormat>,
    out: Signal<String>,
    show_alpha: bool,
    disabled: bool,
) -> Element {
    let ch = use_signal(format!("{id}__cp_ch_{part}").as_str(), || {
        channel_byte(rgba.peek(), part) as f32
    });
    let last = use_signal(format!("{id}__cp_ch_{part}_last").as_str(), || ch.peek());
    let v = ch.get();
    let c = rgba.get();
    if (v - last.peek()).abs() > 0.5 {
        last.set(v);
        let mut next = write_channel_byte(c, part, v.round().clamp(0.0, 255.0) as u8);
        if !show_alpha {
            next.a = 1.0;
        }
        rgba.set(next);
        apply_sv_from_rgb(next, hue, sat, val);
        sync_string_from_rgba(out, next, fmt_sig.peek());
    } else {
        let want = channel_byte(c, part) as f32;
        if (want - ch.peek()).abs() > 0.5 {
            ch.set(want);
            last.set(want);
        }
    }
    let ramp_key = ensure_linear_ramp(format!("rui_cp_ramp:{id}:{part}"), from, to);
    slider(format!("{id}:{part}"))
        .min(0.0)
        .max(255.0)
        .step(1.0)
        .value_signal(ch)
        .track_texture(ramp_key)
        .disabled(disabled)
        .into()
}

pub(super) fn rgb_sliders(
    id: &str,
    rgba: Signal<Rgba>,
    hue: Signal<f32>,
    sat: Signal<f32>,
    val: Signal<f32>,
    fmt_sig: Signal<ColorFormat>,
    out: Signal<String>,
    show_alpha: bool,
    disabled: bool,
) -> Element {
    let c = rgba.peek();
    let rf = c.r as f32 / 255.0;
    let gf = c.g as f32 / 255.0;
    let bf = c.b as f32 / 255.0;
    let mut row = vstack()
        .gap("0.5rem")
        .w("100%")
        .child(channel_track(
            id,
            "r",
            [0.0, gf, bf, 1.0],
            [1.0, gf, bf, 1.0],
            rgba,
            hue,
            sat,
            val,
            fmt_sig,
            out,
            show_alpha,
            disabled,
        ))
        .child(channel_track(
            id,
            "g",
            [rf, 0.0, bf, 1.0],
            [rf, 1.0, bf, 1.0],
            rgba,
            hue,
            sat,
            val,
            fmt_sig,
            out,
            show_alpha,
            disabled,
        ))
        .child(channel_track(
            id,
            "b",
            [rf, gf, 0.0, 1.0],
            [rf, gf, 1.0, 1.0],
            rgba,
            hue,
            sat,
            val,
            fmt_sig,
            out,
            show_alpha,
            disabled,
        ));
    if show_alpha && fmt_sig.peek().supports_alpha() {
        row = row.child(alpha_slider(id, rgba, fmt_sig, out, show_alpha, disabled));
    }
    row.into()
}

pub(super) fn format_value_row(
    id: &str,
    rgba: Signal<Rgba>,
    hue: Signal<f32>,
    sat: Signal<f32>,
    val: Signal<f32>,
    fmt_sig: Signal<ColorFormat>,
    fmt_str: Signal<String>,
    out: Signal<String>,
    disabled: bool,
) -> Element {
    let selected = ColorFormat::parse(&fmt_str.get());
    if selected != fmt_sig.peek() {
        fmt_sig.set(selected);
        sync_string_from_rgba(out, rgba.peek(), selected);
    }
    let fmt = fmt_sig.peek();

    let mut sel = select(format!("{id}:fmt"))
        .value_signal(fmt_str)
        .placeholder("HEX")
        .match_trigger(false)
        .trigger_width("5.75rem")
        .menu_width("6.5rem")
        .max_height("16rem")
        .scroll_selected("center");
    for (value, label) in FORMAT_OPTIONS {
        sel = sel.item(SelectItemData::new(value.as_str(), *label));
    }
    let sel: Element = sel.into();
    // Input chrome is h-9 (2.25rem); SelectTrigger defaults to h-8 (2rem).
    let sel = sel
        .w("5.75rem")
        .h("2.25rem")
        .min_h("2.25rem")
        .flex_shrink(0.0);

    let hex_id = format!("{id}__hex");
    let mut hex_in: Element = input(&hex_id)
        .placeholder(match fmt {
            ColorFormat::Hex => "#RRGGBB",
            ColorFormat::Hex8 => "#RRGGBBAA",
            ColorFormat::Rgb => "rgb()",
            ColorFormat::Rgba => "rgba()",
            ColorFormat::Hsl => "hsl()",
            ColorFormat::Hsla => "hsla()",
        })
        .disabled(disabled)
        .flex_grow(1.0)
        .value_signal(out)
        .into();
    hex_in = hex_in.on_key_down({
        let out_s = out;
        let rgba_s = rgba;
        let hue_s = hue;
        let sat_s = sat;
        let val_s = val;
        let fmt_s = fmt_sig;
        move |ev| {
            if matches!(ev.logical_key, Key::Named(NamedKey::Enter)) {
                if let Some(parsed) = parse_color_str(&out_s.peek()) {
                    apply_parsed(parsed, rgba_s, hue_s, sat_s, val_s, out_s, fmt_s.peek());
                }
                return true;
            }
            false
        }
    });

    hstack()
        .gap("0.5rem")
        .w("100%")
        .items_center()
        .min_w("0")
        .child(sel)
        .child(hex_in)
        .into()
}

pub(super) fn channel_fields(
    id: &str,
    rgba: Signal<Rgba>,
    hue: Signal<f32>,
    sat: Signal<f32>,
    val: Signal<f32>,
    fmt_sig: Signal<ColorFormat>,
    out: Signal<String>,
    disabled: bool,
) -> Element {
    let c = rgba.get();
    let fields = [
        ("r", "R", c.r.to_string()),
        ("g", "G", c.g.to_string()),
        ("b", "B", c.b.to_string()),
        ("a", "A", format!("{:.0}", c.a * 100.0)),
    ];
    let mut row = hstack().gap("0.375rem").w("100%");
    for (key, label, text) in fields {
        let fid = format!("{id}__ch_{key}");
        let sig = use_signal(input_value_signal_key(&fid).as_str(), String::new);
        sig.set(text);
        let mut field: Element = input(&fid)
            .placeholder(label)
            .disabled(disabled)
            .flex_grow(1.0)
            .h("2rem")
            .into();
        field = field.on_key_down({
            let sig = sig;
            let rgba_s = rgba;
            let hue_s = hue;
            let sat_s = sat;
            let val_s = val;
            let out_s = out;
            let fmt_s = fmt_sig;
            let key = key.to_string();
            move |ev| {
                if !matches!(ev.logical_key, Key::Named(NamedKey::Enter)) {
                    return false;
                }
                let n: f32 = sig.peek().trim().parse().unwrap_or(f32::NAN);
                if !n.is_finite() {
                    return true;
                }
                let mut cur = rgba_s.peek();
                match key.as_str() {
                    "r" => cur.r = n.round().clamp(0.0, 255.0) as u8,
                    "g" => cur.g = n.round().clamp(0.0, 255.0) as u8,
                    "b" => cur.b = n.round().clamp(0.0, 255.0) as u8,
                    "a" => cur.a = (n / 100.0).clamp(0.0, 1.0),
                    _ => {}
                }
                apply_parsed(cur, rgba_s, hue_s, sat_s, val_s, out_s, fmt_s.peek());
                true
            }
        });
        let col = vstack()
            .gap("0.125rem")
            .flex_grow(1.0)
            .min_w("0")
            .child(trigger_label(label))
            .child(field);
        row = row.child(col);
    }
    row.into()
}

pub(super) fn preview_bar(rgba: Signal<Rgba>, fmt_sig: Signal<ColorFormat>) -> Element {
    let c = rgba.get();
    let mut bar: Element = r#box().w("100%").h("3.25rem").rounded("0.5rem").into();
    bar = bar.bg(c.to_f32()).border(tokens::border(), 1.0).relative();
    bar = bar.child(
        hstack()
            .w("100%")
            .h("100%")
            .items_center()
            .justify_center()
            .pointer_events("none")
            .child(trigger_label(&format_color(c, fmt_sig.get()))),
    );
    bar
}

pub(super) fn swatch_grid(
    id: &str,
    variant: ColorPickerVariant,
    rgba: Signal<Rgba>,
    hue: Signal<f32>,
    sat: Signal<f32>,
    val: Signal<f32>,
    fmt_sig: Signal<ColorFormat>,
    out: Signal<String>,
    open: Signal<bool>,
    round: &str,
) -> Element {
    let cols = variant.swatch_columns();
    let chip_size = variant.swatch_chip_size();
    let mut swatches: Element = r#box()
        .w("100%")
        .grid_cols_equal(cols)
        .gap("0.375rem")
        .into();
    let circle = variant.circle_swatches();
    for (i, sw) in variant.swatches().iter().enumerate() {
        let parsed = parse_color_str(sw).unwrap_or(Rgba::opaque(59, 130, 246));
        // Prefer explicit w×h for swatches: `w:100%` + `aspect-ratio` inside a
        // `h:auto` / `min-h:0` grid cell often collapses (auto track min contribution).
        let mut chip: Element = {
            let b = r#box().w(chip_size).h(chip_size);
            if circle {
                b.rounded_full().into()
            } else {
                b.rounded(round).into()
            }
        };
        chip = chip.bg(parsed.to_f32()).border(tokens::border(), 1.0);
        let rgba_s = rgba;
        let hue_s = hue;
        let sat_s = sat;
        let val_s = val;
        let out_s = out;
        let fmt_s = fmt_sig;
        let open_s = open;
        let close = variant.close_on_swatch();
        let mut btn: Element = button(format!("{id}:sw{i}"))
            .variant("ghost")
            .size("icon-xs")
            .p("0")
            .w(chip_size)
            .h(chip_size)
            .on_click(move || {
                apply_parsed(parsed, rgba_s, hue_s, sat_s, val_s, out_s, fmt_s.peek());
                if close {
                    open_s.set(false);
                }
            })
            .child(chip)
            .into();
        btn = btn.min_h(chip_size).min_w(chip_size).flex_shrink(0.0);
        swatches = swatches.child(btn);
    }
    swatches
}

pub(super) fn panel_shell(width: &str, body: Element) -> Element {
    let mut panel: Element = vstack()
        .gap("0.5rem")
        .p("0.75rem")
        .w(width)
        .child(body)
        .into();
    panel = panel
        .bg(tokens::popover())
        .rounded("0.5rem")
        .border_style("solid")
        .border_width(1.0)
        .border_color(tokens::border())
        .popup_shadow();
    panel.set_attr(ATTR_CP_SLOT, "color-picker-content");
    panel
}

pub(super) fn build_panel(
    id: &str,
    variant: ColorPickerVariant,
    rgba: Signal<Rgba>,
    hue: Signal<f32>,
    sat: Signal<f32>,
    val: Signal<f32>,
    fmt_sig: Signal<ColorFormat>,
    fmt_str: Signal<String>,
    show_alpha: bool,
    disabled: bool,
    out: Signal<String>,
    open: Signal<bool>,
    slots: &SlotMap,
) -> Element {
    let width = variant.panel_width();
    let alpha_on = show_alpha && fmt_sig.peek().supports_alpha();
    let sliders = || {
        hsv_sliders(
            id, variant, rgba, hue, sat, val, fmt_sig, out, alpha_on, disabled,
        )
    };
    let controls = || {
        let row = format_value_row(id, rgba, hue, sat, val, fmt_sig, fmt_str, out, disabled);
        slots.paint("color-picker-input", row)
    };
    let swatches = |round: &str| {
        let grid = swatch_grid(id, variant, rgba, hue, sat, val, fmt_sig, out, open, round);
        slots.paint("color-picker-swatch", grid)
    };

    let body: Element = match variant {
        ColorPickerVariant::Twitter => vstack()
            .gap("0.5rem")
            .w("100%")
            .child(swatches("0.25rem"))
            .child(controls())
            .into(),
        ColorPickerVariant::Block => vstack()
            .gap("0.5rem")
            .w("100%")
            .child(swatches("0.25rem"))
            .child(controls())
            .into(),
        ColorPickerVariant::Circle => vstack()
            .gap("0.5rem")
            .w("100%")
            .child(swatches("0.25rem"))
            .into(),
        ColorPickerVariant::Slider => vstack()
            .gap("0.5rem")
            .w("100%")
            .child(rgb_sliders(
                id, rgba, hue, sat, val, fmt_sig, out, show_alpha, disabled,
            ))
            .child(controls())
            .into(),
        ColorPickerVariant::Sketch => vstack()
            .gap("0.5rem")
            .w("100%")
            .child(preview_bar(rgba, fmt_sig))
            .child(sliders())
            .child(channel_fields(
                id, rgba, hue, sat, val, fmt_sig, out, disabled,
            ))
            .child(controls())
            .into(),
        ColorPickerVariant::Github => vstack()
            .gap("0.5rem")
            .w("100%")
            .child(swatches("0.25rem"))
            .child(Element::from(separator()))
            .child(sliders())
            .child(controls())
            .into(),
        ColorPickerVariant::Material => vstack()
            .gap("0.5rem")
            .w("100%")
            .child(swatches("0.25rem"))
            .child(Element::from(separator()))
            .child(sliders())
            .child(controls())
            .into(),
        ColorPickerVariant::Chrome => vstack()
            .gap("0.5rem")
            .w("100%")
            .child(sliders())
            .child(controls())
            .child(trigger_label("Chrome 预设"))
            .child(swatches("0.25rem"))
            .into(),
        ColorPickerVariant::Compact => vstack()
            .gap("0.5rem")
            .w("100%")
            .child(sliders())
            .child(controls())
            .into(),
        ColorPickerVariant::Default | ColorPickerVariant::Inline => vstack()
            .gap("0.5rem")
            .w("100%")
            .child(sliders())
            .child(controls())
            .child(trigger_label("预设"))
            .child(swatches("0.25rem"))
            .into(),
    };
    slots.paint("color-picker-content", panel_shell(width, body))
}

pub(super) fn build_color_picker_view(
    props: ColorPickerRenderProps,
    open: Signal<bool>,
    out: Signal<String>,
    rgba: Signal<Rgba>,
    hue: Signal<f32>,
    sat: Signal<f32>,
    val: Signal<f32>,
    fmt_sig: Signal<ColorFormat>,
    fmt_str: Signal<String>,
    slots: &SlotMap,
) -> Element {
    let ColorPickerRenderProps {
        id,
        variant,
        format: _,
        show_alpha,
        disabled,
    } = props;
    let _ = open.get();
    let selected = ColorFormat::parse(&fmt_str.get());
    if selected != fmt_sig.peek() {
        fmt_sig.set(selected);
        sync_string_from_rgba(out, rgba.peek(), selected);
    }

    let mut swatch: Element = r#box().w("1.25rem").h("1.25rem").rounded("0.25rem").into();
    swatch = swatch.border(tokens::border(), 1.0);
    let swatch_id = format!("{id}__cp_trig_swatch");
    swatch = bind_style(swatch, swatch_id, {
        let rgba = rgba;
        move || StylePatch {
            bg: Some(rgba.get().to_f32()),
            ..Default::default()
        }
    });
    let main = bind_text(format!("{id}__cp_trig_label"), {
        let rgba = rgba;
        let fmt_sig = fmt_sig;
        move || format_color(rgba.get(), fmt_sig.peek())
    })
    .text_size(0.875 * 16.0)
    .text_color(parse_text_color("foreground"))
    .min_w(0.0)
    .flex_grow(1.0)
    .whitespace_nowrap()
    .text_overflow_ellipsis();

    if variant == ColorPickerVariant::Inline {
        let panel = build_panel(
            &id, variant, rgba, hue, sat, val, fmt_sig, fmt_str, show_alpha, disabled, out, open,
            slots,
        );
        let view: Element = vstack()
            .gap("0.5rem")
            .w("100%")
            .child(
                hstack()
                    .gap("0.5rem")
                    .items_center()
                    .child(swatch)
                    .child(main),
            )
            .child(panel)
            .into();
        return with_trigger_preview_sync(view, id, rgba, fmt_sig);
    }

    let mut trigger = slots.paint(
        "color-picker-trigger",
        picker_trigger(&id, swatch, main, disabled),
    );
    if !disabled {
        let open_t = open;
        let trig = trigger_focus_id(&id);
        trigger = trigger
            .on_click({
                let trig = trig.clone();
                move || {
                    if open_t.peek() {
                        close_picker(open_t, trig.clone());
                    } else {
                        open_t.set(true);
                    }
                }
            })
            .on_key_down({
                let open_k = open;
                let trig = trig.clone();
                move |ev| match &ev.logical_key {
                    Key::Named(NamedKey::Escape) => {
                        open_k.set(false);
                        focus_by_id(trig.clone());
                        true
                    }
                    Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                        if open_k.peek() {
                            open_k.set(false);
                            focus_by_id(trig.clone());
                        } else {
                            open_k.set(true);
                        }
                        true
                    }
                    _ => false,
                }
            });
    }

    let mut view: Element = vstack().gap("0").child(trigger).into();
    if disabled {
        return with_trigger_preview_sync(view, id, rgba, fmt_sig);
    }
    let overlay_id = id.clone();
    let panel_slots = slots.clone();
    let trigger_fid = use_signal(format!("{overlay_id}__cp_trigger_fid").as_str(), || {
        trigger_focus_id(&overlay_id)
    });
    let panel_slot = with_child_owner(move || {
        provide_anchored_overlay_context(open, overlay_id.clone(), trigger_fid);
        let panel_width = variant.panel_width();
        let ao_opts = AnchoredOverlayContentOpts {
            placement: Some(Placement::trigger(
                trigger_focus_id(&overlay_id),
                PopupPlacement::BottomStart,
                crate::utils::popup::FLOATING_TRIGGER_GAP,
                FloatingStrategy::Fixed,
            )),
            kind: OverlayKind::Popover,
            motion: FloatingMotionKind::Fade,
            mode: OverlayMode::Stacked,
            width: Some(panel_width.to_string()),
            ..Default::default()
        };
        anchored_overlay_content(ao_opts, move || {
            let mut panel = build_panel(
                &overlay_id,
                variant,
                rgba,
                hue,
                sat,
                val,
                fmt_sig,
                fmt_str,
                show_alpha,
                disabled,
                out,
                open,
                &panel_slots,
            );
            panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
            panel
        })
    });
    view = view.child(panel_slot);
    with_trigger_preview_sync(view, id, rgba, fmt_sig)
}
