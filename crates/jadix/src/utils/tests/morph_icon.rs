use super::*;

#[test]
fn stamped_pump_keeps_active_and_newly_mounted_icons() {
    use crate::layout::collect_cache::{
        invalidate_collect_paint_hash_path, stamp_subtree_paint_hashes,
    };
    let mut root = Element::new().child(Element::new().text("static"));
    stamp_subtree_paint_hashes(&mut root, false);
    assert!(!root.collect_skip_live);
    assert!(!tick_morph_icons_subtree(&mut root, 1.0 / 60.0));
    assert!(!subtree_needs_morph_icon_frame(&root));

    let mut icon: Element = morph_icon(IconInput::D("M0 12 L24 12".into()))
        .id("stamped-pump-mounted")
        .into();
    morph_to(&mut icon, IconInput::D("M12 0 L12 24".into()));
    root.children.push(icon);
    invalidate_collect_paint_hash_path(&mut root, &[1]);
    assert_eq!(
        tick_morph_icons_subtree(&mut root, 1.0 / 60.0),
        cfg!(feature = "animation-ex")
    );
    stamp_subtree_paint_hashes(&mut root, true);
    assert!(root.collect_skip_live);
    assert_eq!(
        subtree_needs_morph_icon_frame(&root),
        cfg!(feature = "animation-ex")
    );
    assert_eq!(
        tick_morph_icons_subtree(&mut root, 1.0 / 60.0),
        cfg!(feature = "animation-ex")
    );

    root.children.pop();
    root.collect_paint_hash = 0;
    stamp_subtree_paint_hashes(&mut root, true);
    assert!(!root.collect_skip_live);
    assert!(!tick_morph_icons_subtree(&mut root, 1.0 / 60.0));
    assert!(!subtree_needs_morph_icon_frame(&root));
}

#[test]
fn builder_sets_attrs() {
    let el: Element = morph_icon("menu").size_px(28.0).spring("bouncy").into();
    assert_eq!(
        el.get_attr(ATTR_MORPH_ICON).map(|s| s.as_str()),
        Some("menu")
    );
    assert_eq!(
        el.get_attr(ATTR_MORPH_SPRING).map(|s| s.as_str()),
        Some("bouncy")
    );
    assert!(el
        .template_id
        .as_deref()
        .unwrap()
        .starts_with("morph-icon:"));
}

#[cfg(feature = "gallery-tests")]
#[test]
fn morph_to_changes_target() {
    let mut el: Element = morph_icon("menu")
        .id("morph-to-target")
        .size_px(24.0)
        .into();
    morph_to(&mut el, "x");
    assert_eq!(el.get_attr(ATTR_MORPH_TO).map(|s| s.as_str()), Some("x"));
    assert_eq!(
        tick_morph_icon(&mut el, 1.0 / 60.0),
        cfg!(feature = "animation-ex")
    );
}

#[cfg(feature = "gallery-tests")]
#[test]
fn resvg_morph_frame_nonempty() {
    let el: Element = morph_icon("menu").size_px(32.0).into();
    let fg = crate::style::parse_text_color("foreground");
    let (tex, _) = ensure_morph_icon_texture(&el, 32.0, fg, 2.0).expect("tex");
    assert!(tex.starts_with("jadix_morph_icon:"));
}

#[cfg(feature = "gallery-tests")]
#[test]
fn unchanged_morph_frame_reuses_resident_texture() {
    let el: Element = morph_icon("menu").id("cache-regression").into();
    let color = [17, 41, 97, 255];
    let (key, _) = ensure_morph_icon_texture(&el, 29.0, color, 1.0).unwrap();
    crate::media::svg_raster::flush_svg_textures(|_| false);
    assert!(crate::app::pending_images::take_pending_key(&key).is_some());
    assert_eq!(
        ensure_morph_icon_texture(&el, 29.0, color, 1.0).unwrap().0,
        key
    );
    crate::media::svg_raster::flush_svg_textures(|_| true);
    assert!(
        crate::app::pending_images::take_pending_key(&key).is_none(),
        "an unchanged resident frame must not upload on every scroll"
    );
    let next = ensure_morph_icon_texture(&el, 29.0, [19, 43, 101, 255], 1.0)
        .unwrap()
        .0;
    assert_ne!(key, next, "paint changes must produce a new texture");
    crate::media::svg_raster::flush_svg_textures(|_| false);
    assert!(crate::app::pending_images::take_pending_key(&next).is_some());
}

#[cfg(feature = "gallery-tests")]
#[test]
#[cfg(feature = "animation-ex")]
fn stable_id_does_not_reset_play() {
    let mut el: Element = morph_icon("menu").id("keep-flight").into();
    morph_to(&mut el, "x");
    assert!(tick_morph_icon(&mut el, 1.0 / 60.0));
    let _rebuild: Element = morph_icon("menu").id("keep-flight").into();
    assert!(
        tick_morph_icon(&mut el, 1.0 / 60.0),
        "same template id must keep the in-flight spring"
    );
    assert!((morph_progress_by_id("keep-flight") - 1.0).abs() > 0.01);
}

#[test]
fn bound_progress_stroke_spring() {
    let mut el: Element = morph_icon("menu")
        .id("pg")
        .from_icon("menu")
        .to_icon("x")
        .into();
    assert_eq!(
        apply_bound_morph_stroke(&mut el, 2.5),
        LayoutStatus::StyleChanged
    );
    assert_eq!(
        el.get_attr(ATTR_MORPH_STROKE).map(|s| s.as_str()),
        Some("2.5")
    );
    assert_eq!(
        apply_bound_morph_spring(&mut el, "bouncy"),
        LayoutStatus::StyleChanged
    );
    assert_eq!(
        el.get_attr(ATTR_MORPH_SPRING).map(|s| s.as_str()),
        Some("bouncy")
    );
    assert_eq!(
        apply_bound_morph_progress(&mut el, 0.4),
        LayoutStatus::StyleChanged
    );
    assert_eq!(
        el.get_attr(ATTR_MORPH_PROGRESS).map(|s| s.as_str()),
        Some("0.4")
    );
    assert!((morph_progress_by_id("pg") - 0.4).abs() < 1e-3);
    assert!(!tick_morph_icon(&mut el, 1.0 / 60.0));
    assert_eq!(
        apply_bound_morph_progress(&mut el, -1.0),
        LayoutStatus::StyleChanged
    );
    assert!(el.get_attr(ATTR_MORPH_PROGRESS).is_none());
    assert!(
        is_controlled(&el),
        "progress < 0 must not drop a from&&to pair"
    );
    assert!(!tick_morph_icon(&mut el, 1.0 / 60.0));
}

#[test]
#[cfg(feature = "product-tests")]
fn button_keeps_morph_icon_child() {
    let morph: Element = morph_icon("pause").id("pp").into();
    let el: Element = crate::components::button("gal-pp").child(morph).into();
    fn has_morph(el: &Element) -> bool {
        el.attrs.contains_key(ATTR_MORPH_ICON) || el.children.iter().any(has_morph)
    }
    assert!(
        has_morph(&el),
        "Button must not flatten MorphIcon into a Lucide caption leaf"
    );

    let wrapped = Element::new().child(morph_icon("play").id("pp-wrap"));
    let el: Element = crate::components::button("gal-pp-wrap")
        .child(wrapped)
        .into();
    assert!(
        has_morph(&el),
        "Button must keep MorphIcon even when nested in a wrapper"
    );
}

#[test]
fn spring_cfg_parses_constants() {
    let a = spring_cfg("420,30");
    assert!((a.k - 420.0).abs() < 1e-3 && (a.c - 30.0).abs() < 1e-3);
    let b = spring_cfg("k:170,c:26");
    assert!((b.k - 170.0).abs() < 1e-3 && (b.c - 26.0).abs() < 1e-3);
    let c = spring_cfg("stiffness:300,damping:14");
    assert!((c.k - 300.0).abs() < 1e-3 && (c.c - 14.0).abs() < 1e-3);
    let d = spring_cfg("smooth");
    assert!((d.k - 170.0).abs() < 1e-3);
}

#[test]
fn morph_set_jumps_without_spring() {
    let mut el: Element = morph_icon("menu").id("set-jump").into();
    morph_set(&mut el, "x");
    assert_eq!(el.get_attr(ATTR_MORPH_TO).map(|s| s.as_str()), Some("x"));
    assert!(el.get_attr(ATTR_MORPH_PROGRESS).is_none());
    assert!(!tick_morph_icon(&mut el, 1.0 / 60.0));
    assert!((morph_progress_by_id("set-jump") - 1.0).abs() < 1e-3);
}

#[test]
fn reduced_always_morph_to_is_set() {
    let mut el: Element = morph_icon("menu")
        .id("rm-always")
        .reduced_motion("always")
        .into();
    morph_to(&mut el, "x");
    assert!(!tick_morph_icon(&mut el, 1.0 / 60.0));
    assert!((morph_progress_by_id("rm-always") - 1.0).abs() < 1e-3);
}

#[test]
fn builder_icon_input_and_custom_spring() {
    let input = IconInput::D("M0 12 L24 12".into());
    let el: Element = morph_icon(input)
        .id("ii")
        .spring_constants(200.0, 22.0)
        .interp("linear")
        .into();
    assert_eq!(
        el.get_attr(ATTR_MORPH_SPRING).map(|s| s.as_str()),
        Some("200,22")
    );
    assert_eq!(
        el.get_attr(ATTR_MORPH_INTERP).map(|s| s.as_str()),
        Some("linear")
    );
    let cfg = spring_cfg(el.get_attr(ATTR_MORPH_SPRING).unwrap());
    assert!((cfg.k - 200.0).abs() < 1e-3 && (cfg.c - 22.0).abs() < 1e-3);
}

#[cfg(feature = "gallery-tests")]
#[test]
fn controlled_pair_rebases_and_exit_invalidates() {
    let mut el: Element = morph_icon("menu")
        .id("rebase")
        .from_icon("menu")
        .to_icon("x")
        .progress(0.4)
        .into();
    let d_menu_x = current_d(&el);
    assert!(!d_menu_x.is_empty());
    assert_eq!(
        apply_bound_morph_from(&mut el, "plus"),
        LayoutStatus::StyleChanged
    );
    assert_eq!(
        apply_bound_morph_to(&mut el, "minus"),
        LayoutStatus::StyleChanged
    );
    let d_plus_minus = current_d(&el);
    assert_ne!(
        d_menu_x, d_plus_minus,
        "pair change at mid t must rebase onto the new from"
    );
    assert_eq!(
        apply_bound_morph_icon(&mut el, "circle"),
        LayoutStatus::Clean
    );
    assert_eq!(
        el.get_attr(ATTR_MORPH_TO).map(|s| s.as_str()),
        Some("minus"),
        "icon bind ignored while controlled"
    );
    assert_eq!(
        apply_bound_morph_progress(&mut el, -1.0),
        LayoutStatus::StyleChanged
    );
    assert!(el.get_attr(ATTR_MORPH_PROGRESS).is_none());
    let key = instance_key(&el);
    if let Ok(map) = play_store().lock() {
        let play = map.get(&key).expect("play");
        assert!(play.controlled, "dropping progress keeps the from&&to pair");
        assert_eq!(play.pair_from.as_deref(), Some("plus"));
    }
    assert_eq!(
        apply_bound_morph_from(&mut el, ""),
        LayoutStatus::StyleChanged
    );
    assert!(!is_controlled(&el));
    let key = instance_key(&el);
    if let Ok(map) = play_store().lock() {
        let play = map.get(&key).expect("play");
        assert!(!play.based);
        assert!(play.pair_from.is_none());
    }
    assert_eq!(
        apply_bound_morph_from(&mut el, "plus"),
        LayoutStatus::StyleChanged
    );
    assert_eq!(
        apply_bound_morph_to(&mut el, "minus"),
        LayoutStatus::StyleChanged
    );
    assert_eq!(
        apply_bound_morph_progress(&mut el, 0.5),
        LayoutStatus::StyleChanged
    );
    if let Ok(map) = play_store().lock() {
        let play = map.get(&key).expect("play");
        assert!(play.based);
        assert_eq!(play.pair_from.as_deref(), Some("plus"));
        assert_eq!(play.pair_to.as_deref(), Some("minus"));
    }
}

#[cfg(feature = "gallery-tests")]
#[test]
fn from_and_to_without_progress_is_controlled() {
    let mut el: Element = morph_icon("menu")
        .id("pair-only")
        .from_icon("menu")
        .to_icon("x")
        .into();
    assert!(is_controlled(&el));
    assert!(!tick_morph_icon(&mut el, 1.0 / 60.0));
    assert!((morph_progress(&el) - 0.0).abs() < 1e-3);
    let from_d = rest_d_for("menu");
    assert_eq!(current_d(&el), from_d);
}

#[cfg(feature = "gallery-tests")]
#[test]
fn morph_handle_seek_and_into_morph_src() {
    let mut el: Element = morph_icon("menu").id("morph-handle-seek").into();
    let input = IconInput::from("x");
    {
        let mut h = morph_handle(&mut el);
        h.seek(input, 0.35);
        assert!((h.progress() - 0.35).abs() < 1e-3);
    }
    assert!(!tick_morph_icon(&mut el, 1.0 / 60.0));
    let mut el: Element = morph_icon("menu").id("morph-handle-to").into();
    morph_to(&mut el, IconInput::from("x"));
    assert_eq!(el.get_attr(ATTR_MORPH_ICON).map(|s| s.as_str()), Some("x"));
    assert_eq!(
        tick_morph_icon(&mut el, 1.0 / 60.0),
        cfg!(feature = "animation-ex")
    );
}

#[test]
fn fit_icon_is_public() {
    let input = IconInput::D("M0 0 L32 0".into());
    let d = crate::utils::fit_icon(&input, "0 0 32 32", 24.0).expect("fit");
    assert!(d.starts_with('M'), "{d}");
}

#[cfg(feature = "gallery-tests")]
#[test]
fn polar_and_linear_mid_frames_differ() {
    let polar: Element = morph_icon("menu")
        .id("interp-p")
        .from_icon("menu")
        .to_icon("x")
        .progress(0.5)
        .interp("polar")
        .into();
    let linear: Element = morph_icon("menu")
        .id("interp-l")
        .from_icon("menu")
        .to_icon("x")
        .progress(0.5)
        .interp("linear")
        .into();
    assert_ne!(current_d(&polar), current_d(&linear));
}

fn play_present(id: &str) -> bool {
    let key = key_for_template(&morph_template_id(id));
    play_exists(key)
}

#[cfg(feature = "gallery-tests")]
#[test]
fn morph_to_with_overrides_instance_spring() {
    let mut el: Element = morph_icon("menu").id("spring-once").spring("snappy").into();
    morph_to_with(&mut el, "x", "bouncy");
    let key = instance_key(&el);
    let (k, c) = play_store()
        .lock()
        .ok()
        .and_then(|m| m.get(&key).map(|p| (p.k, p.c)))
        .expect("play");
    assert!((k - 300.0).abs() < 1e-3 && (c - 14.0).abs() < 1e-3);
    assert_eq!(
        el.get_attr(ATTR_MORPH_SPRING).map(String::as_str),
        Some("snappy"),
        "per-call spring must not rewrite the instance attr"
    );
}

#[test]
fn destroy_and_drop_clear_play_store() {
    let el: Element = morph_icon("menu").id("destroy-me").into();
    assert!(play_present("destroy-me"));
    morph_destroy(&el);
    assert!(!play_present("destroy-me"));
    drop(el);

    let el: Element = morph_icon("menu").id("unmount-me").into();
    assert!(play_present("unmount-me"));
    drop(el);
    assert!(
        !play_present("unmount-me"),
        "last Element drop must release the play slot"
    );
}

#[cfg(feature = "gallery-tests")]
#[test]
fn rebuild_keeps_play_until_last_drop() {
    let mut live: Element = morph_icon("menu").id("keep-refs").into();
    morph_to(&mut live, "x");
    {
        let _rebuild: Element = morph_icon("menu").id("keep-refs").into();
        assert!(play_present("keep-refs"));
    }
    assert!(
        play_present("keep-refs"),
        "temporary rebuild must not drop the live instance"
    );
    drop(live);
    assert!(!play_present("keep-refs"));
}

#[cfg(feature = "gallery-tests")]
#[test]
fn lazy_mount_first_icon_does_not_fly() {
    let mut el: Element = morph_icon("").id("lazy-driver").into();
    assert!(el.get_attr(ATTR_MORPH_ICON).is_none());
    morph_to(&mut el, "menu");
    assert_eq!(
        el.get_attr(ATTR_MORPH_ICON).map(String::as_str),
        Some("menu")
    );
    assert!(
        !tick_morph_icon(&mut el, 1.0 / 60.0),
        "first icon on a lazy mount paints, no spring"
    );
    morph_to(&mut el, "x");
    assert_eq!(
        tick_morph_icon(&mut el, 1.0 / 60.0),
        cfg!(feature = "animation-ex"),
        "only enhanced animation plays the second morph_to"
    );
}

#[test]
fn absolute_stroke_width_locks_pixel_width() {
    let rel: Element = morph_icon("menu").id("sw-rel").size_px(48.0).into();
    let abs: Element = morph_icon("menu")
        .id("sw-abs")
        .size_px(48.0)
        .absolute_stroke_width(true)
        .into();
    assert!((stroke_for_paint(&rel, 48.0) - 2.0).abs() < 1e-3);
    assert!((stroke_for_paint(&abs, 48.0) - 1.0).abs() < 1e-3);
}

#[test]
fn view_box_fits_off_grid_d() {
    let el: Element = morph_icon(IconInput::D("M0 0 L32 0".into()))
        .id("vb-fit")
        .view_box("0 0 32 32")
        .into();
    let d = current_d(&el);
    assert!(d.starts_with('M'), "{d}");
    let fitted =
        crate::utils::fit_icon(&IconInput::D("M0 0 L32 0".into()), "0 0 32 32", 24.0).unwrap();
    assert_eq!(d, fitted);
}

#[test]
fn decorative_by_default_labeled_is_image() {
    let quiet: Element = morph_icon("menu").id("a11y-quiet").into();
    assert_eq!(
        quiet.attrs.get("aria-hidden").map(String::as_str),
        Some("true")
    );
    assert!(!quiet.attrs.contains_key("accesskit_role"));
    let named: Element = morph_icon("menu").id("a11y-named").label("Menu").into();
    assert!(!named.attrs.contains_key("aria-hidden"));
    assert_eq!(
        named.attrs.get("accesskit_role").map(String::as_str),
        Some("image")
    );
    assert_eq!(
        named.attrs.get("accesskit_name").map(String::as_str),
        Some("Menu")
    );
}
