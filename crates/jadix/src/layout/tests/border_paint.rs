use super::representative_border_color;
use crate::style::{LineStyle, Style};

#[test]
fn representative_color_uses_first_opaque_side_when_uniform_is_clear() {
    let mut style = Style {
        border_style: LineStyle::Solid,
        ..Default::default()
    };
    style.set_border_side_color(0, [1.0, 0.0, 0.0, 1.0]);
    style.set_border_side_color(1, [0.0, 1.0, 0.0, 1.0]);
    let c = representative_border_color(&style);
    assert!(
        (c[0] - 1.0).abs() < 1e-5 && c[3] > 0.5,
        "expected first opaque side, got {c:?}"
    );
}

#[test]
fn representative_color_prefers_uniform_when_set() {
    let style = Style {
        border_style: LineStyle::Solid,
        border_color: [0.0, 0.0, 1.0, 1.0],
        ..Default::default()
    };
    let c = representative_border_color(&style);
    assert!((c[2] - 1.0).abs() < 1e-5);
}

fn lum(c: [f32; 4]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

#[test]
fn css_bevel_tones_stay_apart_on_dark_primary() {
    let dark_teal = [0.05, 0.18, 0.16, 1.0];
    let d = super::css_bevel_dark(dark_teal);
    let l = super::css_bevel_light(dark_teal);
    assert!(
        lum(l) - lum(d) > 0.35,
        "3D bevel must stay readable on near-black primary, dark={d:?} light={l:?}"
    );
    let top = super::css_inset_tone(dark_teal, 0);
    let bottom = super::css_inset_tone(dark_teal, 2);
    assert!(lum(bottom) > lum(top) + 0.35);
    let top_out = super::css_outset_tone(dark_teal, 0);
    assert!(lum(top_out) > lum(top) + 0.35);
}

#[test]
fn seed_keeps_prior_content_on_cull_zero_max() {
    use super::{seed_scroll_signals_from_collect, ScrollAreaSignalsCached};
    use std::collections::HashMap;

    let mut map = HashMap::new();
    map.insert(
        "page".into(),
        ScrollAreaSignalsCached {
            vw: 320.0,
            vh: 384.0,
            cw: 320.0,
            ch: 900.0,
            sx: 0.0,
            sy: 220.0,
            overflow_x: "hidden".into(),
            overflow_y: "auto".into(),
            scrollbar_vis_x: "auto".into(),
            scrollbar_vis_y: "always".into(),
        },
    );
    let mut slot = Some(&mut map);
    // Same viewport, content collapsed to vh — band/clip cull sample.
    seed_scroll_signals_from_collect(&mut slot, "page", 320.0, 384.0, 320.0, 384.0);
    let e = map.get("page").expect("seeded");
    assert!(
        e.ch > 800.0,
        "cull seed must not collapse content_h (thumb would snap to start); ch={}",
        e.ch
    );
    assert!(
        (e.vh - 384.0).abs() < 0.1 && (e.vw - 320.0).abs() < 0.1,
        "cull seed must not poison viewport while keeping content; vw={} vh={}",
        e.vw,
        e.vh
    );
    assert!(
        (e.sy - 220.0).abs() < 0.1,
        "seed must not touch scroll offset; sy={}",
        e.sy
    );
}

#[test]
fn seed_accepts_expand_viewport_until_content_fits() {
    use super::{seed_scroll_signals_from_collect, ScrollAreaSignalsCached};
    use std::collections::HashMap;

    let mut map = HashMap::new();
    map.insert(
        "sidebar".into(),
        ScrollAreaSignalsCached {
            vw: 208.0,
            vh: 300.0,
            cw: 208.0,
            ch: 500.0,
            sx: 0.0,
            sy: 180.0,
            overflow_x: "hidden".into(),
            overflow_y: "auto".into(),
            scrollbar_vis_x: "auto".into(),
            scrollbar_vis_y: "auto".into(),
        },
    );
    let mut slot = Some(&mut map);
    // Window taller: prior content fits — seed must publish the fit so the rail hides.
    seed_scroll_signals_from_collect(&mut slot, "sidebar", 208.0, 600.0, 208.0, 500.0);
    let e = map.get("sidebar").expect("seeded");
    assert!(
        (e.vh - 600.0).abs() < 0.1 && (e.ch - 500.0).abs() < 0.1,
        "expand-to-fit must update viewport/content; got vh={} ch={}",
        e.vh,
        e.ch
    );
    assert!(
        e.ch <= e.vh + 0.5,
        "after expand-to-fit overlay must see non-overflow metrics"
    );
}
