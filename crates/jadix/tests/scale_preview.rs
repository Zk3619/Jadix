#![cfg(all(feature = "scale-preview", feature = "components"))]

use jadix::components::scale_preview;
use jadix::core::reactivity::create_root;
use jadix::core::tree::{Element, LayoutTree};
use jadix::primitives::r#box::r#box;

fn find<'a>(element: &'a Element, id: &str) -> Option<&'a Element> {
    if element.template_id.as_deref() == Some(id) {
        return Some(element);
    }
    element.children.iter().find_map(|child| find(child, id))
}

#[test]
fn standalone_preview_preserves_logical_layout_and_scaled_footprint() {
    let (_owner, _) = create_root(|| {
        for scale in [0.35, 0.57, 1.0, 1.5] {
            let content = Element::new()
                .absolute()
                .left(20.0)
                .top(30.0)
                .w(100.0)
                .h(32.0)
                .bg([1.0, 0.0, 0.0, 1.0]);
            let mut root = Element::new().child(scale_preview(400.0, 300.0, scale).child(content));
            let result = LayoutTree::new().build_and_layout(&mut root, 1000.0, 800.0, [-1.0; 2]);
            let frame = &root.children[0];
            assert_eq!(frame.layout_bounds[2..], [400.0 * scale, 300.0 * scale]);
            assert_eq!(frame.children[0].layout_bounds[2..], [400.0, 300.0]);
            let fill = result
                .instances
                .iter()
                .find(|quad| quad.color[0] > 0.9)
                .unwrap();
            assert!((fill.rect_size[0] - 100.0 * scale).abs() < 0.1);
            assert!((fill.rect_size[1] - 32.0 * scale).abs() < 0.1);
            assert!((fill.rect_pos[0] - 20.0 * scale).abs() < 0.1);
            assert!((fill.rect_pos[1] - 30.0 * scale).abs() < 0.1);
        }
    });
}

#[test]
fn framework_viewport_capability_works_without_preview_component() {
    use jadix::core::scaled_viewport::configure_viewport;
    let (_owner, _) = create_root(|| {
        let mut viewport = Element::new().child(portal("custom-popup"));
        configure_viewport(&mut viewport, [400.0, 300.0], 0.5, "custom-preview-portals");
        let mut root = Element::new().child(viewport);
        let mut layout = LayoutTree::new();
        for _ in 0..3 {
            layout.build_and_layout(&mut root, 1000.0, 800.0, [-1.0; 2]);
            let viewport = &root.children[0];
            assert_eq!(viewport.layout_bounds[2..], [400.0, 300.0]);
            assert_eq!(viewport.style.scale, [0.5; 2]);
            assert!(!viewport.attrs.contains_key("data-slot"));
            let host = find(viewport, "custom-preview-portals").unwrap();
            let popup = find(host, "custom-popup").unwrap();
            assert!(!popup.style.position_fixed);
        }
    });
}

#[test]
fn standalone_preview_tags_match_builder() {
    use jadix::prelude::*;
    let (_owner, _) = create_root(|| {
        let mut root = Element::new().child(ui! {
            <ScalePreview viewport_width={400.0} viewport_height={300.0} scale={0.5}>
                <Box w="100%" h="100%" />
            </ScalePreview>
        });
        LayoutTree::new().build_and_layout(&mut root, 1000.0, 800.0, [-1.0; 2]);
        assert_eq!(root.children[0].layout_bounds[2..], [200.0, 150.0]);
        assert_eq!(root.children[0].style.scale, [1.0; 2]);
        assert_eq!(root.children[0].children[0].style.scale, [0.5; 2]);
    });
}

fn portal(id: &str) -> Element {
    let mut portal = Element::new().template_id(id).w(60.0).h(30.0);
    portal.set_attr("jadix_portal", "1");
    portal.set_attr("jadix_portal_key", id);
    portal
}

#[test]
fn previews_isolate_nested_portals_and_preserve_explicit_mounts() {
    use jadix::layout::layout_tree::{hoist_portal_subtrees, ATTR_PORTAL_MOUNT};
    let (_owner, _) = create_root(|| {
        let mut explicit = portal("explicit-popup");
        explicit.set_attr(ATTR_PORTAL_MOUNT, "external-host");
        let inner: Element = scale_preview(200.0, 160.0, 0.5)
            .child(portal("inner-popup"))
            .into();
        let outer: Element = scale_preview(400.0, 300.0, 0.5)
            .child(portal("outer-popup"))
            .child(inner)
            .child(explicit)
            .into();
        let sibling: Element = scale_preview(400.0, 300.0, 0.75)
            .child(portal("sibling-popup"))
            .into();
        let outer_mount = outer.children[0]
            .children
            .last()
            .unwrap()
            .template_id
            .clone()
            .unwrap();
        let inner_mount = outer.children[0].children[1].children[0]
            .children
            .last()
            .unwrap()
            .template_id
            .clone()
            .unwrap();
        let sibling_mount = sibling.children[0]
            .children
            .last()
            .unwrap()
            .template_id
            .clone()
            .unwrap();
        let mut root =
            Element::new().children([outer, sibling, Element::new().template_id("external-host")]);
        let mut layout = LayoutTree::new();
        for _ in 0..3 {
            layout.build_and_layout(&mut root, 1000.0, 800.0, [-1.0; 2]);
            for (mount, popup) in [
                (&outer_mount, "outer-popup"),
                (&inner_mount, "inner-popup"),
                (&sibling_mount, "sibling-popup"),
            ] {
                assert!(find(find(&root, mount).unwrap(), popup).is_some());
                assert!(!find(&root, popup).unwrap().style.position_fixed);
            }
            assert!(find(find(&root, "external-host").unwrap(), "explicit-popup").is_some());
            assert_eq!(hoist_portal_subtrees(&mut root), 0);
        }
    });
}

#[test]
fn preview_popovers_use_logical_trigger_coordinates() {
    use jadix::utils::popup::{
        mark_floating, sync_floating_placement, PopupPlacement, FLOATING_TRIGGER_GAP,
    };
    let (_owner, _) = create_root(|| {
        for scale in [0.35, 0.57, 1.0, 1.5] {
            let trigger = Element::new()
                .focus_id("preview-trigger")
                .absolute()
                .left(80.0)
                .top(60.0)
                .w(100.0)
                .h(40.0);
            let mut popup = portal("preview-popup").absolute();
            mark_floating(
                &mut popup,
                "preview-popup",
                "preview-trigger",
                PopupPlacement::BottomStart,
            );
            let mut root =
                Element::new().child(scale_preview(400.0, 300.0, scale).children([trigger, popup]));
            let mut layout = LayoutTree::new();
            layout.build_and_layout(&mut root, 1000.0, 800.0, [-1.0; 2]);
            sync_floating_placement(&mut root, 1000.0, 800.0);
            let popup = find(&root, "preview-popup").unwrap();
            let left = jadix::layout::lpa_length_px(popup.style.inset.left).unwrap();
            let top = jadix::layout::lpa_length_px(popup.style.inset.top).unwrap();
            assert!((left - 80.0).abs() < 0.1, "scale={scale}, left={left}");
            assert!(
                (top - 100.0 - FLOATING_TRIGGER_GAP).abs() < 0.1,
                "scale={scale}, top={top}"
            );
        }
    });
}

#[test]
fn scaled_preview_outline_stays_attached_to_rounded_fill() {
    let (_owner, _) = create_root(|| {
        for zoom in [0.35, 0.57, 1.0, 1.5] {
            let pill: Element = r#box()
                .w(100.0)
                .h(32.0)
                .rounded(12.0)
                .bg([1.0, 0.0, 0.0, 1.0])
                .outline([0.0, 0.0, 0.0, 1.0], 3.0)
                .into();
            let mut root = Element::new()
                .w(400.0)
                .h(240.0)
                .p(20.0)
                .scale(zoom)
                .transform_origin("left top")
                .child(pill);
            let mut layout = LayoutTree::new();
            let result = layout.build_and_layout(&mut root, 400.0, 240.0, [-1.0; 2]);
            let fill = result
                .instances
                .iter()
                .find(|quad| quad.color[0] > 0.9)
                .unwrap();
            let outline = result
                .instances
                .iter()
                .find(|quad| quad.border_width > 0.0)
                .unwrap();
            for axis in 0..2 {
                assert!(
                    (outline.rect_pos[axis] - (fill.rect_pos[axis] - 3.0 * zoom)).abs() < 0.1,
                    "zoom={zoom}, fill={:?}, outline={:?}",
                    fill.rect_pos,
                    outline.rect_pos
                );
                assert!(
                    (outline.rect_size[axis] - (fill.rect_size[axis] + 6.0 * zoom)).abs() < 0.1,
                    "zoom={zoom}, fill={:?}, outline={:?}",
                    fill.rect_size,
                    outline.rect_size
                );
            }
            assert!((outline.border_width - 3.0 * zoom).abs() < 0.1);
            assert!((outline.radii[0] - 15.0 * zoom).abs() < 0.1);
        }
    });
}

#[test]
fn bubble_reaction_outline_scales_with_its_pill() {
    let (_owner, _) = create_root(|| {
        let mut baseline = None;
        for scale in [1.0, 0.35, 0.57, 1.5] {
            let reactions = jadix::components::bubble::bubble_reactions()
                .child(Element::new().text("reaction"));
            let mut root =
                Element::new().child(scale_preview(400.0, 300.0, scale).child(reactions));
            let result = LayoutTree::new().build_and_layout(&mut root, 1000.0, 800.0, [-1.0; 2]);
            let outline = result
                .instances
                .iter()
                .find(|quad| quad.border_width > 0.0 && quad.color[3] == 0.0)
                .unwrap();
            let metrics = [
                outline.rect_size[0] / scale,
                outline.rect_size[1] / scale,
                outline.border_width / scale,
                outline.radii[0] / scale,
            ];
            if let Some(expected) = baseline {
                let expected: [f32; 4] = expected;
                for (actual, expected) in metrics.into_iter().zip(expected) {
                    assert!(
                        (actual - expected).abs() < 0.1,
                        "scale={scale}, actual={actual}, expected={expected}"
                    );
                }
            } else {
                baseline = Some(metrics);
            }
        }
    });
}
