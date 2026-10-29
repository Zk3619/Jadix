use super::*;

#[test]
fn transparent_border_has_zero_width_by_default() {
    assert_eq!(scrollbar_border_width([0.0; 4], None), 0.0);
    assert_eq!(scrollbar_border_width([1.0, 1.0, 1.0, 1.0], None), 1.0);
}

#[test]
fn arrows_remain_outside_tracks_after_runtime_prepass_and_scroll_both_axes() {
    use crate::core::reactivity::{create_root, use_signal};
    use crate::core::scroll_viewport::ScrollViewportSignalKeys;
    use crate::core::tree::{find_in_tree, LayoutTree};
    for (inset, behavior) in [
        ("inside", "jump"),
        ("outside", "jump"),
        ("inside", "smooth"),
        ("outside", "smooth"),
    ] {
        let (_owner, ()) = create_root(|| {
            let id = format!("arrow-geometry-{inset}-{behavior}");
            let mut root: Element = crate::components::scroll_area(&id)
                .w(240.0)
                .h(160.0)
                .scrollbar_inset(inset)
                .scrollbar_visibility("always")
                .scrollbar_arrows(true)
                .scrollbar_arrow_class("rounded-sm hover:bg-primary")
                .scroll_behavior(behavior)
                .child(Element::new().w(600.0).h(700.0).flex_shrink(0.0))
                .into();
            let mut layout = LayoutTree::new();
            crate::app::recompute_scroll_viewport_flags(&mut root);
            let keys = ScrollViewportSignalKeys::new(&id);
            for (key, value) in [
                (&keys.viewport_w, 240.0_f32),
                (&keys.viewport_h, 160.0),
                (&keys.content_w, 600.0),
                (&keys.content_h, 700.0),
            ] {
                use_signal(key.as_str(), || value).set_silent(value);
            }
            for _ in 0..3 {
                crate::app::update_scrollbar_elements_for_test(
                    &mut root,
                    &mut layout,
                    crate::app::ScrollbarUpdateMode::Full,
                );
                layout.build_and_layout(&mut root, 240.0, 160.0, [-1.0; 2]);
            }
            for axis in ["x", "y"] {
                let focus = format!("scrollbar-track:{id}:{axis}");
                let track =
                    find_in_tree(&root, |el| el.focus_id.as_deref() == Some(&focus)).unwrap();
                let siblings = find_in_tree(&root, |el| {
                    el.children
                        .iter()
                        .any(|child| child.focus_id.as_deref() == Some(&focus))
                })
                .unwrap();
                let start = &siblings.children[0];
                let end = &siblings.children[2];
                let main = usize::from(axis == "y");
                assert!(
                    track.layout_bounds[main + 2] > 20.0,
                    "{inset} {axis}: {:?}",
                    track.layout_bounds
                );
                let thumb = &track.children[0];
                assert!(thumb.layout_bounds[main + 2] > 0.0);
                assert!(
                    thumb.layout_bounds[main] + thumb.layout_bounds[main + 2]
                        <= track.layout_bounds[main] + track.layout_bounds[main + 2] + 0.5
                );
                assert!(start.layout_bounds[main + 2] > 0.0);
                assert!(end.layout_bounds[main + 2] > 0.0);
                assert!(
                    start.layout_bounds[main] + start.layout_bounds[main + 2]
                        <= track.layout_bounds[main] + 0.5
                );
                assert!(
                    track.layout_bounds[main] + track.layout_bounds[main + 2]
                        <= end.layout_bounds[main] + 0.5
                );
                let keys = ScrollViewportSignalKeys::new(&id);
                let offset = if axis == "x" {
                    &keys.offset_x
                } else {
                    &keys.offset_y
                };
                let key = if behavior == "smooth" {
                    if axis == "x" {
                        &keys.target_x
                    } else {
                        &keys.target_y
                    }
                } else {
                    offset
                };
                (end.on_click.as_ref().unwrap().borrow_mut())();
                assert_eq!(use_signal(key.as_str(), || 0.0_f32).peek(), 25.0);
                if behavior == "smooth" {
                    assert_eq!(use_signal(offset.as_str(), || 0.0_f32).peek(), 0.0);
                }
                (start.on_click.as_ref().unwrap().borrow_mut())();
                assert_eq!(use_signal(key.as_str(), || 0.0_f32).peek(), 0.0);
            }
            use_signal(keys.overflow_y.as_str(), || String::new()).set_silent("hidden".into());
            crate::app::update_scrollbar_elements_for_test(
                &mut root,
                &mut layout,
                crate::app::ScrollbarUpdateMode::Full,
            );
            let group = find_in_tree(&root, |el| {
                el.get_attr(crate::core::scrollbar::ATTR_SCROLLBAR_GROUP)
                    .map(String::as_str)
                    == Some(format!("{id}:y").as_str())
            })
            .unwrap();
            assert_eq!(group.style.display, taffy::style::Display::None);
            use_signal(keys.overflow_y.as_str(), || String::new()).set_silent("auto".into());
            crate::app::update_scrollbar_elements_for_test(
                &mut root,
                &mut layout,
                crate::app::ScrollbarUpdateMode::Full,
            );
            let group = find_in_tree(&root, |el| {
                el.get_attr(crate::core::scrollbar::ATTR_SCROLLBAR_GROUP)
                    .map(String::as_str)
                    == Some(format!("{id}:y").as_str())
            })
            .unwrap();
            assert_eq!(group.style.display, taffy::style::Display::Flex);
        });
    }
}
