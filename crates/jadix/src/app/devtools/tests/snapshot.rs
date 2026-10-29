use super::*;
use crate::core::tree::Element;

#[test]
fn path_roundtrip() {
    let p = vec![0, 2, 1];
    assert_eq!(key_to_path(&path_to_key(&p)), p);
    assert_eq!(path_to_key(&[]), ROOT_PATH_KEY);
    assert_eq!(key_to_path(ROOT_PATH_KEY), Vec::<usize>::new());
    assert_eq!(key_to_path(""), Vec::<usize>::new());
}

#[test]
fn icon_previews_use_semantic_names_not_private_use_codepoints() {
    let mut icon = Element::new().text("\u{e001}");
    icon.set_attr(crate::utils::ATTR_ICON_NAME, "panels-top-left");
    assert_eq!(
        text_preview_from(&icon).as_deref(),
        Some("[icon: panels-top-left]")
    );
    let anonymous_icon = Element::new().text("\u{e002}");
    assert_eq!(
        text_preview_from(&anonymous_icon).as_deref(),
        Some("[icon]")
    );
    assert_eq!(
        text_preview_from(&Element::new().text("表单")).as_deref(),
        Some("表单")
    );
}

#[test]
fn inspector_pick_can_select_pointer_transparent_content() {
    let mut content = Element::new()
        .template_id("performance-hud")
        .pointer_events("none");
    content.layout_bounds = [10.0, 10.0, 80.0, 50.0];
    let mut root = Element::new().child(content);
    root.layout_bounds = [0.0, 0.0, 200.0, 100.0];
    assert_eq!(
        pick_element_path(&root, 20.0, 20.0, [0.0; 2]),
        Some(vec![0])
    );
}

#[test]
fn snapshot_tree_depth_first() {
    let root = Element::new()
        .focus_id("root")
        .child(Element::new().focus_id("a"))
        .child(Element::new().focus_id("b").text("hello"));
    let nodes = snapshot_tree(&root);
    assert_eq!(nodes.len(), 3);
    assert_eq!(nodes[0].focus_id.as_deref(), Some("root"));
    assert_eq!(nodes[2].text_preview.as_deref(), Some("hello"));
}

#[test]
fn find_by_path() {
    let root = Element::new().children(vec![
        Element::new().focus_id("a"),
        Element::new().children(vec![Element::new().focus_id("b")]),
    ]);
    let b = find_element_by_path(&root, &[1, 0]).unwrap();
    assert_eq!(b.focus_id.as_deref(), Some("b"));
}

#[test]
fn snapshot_hover_anchor_uses_the_runtime_instance_of_the_leaf() {
    let root = Element::new().template_id("card-root").children(vec![
        Element::new().text("title"),
        Element::new().children(vec![Element::new().text("desc")]),
    ]);
    let nodes = snapshot_tree(&root);
    let desc = nodes
        .iter()
        .find(|n| n.text_preview.as_deref() == Some("desc"))
        .expect("desc");
    let (id, rel) = snapshot_hover_anchor(&nodes, desc);
    assert_eq!(id, format!("runtime:{}", desc.instance_id));
    assert!(rel.is_empty());
}

#[test]
fn normalize_promotes_class_parent_over_text_leaf() {
    let mut label_el = Element::new().text("Nav");
    label_el.layout_bounds = [12.0, 12.0, 40.0, 16.0];
    let mut host = Element::new();
    let _ = crate::style::apply_class_list(&mut host, "gal-nav-text");
    host.layout_bounds = [10.0, 10.0, 60.0, 20.0];
    host = host.child(label_el);
    let path = normalize_devtools_pick(&host, vec![0]);
    assert_eq!(path, Vec::<usize>::new());
}

#[test]
fn pick_hits_nested_child() {
    let mut leaf = Element::new().focus_id("leaf");
    leaf.layout_bounds = [10.0, 10.0, 20.0, 20.0];
    let mut parent = Element::new().focus_id("parent").child(leaf);
    parent.layout_bounds = [0.0, 0.0, 100.0, 100.0];
    assert_eq!(
        pick_element_path(&parent, 15.0, 15.0, [0.0, 0.0]),
        Some(vec![0])
    );
}

#[test]
fn pick_and_highlight_use_visual_coords_under_scroll() {
    use taffy::style::Overflow;

    let mut target = Element::new().focus_id("btn").template_id("btn");
    target.layout_bounds = [10.0, 250.0, 80.0, 40.0];

    let mut viewport = Element::new();
    viewport.focus_id = Some("scroll:main".into());
    viewport.style.overflow.y = Overflow::Scroll;
    crate::app::devtools::set_test_scroll_offset(&mut viewport, [0.0, 100.0]);
    viewport.layout_bounds = [0.0, 0.0, 400.0, 200.0];
    viewport = viewport.child(target);

    // Visual y = 250 − 100 = 150
    assert_eq!(
        pick_element_path(&viewport, 40.0, 150.0, [0.0, 0.0]),
        Some(vec![0]),
        "pick must hit scrolled visual position"
    );
    assert!(
        pick_element_path(&viewport, 40.0, 250.0, [0.0, 0.0]) != Some(vec![0]),
        "abs layout y must not win after scroll"
    );

    let acc = scroll_acc_along_path(&viewport, &[0]);
    assert!(
        (acc[1] - 100.0).abs() < 0.5,
        "scroll_acc along path={acc:?}"
    );
    let bm = box_model_for_highlight(&viewport, &[0]).expect("bm");
    assert!(
        (bm.y - 150.0).abs() < 0.5,
        "highlight must stamp visual y, got {}",
        bm.y
    );

    // Translate is part of paint-space geometry (collect `render_*`).
    viewport.children[0].style.translate = [0.0, 12.0];
    let bm_tx = box_model_for_highlight(&viewport, &[0]).expect("bm+translate");
    assert!(
        (bm_tx.y - 162.0).abs() < 0.5,
        "highlight must include translate, got {}",
        bm_tx.y
    );
    assert_eq!(
        pick_element_path(&viewport, 40.0, 162.0, [0.0, 0.0]),
        Some(vec![0]),
        "pick must include translate like paint"
    );

    // `scroll:` still contributes when overflow was cleared to Visible (layout rule).
    viewport.children[0].style.translate = [0.0, 0.0];
    viewport.style.overflow.y = Overflow::Visible;
    assert_eq!(
        pick_element_path(&viewport, 40.0, 150.0, [0.0, 0.0]),
        Some(vec![0]),
        "scroll: viewport must contribute offset even if overflow is Visible"
    );
}

#[test]
fn pick_and_highlight_align_under_docked_host_and_scroll() {
    use taffy::style::Overflow;

    // wrap_app shape: host (overflow hidden, squeezed) + panel slot.
    let mut target = Element::new().focus_id("card").template_id("card");
    target.layout_bounds = [20.0, 300.0, 100.0, 40.0];

    let mut viewport = Element::new();
    viewport.focus_id = Some("scroll:gallery".into());
    viewport.style.overflow.y = Overflow::Scroll;
    crate::app::devtools::set_test_scroll_offset(&mut viewport, [0.0, 120.0]);
    viewport.layout_bounds = [0.0, 0.0, 680.0, 800.0];
    viewport = viewport.child(target);

    let mut host = Element::new();
    host.template_id = Some("__jadix_app_host".into());
    host.style.overflow.x = Overflow::Hidden;
    host.style.overflow.y = Overflow::Hidden;
    host.layout_bounds = [0.0, 0.0, 680.0, 800.0];
    let mut hl = Element::new();
    hl.template_id = Some(crate::app::devtools::HIGHLIGHT_SLOT_ID.into());
    hl.layout_bounds = [0.0, 0.0, 680.0, 800.0];
    host = host.children(vec![viewport, hl]);

    let mut panel = Element::new();
    panel.template_id = Some("__jadix_devtools_panel_slot".into());
    panel.layout_bounds = [680.0, 0.0, 320.0, 800.0];

    let mut root = Element::new().children(vec![host, panel]);
    root.layout_bounds = [0.0, 0.0, 1000.0, 800.0];

    // Visual y = 300 − 120 = 180
    let path = pick_element_path(&root, 50.0, 180.0, [0.0, 0.0]).expect("pick");
    let hit = find_element_by_path(&root, &path).expect("resolve");
    assert_eq!(hit.focus_id.as_deref(), Some("card"));
    assert!(
        !path_is_devtools_chrome_for_test(&root, &path),
        "must not pick highlight/panel chrome"
    );

    let bm = box_model_for_highlight(&root, &path).expect("bm");
    assert!(
        (bm.y - 180.0).abs() < 0.5,
        "docked+scrolled highlight must match pick visual y, got {}",
        bm.y
    );
    assert!(
        (bm.x - 20.0).abs() < 0.5,
        "highlight x must stay in squeezed host, got {}",
        bm.x
    );
}

fn path_is_devtools_chrome_for_test(root: &Element, path: &[usize]) -> bool {
    let mut el = root;
    if el
        .template_id
        .as_deref()
        .is_some_and(|id| id.starts_with("__jadix_devtools"))
    {
        return true;
    }
    for &idx in path {
        el = match el.children.get(idx) {
            Some(c) => c,
            None => return false,
        };
        if el
            .template_id
            .as_deref()
            .is_some_and(|id| id.starts_with("__jadix_devtools"))
        {
            return true;
        }
    }
    false
}

#[test]
fn pick_skips_portal_layer_shell() {
    let mut leaf = Element::new().focus_id("leaf");
    leaf.layout_bounds = [10.0, 10.0, 20.0, 20.0];
    let mut content = Element::new().focus_id("content").child(leaf);
    content.layout_bounds = [0.0, 0.0, 100.0, 100.0];
    let mut portal = Element::new();
    portal.template_id = Some(crate::layout::layout_tree::PORTAL_LAYER_ID.into());
    portal.layout_bounds = [0.0, 0.0, 100.0, 100.0];
    let mut root = Element::new().children(vec![content, portal]);
    root.layout_bounds = [0.0, 0.0, 100.0, 100.0];
    assert_eq!(
        pick_element_path(&root, 15.0, 15.0, [0.0, 0.0]),
        Some(vec![0, 0]),
        "portal shell must not steal the pick"
    );
}

#[test]
fn cssom_full_element_dump_lists_layout_and_text() {
    let mut el = Element::new().focus_id("btn");
    el.style.bg_color = [0.0, 0.0, 0.0, 0.0];
    el.text_style.size = Some(14.0);
    el.text_style.weight = Some(600);
    let css = cssom_text_from_element(&el);
    for needle in [
        "display:",
        "flex-direction:",
        "position:",
        "width:",
        "height:",
        "padding:",
        "margin:",
        "background:",
        "border-width:",
        "opacity:",
        "box-shadow:",
        "color:",
        "font-size:",
        "font-weight:",
        "z-index:",
        "overflow-x:",
    ] {
        assert!(css.contains(needle), "missing {needle} in:\n{css}");
    }
    assert!(css.contains("background: transparent;"));
}

#[test]
fn cssom_uses_button_rest_chrome_not_hover_paint() {
    use crate::style::parse_color;

    let mut el = Element::new().focus_id("gal-nav-data");
    el.authored_style = Some(|element| {
        let mut style = element.style.as_ref().clone();
        style.bg_color = [0.0; 4];
        style.border_width = 0.0;
        Some(style)
    });
    // Live paint after hover: accent fill (what the bug was dumping into CSSOM).
    el.style.bg_color = parse_color("accent");
    el.style.border_width = 0.0;

    let live = style_snapshot(&el.style);
    assert_ne!(live.bg, "transparent");

    let authored = style_snapshot_for_element(&el);
    assert_eq!(authored.bg, "transparent");
    let css = cssom_text_from_style(&authored);
    assert!(
        css.contains("background: transparent;"),
        "cssom must dump rest chrome, got:\n{css}"
    );
}

#[test]
fn cssom_prefers_class_set_base_style_over_live_hover() {
    let mut el = Element::new();
    let _ = crate::style::apply_class_list(&mut el, "bg-transparent hover:bg-accent");
    // Simulate resolved hover variant writing into live style.
    el.style.bg_color = crate::style::parse_color("accent");

    let snap = style_snapshot_for_element(&el);
    assert_eq!(snap.bg, "transparent");
}

#[test]
fn box_model_reads_padding() {
    let el = Element::new().p(8.0);
    let bm = box_model_snapshot(&el);
    assert!((bm.padding_top - 8.0).abs() < 0.01);
}

#[test]
fn snapshot_skips_inspector_chrome() {
    let mut overlay = Element::new().focus_id("secret");
    overlay.template_id = Some("__jadix_devtools_overlay".into());
    overlay = overlay.child(Element::new().focus_id("dock"));
    let root = Element::new()
        .focus_id("app")
        .child(Element::new().focus_id("leaf"))
        .child(overlay);
    let nodes = snapshot_tree(&root);
    assert!(nodes.iter().all(|n| n.focus_id.as_deref() != Some("dock")));
    assert!(nodes
        .iter()
        .all(|n| n.focus_id.as_deref() != Some("secret")));
    assert_eq!(nodes.len(), 2);
}

#[test]
fn template_id_source_location() {
    let loc = source_location_from_template_id("src/main.rs:42:8").unwrap();
    assert_eq!(loc.line, 42);
    assert_eq!(loc.column, 8);
    assert!(source_location_from_template_id("opaque-id").is_none());
}
