//! Input clearable + password_toggle adornments.

use crate::app::damage::{expand_paint_dirties_for_translated_slots, take_frame_paint_nodes};
use crate::app::draw_cull::node_present_dest_reset_full;
use crate::components::input::{
    focus_id_for_input, input, input_password_reveal_signal_key, input_value_signal_key, InputType,
};
use crate::components::internal::text_edit::{
    push_focused_text_edit_caret_damage, sync_text_edit_paint_subtree,
};
use crate::core::reactivity::use_signal;
use crate::core::tree::{Element, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::renderer::font_loader;
use crate::text::install_shared_font_system;

fn find_text<'a>(el: &'a Element, tid: &str) -> Option<&'a str> {
    if el.template_id.as_deref() == Some(tid) {
        return el.text.as_deref();
    }
    for c in &el.children {
        if let Some(t) = find_text(c, tid) {
            return Some(t);
        }
    }
    None
}

#[test]
fn clearable_shows_clear_when_nonempty_and_clears_value() {
    let id = "diag-input-clear";
    use_signal(&input_value_signal_key(id), String::new).set("hello".into());
    use_signal("__rui_button_focus_id", String::new).set(String::new());

    let mut el: Element = input(id).clearable(true).into();
    let clear_tid = format!("__rui_input:{id}:clear");
    let clear = el
        .children
        .iter()
        .find(|c| c.template_id.as_deref() == Some(clear_tid.as_str()))
        .expect("clear adornment present");
    assert!(
        (clear.style.opacity - 1.0).abs() < 0.001,
        "unfocused nonempty must show clear"
    );
    clear.on_click.as_ref().unwrap().borrow_mut()();
    assert_eq!(
        use_signal(&input_value_signal_key(id), String::new).peek(),
        ""
    );
    assert_eq!(
        use_signal("__rui_button_focus_id", String::new).peek(),
        focus_id_for_input(id),
        "clear click must focus the field"
    );

    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut el, &mut tree, &mut impact);
}

#[test]
fn password_toggle_reveals_plaintext() {
    let id = "diag-input-pw-toggle";
    use_signal(&input_value_signal_key(id), String::new).set("secret".into());
    use_signal("__rui_button_focus_id", String::new)
        .set(crate::components::input::focus_id_for_input(id));

    let mut el: Element = input(id)
        .input_type(InputType::Password)
        .password_toggle(true)
        .into();

    let text_tid = format!("__rui_input:{id}:text");
    let display = find_text(&el, &text_tid).unwrap().to_string();
    assert_eq!(display.chars().count(), 6);
    assert!(display.chars().all(|c| c == '•'), "masked: {display}");

    let reveal_tid = format!("__rui_input:{id}:reveal");
    let reveal = el
        .children
        .iter()
        .find(|c| c.template_id.as_deref() == Some(reveal_tid.as_str()))
        .expect("reveal btn");
    reveal.on_click.as_ref().unwrap().borrow_mut()();
    assert!(use_signal(&input_password_reveal_signal_key(id), || false).peek());

    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut el, &mut tree, &mut impact);

    let display = find_text(&el, &text_tid).unwrap().to_string();
    assert_eq!(display, "secret");
}

#[test]
fn adornment_slots_keep_stable_width_when_hidden() {
    let id = "diag-input-slot-stable";
    use_signal(&input_value_signal_key(id), String::new).set("ab".into());
    use_signal("__rui_button_focus_id", String::new).set(String::new()); // unfocused

    let mut el: Element = input(id)
        .input_type(InputType::Password)
        .password_toggle(true)
        .clearable(true)
        .into();
    let clear_tid = format!("__rui_input:{id}:clear");
    let reveal_tid = format!("__rui_input:{id}:reveal");
    let clear_w = el
        .children
        .iter()
        .find(|c| c.template_id.as_deref() == Some(clear_tid.as_str()))
        .unwrap()
        .style
        .size
        .width;
    let eye_w = el
        .children
        .iter()
        .find(|c| c.template_id.as_deref() == Some(reveal_tid.as_str()))
        .unwrap()
        .style
        .size
        .width;
    assert_eq!(clear_w, eye_w);

    use_signal("__rui_button_focus_id", String::new)
        .set(crate::components::input::focus_id_for_input(id));
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut el, &mut tree, &mut impact);
    let clear_w2 = el
        .children
        .iter()
        .find(|c| c.template_id.as_deref() == Some(clear_tid.as_str()))
        .unwrap()
        .style
        .size
        .width;
    assert_eq!(
        clear_w, clear_w2,
        "focus must not change adornment slot width"
    );
}

#[test]
fn masked_hit_index_maps_to_real_caret() {
    use crate::components::input::map_masked_byte_index_to_real;
    let real = "secret";
    // 3rd bullet starts at byte 6 in "••••••" (each • is 3 bytes)
    assert_eq!(map_masked_byte_index_to_real(real, 6), 2);
    assert_eq!(map_masked_byte_index_to_real(real, 18), 6);
}

fn adorn_opacity(el: &Element, tid: &str) -> Option<f32> {
    el.children
        .iter()
        .find(|c| c.template_id.as_deref() == Some(tid))
        .map(|c| c.style.opacity)
}

#[test]
fn nonempty_makes_clear_visible_even_when_unfocused() {
    let id = "diag-adorn-opacity";
    use_signal(&input_value_signal_key(id), String::new).set("secret".into());
    use_signal("__rui_button_focus_id", String::new).set(String::new());

    let mut el: Element = input(id)
        .input_type(InputType::Password)
        .password_toggle(true)
        .clearable(true)
        .into();
    let clear_tid = format!("__rui_input:{id}:clear");
    let reveal_tid = format!("__rui_input:{id}:reveal");
    assert_eq!(
        adorn_opacity(&el, &clear_tid),
        Some(1.0),
        "nonempty clear must be clickable while unfocused"
    );
    assert_eq!(
        adorn_opacity(&el, &reveal_tid),
        Some(0.0),
        "eye stays focused-only"
    );

    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(id));
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut el, &mut tree, &mut impact);
    assert_eq!(
        adorn_opacity(&el, &clear_tid),
        Some(1.0),
        "focused+nonempty clear must stay opaque"
    );
    assert_eq!(
        adorn_opacity(&el, &reveal_tid),
        Some(1.0),
        "focused eye must be opaque"
    );

    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let mut root = Element::new().w(400.0).h(80.0).child(el);
    let result = tree.build_and_layout(&mut root, 400.0, 80.0, [-1.0, -1.0]);
    let lucide = result
        .text_nodes
        .iter()
        .filter(|n| n.style.font_family.as_deref() == Some("lucide"))
        .count();
    assert!(
        lucide >= 2 || result.textured_draws.len() >= 2,
        "expected clear+eye as lucide text or SVG textures, lucide={lucide} textured={}",
        result.textured_draws.len()
    );
}

#[test]
fn string_bool_props_mount_adornments_like_rsx() {
    use crate::components::{Input, InputProps};
    use crate::core::tree::parse_bool_like;

    let id = "diag-str-bool-adorn";
    let el = Input(InputProps {
        id: id.into(),
        placeholder: None,
        disabled: None,
        invalid: None,
        clearable: Some(parse_bool_like("true")),
        password_toggle: Some(parse_bool_like("true")),
        number_steppers: None,
        input_type: Some("password".into()),
        step: None,
        value: None,
        truncate: None,
        text_overflow: None,
        undo_enabled: None,
        max_undo_entries: None,
        max_undo_memory: None,
        typing_merge_timeout_ms: None,
        on_click: None,
        on_key_down: None,
        children: None,
        base: None,
    });
    let clear_tid = format!("__rui_input:{id}:clear");
    let reveal_tid = format!("__rui_input:{id}:reveal");
    assert!(
        el.children
            .iter()
            .any(|c| c.template_id.as_deref() == Some(clear_tid.as_str())),
        "clearable=\"true\" path must mount clear; tids={:?}",
        el.children
            .iter()
            .map(|c| c.template_id.clone())
            .collect::<Vec<_>>()
    );
    assert!(el
        .children
        .iter()
        .any(|c| c.template_id.as_deref() == Some(reveal_tid.as_str())));
}

fn find_tid<'a>(el: &'a Element, tid: &str) -> Option<&'a Element> {
    if el.template_id.as_deref() == Some(tid) {
        return Some(el);
    }
    el.children.iter().find_map(|c| find_tid(c, tid))
}

fn rect_right(r: [f32; 4]) -> f32 {
    r[0] + r[2]
}

fn rect_contains_point(r: [f32; 4], x: f32, y: f32) -> bool {
    x >= r[0] && y >= r[1] && x < r[0] + r[2] && y < r[1] + r[3]
}

/// Long CJK + caret at the fold must not paint over the clear glyph, and the
/// I-beam must stay inside the padded content (not on the 1px rounded stroke).
#[test]
fn long_value_must_not_cover_clear_or_paint_caret_on_border() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "diag-input-clear-clip";
    let value: String = "水".repeat(24);
    use_signal(&input_value_signal_key(id), String::new).set(value.clone());
    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(id));
    use_signal(
        &crate::components::input::input_caret_signal_key(id),
        || None::<usize>,
    )
    .set(Some(value.len()));

    let el: Element = input(id).clearable(true).w(240.0).into();
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;

    let mut root = Element::new()
        .w(280.0)
        .h(80.0)
        .p(8.0)
        .bg([0.96, 0.96, 0.97, 1.0])
        .child(el);
    // Viewport must exist before paint sync: otherwise `scroll_x` stays 0, the
    // end caret sits past the leaf clip, and collect emits no I-beam.
    let _ = tree.build_and_layout(&mut root, 280.0, 80.0, [-1.0, -1.0]);
    sync_text_edit_paint_subtree(&mut root, &mut tree, &mut impact);
    let result = tree.build_and_layout(&mut root, 280.0, 80.0, [-1.0, -1.0]);

    let field = find_tid(&root, id).expect("input chrome");
    let text_tid = format!("__rui_input:{id}:text");
    let clear_tid = format!("__rui_input:{id}:clear");
    let text_el = find_tid(&root, &text_tid).expect("text leaf");
    let clear_el = find_tid(&root, &clear_tid).expect("clear button");

    let field_b = field.layout_bounds;
    let text_b = text_el.layout_bounds;
    let clear_b = clear_el.layout_bounds;
    let border_w = field.style.border_width.max(1.0);

    assert!(
        (clear_el.style.opacity - 1.0).abs() < 0.01,
        "nonempty clear must be opaque, opacity={}",
        clear_el.style.opacity
    );
    assert!(
        clear_b[2] > 8.0 && clear_b[3] > 8.0,
        "clear slot crushed: {clear_b:?}"
    );
    assert!(
        rect_right(text_b) <= clear_b[0] + 1.5,
        "text leaf must stop before the clear slot\n text={text_b:?} clear={clear_b:?} field={field_b:?}"
    );

    let value_node = result
        .text_nodes
        .iter()
        .find(|n| n.template_id.as_deref() == Some(text_tid.as_str()))
        .expect("value text node");
    let clip = value_node.clip_bounds.expect("input text must clip");
    let clear_cx = clear_b[0] + clear_b[2] * 0.5;
    let clear_cy = clear_b[1] + clear_b[3] * 0.5;
    assert!(
        !rect_contains_point(clip, clear_cx, clear_cy),
        "value clip must not cover the clear glyph\n clip={clip:?} clear={clear_b:?}"
    );

    let icon_name = clear_el
        .children
        .first()
        .and_then(|c| c.get_attr(crate::utils::ATTR_ICON_NAME).map(String::as_str));
    assert_eq!(
        icon_name,
        Some("x"),
        "clear glyph must carry rui_icon_name for SVG texture lookup"
    );
    let lucide_ok = result.text_nodes.iter().any(|n| {
        n.style.font_family.as_deref() == Some("lucide")
            && n.width > 0.5
            && n.height > 0.5
            && n.style.color_resolved()[3] > 0
            && rect_contains_point(
                [n.x - 2.0, n.y - 2.0, n.width + 4.0, n.height + 4.0],
                clear_cx,
                clear_cy,
            )
    }) || result.textured_draws.iter().any(|d| {
        d.rect[2] > 0.5 && d.rect[3] > 0.5 && rect_contains_point(d.rect, clear_cx, clear_cy)
    });
    assert!(
        lucide_ok,
        "clear X must paint as lucide text or named SVG texture at {clear_b:?}; lucide={:?} textured={}",
        result
            .text_nodes
            .iter()
            .filter(|n| n.style.font_family.as_deref() == Some("lucide"))
            .map(|n| (n.x, n.y, n.width, n.height, n.text.clone()))
            .collect::<Vec<_>>(),
        result.textured_draws.len()
    );

    assert!(
        !result.input_chrome_instances.is_empty(),
        "focused long CJK must collect an I-beam after post-layout paint sync"
    );
    let inner_right = rect_right(field_b) - border_w;
    for caret in &result.input_chrome_instances {
        let caret_r = caret.rect_pos[0] + caret.rect_size[0];
        assert!(
            caret_r <= inner_right - 0.5,
            "caret must not sit on the chrome stroke\n caret_x={}..{} inner_right={} field={field_b:?} clip={clip:?}",
            caret.rect_pos[0],
            caret_r,
            inner_right
        );
        if caret.clip_size[0] >= 0.0 {
            let clip_r = caret.clip_pos[0] + caret.clip_size[0];
            assert!(
                clip_r <= inner_right + 0.5,
                "caret clip must not include the 1px border\n clip_r={clip_r} inner_right={inner_right}"
            );
        }
    }

    let _ = take_frame_paint_nodes();
    push_focused_text_edit_caret_damage(&root);
    let dirties = take_frame_paint_nodes();
    assert!(
        !dirties.is_empty(),
        "focused caret blink must dirty the I-beam"
    );
    let stroke_x = rect_right(field_b) - border_w * 0.5;
    let stroke_y = field_b[1] + field_b[3] * 0.5;
    let scaled_tex = crate::renderer::scale_textured_draws(&result.textured_draws, 1.0);
    let prev_slots = crate::app::draw_cull::last_present_visual_slots(
        &result.node_paints,
        &result.instances,
        &scaled_tex,
        &result.text_nodes,
        &result.input_chrome_instances,
    );
    let off_slots = crate::app::draw_cull::last_present_visual_slots(
        &result.node_paints,
        &result.instances,
        &scaled_tex,
        &result.text_nodes,
        &[],
    );
    let mut live_dirties = dirties.clone();
    expand_paint_dirties_for_translated_slots(&mut live_dirties, &off_slots, &prev_slots);
    expand_paint_dirties_for_translated_slots(&mut live_dirties, &prev_slots, &off_slots);
    let mut saw_overlay = false;
    for d in &live_dirties {
        if crate::app::draw_cull::paint_dirty_is_caret_overlay(
            d,
            &result.node_paints,
            &result.instances,
        ) {
            saw_overlay = true;
            assert!(
                d.bounds[2] < text_b[2] * 0.25,
                "blink dirty must be the I-beam, not the flex-grown text leaf\n dirty={:?} text={text_b:?}",
                d.bounds
            );
            continue;
        }
        let prev = prev_slots
            .iter()
            .find(|(k, _)| *k == d.key)
            .map(|(_, b)| *b);
        let Some(reset) = node_present_dest_reset_full(
            d.key,
            d.bounds,
            &result.node_paints,
            &result.instances,
            &scaled_tex,
            &result.input_chrome_instances,
            1.0,
            prev,
        ) else {
            continue;
        };
        let wipe = reset.rect;
        assert!(
            !rect_contains_point(wipe, stroke_x, stroke_y),
            "caret dest-reset must not punch the chrome stroke\n wipe={wipe:?} stroke=({stroke_x},{stroke_y}) dirty={:?} field={field_b:?} prev={prev:?}",
            d.bounds
        );
        assert!(
            !rect_contains_point(wipe, clear_cx, clear_cy),
            "caret dest-reset must not erase the clear X\n wipe={wipe:?} clear={clear_b:?} dirty={:?} prev={prev:?}",
            d.bounds
        );
    }
    assert!(
        saw_overlay,
        "focused caret blink must be a swapchain overlay (no dest-reset of the I-beam)"
    );
}

fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
    if el.attrs.get("data-slot").map(String::as_str) == Some(slot) {
        return Some(el);
    }
    el.children.iter().find_map(|c| find_slot(c, slot))
}

/// Gallery 2b: the 2.25rem group shell is the click/focus target. A nested
/// auto-height row + `h:100%` control used to leave a dead band, so only the
/// glyph strip focused and the group ring looked nested inside the field.
#[test]
fn single_line_adornment_control_fills_shell_and_takes_focus() {
    use crate::app::test_support::sync_reactive_elements_subtree;
    use crate::app::tree_hit::{element_self_pointer_hit, find_focus_id_at};
    use crate::components::input_adornment::input_adornment;
    use crate::style::{focus_ring_color, focus_ring_shadow_matches_color};

    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "diag-ia-sl-focus";
    use_signal(&input_value_signal_key(id), String::new).set(String::new());
    use_signal("__rui_button_focus_id", String::new).set(String::new());

    let mut el: Element = input_adornment()
        .addon("inline-start", Element::new().text("https://"))
        .child(input(id).placeholder("example.com").plain_chrome(true))
        .addon("inline-end", Element::new().text("USD"))
        .into();

    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut el, &mut tree, &mut impact);

    let mut root = Element::new().w(400.0).h(80.0).p(8.0).child(el);
    let _ = tree.build_and_layout(&mut root, 400.0, 80.0, [-1.0, -1.0]);

    let shell = find_slot(&root, "input-adornment").expect("adornment shell");
    let control = find_tid(&root, id).expect("plain_chrome input");
    let shell_b = shell.layout_bounds;
    let control_b = control.layout_bounds;
    let border = shell.style.border_width.max(1.0);

    assert!(
        shell_b[3] >= 32.0,
        "shell must be ~2.25rem tall, got {shell_b:?}"
    );
    assert!(
        (control_b[3] - (shell_b[3] - 2.0 * border)).abs() <= 1.5,
        "control must fill the shell content box\n control={control_b:?} shell={shell_b:?} border={border}"
    );
    assert!(
        (control_b[1] - (shell_b[1] + border)).abs() <= 1.5,
        "control must start at the shell content top\n control={control_b:?} shell={shell_b:?}"
    );

    let fid = focus_id_for_input(id);
    let x = control_b[0] + control_b[2] * 0.5;
    let y_top = shell_b[1] + border + 3.0;
    let y_bot = shell_b[1] + shell_b[3] - border - 3.0;
    let y_mid = shell_b[1] + shell_b[3] * 0.5;
    assert_eq!(
        find_focus_id_at(&root, x, y_top, [0.0, 0.0]).as_deref(),
        Some(fid.as_str()),
        "click near shell top must focus the field\n shell={shell_b:?} control={control_b:?}"
    );
    assert_eq!(
        find_focus_id_at(&root, x, y_bot, [0.0, 0.0]).as_deref(),
        Some(fid.as_str()),
        "click near shell bottom must focus the field\n shell={shell_b:?} control={control_b:?}"
    );
    assert_eq!(
        find_focus_id_at(&root, shell_b[0] + 16.0, y_mid, [0.0, 0.0]).as_deref(),
        Some(fid.as_str()),
        "click on the start addon must still resolve to the field"
    );
    assert!(
        element_self_pointer_hit(control, x, y_top, [0.0, 0.0]),
        "control border box must include the shell interior, control={control_b:?} pt=({x},{y_top})"
    );
    assert!(
        !control.focus_ring,
        "plain_chrome control must opt out of the shared ring"
    );

    use_signal("__rui_button_focus_id", String::new).set(fid);
    impact = LayoutStatus::Clean;
    sync_reactive_elements_subtree(&mut root, &mut tree, &mut impact);
    sync_text_edit_paint_subtree(&mut root, &mut tree, &mut impact);

    let shell = find_slot(&root, "input-adornment").expect("shell after focus");
    let control = find_tid(&root, id).expect("control after focus");
    assert!(
        focus_ring_shadow_matches_color(shell, true, focus_ring_color()),
        "group shell must own the focus ring, blur={} spread={} color={:?}",
        shell.style.shadow_blur,
        shell.style.shadow_spread,
        shell.style.shadow_color
    );
    let inner_ring = control.style.shadow_blur > 0.01 || control.style.shadow_spread.abs() > 0.01;
    assert!(
        !inner_ring,
        "plain_chrome control must not paint its own ring, blur={} spread={}",
        control.style.shadow_blur, control.style.shadow_spread
    );
}

/// Gallery forms: wrap row + `flex_grow` + short value (screenshot `2222`).
#[test]
fn gallery_flex_grow_short_value_clear_and_caret_stroke() {
    use crate::primitives::r#box::hstack;

    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "gal-s2-input-a";
    let value = "2222".to_string();
    use_signal(&input_value_signal_key(id), String::new).set(value.clone());
    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(id));
    use_signal(
        &crate::components::input::input_caret_signal_key(id),
        || None::<usize>,
    )
    .set(Some(value.len()));

    let a: Element = input(id).clearable(true).flex_grow(1.0).into();
    let b: Element = input("gal-s2-input-b")
        .clearable(true)
        .flex_grow(1.0)
        .into();
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;

    let mut root = Element::new()
        .w(480.0)
        .h(80.0)
        .p(8.0)
        .bg([0.96, 0.96, 0.97, 1.0])
        .child(
            hstack()
                .w("100%")
                .gap("0.5rem")
                .wrap("wrap")
                .child(a)
                .child(b),
        );
    let _ = tree.build_and_layout(&mut root, 480.0, 80.0, [-1.0, -1.0]);
    sync_text_edit_paint_subtree(&mut root, &mut tree, &mut impact);
    let result = tree.build_and_layout(&mut root, 480.0, 80.0, [-1.0, -1.0]);

    let field = find_tid(&root, id).expect("input chrome");
    let text_tid = format!("__rui_input:{id}:text");
    let clear_tid = format!("__rui_input:{id}:clear");
    let text_el = find_tid(&root, &text_tid).expect("text leaf");
    let clear_el = find_tid(&root, &clear_tid).expect("clear button");
    let field_b = field.layout_bounds;
    let text_b = text_el.layout_bounds;
    let clear_b = clear_el.layout_bounds;
    let border_w = field.style.border_width.max(1.0);
    let clear_cx = clear_b[0] + clear_b[2] * 0.5;
    let clear_cy = clear_b[1] + clear_b[3] * 0.5;

    assert!(
        (clear_el.style.opacity - 1.0).abs() < 0.01,
        "nonempty clear must be opaque"
    );
    assert_eq!(
        clear_el
            .children
            .first()
            .and_then(|c| c.get_attr(crate::utils::ATTR_ICON_NAME).map(String::as_str)),
        Some("x")
    );
    assert!(
        rect_right(text_b) <= clear_b[0] + 1.5,
        "text leaf must stop before the clear slot\n text={text_b:?} clear={clear_b:?}"
    );

    let lucide_ok = result.text_nodes.iter().any(|n| {
        n.style.font_family.as_deref() == Some("lucide")
            && n.width > 0.5
            && n.height > 0.5
            && n.style.color_resolved()[3] > 0
            && rect_contains_point(
                [n.x - 2.0, n.y - 2.0, n.width + 4.0, n.height + 4.0],
                clear_cx,
                clear_cy,
            )
    }) || result.textured_draws.iter().any(|d| {
        d.rect[2] > 0.5
            && d.rect[3] > 0.5
            && d.opacity > 0.01
            && rect_contains_point(d.rect, clear_cx, clear_cy)
    });
    assert!(
        lucide_ok,
        "clear X must paint at the slot\n clear={clear_b:?} lucide={:?} textured={:?}",
        result
            .text_nodes
            .iter()
            .filter(|n| n.style.font_family.as_deref() == Some("lucide"))
            .map(|n| (n.x, n.y, n.width, n.height))
            .collect::<Vec<_>>(),
        result
            .textured_draws
            .iter()
            .map(|d| d.rect)
            .collect::<Vec<_>>()
    );

    let inner_right = rect_right(field_b) - border_w;
    for caret in &result.input_chrome_instances {
        let caret_r = caret.rect_pos[0] + caret.rect_size[0];
        assert!(
            caret_r <= inner_right - 0.5,
            "caret must not sit on the chrome stroke\n caret_r={caret_r} inner_right={inner_right} field={field_b:?}"
        );
    }

    let _ = take_frame_paint_nodes();
    push_focused_text_edit_caret_damage(&root);
    let dirties = take_frame_paint_nodes();
    assert!(
        !dirties.is_empty(),
        "focused caret blink must dirty the I-beam"
    );
    let stroke_x = rect_right(field_b) - border_w * 0.5;
    let stroke_y = field_b[1] + field_b[3] * 0.5;
    let scaled_tex = crate::renderer::scale_textured_draws(&result.textured_draws, 1.0);
    let prev_slots = crate::app::draw_cull::last_present_visual_slots(
        &result.node_paints,
        &result.instances,
        &scaled_tex,
        &result.text_nodes,
        &result.input_chrome_instances,
    );
    // Blink OFF has no I-beam chrome. Last-present must not record a 0×0 box at
    // the flex-leaf origin — expand would union that with the I-beam and dirty
    // "2222" plus the empty stretch (live PartialLoad path).
    let off_slots = crate::app::draw_cull::last_present_visual_slots(
        &result.node_paints,
        &result.instances,
        &scaled_tex,
        &result.text_nodes,
        &[],
    );
    let mut live_dirties = dirties.clone();
    expand_paint_dirties_for_translated_slots(&mut live_dirties, &off_slots, &prev_slots);
    expand_paint_dirties_for_translated_slots(&mut live_dirties, &prev_slots, &off_slots);
    let mut saw_overlay = false;
    for d in &live_dirties {
        if crate::app::draw_cull::paint_dirty_is_caret_overlay(
            d,
            &result.node_paints,
            &result.instances,
        ) {
            saw_overlay = true;
            assert!(
                d.bounds[2] < text_b[2] * 0.25,
                "blink dirty must be the I-beam, not the flex-grown text leaf\n dirty={:?} text={text_b:?}",
                d.bounds
            );
            continue;
        }
        let prev = prev_slots
            .iter()
            .find(|(k, _)| *k == d.key)
            .map(|(_, b)| *b);
        let Some(reset) = node_present_dest_reset_full(
            d.key,
            d.bounds,
            &result.node_paints,
            &result.instances,
            &scaled_tex,
            &result.input_chrome_instances,
            1.0,
            prev,
        ) else {
            continue;
        };
        let wipe = reset.rect;
        assert!(
            !rect_contains_point(wipe, stroke_x, stroke_y),
            "caret dest-reset must not punch the chrome stroke\n wipe={wipe:?} stroke=({stroke_x},{stroke_y}) dirty={:?} field={field_b:?}",
            d.bounds
        );
        assert!(
            !rect_contains_point(wipe, clear_cx, clear_cy),
            "caret dest-reset must not erase the clear X\n wipe={wipe:?} clear={clear_b:?} dirty={:?}",
            d.bounds
        );
    }
    assert!(
        saw_overlay,
        "focused caret blink must be a swapchain overlay (else dest-reset punches Search)"
    );
}
