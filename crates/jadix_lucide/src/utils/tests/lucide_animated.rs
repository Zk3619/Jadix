use super::*;
#[cfg(feature = "animation-path")]
use crate::utils::icon_provider::hover_intent_map;

#[cfg(feature = "animation-path")]
#[test]
fn morph_hover_switches_renderers_and_restarts_after_settling() {
    use crate::utils::morph_icon::{apply_bound_morph_icon, morph_to, tick_morph_icon};
    let mut el: Element = crate::utils::morph_icon("sun")
        .id("hybrid-hover-switch")
        .hover_animation(true)
        .label("Switch theme")
        .into();
    let key = instance_key(&el);
    let color = [25, 45, 65, 255];
    let texture = |el: &Element| {
        ensure_animated_icon_texture(el, 24.0, color, 1.0)
            .unwrap()
            .0
    };
    assert!(!subtree_needs_animated_icon_frame(&el));
    assert_eq!(
        el.get_attr("accesskit_name").map(String::as_str),
        Some("Switch theme")
    );

    (el.on_hover.as_ref().unwrap().borrow_mut())(true);
    sync_animated_icon_hover(&mut el);
    assert!(tick_animated_icon(&mut el, 0.016));
    assert!(texture(&el).starts_with("jadix_lucide_anim:"));
    // A nearly finished old recipe must not carry its playhead to the new icon.
    play_store().lock().unwrap().get_mut(&key).unwrap().elapsed = 9.0;
    apply_bound_morph_icon(&mut el, "moon");
    assert_eq!(
        el.get_attr(ATTR_ANIMATED_ICON).map(String::as_str),
        Some("moon")
    );
    assert!(!tick_animated_icon(&mut el, 0.016));
    assert!(!subtree_needs_animated_icon_frame(&el));
    assert!(texture(&el).starts_with("jadix_morph_icon:"));
    assert!(tick_morph_icon(&mut el, 0.016));
    // Retarget while morphing, then finish while still hovered.
    morph_to(&mut el, "heart");
    assert!(
        tick_morph_icon(&mut el, 10.0),
        "handoff must wake another frame"
    );
    let pb = play_store().lock().unwrap().get(&key).unwrap().clone();
    assert_eq!(pb.state, "animate");
    assert_eq!(pb.elapsed, 0.0);
    assert!(subtree_needs_animated_icon_frame(&el));
    assert!(texture(&el).starts_with("jadix_lucide_anim:"));

    (el.on_hover.as_ref().unwrap().borrow_mut())(false);
    sync_animated_icon_hover(&mut el);
    assert_eq!(
        el.get_attr(ATTR_ANIMATED_ICON_STATE).map(String::as_str),
        Some("normal")
    );
    // Deterministically finish the leave transition without sleeping.
    play_store()
        .lock()
        .unwrap()
        .get_mut(&key)
        .unwrap()
        .started_secs = now_secs() - 10.0;
    assert!(!tick_animated_icon(&mut el, 0.016));
    assert!(!subtree_needs_animated_icon_frame(&el));
    let tid = el.template_id.clone().unwrap();
    drop(el);
    assert!(!play_store().lock().unwrap().contains_key(&key));
    assert!(!hover_intent_map().lock().unwrap().contains_key(&tid));
}

#[cfg(feature = "animation-path")]
#[test]
fn morph_hover_respects_controlled_geometry_reduced_motion_and_fallbacks() {
    use crate::utils::morph_icon::{apply_bound_morph_from, apply_bound_morph_reduced, morph_to};
    let color = [50, 70, 90, 255];
    let texture = |el: &Element| {
        ensure_animated_icon_texture(el, 32.0, color, 1.0)
            .unwrap()
            .0
    };
    let mut el: Element = crate::utils::morph_icon("sun")
        .id("hybrid-controlled")
        .from_icon("sun")
        .to_icon("moon")
        .progress(0.5)
        .hover_animation(true)
        .into();
    start_animated_icon(&mut el);
    assert!(!tick_animated_icon(&mut el, 0.016));
    assert!(texture(&el).starts_with("jadix_morph_icon:"));
    apply_bound_morph_reduced(&mut el, "always");
    apply_bound_morph_from(&mut el, "");
    morph_to(&mut el, "heart");
    assert!(!tick_animated_icon(&mut el, 0.016));
    assert!(!subtree_needs_animated_icon_frame(&el));
    assert!(texture(&el).starts_with("jadix_morph_icon:"));

    apply_bound_morph_reduced(&mut el, "never");
    morph_to(&mut el, "M4 12 L20 12");
    crate::utils::morph_icon::tick_morph_icon(&mut el, 10.0);
    assert!(!tick_animated_icon(&mut el, 0.016));
    assert!(texture(&el).starts_with("jadix_morph_icon:"));
    let plain: Element = crate::utils::morph_icon("sun").into();
    assert!(plain.on_hover.is_none());
    assert!(plain.get_attr(ATTR_ANIMATED_ICON).is_none());
}

#[test]
fn collect_summary_keeps_cold_and_new_animated_icons_live() {
    use crate::layout::collect_cache::{
        invalidate_collect_paint_hash_path, stamp_subtree_paint_hashes,
    };
    let mut root = Element::new().child(Element::new().text("static"));
    stamp_subtree_paint_hashes(&mut root, false);
    assert!(!subtree_needs_animated_icon_frame(&root));
    assert!(!tick_animated_icons_subtree(&mut root, 0.016));
    let mut icon: Element = animated_icon("menu").controlled(true).into();
    set_animated_icon_state(&mut icon, "animate");
    root.children[0] = icon;
    invalidate_collect_paint_hash_path(&mut root, &[0]);
    assert!(
        subtree_needs_animated_icon_frame(&root),
        "invalid summary must walk new icon"
    );
    stamp_subtree_paint_hashes(&mut root, true);
    assert!(root.collect_skip_live);
    assert!(
        subtree_needs_animated_icon_frame(&root),
        "cached summary must retain icon path"
    );
}

#[test]
fn recipes_loaded() {
    assert!(animated_icon_names().count() >= 400);
    assert!(animated_icon_recipe("heart").is_some());
    assert!(animated_icon_recipe("check").is_some());
    assert!(animated_icon_recipe("menu").is_some());
    assert!(animated_icon_recipe("download").is_some());
}

#[test]
fn animated_icon_sets_text_color_attr_for_cascade() {
    let el: Element = animated_icon("heart").size_px(28.0).controlled(true).into();
    let fg = crate::style::parse_text_color("foreground");
    assert_eq!(el.text_style.color, Some(fg));
    assert!(
        el.text_style.color.is_some(),
        "explicit color must survive cascade (no parent white overwrite)"
    );
    assert_ne!(el.text_style.color_resolved(), [255, 255, 255, 255]);
}

#[test]
fn heart_scale_keyframes_emit() {
    let r = animated_icon_recipe("heart").unwrap();
    let doc0 = emit_animated_icon_svg(r, "animate", 0.15, "#000");
    assert!(doc0.contains("<svg"), "{doc0}");
    assert!(doc0.contains("scale("), "{doc0}");
    let doc1 = emit_animated_icon_svg(r, "normal", 0.0, "#000");
    assert!(doc1.contains("<path"), "{doc1}");
}

#[test]
fn settings_single_value_rotate_lerps_from_normal() {
    let r = animated_icon_recipe("settings").unwrap();
    // Bridging on: mid-tween between normal(0) and animate(180).
    let mid = emit_animated_icon_svg_ex(r, "animate", 0.2, "#000", true);
    assert!(mid.contains("rotate("), "{mid}");
    assert!(!mid.contains("rotate(0)"), "{mid}");
    // Idle (no bridge): must be identity, not stuck at animate end.
    let idle = emit_animated_icon_svg(r, "normal", 0.0, "#000");
    assert!(
        !idle.contains("rotate(90)") && !idle.contains("rotate(180)"),
        "idle must not show animate end pose: {idle}"
    );
}

#[test]
fn menu_and_copy_idle_not_animate_end() {
    let menu = animated_icon_recipe("menu").unwrap();
    let idle = emit_animated_icon_svg(menu, "normal", 0.0, "#000");
    // Three horizontal lines — no X (45/-45) transform at idle.
    assert!(!idle.contains("rotate(45)"), "{idle}");
    assert!(!idle.contains("rotate(-45)"), "{idle}");
    assert!(!idle.contains("translate(0 6)"), "{idle}");

    let copy = animated_icon_recipe("copy").unwrap();
    let idle_c = emit_animated_icon_svg(copy, "normal", 0.0, "#000");
    // Split offset is animate end; idle must not translate by ±3.
    assert!(!idle_c.contains("translate(-3"), "{idle_c}");
    assert!(!idle_c.contains("translate(3"), "{idle_c}");
}

#[test]
fn download_group_nests_polyline() {
    let r = animated_icon_recipe("download").unwrap();
    let g = r.nodes.iter().find(|n| n.tag == "g").expect("g node");
    assert!(
        g.children.iter().any(|c| c.tag == "polyline"),
        "arrow polyline must be nested under motion.g"
    );
    let doc = emit_animated_icon_svg(r, "animate", 0.3, "#222");
    assert!(doc.contains("<polyline"), "{doc}");
    assert!(doc.contains("<g "), "{doc}");
}

#[test]
fn menu_custom_variants_materialized() {
    let r = animated_icon_recipe("menu").unwrap();
    assert_eq!(r.nodes.len(), 3);
    let top = &r.nodes[0];
    let rot = top
        .tracks
        .iter()
        .find(|t| t.prop == "rotate")
        .expect("rotate");
    assert_eq!(
        rot.keyframes.get("animate").and_then(value_as_f32),
        Some(45.0)
    );
    let mid = &r.nodes[1];
    let op = mid
        .tracks
        .iter()
        .find(|t| t.prop == "opacity")
        .expect("opacity");
    assert_eq!(
        op.keyframes.get("animate").and_then(value_as_f32),
        Some(0.0)
    );
}

#[test]
fn check_path_length_dash() {
    let r = animated_icon_recipe("check").unwrap();
    let d = r.nodes[0].attrs.get("d").unwrap();
    let total = svg_path_d_length(d);
    assert!(
        total > 5.0,
        "check path should be longer than a few ux, got {total}"
    );
    let doc = emit_animated_icon_svg(r, "animate", 0.0, "#111");
    // t=0 → pathLength 0 → dash starts at 0 (hidden), scaled by real length.
    assert!(doc.contains("stroke-dasharray=\"0 "), "{doc}");
    assert!(
        !doc.contains("pathLength=\""),
        "resvg ignores pathLength; omit it: {doc}"
    );
}

#[test]
fn activity_mid_draws_half_path_in_user_units() {
    let r = animated_icon_recipe("activity").unwrap();
    let d = r.nodes[0].attrs.get("d").unwrap();
    let total = svg_path_d_length(d);
    let mid = emit_animated_icon_svg(r, "animate", 0.3, "#000");
    // At t=0.5: pl≈0.5, po≈0.5 → dash≈0.5L, gap≥L (anti-residue), offset≈-0.5L.
    let half = total * 0.5;
    assert!(
        mid.contains(&format!("stroke-dasharray=\"{half} {total}\"")),
        "expected dash={half} gap≥L ({total}), got {mid}"
    );
    assert!(
        mid.contains(&format!("stroke-dashoffset=\"-{half}\"")),
        "pathOffset must negate for Motion parity: {mid}"
    );
    assert!(!mid.contains("stroke-dasharray=\"0.5 1\""), "{mid}");
}

/// Activity path is authored right→left; Motion's pl↑ + po↓ must reveal left→right.
#[cfg(feature = "svg")]
#[test]
fn activity_draw_progresses_left_to_right() {
    use crate::media::svg_raster::rasterize_svg_str;
    let r = animated_icon_recipe("activity").unwrap();
    let size = 96u32;
    let ink_left_ratio = |elapsed: f32| -> f32 {
        let doc = emit_animated_icon_svg(r, "animate", elapsed, "rgb(0,0,0)");
        let rgba = rasterize_svg_str(&doc, size, size).expect("raster");
        let mut left = 0u32;
        let mut right = 0u32;
        let mid = size / 2;
        for y in 0..size {
            for x in 0..size {
                let i = ((y * size + x) * 4) as usize;
                if rgba[i + 3] > 20 {
                    if x < mid {
                        left += 1;
                    } else {
                        right += 1;
                    }
                }
            }
        }
        let t = (left + right).max(1) as f32;
        left as f32 / t
    };
    // Early: mostly left half. Late: more balanced / includes right.
    let early = ink_left_ratio(0.12);
    let late = ink_left_ratio(0.48);
    assert!(
        early > 0.7,
        "early draw should start on the left (ratio={early})"
    );
    assert!(
        late < early,
        "as path progresses, right side should gain ink (early={early} late={late})"
    );
}

#[test]
fn check_mid_reveals_partial_stroke() {
    let r = animated_icon_recipe("check").unwrap();
    let d = r.nodes[0].attrs.get("d").unwrap();
    let total = svg_path_d_length(d);
    let mid = emit_animated_icon_svg(r, "animate", 0.2, "#111");
    // pathLength ~0.5 at mid (easeOut may skew); dash must be a sizable fraction of L.
    let dash = mid
        .split("stroke-dasharray=\"")
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(0.0);
    assert!(
        dash > total * 0.15 && dash < total * 0.95,
        "mid dash {dash} should be partial of total {total}: {mid}"
    );
    assert!(mid.contains("scale("), "check scales while drawing: {mid}");
    // Anti-residue: gap floored to ≥ L so a second dash period cannot leak.
    let gap = mid
        .split("stroke-dasharray=\"")
        .nth(1)
        .and_then(|s| s.split(|c: char| c == '"' || c.is_whitespace()).nth(1))
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(-1.0);
    assert!(
        gap + 0.5 >= total,
        "gap {gap} should be ≥ total {total}: {mid}"
    );
}

#[test]
fn sun_map_paths_have_staggered_opacity() {
    let r = animated_icon_recipe("sun").unwrap();
    let rays: Vec<_> = r.nodes.iter().filter(|n| n.tag == "path").collect();
    assert!(rays.len() >= 8, "sun rays={}", rays.len());
    assert!(rays.iter().all(|n| n.attrs.get("d").is_some()));
    let delays: Vec<f32> = rays
        .iter()
        .filter_map(|n| {
            n.tracks
                .iter()
                .find(|t| t.prop == "opacity")
                .and_then(|t| t.transition.as_ref())
                .and_then(|tr| tr.delay)
        })
        .collect();
    assert!(delays.len() >= 8, "{delays:?}");
    assert!(delays.windows(2).all(|w| w[1] > w[0] - 1e-4), "{delays:?}");
    let mid = emit_animated_icon_svg(r, "animate", 0.25, "#000");
    assert!(mid.matches("<path").count() >= 8, "{mid}");
}

#[test]
fn wifi_levels_extracted_with_fade_sequence() {
    let r = animated_icon_recipe("wifi").unwrap();
    assert!(r.animated);
    assert_eq!(r.nodes.len(), 4);
    // Dot stays opaque; rings flash out then in.
    let ring = &r.nodes[1];
    let op = ring
        .tracks
        .iter()
        .find(|t| t.prop == "opacity")
        .expect("opacity");
    let anim = op.keyframes.get("animate").expect("animate");
    let series = keyframe_series(anim);
    assert!(series.len() >= 2, "{series:?}");
    assert!(series.iter().any(|v| *v < 0.5), "must dip: {series:?}");
    let doc = emit_animated_icon_svg(r, "animate", 0.2, "#111");
    assert!(doc.matches("<path").count() >= 4, "{doc}");
}

#[test]
fn battery_width_and_fill_animate() {
    let r = animated_icon_recipe("battery").unwrap();
    let fill = r
        .nodes
        .iter()
        .find(|n| n.tag == "rect" && n.attrs.get("fill").is_some())
        .expect("fill rect");
    assert!(fill.tracks.iter().any(|t| t.prop == "width"));
    let idle = emit_animated_icon_svg(r, "normal", 0.0, "#222");
    assert!(
        idle.contains("width=\"0\"") || idle.contains("opacity=\"0\""),
        "{idle}"
    );
    let end = emit_animated_icon_svg_ex(r, "animate", 0.4, "#222", true);
    assert!(
        end.contains("width=\"12\"") || end.contains("width=\"12"),
        "{end}"
    );
    assert!(
        end.contains("fill=\"#222\"") || end.contains("fill=\"rgb"),
        "{end}"
    );
}

#[test]
fn smile_and_align_and_grip_recipes_animate() {
    let smile = animated_icon_recipe("smile").unwrap();
    assert!(smile.animated);
    assert!(
        smile.root.tracks.iter().any(|t| t.prop == "scale"),
        "face scale on root"
    );
    assert!(
        smile
            .nodes
            .iter()
            .any(|n| n.tracks.iter().any(|t| t.prop == "d")),
        "mouth d morph track"
    );

    let align = animated_icon_recipe("align-left").unwrap();
    let mid = emit_animated_icon_svg_ex(align, "animate", 0.2, "#000", true);
    assert!(mid.contains("x2=\"1"), "x2 should tween toward 19: {mid}");

    let grip = animated_icon_recipe("grip").unwrap();
    assert_eq!(grip.nodes.len(), 9);
    let t0 = &grip.nodes[0].tracks[0];
    assert!(matches!(
        t0.keyframes.get("animate"),
        Some(serde_json::Value::Array(a)) if a.len() >= 3
    ));
    let blink = emit_animated_icon_svg(grip, "animate", 0.3, "#000");
    assert!(blink.matches("<circle").count() >= 9, "{blink}");

    let volume = animated_icon_recipe("volume").unwrap();
    assert!(volume.animated);
    assert!(volume.nodes.iter().any(|n| n.tag == "line"), "mute X lines");
    let v_idle = emit_animated_icon_svg(volume, "normal", 0.0, "#000");
    assert!(v_idle.contains("<line"), "{v_idle}");
    let v_anim = emit_animated_icon_svg(volume, "animate", 0.3, "#000");
    assert!(
        v_anim.contains("M16 9a5") || v_anim.contains("opacity="),
        "{v_anim}"
    );

    let keyboard = animated_icon_recipe("keyboard").unwrap();
    assert!(keyboard.animated);
    assert!(keyboard.nodes.len() >= 8);
    let k = emit_animated_icon_svg(keyboard, "animate", 0.4, "#000");
    assert!(k.matches("<path").count() >= 8, "{k}");
}

#[test]
fn check_draw_and_scale_at_mid() {
    let r = animated_icon_recipe("check").unwrap();
    let d = r.nodes[0].attrs.get("d").unwrap();
    let total = svg_path_d_length(d);
    let mid = emit_animated_icon_svg(r, "animate", 0.2, "#111");
    assert!(mid.contains("stroke-dasharray="), "{mid}");
    assert!(mid.contains("scale("), "check scales while drawing: {mid}");
    let idle = emit_animated_icon_svg(r, "normal", 0.0, "#111");
    // Idle pathLength=1 → dash = L; gap floored to ≥ L (still fully covered).
    assert!(
        idle.contains(&format!("stroke-dasharray=\"{total} {total}\"")),
        "idle should be solid full path: {idle}"
    );
}

#[cfg(feature = "svg")]
#[test]
fn resvg_anim_frame_nonempty() {
    let mut el: Element = animated_icon("bell").size_px(32.0).controlled(true).into();
    set_animated_icon_state(&mut el, "animate");
    let fg = crate::style::parse_text_color("foreground");
    let (tex, _) = ensure_animated_icon_texture(&el, 32.0, fg, 2.0).expect("tex");
    assert!(tex.starts_with("jadix_lucide_anim:"));
}

#[cfg(feature = "svg")]
#[test]
fn unchanged_animated_frame_reuses_resident_texture() {
    let el: Element = animated_icon("bell").size_px(31.0).into();
    let color = [13, 37, 91, 255];
    let (key, _) = ensure_animated_icon_texture(&el, 31.0, color, 1.0).unwrap();
    crate::media::svg_raster::flush_svg_textures(|_| false);
    assert!(crate::app::pending_images::take_pending_key(&key).is_some());
    assert_eq!(
        ensure_animated_icon_texture(&el, 31.0, color, 1.0)
            .unwrap()
            .0,
        key
    );
    crate::media::svg_raster::flush_svg_textures(|_| true);
    assert!(
        crate::app::pending_images::take_pending_key(&key).is_none(),
        "an unchanged resident frame must not rasterize/upload on every scroll"
    );
    ensure_animated_icon_texture(&el, 31.0, color, 1.0).unwrap();
    crate::media::svg_raster::flush_svg_textures(|_| false);
    assert!(
        crate::app::pending_images::take_pending_key(&key).is_some(),
        "GPU eviction must still rehydrate the frame"
    );
}

#[cfg(feature = "svg")]
#[test]
fn animated_frame_key_tracks_pixels_within_one_time_bucket() {
    let mut el: Element = animated_icon("bell").size_px(31.0).into();
    set_animated_icon_state(&mut el, "animate");
    let sample = |elapsed: f32| {
        play_store()
            .lock()
            .unwrap()
            .get_mut(&instance_key(&el))
            .unwrap()
            .elapsed = elapsed;
        ensure_animated_icon_texture(&el, 31.0, [13, 37, 91, 255], 1.0)
            .unwrap()
            .0
    };
    assert_ne!(
        sample(0.101),
        sample(0.102),
        "different SVG pixels in one 60Hz bucket must not alias in the raster cache"
    );
}
