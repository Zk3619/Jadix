//! Input paint-only PartialLoad must not stamp sidebar captions into the retain buffer.
//!
//! Gallery repro: focus / type in the Kbd Search field → leftover glyphs on the left rail.
//! Capture text is clipped to the owning dirty box and submitted once; this test checks
//! an inset shell so Input's dirty AABB never swallows the rail captions.

use crate::app::damage::{
    expand_paint_dirties_for_translated_slots, force_full_redraw_after_chrome, intersect_rects,
    node_present_slot_moved, paint_dirty_has_keyed_slot, paint_dirty_matches_present_slot,
    paint_dirty_nested_in_larger_node, paint_element_key, push_frame_paint_damage,
    push_frame_paint_element, take_frame_paint_nodes, union_rects, PaintDirty, PaintDirtyKind,
};
use crate::app::draw_cull::{
    instance_paint_bounds, last_present_visual_slots, node_paint_visual_bounds,
    node_present_capture_text_indices, node_present_dest_reset, node_present_dest_reset_scaled,
    node_present_foreign_glyph_indices, node_present_glyphs_for_dirty, node_present_needs_glyphs,
    node_present_scissor_logical, text_paint_bounds,
};
use crate::components::form::form_field;
use crate::components::input::{focus_id_for_input, input, input_value_signal_key};
use crate::components::input_adornment::input_adornment;
use crate::components::internal::text_edit::{
    push_focused_text_edit_caret_damage, sync_text_edit_paint_subtree_in_viewport,
};
use crate::components::sidebar::{
    sidebar, sidebar_provider_scope, SidebarInset, SidebarInsetProps, SidebarMenuButton,
    SidebarMenuButtonProps,
};
use crate::core::reactivity::use_signal;
use crate::core::tree::{
    sync_reactive_elements_subtree_ex, Element, LayoutStatus, LayoutTree, ReactiveSyncCollect,
};
use crate::layout::layout_tree::RenderResult;
use crate::primitives::r#box::{hstack, vstack};
use crate::renderer::font_loader;
use crate::style::{
    apply_class_list, sync_class_variants_subtree, sync_focus_ring_subtree,
    tick_active_css_transitions_subtree,
};
use crate::text::install_shared_font_system;

const NAV_LAYOUT: &str = "布局";
const NAV_GALLERY: &str = "Gallery";
const NAV_TEXT: &str = "文本";
const INPUT_ID: &str = "gal-kbd-search";

/// Matches `encode_node_present` for `PaintDirtyKind::Node`.
const NODE_SCISSOR_PAD: f32 = 8.0;

fn menu_btn(id: &str, label: &str) -> Element {
    SidebarMenuButton(SidebarMenuButtonProps {
        id: id.into(),
        is_active: None,
        variant: None,
        size: None,
        on_click: None,
        children: Some(Element::from(label)),
        base: None,
    })
}

fn leak_shell() -> Element {
    sidebar_provider_scope("gal-shell", None, true, Some("inset"), || {
        let mut frag = Element::new();
        frag.set_attr("rui_fragment", "1");
        let mut spin = Element::new().w(16.0).h(16.0);
        apply_class_list(&mut spin, "animate-spin");
        spin.set_attr("data-slot", "spin-icon");
        let kbd_row = hstack()
            .w("100%")
            .min_w(0.0)
            .gap("0.75rem")
            .items("center")
            .wrap("wrap")
            .child(Element::new().w(88.0).h(32.0))
            .child(
                input_adornment()
                    .child(input(INPUT_ID).placeholder("Search…").plain_chrome(true))
                    .addon("inline-end", Element::new().text("⌘K")),
            );
        let inset_body = vstack()
            .w("100%")
            .min_w(0.0)
            .gap("1rem")
            .p("1.5rem")
            .child(Element::new().h(80.0).w("100%"))
            .child(kbd_row)
            .child(spin);
        frag.children(vec![
            sidebar("gal-sidebar")
                .variant("inset")
                .w("13rem")
                .item(menu_btn("nav-basics", "NavBasics"))
                .item(menu_btn("nav-events", "NavEvents"))
                .item(menu_btn("nav-forms", "NavForms"))
                .item(menu_btn("nav-overlay", "NavOverlay"))
                .item(menu_btn("nav-data", "NavData"))
                .item(menu_btn("nav-draw", "NavDraw"))
                .item(menu_btn("nav-nav", "NavNav"))
                .item(menu_btn("nav-layout", NAV_LAYOUT))
                .item(menu_btn("nav-gallery", NAV_GALLERY))
                .item(menu_btn("nav-text", NAV_TEXT))
                .into(),
            SidebarInset(SidebarInsetProps {
                variant: Some("inset".into()),
                children: Some(inset_body.into()),
                base: None,
            }),
        ])
    })
}

fn find_input(el: &Element) -> Option<&Element> {
    if el.input_paint_sync.is_some() {
        return Some(el);
    }
    el.children.iter().find_map(find_input)
}

fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
    if el.attrs.get("data-slot").map(String::as_str) == Some(slot) {
        return Some(el);
    }
    el.children.iter().find_map(|c| find_slot(c, slot))
}

fn sync_like_app(root: &mut Element, tree: &mut LayoutTree, impact: &mut LayoutStatus) {
    crate::components::sidebar::sync_sidebar_menu_button_paint_subtree(root, tree, impact);
    sync_focus_ring_subtree(root, tree, impact);
    sync_class_variants_subtree(root, [0.0, 0.0], "", false, 1280.0, 800.0, tree, impact);
    sync_text_edit_paint_subtree_in_viewport(root, tree, impact, 1280.0, 800.0, [0.0, 0.0]);
}

fn fmt_rect(r: [f32; 4]) -> String {
    format!("[{:.1},{:.1} {:.1}x{:.1}]", r[0], r[1], r[2], r[3])
}

fn sidebar_captions<'a>(
    result: &'a RenderResult,
) -> Vec<&'a crate::layout::layout_tree::TextLayoutInfo> {
    result
        .text_nodes
        .iter()
        .filter(|n| n.text == NAV_LAYOUT || n.text == NAV_GALLERY || n.text == NAV_TEXT)
        .collect()
}

#[test]
fn input_partial_load_must_not_capture_sidebar_captions() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let _ = take_frame_paint_nodes();

    // Value exists before first layout so focus is paint-only (ring / caret), not LayoutChanged.
    use_signal(&input_value_signal_key(INPUT_ID), String::new).set("asdasdandsad".into());

    let mut root = leak_shell();
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    let _warm = tree.build_and_layout(&mut root, 1280.0, 800.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    fn count_paint_sync(el: &Element) -> usize {
        let here = usize::from(el.input_paint_sync.is_some());
        here + el.children.iter().map(count_paint_sync).sum::<usize>()
    }
    assert!(
        count_paint_sync(&root) >= 1,
        "shell must contain the Search Input paint-sync node"
    );

    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(INPUT_ID));
    sync_like_app(&mut root, &mut tree, &mut impact);
    let _focused = tree.build_and_layout(&mut root, 1280.0, 800.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    let (b, pad) = {
        let input_el = find_input(&root).expect("Search Input");
        let b = input_el.layout_bounds;
        let pad = input_el.style.shadow_blur + input_el.style.shadow_spread.abs() + 8.0;
        (b, pad)
    };
    assert!(
        b[2] > 0.0 && b[3] > 0.0,
        "Input must be laid out, got {}",
        fmt_rect(b)
    );
    // Same pad as `sync_one_text_edit_paint` StyleChanged (caret blink / focus ring).
    push_frame_paint_damage([b[0] - pad, b[1] - pad, b[2] + pad * 2.0, b[3] + pad * 2.0]);
    let dirty = take_frame_paint_nodes();
    let result = tree.build_and_layout(&mut root, 1280.0, 800.0, [-1.0, -1.0]);

    let rail = find_slot(&root, "sidebar-inner")
        .or_else(|| find_slot(&root, "sidebar"))
        .expect("sidebar rail");
    let rail_bounds = rail.layout_bounds;
    let rail_right = rail_bounds[0] + rail_bounds[2];

    assert!(
        b[0] >= rail_right - 1.0,
        "Input box overlaps the sidebar rail (PartialLoad scissor would clip through it):\n\
         input={} pad={pad:.1} rail={} impact={impact:?}",
        fmt_rect(b),
        fmt_rect(rail_bounds)
    );

    let captions = sidebar_captions(&result);
    assert_eq!(
        captions.len(),
        3,
        "expected three nav captions, got {:?}",
        result
            .text_nodes
            .iter()
            .map(|n| n.text.as_str())
            .collect::<Vec<_>>()
    );

    let input_text: Vec<String> = result
        .text_nodes
        .iter()
        .filter(|n| n.text.contains("asdasd") || n.text.contains("Search"))
        .map(|n| format!("{}@{}", n.text, fmt_rect([n.x, n.y, n.width, n.height])))
        .collect();
    for n in &result.text_nodes {
        if n.text.contains("asdasd") {
            assert!(
                n.x + n.width * 0.5 >= rail_right - 1.0,
                "Input glyphs must not be placed in the sidebar rail: {} rail={}",
                fmt_rect([n.x, n.y, n.width, n.height]),
                fmt_rect(rail_bounds)
            );
        }
    }

    for d in &dirty {
        let scissor = crate::app::draw_cull::padded_xywh(d.bounds, NODE_SCISSOR_PAD);
        for cap in &captions {
            let g = [cap.x, cap.y, cap.width, cap.height];
            if crate::app::damage::intersect_rects(scissor, g).is_some() {
                panic!(
                    "Input node-present scissor overlaps sidebar caption {} {}\n\
                     scissor={} dirty={} input={} rail={}\n\
                     input_text={input_text:?}",
                    cap.text,
                    fmt_rect(g),
                    fmt_rect(scissor),
                    fmt_rect(d.bounds),
                    fmt_rect(b),
                    fmt_rect(rail_bounds)
                );
            }
        }
    }

    let capture =
        node_present_capture_text_indices(&dirty, &result.text_nodes, &result.node_paints);

    let captured_sidebar: Vec<String> = capture
        .iter()
        .filter_map(|&i| result.text_nodes.get(i))
        .filter(|n| {
            let cx = n.x + n.width * 0.5;
            cx < rail_right - 1.0
                && (n.text == NAV_LAYOUT || n.text == NAV_GALLERY || n.text == NAV_TEXT)
        })
        .map(|n| format!("{}@{}", n.text, fmt_rect([n.x, n.y, n.width, n.height])))
        .collect();

    let dirty_dump: Vec<String> = dirty
        .iter()
        .map(|d| format!("{:?} {}", d.kind, fmt_rect(d.bounds)))
        .collect();

    assert!(
        captured_sidebar.is_empty(),
        "Input PartialLoad captured sidebar captions {captured_sidebar:?}\n\
         dirty={dirty_dump:?}\n\
         rail={}\n\
         captions={:?}\n\
         impact={impact:?}",
        fmt_rect(rail_bounds),
        captions
            .iter()
            .map(|n| format!("{} {}", n.text, fmt_rect([n.x, n.y, n.width, n.height])))
            .collect::<Vec<_>>()
    );

    let mut foreign = Vec::new();
    for d in &dirty {
        if d.kind != PaintDirtyKind::Node {
            continue;
        }
        let leaks = node_present_foreign_glyph_indices(
            d,
            &result.text_nodes,
            &result.node_paints,
            &capture,
            NODE_SCISSOR_PAD,
        );
        for i in leaks {
            let n = &result.text_nodes[i];
            foreign.push(format!(
                "{} in scissor {} (glyph {})",
                n.text,
                fmt_rect(d.bounds),
                fmt_rect(text_paint_bounds(n))
            ));
        }
    }
    assert!(
        foreign.is_empty(),
        "node present would stamp foreign glyphs into a dirty scissor: {foreign:?}\n\
         dirty={dirty_dump:?}"
    );

    let page = &result.instances;
    for d in &dirty {
        if d.kind != PaintDirtyKind::Node {
            continue;
        }
        let Some((reset, _)) = node_present_dest_reset(d.key, d.bounds, &result.node_paints, page)
        else {
            continue;
        };
        // dest-reset is in the same space as instances (physical-or-logical collect).
        for cap in &captions {
            let g = [cap.x, cap.y, cap.width, cap.height];
            let overlap = crate::app::damage::intersect_rects(reset, g);
            let dirty_is_sidebar = d.bounds[0] + d.bounds[2] * 0.5 < rail_right;
            assert!(
                overlap.is_none() || dirty_is_sidebar,
                "content dirty dest-reset {} overlaps sidebar caption {} {}\n\
                 dirty={} rail={}",
                fmt_rect(reset),
                cap.text,
                fmt_rect(g),
                fmt_rect(d.bounds),
                fmt_rect(rail_bounds)
            );
        }
    }

    assert!(
        !dirty.is_empty(),
        "Input StyleChanged must emit a local dirty node (got impact={impact:?} input={})",
        fmt_rect(b)
    );
}

#[test]
fn kbd_adornment_focus_is_not_page_geometry() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let _ = take_frame_paint_nodes();
    use_signal(&input_value_signal_key(INPUT_ID), String::new).set(String::new());

    let mut root = leak_shell();
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    let _ = tree.build_and_layout(&mut root, 1280.0, 800.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(INPUT_ID));
    let mut collect = ReactiveSyncCollect::default();
    sync_reactive_elements_subtree_ex(
        &mut root,
        &mut tree,
        &mut impact,
        1280.0,
        800.0,
        [0.0, 0.0],
        false,
        &mut collect,
    );
    assert!(
        !collect.page_layout_changed,
        "InputAdornment focus ring must not set page_layout_changed (that FullClears Gallery and jumps Marker spin); impact={impact:?}"
    );
    assert!(
        !matches!(
            impact,
            LayoutStatus::LayoutChanged | LayoutStatus::StructureChanged
        ),
        "focus ring is paint chrome, got {impact:?}"
    );
    assert!(
        !force_full_redraw_after_chrome(
            LayoutStatus::Clean,
            impact,
            collect.page_layout_changed,
            false,
            true,
            false,
        ),
        "focus must PartialLoad named nodes, not FullClear"
    );
}

/// Click-focus: shell Hard ring + nested Input/Kbd must stay Node dirties (not Region).
///
/// Shell wipe clears nested pixels; nested slots must also be dirty and sort *after*
/// the larger shell so caret / ⌘K / typed glyphs redraw on top. Region+coalesce ate
/// the Input Node and left focus chrome + keystrokes invisible (placeholder flicker).
#[test]
fn kbd_adornment_focus_must_dirty_nested_slots_after_shell() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let _ = take_frame_paint_nodes();
    use_signal(&input_value_signal_key(INPUT_ID), String::new).set(String::new());
    use_signal("__rui_button_focus_id", String::new).set(String::new());

    let mut root = leak_shell();
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    let _ = tree.build_and_layout(&mut root, 1280.0, 800.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(INPUT_ID));
    sync_reactive_elements_subtree_ex(
        &mut root,
        &mut tree,
        &mut impact,
        1280.0,
        800.0,
        [0.0, 0.0],
        false,
        &mut ReactiveSyncCollect::default(),
    );
    sync_like_app(&mut root, &mut tree, &mut impact);
    crate::renderer::restart_caret_blink();
    let dirty = take_frame_paint_nodes();
    let result = tree.build_and_layout(&mut root, 1280.0, 800.0, [-1.0, -1.0]);

    let shell = find_slot(&root, "input-adornment").expect("adornment shell");
    let input_el = find_input(&root).expect("Search Input");
    let addon_el = find_slot(&root, "input-adornment-addon").expect("⌘K addon");
    let shell_key = paint_element_key(shell);
    let input_key = paint_element_key(input_el);
    let addon_key = paint_element_key(addon_el);

    assert!(
        !dirty.iter().any(|d| d.kind == PaintDirtyKind::Region),
        "adornment focus must not use Region (coalesce ate Input Node); dirty={:?}",
        dirty
            .iter()
            .map(|d| format!("{:?} {}", d.kind, fmt_rect(d.bounds)))
            .collect::<Vec<_>>()
    );
    assert!(
        dirty
            .iter()
            .any(|d| d.kind == PaintDirtyKind::Node && d.key == shell_key),
        "shell Hard ring must push Node; dirty={:?}",
        dirty
            .iter()
            .map(|d| format!("{:?} key={}", d.kind, d.key))
            .collect::<Vec<_>>()
    );
    assert!(
        dirty
            .iter()
            .any(|d| d.kind == PaintDirtyKind::Node && d.key == input_key),
        "nested Input must stay dirty after shell wipe; dirty={:?}",
        dirty
            .iter()
            .map(|d| format!("{:?} key={}", d.kind, d.key))
            .collect::<Vec<_>>()
    );
    assert!(
        dirty
            .iter()
            .any(|d| d.kind == PaintDirtyKind::Node && d.key == addon_key),
        "⌘K addon must stay dirty after shell wipe; dirty={:?}",
        dirty
            .iter()
            .map(|d| format!("{:?} key={}", d.kind, d.key))
            .collect::<Vec<_>>()
    );

    let shell_i = dirty.iter().position(|d| d.key == shell_key).unwrap();
    let input_i = dirty.iter().position(|d| d.key == input_key).unwrap();
    assert!(
        shell_i < input_i,
        "shell wipe must sort before Input redraw (got shell@{shell_i} input@{input_i})"
    );

    let shell_ring = result.instances.iter().any(|i| {
        i.shadow_spread.abs() > 1.0 && i.shadow_color[3] > 0.05 && i.shadow_blur.abs() < 0.5
    });
    assert!(
        shell_ring,
        "click-focus must collect Hard-ring on the adornment shell"
    );
    assert!(
        !result.input_chrome_instances.is_empty(),
        "click-focus must collect caret chrome"
    );

    let capture =
        node_present_capture_text_indices(&dirty, &result.text_nodes, &result.node_paints);
    let captured: Vec<&str> = capture
        .iter()
        .filter_map(|&i| result.text_nodes.get(i).map(|n| n.text.as_str()))
        .collect();
    assert!(
        captured.iter().any(|t| t.contains("Search")),
        "focus present must recapture placeholder; captured={captured:?}"
    );
    assert!(
        captured
            .iter()
            .any(|t| t.contains('⌘') || *t == "⌘K" || t.contains("Ctrl")),
        "focus present must recapture ⌘K; captured={captured:?}"
    );

    // Transparent-fill shell used to fail opaque-nesting skip: Input dest-reset
    // after the shell wipe punched the Hard ring (click looks unfocused).
    let shell_dirty = dirty.iter().find(|d| d.key == shell_key).unwrap();
    let input_dirty = dirty.iter().find(|d| d.key == input_key).unwrap();
    assert!(
        crate::app::draw_cull::paint_dirty_skips_node_dest_reset_covered(
            input_dirty,
            &dirty,
            false,
            &result.node_paints,
            &result.instances,
            1.0,
        ),
        "nested Input must skip dest-reset when adornment shell is dirty (else Input wipe punches Hard ring)"
    );
    let ring = hard_ring_aabb(&result, shell_key).expect("shell Hard ring AABB");
    let ring_sample = [ring[0] + 1.0, ring[1] + ring[3] * 0.5];
    if let Some((wipe, _)) = node_present_dest_reset_scaled(
        input_dirty.key,
        input_dirty.bounds,
        &result.node_paints,
        &result.instances,
        &[],
        1.0,
        None,
    ) {
        // Skip means encode will not wipe — if we *did* wipe, ring must stay.
        assert!(
            !rect_contains_point(wipe, ring_sample[0], ring_sample[1])
                || crate::app::draw_cull::paint_dirty_skips_node_dest_reset_covered(
                    input_dirty,
                    &dirty,
                    false,
                    &result.node_paints,
                    &result.instances,
                    1.0,
                ),
            "Input dest-reset must not punch shell Hard ring\n wipe={} ring={} sample={:?}",
            fmt_rect(wipe),
            fmt_rect(ring),
            ring_sample
        );
    }
    let _ = shell_dirty;
}

/// Typing while adornment-focused must keep an Input Node dirty (not Region-coalesced)
/// and collect the typed glyphs — not only flash the placeholder.
#[test]
fn kbd_adornment_typing_must_keep_input_node_and_typed_glyphs() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let _ = take_frame_paint_nodes();
    use_signal(&input_value_signal_key(INPUT_ID), String::new).set(String::new());
    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(INPUT_ID));

    let mut root = leak_shell();
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    let _ = tree.build_and_layout(&mut root, 1280.0, 800.0, [-1.0, -1.0]);
    sync_like_app(&mut root, &mut tree, &mut impact);
    let _ = tree.build_and_layout(&mut root, 1280.0, 800.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    use_signal(&input_value_signal_key(INPUT_ID), String::new).set("你好".into());
    impact = LayoutStatus::Clean;
    sync_reactive_elements_subtree_ex(
        &mut root,
        &mut tree,
        &mut impact,
        1280.0,
        800.0,
        [0.0, 0.0],
        false,
        &mut ReactiveSyncCollect::default(),
    );
    sync_like_app(&mut root, &mut tree, &mut impact);
    crate::renderer::restart_caret_blink();
    let dirty = take_frame_paint_nodes();
    let result = tree.build_and_layout(&mut root, 1280.0, 800.0, [-1.0, -1.0]);

    let input_el = find_input(&root).expect("Search Input");
    let input_key = paint_element_key(input_el);
    assert!(
        dirty
            .iter()
            .any(|d| d.kind == PaintDirtyKind::Node && d.key == input_key),
        "typing must keep Input Node dirty (Region coalesce swallowed it); dirty={:?}",
        dirty
            .iter()
            .map(|d| format!("{:?} key={}", d.kind, d.key))
            .collect::<Vec<_>>()
    );
    assert!(
        !dirty.iter().any(|d| d.kind == PaintDirtyKind::Region),
        "typing must not promote adornment to Region; dirty={:?}",
        dirty
            .iter()
            .map(|d| format!("{:?} {}", d.kind, fmt_rect(d.bounds)))
            .collect::<Vec<_>>()
    );
    assert!(
        result.text_nodes.iter().any(|n| n.text.contains('你') || n.text.contains("你好")),
        "typed value must be in collect (not placeholder-only); texts={:?}",
        result
            .text_nodes
            .iter()
            .map(|n| n.text.as_str())
            .collect::<Vec<_>>()
    );
    let capture =
        node_present_capture_text_indices(&dirty, &result.text_nodes, &result.node_paints);
    let captured: Vec<&str> = capture
        .iter()
        .filter_map(|&i| result.text_nodes.get(i).map(|n| n.text.as_str()))
        .collect();
    assert!(
        captured
            .iter()
            .any(|t| t.contains('你') || t.contains("你好")),
        "typing present must recapture typed glyphs; captured={captured:?}"
    );
}

#[test]
fn kbd_adornment_typing_must_not_capture_sidebar_or_wipe_spin() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let _ = take_frame_paint_nodes();
    use_signal(&input_value_signal_key(INPUT_ID), String::new).set(String::new());
    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(INPUT_ID));

    let mut root = leak_shell();
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    let _ = tree.build_and_layout(&mut root, 1280.0, 800.0, [-1.0, -1.0]);
    let mut collect = ReactiveSyncCollect::default();
    sync_reactive_elements_subtree_ex(
        &mut root,
        &mut tree,
        &mut impact,
        1280.0,
        800.0,
        [0.0, 0.0],
        false,
        &mut collect,
    );
    sync_like_app(&mut root, &mut tree, &mut impact);
    let _ = tree.build_and_layout(&mut root, 1280.0, 800.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    use_signal(&input_value_signal_key(INPUT_ID), String::new).set("asdasdandsad".into());
    impact = LayoutStatus::Clean;
    collect = ReactiveSyncCollect::default();
    sync_reactive_elements_subtree_ex(
        &mut root,
        &mut tree,
        &mut impact,
        1280.0,
        800.0,
        [0.0, 0.0],
        false,
        &mut collect,
    );
    sync_like_app(&mut root, &mut tree, &mut impact);
    let dirty = take_frame_paint_nodes();
    let result = tree.build_and_layout(&mut root, 1280.0, 800.0, [-1.0, -1.0]);

    assert!(
        !collect.page_layout_changed,
        "typing must not be page geometry; impact={impact:?}"
    );
    assert_eq!(
        impact,
        LayoutStatus::StyleChanged,
        "fixed-box Kbd Input typing must stay StyleChanged, got {impact:?}"
    );
    assert!(
        !force_full_redraw_after_chrome(
            LayoutStatus::Clean,
            impact,
            collect.page_layout_changed,
            false,
            !dirty.is_empty(),
            false,
        ),
        "typing+spin must PartialLoad; dirty={:?}",
        dirty
            .iter()
            .map(|d| format!("{:?} {}", d.kind, fmt_rect(d.bounds)))
            .collect::<Vec<_>>()
    );

    let rail = find_slot(&root, "sidebar-inner")
        .or_else(|| find_slot(&root, "sidebar"))
        .expect("sidebar rail");
    let rail_right = rail.layout_bounds[0] + rail.layout_bounds[2];
    let captions = sidebar_captions(&result);
    assert_eq!(
        captions.len(),
        3,
        "expected 布局/Gallery/文本, got {:?}",
        result
            .text_nodes
            .iter()
            .map(|n| n.text.as_str())
            .collect::<Vec<_>>()
    );

    let capture =
        node_present_capture_text_indices(&dirty, &result.text_nodes, &result.node_paints);
    let captured_sidebar: Vec<String> = capture
        .iter()
        .filter_map(|&i| result.text_nodes.get(i))
        .filter(|n| {
            n.x + n.width * 0.5 < rail_right - 1.0
                && (n.text == NAV_LAYOUT || n.text == NAV_GALLERY || n.text == NAV_TEXT)
        })
        .map(|n| format!("{}@{}", n.text, fmt_rect([n.x, n.y, n.width, n.height])))
        .collect();
    assert!(
        captured_sidebar.is_empty(),
        "typing PartialLoad captured sidebar captions {captured_sidebar:?}\n dirty={:?}",
        dirty
            .iter()
            .map(|d| format!("{:?} {}", d.kind, fmt_rect(d.bounds)))
            .collect::<Vec<_>>()
    );

    let spin_el = find_slot(&root, "spin-icon").expect("spin");
    let spin_box = spin_el.layout_bounds;
    let input_el = find_input(&root).expect("Search Input");
    let input_box = input_el.layout_bounds;
    assert!(
        dirty.iter().any(|d| {
            d.kind == PaintDirtyKind::Node
                && crate::app::damage::intersect_rects(d.bounds, spin_box).is_some()
        }),
        "spin must stay in damage_nodes while typing\n dirty={:?} spin={}",
        dirty
            .iter()
            .map(|d| format!("{:?} {}", d.kind, fmt_rect(d.bounds)))
            .collect::<Vec<_>>(),
        fmt_rect(spin_box)
    );

    let page = &result.instances;
    for d in &dirty {
        if d.kind != PaintDirtyKind::Node {
            continue;
        }
        let Some((reset, _)) = node_present_dest_reset(d.key, d.bounds, &result.node_paints, page)
        else {
            continue;
        };
        let input_owned = crate::app::damage::intersect_rects(d.bounds, input_box).is_some()
            && crate::app::damage::intersect_rects(d.bounds, spin_box).is_none();
        if input_owned {
            assert!(
                crate::app::damage::intersect_rects(reset, spin_box).is_none(),
                "Input dest-reset {} must not wipe Marker spin {}\n dirty={}",
                fmt_rect(reset),
                fmt_rect(spin_box),
                fmt_rect(d.bounds)
            );
        }
        for cap in &captions {
            let g = [cap.x, cap.y, cap.width, cap.height];
            let dirty_is_sidebar = d.bounds[0] + d.bounds[2] * 0.5 < rail_right;
            assert!(
                crate::app::damage::intersect_rects(reset, g).is_none() || dirty_is_sidebar,
                "content dest-reset {} overlaps sidebar caption {} {}",
                fmt_rect(reset),
                cap.text,
                fmt_rect(g)
            );
        }
    }
}

#[test]
fn kbd_input_dest_reset_must_not_wipe_adornment_chrome() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let _ = take_frame_paint_nodes();
    use_signal(&input_value_signal_key(INPUT_ID), String::new).set(String::new());

    let mut root = leak_shell();
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    let _ = tree.build_and_layout(&mut root, 1280.0, 800.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(INPUT_ID));
    sync_like_app(&mut root, &mut tree, &mut impact);
    let result = tree.build_and_layout(&mut root, 1280.0, 800.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    let input_el = find_input(&root).expect("Search Input");
    let addon_el = find_slot(&root, "input-adornment-addon").expect("⌘K addon");
    let input_box = input_el.layout_bounds;
    let addon_box = addon_el.layout_bounds;
    let pad = input_el.style.shadow_blur + input_el.style.shadow_spread.abs() + 8.0;
    let input_dirty = [
        input_box[0] - pad,
        input_box[1] - pad,
        input_box[2] + pad * 2.0,
        input_box[3] + pad * 2.0,
    ];
    let input_only = PaintDirty {
        key: paint_element_key(input_el),
        bounds: input_dirty,
        kind: PaintDirtyKind::Node,
    };
    let page = &result.instances;
    let Some((reset, _)) =
        node_present_dest_reset(input_only.key, input_only.bounds, &result.node_paints, page)
    else {
        panic!(
            "transparent Kbd Input must dest-reset its own box; input={}",
            fmt_rect(input_box)
        );
    };
    assert!(
        reset[2] < input_dirty[2] - 4.0 && reset[3] < input_dirty[3] - 4.0,
        "dest-reset must not follow the +8px caret scissor pad\n reset={} dirty={} input={}",
        fmt_rect(reset),
        fmt_rect(input_dirty),
        fmt_rect(input_box)
    );
    let addon_cx = addon_box[0] + addon_box[2] * 0.5;
    let addon_cy = addon_box[1] + addon_box[3] * 0.5;
    let reset_covers_addon = addon_cx >= reset[0]
        && addon_cy >= reset[1]
        && addon_cx < reset[0] + reset[2]
        && addon_cy < reset[1] + reset[3];
    assert!(
        !reset_covers_addon,
        "Input dest-reset {} must not erase ⌘K addon {}",
        fmt_rect(reset),
        fmt_rect(addon_box)
    );
}

fn forms_like_stack() -> Element {
    Element::new()
        .w(400.0)
        .bg([0.94, 0.94, 0.95, 1.0])
        .p(16.0)
        .child(
            vstack()
                .w("100%")
                .gap(8.0)
                .child(
                    input("gal-like-a")
                        .placeholder("Placeholder…")
                        .clearable(true),
                )
                .child(
                    input("gal-like-b")
                        .placeholder("Placeholder…")
                        .clearable(true),
                )
                .child(input("gal-like-c").placeholder("email").clearable(true))
                .child(
                    input("gal-like-pw")
                        .input_type("password")
                        .password_toggle(true)
                        .clearable(true),
                ),
        )
}

fn sync_forms(root: &mut Element, tree: &mut LayoutTree, impact: &mut LayoutStatus) {
    sync_focus_ring_subtree(root, tree, impact);
    sync_class_variants_subtree(root, [0.0, 0.0], "", false, 400.0, 240.0, tree, impact);
    sync_text_edit_paint_subtree_in_viewport(root, tree, impact, 400.0, 240.0, [0.0, 0.0]);
}

fn find_input_id<'a>(el: &'a Element, id: &str) -> Option<&'a Element> {
    if el.template_id.as_deref() == Some(id) && el.input_paint_sync.is_some() {
        return Some(el);
    }
    el.children.iter().find_map(|c| find_input_id(c, id))
}

/// Standalone Input Hard ring is blur=0 / spread=3. Dest-reset of the unfocused
/// slot must still erase that outset, and must recapture the field's glyphs.
///
/// Gallery 图一/图二: click focus must show the Hard ring; caret blink dest-reset
/// must not punch that outset (looks like default border / lost focus chrome).
#[test]
fn focused_input_caret_blink_must_keep_hard_ring() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let _ = take_frame_paint_nodes();
    const ID: &str = "gal-like-caret-ring";
    use_signal(&input_value_signal_key(ID), String::new).set(String::new());

    let mut root = Element::new()
        .w(400.0)
        .h(80.0)
        .bg([0.96, 0.96, 0.97, 1.0])
        .child(input(ID).placeholder("Search…"));
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    let _ = tree.build_and_layout(&mut root, 400.0, 80.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(ID));
    sync_forms(&mut root, &mut tree, &mut impact);
    let focused = tree.build_and_layout(&mut root, 400.0, 80.0, [-1.0, -1.0]);

    let input_el = find_input_id(&root, ID).expect("Input");
    let key = paint_element_key(input_el);
    let input_box = input_el.layout_bounds;
    let ring = hard_ring_aabb(&focused, key).expect(
        "focused Input must emit Hard-ring (click focus must not look like default border)",
    );
    assert!(
        ring[0] <= input_box[0] - 1.5,
        "Hard ring must outset the field\n ring={} input={}",
        fmt_rect(ring),
        fmt_rect(input_box)
    );

    let _ = take_frame_paint_nodes();
    push_focused_text_edit_caret_damage(&root);
    let dirties = take_frame_paint_nodes();
    assert!(!dirties.is_empty(), "caret blink must dirty");
    let prev_slots = last_present_visual_slots(
        &focused.node_paints,
        &focused.instances,
        &[],
        &focused.text_nodes,
        &focused.input_chrome_instances,
    );
    let ring_sample = [ring[0] + 1.0, ring[1] + ring[3] * 0.5];
    for d in &dirties {
        let prev = prev_slots.iter().find(|(k, _)| *k == d.key).map(|(_, b)| *b);
        let Some((wipe, _)) = node_present_dest_reset_scaled(
            d.key,
            d.bounds,
            &focused.node_paints,
            &focused.instances,
            &[],
            1.0,
            prev,
        ) else {
            continue;
        };
        assert!(
            !rect_contains_point(wipe, ring_sample[0], ring_sample[1]),
            "caret blink dest-reset must not punch Hard ring\n wipe={} ring={} sample={:?} dirty={}\n prev={prev:?}",
            fmt_rect(wipe),
            fmt_rect(ring),
            ring_sample,
            fmt_rect(d.bounds),
        );
    }
}

#[test]
fn standalone_input_unfocus_dest_reset_must_cover_hard_ring_and_glyphs() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let _ = take_frame_paint_nodes();
    const ID: &str = "gal-like-b";
    use_signal(&input_value_signal_key(ID), String::new).set("hello".into());

    let mut root = forms_like_stack();
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    let _ = tree.build_and_layout(&mut root, 400.0, 240.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(ID));
    sync_forms(&mut root, &mut tree, &mut impact);
    let focused = tree.build_and_layout(&mut root, 400.0, 240.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    let input_el = find_input_id(&root, ID).expect("focused Input");
    let key = paint_element_key(input_el);
    let input_box = input_el.layout_bounds;
    let focused_paint = focused
        .node_paints
        .iter()
        .find(|p| p.key == key)
        .copied()
        .expect("focused Input collect slot");
    let prev = node_paint_visual_bounds(
        &focused_paint,
        &focused.instances,
        &[],
        &focused.text_nodes,
        &focused.input_chrome_instances,
    );
    let lo = focused_paint.instance.0.min(focused.instances.len());
    let hi = focused_paint.instance.1.min(focused.instances.len());
    let ring = focused.instances[lo..hi]
        .iter()
        .filter(|i| i.shadow_spread.abs() > 1.0 && i.shadow_color[3] > 0.05)
        .map(instance_paint_bounds)
        .reduce(|a, b| {
            let x0 = a[0].min(b[0]);
            let y0 = a[1].min(b[1]);
            let x1 = (a[0] + a[2]).max(b[0] + b[2]);
            let y1 = (a[1] + a[3]).max(b[1] + b[3]);
            [x0, y0, x1 - x0, y1 - y0]
        })
        .expect("focused Input must emit a Hard-ring outset shadow");
    assert!(
        ring[0] <= input_box[0] - 2.0 && ring[0] + ring[2] >= input_box[0] + input_box[2] + 2.0,
        "Hard ring AABB must sit outside the field\n ring={} input={}",
        fmt_rect(ring),
        fmt_rect(input_box)
    );

    use_signal("__rui_button_focus_id", String::new).set(String::new());
    impact = LayoutStatus::Clean;
    sync_forms(&mut root, &mut tree, &mut impact);
    let unfocused = tree.build_and_layout(&mut root, 400.0, 240.0, [-1.0, -1.0]);
    let input_el = find_input_id(&root, ID).expect("unfocused Input");
    let key = paint_element_key(input_el);
    let pad = input_el.style.shadow_blur + input_el.style.shadow_spread.abs() + 8.0;
    let b = input_el.layout_bounds;
    let dirty = PaintDirty {
        key,
        bounds: [b[0] - pad, b[1] - pad, b[2] + pad * 2.0, b[3] + pad * 2.0],
        kind: PaintDirtyKind::Node,
    };
    let (wipe, _) = node_present_dest_reset_scaled(
        dirty.key,
        dirty.bounds,
        &unfocused.node_paints,
        &unfocused.instances,
        &[],
        1.0,
        Some(prev),
    )
    .expect("card behind standalone Input");
    assert!(
        wipe[0] <= ring[0] + 0.5
            && wipe[1] <= ring[1] + 0.5
            && wipe[0] + wipe[2] >= ring[0] + ring[2] - 0.5
            && wipe[1] + wipe[3] >= ring[1] + ring[3] - 0.5,
        "unfocus dest-reset must erase the leftover Hard ring\n wipe={} ring={} prev={}",
        fmt_rect(wipe),
        fmt_rect(ring),
        fmt_rect(prev)
    );
    assert!(
        node_present_needs_glyphs(
            dirty.key,
            dirty.bounds,
            &unfocused.node_paints,
            &unfocused.instances
        ),
        "wiping the field must recapture its glyphs"
    );
    let owned =
        node_present_glyphs_for_dirty(&dirty, &unfocused.text_nodes, &unfocused.node_paints);
    assert!(
        owned
            .iter()
            .any(|&i| unfocused.text_nodes[i].text.contains("hello")),
        "Input dest-reset must own the typed glyphs, not leave a gray hole; owned={owned:?} texts={:?}",
        unfocused
            .text_nodes
            .iter()
            .map(|n| n.text.as_str())
            .collect::<Vec<_>>()
    );
}

fn hard_ring_aabb(result: &RenderResult, key: u64) -> Option<[f32; 4]> {
    let p = result.node_paints.iter().find(|p| p.key == key)?;
    let lo = p.instance.0.min(result.instances.len());
    let hi = p.instance.1.min(result.instances.len());
    result.instances[lo..hi]
        .iter()
        .filter(|i| i.shadow_spread.abs() > 1.0 && i.shadow_color[3] > 0.05)
        .map(instance_paint_bounds)
        .reduce(|a, b| {
            let x0 = a[0].min(b[0]);
            let y0 = a[1].min(b[1]);
            let x1 = (a[0] + a[2]).max(b[0] + b[2]);
            let y1 = (a[1] + a[3]).max(b[1] + b[3]);
            [x0, y0, x1 - x0, y1 - y0]
        })
}

fn input_dirty(el: &Element) -> PaintDirty {
    let b = el.layout_bounds;
    let pad = el.style.shadow_blur + el.style.shadow_spread.abs() + 8.0;
    PaintDirty {
        key: paint_element_key(el),
        bounds: [b[0] - pad, b[1] - pad, b[2] + pad * 2.0, b[3] + pad * 2.0],
        kind: PaintDirtyKind::Node,
    }
}

/// Clicking Input B must dest-reset A. Sibling dirty pads overlap; treating that
/// as nesting skipped A's wipe and src-over'd another Hard ring onto retain.
#[test]
fn switching_input_focus_must_dest_reset_the_leave_field() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let _ = take_frame_paint_nodes();
    const LEAVE: &str = "gal-like-a";
    const ENTER: &str = "gal-like-b";
    use_signal(&input_value_signal_key(LEAVE), String::new).set("Placeholder…".into());
    use_signal(&input_value_signal_key(ENTER), String::new).set("Invalid".into());

    let mut root = forms_like_stack();
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    let _ = tree.build_and_layout(&mut root, 400.0, 240.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(LEAVE));
    sync_forms(&mut root, &mut tree, &mut impact);
    let focused = tree.build_and_layout(&mut root, 400.0, 240.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    let leave_el = find_input_id(&root, LEAVE).expect("leave Input");
    let leave_key = paint_element_key(leave_el);
    let ring = hard_ring_aabb(&focused, leave_key).expect("focused leave field emits Hard ring");

    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(ENTER));
    impact = LayoutStatus::Clean;
    sync_forms(&mut root, &mut tree, &mut impact);
    let switched = tree.build_and_layout(&mut root, 400.0, 240.0, [-1.0, -1.0]);

    let leave_el = find_input_id(&root, LEAVE).expect("unfocused leave Input");
    let enter_el = find_input_id(&root, ENTER).expect("focused enter Input");
    let leave_dirty = input_dirty(leave_el);
    let enter_dirty = input_dirty(enter_el);
    assert!(
        !paint_dirty_nested_in_larger_node(&leave_dirty, &[leave_dirty, enter_dirty]),
        "leave Input dest-reset must not be skipped as nested in the newly focused sibling"
    );
    assert!(
        hard_ring_aabb(&switched, paint_element_key(leave_el)).is_none(),
        "leave Input collect must not still emit a Hard ring (that src-overs onto retain)"
    );
    assert!(
        hard_ring_aabb(&switched, paint_element_key(enter_el)).is_some(),
        "enter Input must emit the Hard ring"
    );
    let (wipe, _) = node_present_dest_reset_scaled(
        leave_dirty.key,
        leave_dirty.bounds,
        &switched.node_paints,
        &switched.instances,
        &[],
        1.0,
        Some(ring),
    )
    .expect("card behind leave Input");
    assert!(
        wipe[0] <= ring[0] + 0.5
            && wipe[1] <= ring[1] + 0.5
            && wipe[0] + wipe[2] >= ring[0] + ring[2] - 0.5
            && wipe[1] + wipe[3] >= ring[1] + ring[3] - 0.5,
        "leave dest-reset must cover the previous Hard ring\n wipe={} ring={}",
        fmt_rect(wipe),
        fmt_rect(ring)
    );
}

fn wp39_form_field() -> Element {
    Element::new()
        .w(400.0)
        .bg([1.0, 1.0, 1.0, 1.0])
        .p(16.0)
        .child(form_field(
            "email",
            "邮箱",
            Element::from(input("wp39-email").placeholder("you@example.com")),
        ))
}

fn find_text_node<'a>(el: &'a Element, s: &str) -> Option<&'a Element> {
    if el.text.as_deref() == Some(s) {
        return Some(el);
    }
    el.children.iter().find_map(|c| find_text_node(c, s))
}

fn find_caret_leaf<'a>(el: &'a Element) -> Option<&'a Element> {
    if el.text_style.input_caret.is_some() {
        return Some(el);
    }
    el.children.iter().find_map(find_caret_leaf)
}

fn rect_contains_point(r: [f32; 4], x: f32, y: f32) -> bool {
    x >= r[0] && y >= r[1] && x < r[0] + r[2] && y < r[1] + r[3]
}

/// WP-3.9 `form_field` gap is 4px. Caret blink used to dirty Input chrome
/// (Hard ring + vertex AA + dest-reset pad) and dest-reset the card color over
/// the 「邮箱」 glyph bottoms — the label node was not in that dirty set.
#[test]
fn form_field_caret_blink_must_not_dirty_or_wipe_the_label() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let _ = take_frame_paint_nodes();
    const ID: &str = "wp39-email";
    use_signal(&input_value_signal_key(ID), String::new).set("you@example.com".into());

    let mut root = wp39_form_field();
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    let _ = tree.build_and_layout(&mut root, 400.0, 200.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(ID));
    sync_forms(&mut root, &mut tree, &mut impact);
    let focused = tree.build_and_layout(&mut root, 400.0, 200.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    let label = find_text_node(&root, "邮箱").expect("邮箱 label");
    let label_box = label.layout_bounds;
    let leaf = find_caret_leaf(&root).expect("focused Input text leaf");
    let input_el = find_input_id(&root, ID).expect("email Input");
    assert!(
        label_box[3] > 0.0 && leaf.layout_bounds[3] > 0.0,
        "label and text leaf must be laid out\n label={} leaf={}",
        fmt_rect(label_box),
        fmt_rect(leaf.layout_bounds)
    );
    assert!(
        intersect_rects(label_box, leaf.layout_bounds).is_none(),
        "precondition: label and text leaf are separate boxes\n label={} leaf={}",
        fmt_rect(label_box),
        fmt_rect(leaf.layout_bounds)
    );

    push_focused_text_edit_caret_damage(&root);
    let dirties = take_frame_paint_nodes();
    assert!(
        !dirties.is_empty(),
        "caret blink must record a paint-dirty node"
    );
    for d in &dirties {
        assert!(
            intersect_rects(d.bounds, label_box).is_none(),
            "caret dirty {} must not intersect the 邮箱 label {}",
            fmt_rect(d.bounds),
            fmt_rect(label_box)
        );
        assert_ne!(
            d.key,
            paint_element_key(input_el),
            "caret blink must dirty the text leaf, not Input chrome {}\n dirty={}",
            fmt_rect(input_el.layout_bounds),
            fmt_rect(d.bounds)
        );
    }

    let label_bottom = [
        label_box[0] + label_box[2] * 0.5,
        label_box[1] + label_box[3] - 0.5,
    ];
    for d in &dirties {
        let prev = focused
            .node_paints
            .iter()
            .find(|p| p.key == d.key)
            .map(|p| {
                node_paint_visual_bounds(
                    p,
                    &focused.instances,
                    &[],
                    &focused.text_nodes,
                    &focused.input_chrome_instances,
                )
            });
        let Some((wipe, _)) = node_present_dest_reset_scaled(
            d.key,
            d.bounds,
            &focused.node_paints,
            &focused.instances,
            &[],
            1.0,
            prev,
        ) else {
            continue;
        };
        assert!(
            !rect_contains_point(wipe, label_bottom[0], label_bottom[1]),
            "caret dest-reset {} must not erase the 邮箱 label bottom {:?}\n label={}",
            fmt_rect(wipe),
            label_bottom,
            fmt_rect(label_box)
        );
    }
}

/// WP-3.9 `form_field` gap is 4px. Input Hard-ring dest-reset / 8px node scissor
/// used to punch the 「邮箱」 glyph bottoms (vertex AA + RESET_PAD on spread 3).
#[test]
fn form_field_focus_ring_must_not_dirty_or_wipe_the_label() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let _ = take_frame_paint_nodes();
    const ID: &str = "wp39-email";
    use_signal(&input_value_signal_key(ID), String::new).set("you@example.com".into());

    let mut root = wp39_form_field();
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    let _ = tree.build_and_layout(&mut root, 400.0, 200.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(ID));
    sync_forms(&mut root, &mut tree, &mut impact);
    let focused = tree.build_and_layout(&mut root, 400.0, 200.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    let label = find_text_node(&root, "邮箱").expect("邮箱 label");
    let label_box = label.layout_bounds;
    let input_el = find_input_id(&root, ID).expect("email Input");
    let input_box = input_el.layout_bounds;
    assert!(
        label_box[1] + label_box[3] <= input_box[1] + 0.5,
        "precondition: label sits above the Input\n label={} input={}",
        fmt_rect(label_box),
        fmt_rect(input_box)
    );

    push_frame_paint_element(input_el, input_el.paint_scroll_acc);
    let dirties = take_frame_paint_nodes();
    assert!(!dirties.is_empty(), "focus must dirty the Input chrome");
    let label_bottom = [
        label_box[0] + label_box[2] * 0.5,
        label_box[1] + label_box[3] - 0.5,
    ];
    for d in &dirties {
        assert!(
            intersect_rects(d.bounds, label_box).is_none(),
            "Input focus dirty {} must not intersect the 邮箱 label {}",
            fmt_rect(d.bounds),
            fmt_rect(label_box)
        );
        let prev = focused
            .node_paints
            .iter()
            .find(|p| p.key == d.key)
            .map(|p| {
                node_paint_visual_bounds(
                    p,
                    &focused.instances,
                    &[],
                    &focused.text_nodes,
                    &focused.input_chrome_instances,
                )
            });
        if let Some((wipe, _)) = node_present_dest_reset_scaled(
            d.key,
            d.bounds,
            &focused.node_paints,
            &focused.instances,
            &[],
            1.0,
            prev,
        ) {
            assert!(
                !rect_contains_point(wipe, label_bottom[0], label_bottom[1]),
                "focus dest-reset {} must not erase the 邮箱 label bottom {:?}\n label={}",
                fmt_rect(wipe),
                label_bottom,
                fmt_rect(label_box)
            );
        }
        let prev_slots: Vec<(u64, [f32; 4])> = prev.map(|b| (d.key, b)).into_iter().collect();
        let scissor = node_present_scissor_logical(
            PaintDirtyKind::Node,
            d.key,
            d.bounds,
            &prev_slots,
            &focused.node_paints,
            8.0,
        );
        assert!(
            !rect_contains_point(scissor, label_bottom[0], label_bottom[1]),
            "focus scissor {} must not cover the 邮箱 label bottom {:?}\n label={}",
            fmt_rect(scissor),
            label_bottom,
            fmt_rect(label_box)
        );
    }
}

fn aabb_covers(outer: [f32; 4], inner: [f32; 4]) -> bool {
    outer[0] <= inner[0] + 0.5
        && outer[1] <= inner[1] + 0.5
        && outer[0] + outer[2] >= inner[0] + inner[2] - 0.5
        && outer[1] + outer[3] >= inner[1] + inner[3] - 0.5
}

fn outset_shadow_blob(inst: &crate::renderer::InstanceInput) -> Option<[f32; 4]> {
    if inst.shadow_inset != 0.0 || inst.shadow_color[3] <= 0.05 || inst.shadow_blur <= 0.0 {
        return None;
    }
    let b = instance_paint_bounds(inst);
    if b[2] <= 0.0 || b[3] <= 0.0 {
        return None;
    }
    Some(b)
}

/// Gallery inset card: overflow:hidden + Md halo. A live descendant (spin) must not
/// PartialLoad src-over that halo — dest-reset has to wipe whatever the scissor paints.
#[test]
fn inset_outset_shadow_must_not_stack_on_live_child_ticks() {
    crate::style::utility::set_frame_dt(1.0 / 60.0);
    let _ = take_frame_paint_nodes();

    let mut root = leak_shell();
    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 1280.0, 800.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    let inset = find_slot(&root, "sidebar-inset").expect("sidebar-inset");
    let inset_key = paint_element_key(inset);
    let inset_box = inset.layout_bounds;
    let inset_paint = first
        .node_paints
        .iter()
        .find(|p| p.key == inset_key)
        .expect("inset NodePaint");
    let shadow = first.instances[inset_paint.instance.0..inset_paint.instance.1]
        .iter()
        .find_map(outset_shadow_blob)
        .expect("inset outset shadow instance");
    assert!(
        shadow[0] < inset_box[0] - 4.0 && shadow[1] < inset_box[1] - 4.0,
        "precondition: Md halo extends past the inset box\n shadow={} inset={}",
        fmt_rect(shadow),
        fmt_rect(inset_box)
    );

    let prev = node_paint_visual_bounds(
        inset_paint,
        &first.instances,
        &[],
        &first.text_nodes,
        &first.input_chrome_instances,
    );
    let (wipe, _) = node_present_dest_reset_scaled(
        inset_key,
        inset_box,
        &first.node_paints,
        &first.instances,
        &[],
        1.0,
        Some(prev),
    )
    .expect("inset dest-reset");
    let scissor = node_present_scissor_logical(
        PaintDirtyKind::Node,
        inset_key,
        inset_box,
        &[(inset_key, prev)],
        &first.node_paints,
        NODE_SCISSOR_PAD,
    );
    let painted = intersect_rects(shadow, scissor).expect("inset halo sits in node scissor");
    assert!(
        aabb_covers(wipe, painted),
        "dirtying the inset must dest-reset the halo the scissor will paint\n wipe={} painted={} scissor={} shadow={} prev={} overflow={:?} bounds={}",
        fmt_rect(wipe),
        fmt_rect(painted),
        fmt_rect(scissor),
        fmt_rect(shadow),
        fmt_rect(prev),
        inset_paint.overflow_clip.map(fmt_rect),
        fmt_rect(inset_paint.bounds)
    );

    let mut impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut root,
        [-1.0, -1.0],
        "",
        false,
        1280.0,
        800.0,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    let dirties = take_frame_paint_nodes();
    let collected =
        tree.build_and_layout_fast(&mut root, 1280.0, 800.0, [-1.0, -1.0], false, false);
    let inset_paint = collected
        .node_paints
        .iter()
        .find(|p| p.key == inset_key)
        .expect("inset NodePaint after tick");
    let shadow = collected.instances[inset_paint.instance.0..inset_paint.instance.1]
        .iter()
        .find_map(outset_shadow_blob)
        .expect("inset outset shadow after tick");

    let mut stacked = Vec::new();
    for d in dirties.iter().filter(|d| d.kind == PaintDirtyKind::Node) {
        if paint_dirty_nested_in_larger_node(d, &dirties) {
            continue;
        }
        let has_keyed = paint_dirty_has_keyed_slot(
            d.key,
            d.bounds,
            collected
                .node_paints
                .iter()
                .filter(|p| !p.overlay)
                .map(|p| (p.key, p.bounds)),
        );
        let draws_halo = collected.node_paints.iter().any(|p| {
            !p.overlay
                && paint_dirty_matches_present_slot(d.key, d.bounds, p.key, p.bounds, has_keyed)
                && p.instance.0 <= inset_paint.instance.0
                && p.instance.1 >= inset_paint.instance.0 + 1
                && collected.instances[p.instance.0..p.instance.1]
                    .iter()
                    .any(|i| outset_shadow_blob(i).is_some())
        });
        if !draws_halo {
            continue;
        }
        let prev = collected
            .node_paints
            .iter()
            .find(|p| p.key == d.key)
            .map(|p| {
                node_paint_visual_bounds(
                    p,
                    &collected.instances,
                    &[],
                    &collected.text_nodes,
                    &collected.input_chrome_instances,
                )
            });
        let slots: Vec<(u64, [f32; 4])> = prev.map(|b| (d.key, b)).into_iter().collect();
        let scissor = node_present_scissor_logical(
            d.kind,
            d.key,
            d.bounds,
            &slots,
            &collected.node_paints,
            NODE_SCISSOR_PAD,
        );
        let Some(painted) = intersect_rects(shadow, scissor) else {
            continue;
        };
        let wipe = node_present_dest_reset_scaled(
            d.key,
            d.bounds,
            &collected.node_paints,
            &collected.instances,
            &[],
            1.0,
            prev,
        )
        .map(|(r, _)| r);
        if wipe.is_none_or(|w| !aabb_covers(w, painted)) {
            stacked.push(format!(
                "dirty {} {} scissor={} wipe={:?} painted={} shadow={}",
                d.key,
                fmt_rect(d.bounds),
                fmt_rect(scissor),
                wipe.map(fmt_rect),
                fmt_rect(painted),
                fmt_rect(shadow)
            ));
        }
    }
    assert!(
        stacked.is_empty(),
        "live child PartialLoad would src-over the inset halo without dest-reset:\n{}",
        stacked.join("\n")
    );
}

fn layout_present_slots(result: &RenderResult) -> Vec<(u64, [f32; 4])> {
    result
        .node_paints
        .iter()
        .filter(|p| !p.overlay)
        .map(|p| (p.key, p.bounds))
        .collect()
}

fn visual_present_slots(result: &RenderResult) -> Vec<(u64, [f32; 4])> {
    last_present_visual_slots(
        &result.node_paints,
        &result.instances,
        &[],
        &result.text_nodes,
        &result.input_chrome_instances,
    )
}

fn slot_of(slots: &[(u64, [f32; 4])], key: u64) -> Option<[f32; 4]> {
    slots.iter().find(|(k, _)| *k == key).map(|(_, b)| *b)
}

/// `encode_node_present` unions dest-reset into the GPU scissor after overflow clip.
fn encode_node_scissor(
    dirty: &PaintDirty,
    prev_slots: &[(u64, [f32; 4])],
    result: &RenderResult,
    prev_for_key: Option<[f32; 4]>,
) -> ([f32; 4], Option<[f32; 4]>) {
    let mut scissor = node_present_scissor_logical(
        dirty.kind,
        dirty.key,
        dirty.bounds,
        prev_slots,
        &result.node_paints,
        NODE_SCISSOR_PAD,
    );
    let wipe = node_present_dest_reset_scaled(
        dirty.key,
        dirty.bounds,
        &result.node_paints,
        &result.instances,
        &[],
        1.0,
        prev_for_key,
    )
    .map(|(r, _)| r);
    if let Some(rect) = wipe {
        scissor = union_rects(scissor, rect);
    }
    (scissor, wipe)
}

fn inset_halo_stack_reasons(
    dirties: &[PaintDirty],
    result: &RenderResult,
    prev_slots: &[(u64, [f32; 4])],
    inset_key: u64,
    shadow: [f32; 4],
) -> Vec<String> {
    let inset_paint = result
        .node_paints
        .iter()
        .find(|p| p.key == inset_key)
        .expect("inset NodePaint");
    let mut stacked = Vec::new();
    for d in dirties.iter().filter(|d| d.kind == PaintDirtyKind::Node) {
        let skip_wipe = paint_dirty_nested_in_larger_node(d, dirties);
        let has_keyed = paint_dirty_has_keyed_slot(
            d.key,
            d.bounds,
            result
                .node_paints
                .iter()
                .filter(|p| !p.overlay)
                .map(|p| (p.key, p.bounds)),
        );
        let draws_halo = result.node_paints.iter().any(|p| {
            !p.overlay
                && paint_dirty_matches_present_slot(d.key, d.bounds, p.key, p.bounds, has_keyed)
                && p.instance.0 <= inset_paint.instance.0
                && p.instance.1 >= inset_paint.instance.0 + 1
                && result.instances[p.instance.0..p.instance.1]
                    .iter()
                    .any(|i| outset_shadow_blob(i).is_some())
        });
        if !draws_halo {
            continue;
        }
        let prev = slot_of(prev_slots, d.key);
        let (scissor, wipe) = if skip_wipe {
            (
                node_present_scissor_logical(
                    d.kind,
                    d.key,
                    d.bounds,
                    prev_slots,
                    &result.node_paints,
                    NODE_SCISSOR_PAD,
                ),
                None,
            )
        } else {
            encode_node_scissor(d, prev_slots, result, prev)
        };
        let Some(painted) = intersect_rects(shadow, scissor) else {
            continue;
        };
        if wipe.is_none_or(|w| !aabb_covers(w, painted)) {
            stacked.push(format!(
                "dirty {} {} skip_wipe={skip_wipe} scissor={} wipe={:?} painted={} shadow={}",
                d.key,
                fmt_rect(d.bounds),
                fmt_rect(scissor),
                wipe.map(fmt_rect),
                fmt_rect(painted),
                fmt_rect(shadow)
            ));
        }
    }
    stacked
}

/// Gallery idle: CSS ticks a descendant, but `last_present_node_bounds` is visual
/// (inset Md halo). Expand used to compare layout `p.bounds` to that visual prev,
/// treat the stable card as translated, and PartialLoad src-over the gutter halo.
#[test]
fn idle_css_tick_must_not_treat_inset_halo_as_translation() {
    crate::style::utility::set_frame_dt(1.0 / 60.0);
    let _ = take_frame_paint_nodes();

    let mut root = leak_shell();
    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 1280.0, 800.0, [-1.0, -1.0]);
    let _ = take_frame_paint_nodes();

    let inset = find_slot(&root, "sidebar-inset").expect("sidebar-inset");
    let inset_key = paint_element_key(inset);
    let inset_layout = inset.layout_bounds;
    let prev_visual = visual_present_slots(&first);
    let prev_inset = slot_of(&prev_visual, inset_key).expect("inset last-present");
    assert!(
        node_present_slot_moved(prev_inset, inset_layout),
        "precondition: Md halo makes last-present visual ≠ layout box\n visual={} layout={}",
        fmt_rect(prev_inset),
        fmt_rect(inset_layout)
    );

    let mut prev_visual = prev_visual;
    let mut impact = LayoutStatus::Clean;
    for tick in 0..5 {
        tick_active_css_transitions_subtree(
            &mut root,
            [-1.0, -1.0],
            "",
            false,
            1280.0,
            800.0,
            [0.0, 0.0],
            &mut tree,
            &mut impact,
        );
        let mut dirties = take_frame_paint_nodes();
        let collected =
            tree.build_and_layout_fast(&mut root, 1280.0, 800.0, [-1.0, -1.0], false, false);
        let now_visual = visual_present_slots(&collected);
        let now_layout = layout_present_slots(&collected);
        let now_inset_visual = slot_of(&now_visual, inset_key).expect("inset visual");
        let prev_inset_visual = slot_of(&prev_visual, inset_key).unwrap_or(prev_inset);
        assert!(
            !node_present_slot_moved(prev_inset_visual, now_inset_visual),
            "tick {tick}: inset visual jittered without a layout move\n prev={} now={}",
            fmt_rect(prev_inset_visual),
            fmt_rect(now_inset_visual)
        );

        if tick == 0 {
            let mut mixed = Vec::new();
            expand_paint_dirties_for_translated_slots(&mut mixed, &now_layout, &prev_visual);
            assert!(
                mixed.iter().any(|d| d.key == inset_key),
                "canary: layout-now vs visual-prev must still look like a move (last-present space)"
            );
        }

        // Same comparison `render_inner` must use: visual now vs visual last-present.
        expand_paint_dirties_for_translated_slots(&mut dirties, &now_visual, &prev_visual);
        assert!(
            dirties
                .iter()
                .all(|d| d.kind != PaintDirtyKind::Node || d.key != inset_key),
            "tick {tick}: idle CSS must not expand-dirty the inset card\n dirties={:?}\n layout={} visual_prev={} visual_now={}",
            dirties
                .iter()
                .map(|d| format!("{} {}", d.key, fmt_rect(d.bounds)))
                .collect::<Vec<_>>(),
            fmt_rect(
                slot_of(&now_layout, inset_key).unwrap_or([0.0; 4])
            ),
            fmt_rect(slot_of(&prev_visual, inset_key).unwrap_or([0.0; 4])),
            fmt_rect(now_inset_visual)
        );

        let inset_paint = collected
            .node_paints
            .iter()
            .find(|p| p.key == inset_key)
            .expect("inset NodePaint");
        let shadow = collected.instances[inset_paint.instance.0..inset_paint.instance.1]
            .iter()
            .find_map(outset_shadow_blob)
            .expect("inset outset shadow");
        let stacked =
            inset_halo_stack_reasons(&dirties, &collected, &prev_visual, inset_key, shadow);
        assert!(
            stacked.is_empty(),
            "tick {tick}: idle PartialLoad would src-over the inset halo:\n{}",
            stacked.join("\n")
        );
        prev_visual = now_visual;
    }
}
