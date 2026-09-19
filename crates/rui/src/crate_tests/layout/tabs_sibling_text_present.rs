//! Gallery Forms: switching Tabs must not punch 8b ToggleGroup glyphs for one frame.
//!
//! `<TabsContent>` reconcile used to dest-reset the page `ScrollArea` clip (the whole
//! inset). PartialLoad wiped sibling captions; the next empty-damage frame FullCleared
//! — one-frame light/dark flash on 8b.

use crate::app::damage::{
    clipped_island_structure_damage, expand_paint_dirties_for_translated_slots,
    force_full_redraw_after_chrome, intersect_rects, take_frame_paint_nodes,
    unnamed_translated_page_slots, PaintDirty, PaintDirtyKind,
};
use crate::app::draw_cull::{node_present_capture_text_indices, padded_xywh};
use crate::components::tabs::{
    tabs_scope, TabsContent, TabsContentProps, TabsList, TabsListProps, TabsTrigger,
    TabsTriggerProps,
};
use crate::core::reactivity::{create_root, use_signal, Signal};
use crate::core::tree::{
    sync_reactive_elements_subtree_ex, Element, LayoutStatus, LayoutTree, ReactiveSyncCollect,
};
use crate::layout::layout_tree::RenderResult;
use crate::primitives::r#box::vstack;
use crate::renderer::font_loader;
use crate::text::install_shared_font_system;

const SIB_TITLE: &str = "8b) ToggleGroup";
const SIB_CAPTION: &str = "Dual style chips stay on";
const NODE_SCISSOR_PAD: f32 = 8.0;

fn fmt_rect(r: [f32; 4]) -> String {
    format!("[{:.1},{:.1} {:.1}x{:.1}]", r[0], r[1], r[2], r[3])
}

fn frag(children: Vec<Element>) -> Element {
    let mut el = Element::new();
    el.set_attr("rui_fragment", "1");
    el.children(children)
}

fn tabs_and_sibling(value: Signal<String>, tall_list: bool) -> Element {
    let list_text = if tall_list {
        "List panel — click a tab or use arrows then Enter."
    } else {
        "List panel"
    };
    let tabs = tabs_scope(
        "gal-tabs-8b",
        Some(value),
        "list",
        "horizontal",
        false,
        || {
            let list = TabsList(TabsListProps {
                id: String::new(),
                variant: None,
                value: None,
                default_value: None,
                options: None,
                children: Some(frag(vec![
                    TabsTrigger(TabsTriggerProps {
                        value: Some("list".into()),
                        disabled: None,
                        children: Some(Element::new().text("List")),
                        base: None,
                    }),
                    TabsTrigger(TabsTriggerProps {
                        value: Some("grid".into()),
                        disabled: None,
                        children: Some(Element::new().text("Grid")),
                        base: None,
                    }),
                ])),
                base: None,
            });
            frag(vec![
                list,
                TabsContent(TabsContentProps {
                    value: Some("list".into()),
                    children: Some(Element::new().text(list_text)),
                    ..Default::default()
                }),
                TabsContent(TabsContentProps {
                    value: Some("grid".into()),
                    children: Some(Element::new().text("Grid panel")),
                    ..Default::default()
                }),
            ])
        },
    );
    let sibling = Element::from(
        vstack()
            .w("100%")
            .gap("0.75rem")
            .child(
                Element::new()
                    .text(SIB_TITLE)
                    .font_weight(700.0)
                    .text_size(14.0),
            )
            .child(
                Element::new()
                    .text(SIB_CAPTION)
                    .text_size(12.0)
                    .text_color("muted-foreground"),
            )
            .child(
                Element::new()
                    .text("Left")
                    .text_size(14.0)
                    .font_weight(500.0),
            )
            .child(
                Element::new()
                    .text("Center")
                    .text_size(14.0)
                    .font_weight(500.0),
            ),
    );
    Element::from(vstack().w(480.0).gap("1rem").child(tabs).child(sibling))
}

fn sibling_runs(result: &RenderResult) -> Vec<&crate::core::tree::TextLayoutInfo> {
    result
        .text_nodes
        .iter()
        .filter(|n| {
            n.text == SIB_TITLE || n.text == SIB_CAPTION || n.text == "Left" || n.text == "Center"
        })
        .collect()
}

fn present_slots(result: &RenderResult) -> Vec<(u64, [f32; 4])> {
    result
        .node_paints
        .iter()
        .filter(|p| !p.overlay)
        .map(|p| (p.key, p.bounds))
        .collect()
}

#[test]
fn scroll_clip_must_not_be_tabs_island_damage() {
    let island = [16.0, 80.0, 400.0, 48.0];
    let sibling = [16.0, 200.0, 400.0, 160.0];
    let clip = [0.0, 0.0, 720.0, 640.0];
    assert!(
        intersect_rects(clip, sibling).is_some(),
        "page ScrollArea clip covers 8b — that was the punch"
    );
    let d = clipped_island_structure_damage(island, Some(clip)).expect("island in clip");
    assert!(
        intersect_rects(d, sibling).is_none(),
        "island damage {} must not cover 8b {}",
        fmt_rect(d),
        fmt_rect(sibling)
    );
    assert!(
        clipped_island_structure_damage(island, None).is_none(),
        "unclipped page Show still FullClears"
    );
}

#[test]
fn translated_sibling_is_named_instead_of_left_unnamed() {
    let mut dirties = vec![PaintDirty {
        key: 1,
        bounds: [10.0, 10.0, 400.0, 80.0],
        kind: PaintDirtyKind::Node,
    }];
    let prev = vec![
        (1, [10.0, 10.0, 400.0, 80.0]),
        (8, [10.0, 200.0, 400.0, 40.0]),
    ];
    let now = vec![
        (1, [10.0, 10.0, 400.0, 40.0]),
        (8, [10.0, 160.0, 400.0, 40.0]),
    ];
    assert!(
        unnamed_translated_page_slots(&now, &prev, &dirties),
        "8b moved and was not named"
    );
    expand_paint_dirties_for_translated_slots(&mut dirties, &now, &prev);
    assert!(
        !unnamed_translated_page_slots(&now, &prev, &dirties),
        "expand must name 8b so dest-reset covers prev∪now"
    );
    assert!(
        dirties.iter().any(|d| d.key == 8),
        "8b slot must become a Node dirty, got {dirties:?}"
    );
}

#[test]
fn equal_height_tab_switch_must_not_fullclear_or_wipe_8b_text() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);
    let _ = take_frame_paint_nodes();

    let (_owner, (mut root, value)): (_, (Element, Signal<String>)) = create_root(|| {
        let value = use_signal("gal_tabs_8b_eq", || "list".to_string());
        (tabs_and_sibling(value, false), value)
    });

    let mut tree = LayoutTree::new();
    let mut impact0 = LayoutStatus::Clean;
    let mut collect0 = ReactiveSyncCollect::default();
    sync_reactive_elements_subtree_ex(
        &mut root,
        &mut tree,
        &mut impact0,
        640.0,
        720.0,
        [0.0, 0.0],
        false,
        &mut collect0,
    );
    let _ = take_frame_paint_nodes();
    let first = tree.build_and_layout(&mut root, 640.0, 720.0, [-1.0, -1.0]);
    let before = sibling_runs(&first);
    assert_eq!(
        before.len(),
        4,
        "expected 8b title/caption/chips, got {:?}",
        first
            .text_nodes
            .iter()
            .map(|n| n.text.as_str())
            .collect::<Vec<_>>()
    );
    let before_snap: Vec<(String, [u8; 4], u16, f32)> = before
        .iter()
        .map(|n| {
            (
                n.text.clone(),
                n.style.color_resolved(),
                n.style.weight_resolved(),
                n.y,
            )
        })
        .collect();
    let prev_slots = present_slots(&first);
    let _ = take_frame_paint_nodes();

    value.set("grid".to_string());
    let mut impact = LayoutStatus::Clean;
    let mut collect = ReactiveSyncCollect::default();
    sync_reactive_elements_subtree_ex(
        &mut root,
        &mut tree,
        &mut impact,
        640.0,
        720.0,
        [0.0, 0.0],
        false,
        &mut collect,
    );
    assert!(
        !collect.page_layout_changed,
        "TabsContent display toggle must not page-FullClear; impact={impact:?}"
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
        "named tab panel must PartialLoad, not FullClear the 8b card"
    );

    let mut dirties = take_frame_paint_nodes();
    tree.mark_dirty_nodes_in_tree(&root, &collect.layout_dirty.iter().copied().collect());
    let second =
        tree.build_and_layout_fast_ex(&mut root, 640.0, 720.0, [-1.0, -1.0], false, false, None);
    let after = sibling_runs(&second);
    assert_eq!(after.len(), before_snap.len());
    for (n, (text, color, weight, y)) in after.iter().zip(before_snap.iter()) {
        assert_eq!(n.text, *text);
        assert_eq!(
            n.style.color_resolved(),
            *color,
            "{text} color must not flash on tab switch"
        );
        assert_eq!(
            n.style.weight_resolved(),
            *weight,
            "{text} weight must not flash on tab switch"
        );
        assert!(
            (n.y - y).abs() <= 0.5,
            "{text} Y moved on equal-height tab swap: {y} → {}",
            n.y
        );
    }

    let curr_slots = present_slots(&second);
    expand_paint_dirties_for_translated_slots(&mut dirties, &curr_slots, &prev_slots);
    assert!(
        !unnamed_translated_page_slots(&curr_slots, &prev_slots, &dirties),
        "equal-height swap must not leave unnamed translations"
    );

    let capture =
        node_present_capture_text_indices(&dirties, &second.text_nodes, &second.node_paints);
    for n in after {
        let g = [n.x, n.y, n.width, n.height];
        for d in &dirties {
            let scissor = padded_xywh(d.bounds, NODE_SCISSOR_PAD);
            if intersect_rects(scissor, g).is_none() {
                continue;
            }
            let owned = capture
                .iter()
                .any(|&i| second.text_nodes.get(i).is_some_and(|c| c.text == n.text));
            assert!(
                owned,
                "tab dirty scissor {} overlaps 8b `{}` {} without capturing it\n dirty={}",
                fmt_rect(scissor),
                n.text,
                fmt_rect(g),
                fmt_rect(d.bounds)
            );
        }
    }
}
