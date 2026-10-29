use super::*;
use crate::style::{TextDirection, TextSpan};

#[test]
fn reactive_sync_keeps_the_correct_spine_across_dfs_siblings() {
    let changed = || Element::new().reactive_sync(|_, _| LayoutStatus::StyleChanged);
    let mut root = Element::new()
        .child(
            Element::new()
                .child(Element::new().child(changed()))
                .child(Element::new()),
        )
        .child(Element::new().child(Element::new()))
        .child(changed());
    crate::layout::collect_cache::stamp_subtree_paint_hashes(&mut root, false);
    walk_elements_post_order_mut(&mut root, |el| el.collect_paint_hash = 42);
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_reactive_elements_subtree(&mut root, &mut tree, &mut impact, 400.0, 300.0, [0.0; 2]);
    assert_eq!(impact, LayoutStatus::StyleChanged);
    assert_eq!(root.collect_paint_hash, 0);
    assert_eq!(root.children[0].collect_paint_hash, 0);
    assert_eq!(root.children[0].children[0].collect_paint_hash, 0);
    assert_eq!(
        root.children[0].children[0].children[0].collect_paint_hash,
        0
    );
    assert_eq!(root.children[0].children[1].collect_paint_hash, 42);
    assert_eq!(root.children[1].collect_paint_hash, 42);
    assert_eq!(root.children[1].children[0].collect_paint_hash, 42);
    assert_eq!(root.children[2].collect_paint_hash, 0);
}

#[test]
fn reactive_hover_damage_stays_inside_scrolled_viewport() {
    crate::core::reactivity::create_root(|| {
        let mut row = Element::new().reactive_sync(|element, _| {
            element.style.bg_color = [0.2, 0.3, 0.4, 1.0];
            LayoutStatus::StyleChanged
        });
        row.layout_bounds = [100.0, 210.0, 600.0, 22.0];
        let mut root = Element::new()
            .focus_id(crate::core::scroll_viewport::focus_id_for_scroll_viewport(
                "hover-damage",
            ))
            .overflow("hidden")
            .child(row);
        root.layout_bounds = [100.0, 100.0, 160.0, 100.0];
        root.scroll_offset = [80.0, 100.0];
        let mut tree = LayoutTree::new();
        let mut impact = LayoutStatus::Clean;
        crate::app::damage::take_frame_paint_nodes();
        for (offset, expected) in [
            ([80.0, 100.0], Some([100.0, 110.0, 160.0, 22.0])),
            ([400.0, 90.0], Some([100.0, 120.0, 160.0, 22.0])),
            ([400.0, 125.0], Some([100.0, 100.0, 160.0, 7.0])),
            ([400.0, 400.0], None),
        ] {
            root.scroll_offset = offset;
            sync_reactive_elements_subtree(
                &mut root,
                &mut tree,
                &mut impact,
                800.0,
                600.0,
                [0.0; 2],
            );
            let damage = crate::app::damage::take_frame_paint_nodes();
            if let Some(bounds) = expected {
                assert_eq!(damage.len(), 1);
                assert_eq!(damage[0].bounds, bounds);
                assert_eq!(damage[0].kind, crate::app::damage::PaintDirtyKind::Region);
            } else {
                assert!(damage.is_empty());
            }
        }
    });
}

#[test]
fn reactive_sync_summary_tracks_invalidated_mounts() {
    let calls = std::rc::Rc::new(std::cell::Cell::new(0));
    let mut root = Element::new().child(Element::new().child(Element::new()));
    let mut tree = LayoutTree::new();
    crate::layout::collect_cache::stamp_subtree_paint_hashes(&mut root, false);
    assert!(!root.subtree_reactive_sync);
    let counter = calls.clone();
    root.children[0]
        .children
        .push(Element::new().reactive_sync(move |_, _| {
            counter.set(counter.get() + 1);
            LayoutStatus::Clean
        }));
    crate::layout::collect_cache::invalidate_collect_paint_hash_path(&mut root, &[0]);
    let sync = |root: &mut Element, tree: &mut LayoutTree| {
        let mut impact = LayoutStatus::Clean;
        sync_reactive_elements_subtree(root, tree, &mut impact, 400.0, 300.0, [0.0; 2]);
        assert_eq!(impact, LayoutStatus::Clean);
    };
    // A newly mounted callback must run even before the next stamp.
    sync(&mut root, &mut tree);
    assert_eq!(calls.get(), 1);
    crate::layout::collect_cache::stamp_subtree_paint_hashes(&mut root, true);
    assert!(root.subtree_reactive_sync);
    assert!(!root.children[0].children[0].subtree_reactive_sync);
    sync(&mut root, &mut tree);
    assert_eq!(calls.get(), 2);
    root.children[0].children.pop();
    crate::layout::collect_cache::invalidate_collect_paint_hash_path(&mut root, &[0]);
    crate::layout::collect_cache::stamp_subtree_paint_hashes(&mut root, true);
    assert!(!root.subtree_reactive_sync);
    sync(&mut root, &mut tree);
    assert_eq!(calls.get(), 2);
}

#[test]
fn text_spans_builder_sets_concat_text() {
    let el = Element::new().text_spans(vec![
        TextSpan::new("Hello").weight(700),
        TextSpan::new(" "),
        TextSpan::new("world").color([255, 0, 0, 255]),
    ]);
    assert_eq!(el.text.as_deref(), Some("Hello world"));
    assert_eq!(el.text_spans.as_ref().map(|s| s.len()), Some(3));
}

#[test]
fn text_direction_builder() {
    let el = Element::new().text("hi").text_direction(TextDirection::Rtl);
    assert_eq!(el.text_style.direction, TextDirection::Rtl);
}

#[test]
fn text_overflow_modes_set_whitespace_and_box_overflow() {
    let clip = Element::new().text("hi").text_overflow("clip");
    assert_eq!(clip.text_style.overflow, TextOverflow::Clip);
    assert_eq!(
        clip.text_style.white_space,
        crate::style::WhiteSpace::Nowrap
    );
    assert_eq!(clip.style.overflow.x, taffy::style::Overflow::Hidden);
    assert_eq!(clip.style.overflow.y, taffy::style::Overflow::Visible);

    let ellipsis = Element::new().text("hi").text_overflow_ellipsis();
    assert_eq!(ellipsis.style.overflow.x, taffy::style::Overflow::Hidden);
    assert_eq!(ellipsis.style.overflow.y, taffy::style::Overflow::Visible);

    let hidden = Element::new().text("hi").text_overflow_hidden();
    assert_eq!(hidden.text_style.overflow, TextOverflow::Hidden);
    assert_eq!(
        hidden.text_style.white_space,
        crate::style::WhiteSpace::Nowrap
    );
    assert_eq!(hidden.style.overflow.x, taffy::style::Overflow::Hidden);
    assert_eq!(hidden.style.overflow.y, taffy::style::Overflow::Hidden);

    let wrap = Element::new()
        .text("hi")
        .text_overflow_ellipsis()
        .text_overflow_newline();
    assert_eq!(wrap.text_style.overflow, TextOverflow::Newline);
    assert_eq!(
        wrap.text_style.white_space,
        crate::style::WhiteSpace::Normal
    );
    assert_eq!(wrap.style.overflow.x, taffy::style::Overflow::Visible);
}

#[test]
fn fixed_sets_absolute_and_flag() {
    let el = Element::new().fixed();
    assert_eq!(el.style.position, Position::Absolute);
    assert!(el.style.position_fixed);
}

#[test]
fn unpack_fragment_children_expands_jadix_fragment() {
    let mut frag = Element::new();
    frag.set_attr("jadix_fragment", "1");
    let frag = frag
        .child(Element::new().text("a"))
        .child(Element::new().text("b"));
    let kids = Element::unpack_fragment_children(Some(frag));
    assert_eq!(kids.len(), 2);
    assert_eq!(kids[0].text.as_deref(), Some("a"));
    assert_eq!(kids[1].text.as_deref(), Some("b"));
}

#[test]
fn unpack_fragment_children_keeps_plain_element() {
    let el = Element::new().text("only");
    let kids = Element::unpack_fragment_children(Some(el));
    assert_eq!(kids.len(), 1);
    assert_eq!(kids[0].text.as_deref(), Some("only"));
}

#[test]
fn unpack_fragment_children_none_is_empty() {
    assert!(Element::unpack_fragment_children(None).is_empty());
}

#[test]
fn into_element_list_flattens_jadix_fragment() {
    let mut frag = Element::new();
    frag.set_attr("jadix_fragment", "1");
    let frag = frag
        .child(Element::new().text("a"))
        .child(Element::new().text("b"));
    let kids = frag.into_element_list();
    assert_eq!(kids.len(), 2);
    assert_eq!(kids[0].text.as_deref(), Some("a"));
    assert_eq!(kids[1].text.as_deref(), Some("b"));
}

#[test]
fn bind_element_ref_writes_template_id() {
    let slot = crate::core::reactivity::create_signal(None::<String>);
    let el = Element::new().template_id("tid:ref");
    bind_element_ref(&slot, &el);
    assert_eq!(slot.get().as_deref(), Some("tid:ref"));
}

#[test]
fn into_dynamic_component_calls_closure() {
    let el = (|| Element::new().text("dyn")).into_dynamic_component();
    assert_eq!(el.text.as_deref(), Some("dyn"));
    let el2 = Element::new().text("plain").into_dynamic_component();
    assert_eq!(el2.text.as_deref(), Some("plain"));
}
