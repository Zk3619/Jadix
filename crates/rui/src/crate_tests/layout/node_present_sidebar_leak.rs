//! Input paint-only PartialLoad must not stamp sidebar captions into the retain buffer.
//!
//! Gallery repro: focus / type in the Kbd Search field → leftover glyphs on the left rail.
//! Capture text is clipped to the owning dirty box and submitted once; this test checks
//! an inset shell so Input's dirty AABB never swallows the rail captions.

use crate::app::damage::{
    force_full_redraw_after_chrome, paint_element_key, push_frame_paint_damage,
    take_frame_paint_nodes, PaintDirty, PaintDirtyKind,
};
use crate::app::draw_cull::{
    node_present_capture_text_indices, node_present_dest_reset, node_present_foreign_glyph_indices,
    text_paint_bounds,
};
use crate::components::input::{focus_id_for_input, input, input_value_signal_key};
use crate::components::input_adornment::input_adornment;
use crate::components::internal::text_edit::sync_text_edit_paint_subtree_in_viewport;
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
use crate::style::{apply_class_list, sync_class_variants_subtree, sync_focus_ring_subtree};
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
        class: None,
        on_click: None,
        children: Some(Element::from(label)),
        base: None,
    })
}

fn leak_shell() -> Element {
    sidebar_provider_scope("gal-shell", None, true, Some("inset"), None, || {
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
                class: None,
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
        ),
        "focus must PartialLoad named nodes, not FullClear"
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
