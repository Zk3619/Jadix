use super::*;

#[test]
fn expanded_avatar_faces_inherit_panel_fade() {
    crate::core::reactivity::create_root(|| {
        let face = AvatarFaceSpec {
            size: AvatarSize::Default,
            shape: AvatarShape::Circle,
            src: None,
            fallback: Some("A".into()),
            class: None,
        };
        let mut root = default_expand_container(expand_group_faces(&[face]), None)
            .w(120.0)
            .h(64.0);
        let mut tree = crate::core::tree::LayoutTree::new();
        tree.set_collect_skip_allowed(true);
        tree.build_and_layout(&mut root, 120.0, 64.0, [-1.0; 2]);
        for alpha in [0.25, 0.75, 0.5, 0.1] {
            root.style.opacity = alpha;
            let result = tree.build_and_layout_fast(&mut root, 120.0, 64.0, [-1.0; 2], true, false);
            assert!(result.instances.len() >= 3);
            for instance in &result.instances {
                assert!(
                    instance.color[3] <= alpha + 0.001,
                    "face did not inherit panel fade {alpha}: {:?}",
                    instance.color
                );
                assert!(instance.border_color[3] <= alpha + 0.001);
            }
            assert!(result
                .text_nodes
                .iter()
                .all(|text| text.style.color_resolved()[3] as f32 <= alpha * 255.0 + 1.0));
        }
    });
}

#[test]
fn grouped_faces_fill_under_the_ring() {
    for slot in [SLOT_AVATAR, SLOT_GROUP_COUNT] {
        let mut fallback = Element::new().bg("#d83030");
        fallback.set_attr(ATTR_SLOT, SLOT_FALLBACK);
        let mut face = Element::new().w(40.0).h(40.0).rounded(20.0).child(fallback);
        face.set_attr(ATTR_SLOT, slot);
        let face = apply_group_ring(face);
        assert_eq!(face.style.bg_color, parse_color("#d83030"));
        assert_eq!(face.style.border_width, 2.0);
        assert_eq!(face.style.radii, [20.0; 4]);
    }
}

#[test]
fn avatar_group_count_only_toggles_on_click_and_uses_popover_dismissal() {
    use crate::app::portal_overlay::{
        clear_overlay_stack, try_close_top, try_dismiss_outside_press,
    };
    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use std::rc::Rc;

    fn find_slot<'a>(element: &'a Element, slot: &str) -> Option<&'a Element> {
        if element.attrs.get(ATTR_SLOT).map(String::as_str) == Some(slot) {
            return Some(element);
        }
        element
            .children
            .iter()
            .find_map(|child| find_slot(child, slot))
    }
    for manual in [false, true] {
        clear_overlay_stack();
        reset_registered_anchors();
        let live = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(Rc::clone(&live));
        let (owner, _) = crate::core::reactivity::create_root(|| {
            let live = Rc::clone(&live);
            create_effect(move || {
                let mut children = vec![avatar_with_fallback("A"), avatar_with_fallback("B")];
                if manual {
                    let mut packed = Element::new();
                    packed.set_attr("jadix_fragment", "1");
                    children.push(AvatarGroupCount(AvatarGroupCountProps {
                        children: Some(
                            packed.children([Element::new().text("+1"), avatar_with_fallback("C")]),
                        ),
                        ..Default::default()
                    }));
                } else {
                    children.push(avatar_with_fallback("C"));
                }
                *live.borrow_mut() = build_avatar_group(
                    children,
                    PopupPlacement::BottomStart,
                    None,
                    if manual { usize::MAX } else { 2 },
                );
            });
        });
        drain_deferred_view_remounts();
        let (expanded, click) = {
            let group = live.borrow();
            assert!(group.on_hover.is_none());
            let group_id = group.attrs.get(ATTR_GROUP_ID).unwrap();
            let expanded = AVATAR_GROUPS.with(|groups| groups.borrow()[group_id].expanded);
            let count = find_slot(&group, SLOT_GROUP_COUNT).unwrap();
            assert!(count.on_hover.is_none());
            assert!(count.focusable);
            assert_eq!(
                count.focus_id.as_deref(),
                Some(format!("{group_id}__focus").as_str())
            );
            (expanded, count.on_click.clone().unwrap())
        };
        assert!(!expanded.peek());
        (click.borrow_mut())();
        drain_deferred_view_remounts();
        assert!(expanded.peek());
        {
            let group = live.borrow();
            let panel = find_slot(&group, "popover-content").expect("mounted avatar popover");
            assert_eq!(panel.children.len(), 3);
            assert!(panel.on_hover.is_none());
            assert!(!panel
                .attrs
                .contains_key(crate::utils::popup::ATTR_FLOATING_EXPAND_FROM_W));
        }
        (click.borrow_mut())();
        drain_deferred_view_remounts();
        assert!(!expanded.peek());
        (click.borrow_mut())();
        drain_deferred_view_remounts();
        assert!(try_close_top());
        assert!(!expanded.peek());
        drain_deferred_view_remounts();
        (click.borrow_mut())();
        drain_deferred_view_remounts();
        crate::utils::popup::arm_underlay_close();
        assert!(try_dismiss_outside_press());
        assert!(!expanded.peek());
        drop(owner);
        clear_test_reconcile_root();
        clear_overlay_stack();
        reset_registered_anchors();
    }
}

#[test]
fn avatar_size_parse_and_dims() {
    assert_eq!(AvatarSize::parse("SM"), AvatarSize::Sm);
    assert_eq!(AvatarSize::Sm.box_dim(), "1.5rem");
    assert_eq!(AvatarSize::Default.box_dim(), "2rem");
    assert_eq!(AvatarSize::Lg.box_dim(), "2.5rem");
}

#[test]
fn builder_sets_slot_and_size() {
    let el: Element = avatar().size("lg").fallback("CN").into();
    assert_eq!(
        el.attrs.get(ATTR_SLOT).map(|s| s.as_str()),
        Some(SLOT_AVATAR)
    );
    assert_eq!(el.attrs.get(ATTR_SIZE).map(|s| s.as_str()), Some("lg"));
}

#[test]
fn fallback_icon_remains_a_centered_lucide_leaf_on_first_layout() {
    for size in ["sm", "default", "lg"] {
        crate::core::reactivity::create_root(|| {
            let mut root: Element = avatar().size(size).fallback_icon("user").into();
            let mut layout = crate::layout::LayoutTree::new();
            layout.build_and_layout(&mut root, 200.0, 160.0, [-1.0; 2]);
            let fallback = root
                .children
                .iter()
                .find(|child| child.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_FALLBACK))
                .unwrap();
            let icon = fallback
                .children
                .iter()
                .find(|child| child.attrs.contains_key(crate::utils::ATTR_ICON_NAME))
                .unwrap();
            assert!(
                icon.text.is_some(),
                "icon must keep its glyph on the Lucide leaf"
            );
            assert_eq!(icon.text_style.font_family.as_deref(), Some("lucide"));
            for axis in 0..2 {
                let icon_center = icon.layout_bounds[axis] + icon.layout_bounds[axis + 2] * 0.5;
                let center = fallback.layout_bounds[axis] + fallback.layout_bounds[axis + 2] * 0.5;
                assert!(
                    (icon_center - center).abs() < 0.5,
                    "{size} axis {axis}: {icon_center} vs {center}"
                );
            }
        });
    }
}

#[test]
fn avatar_group_count_slot() {
    let el = AvatarGroupCount(AvatarGroupCountProps {
        size: Some(AvatarSize::Default),
        children: Some(Element::new().text("+3")),
        base: None,
        ..Default::default()
    });
    assert_eq!(
        el.attrs.get(ATTR_SLOT).map(|s| s.as_str()),
        Some(SLOT_GROUP_COUNT)
    );
    assert!(
        (el.style.border_width - 2.0).abs() < 0.01,
        "+N shell must use the same 2px group ring as faces, got {}",
        el.style.border_width
    );
    let face = el
        .children
        .iter()
        .find(|c| c.attrs.get("data-avatar-count-face").map(|s| s.as_str()) == Some("1"))
        .expect("count inner face");
    let muted = parse_color("muted");
    assert!(
        (face.style.bg_color[0] - muted[0]).abs() < 0.02
            && (face.style.bg_color[1] - muted[1]).abs() < 0.02,
        "inner face must carry muted fill so the ring reads as an inner stroke"
    );
    assert!(
        el.style.bg_color == muted,
        "count shell must back the inner face so antialiasing cannot expose underlying content"
    );
}

fn collect_texts(el: &Element, out: &mut Vec<String>) {
    if let Some(t) = el.text.as_ref().filter(|s| !s.is_empty()) {
        out.push(t.clone());
    }
    for c in &el.children {
        collect_texts(c, out);
    }
}

#[test]
fn avatar_group_count_keeps_label_when_overflow_faces_nested() {
    let el = AvatarGroupCount(AvatarGroupCountProps {
        size: Some(AvatarSize::Default),
        children: Some({
            let mut f = Element::new();
            f.set_attr("jadix_fragment", "1");
            f = f.children(vec![
                Element::new().text("+3"),
                avatar_with_fallback("D"),
                avatar_with_fallback("E"),
                avatar_with_fallback("F"),
            ]);
            f
        }),
        ..Default::default()
    });
    let mut texts = Vec::new();
    collect_texts(&el, &mut texts);
    assert!(
        texts.iter().any(|t| t.contains("+3")),
        "+N label must stay on the count capsule, got {texts:?}"
    );
    let nested: Vec<&Element> = el
        .children
        .iter()
        .filter(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_AVATAR))
        .collect();
    assert_eq!(nested.len(), 3, "overflow faces stay as count children");
    for n in nested {
        assert_eq!(
            n.style.position,
            taffy::style::Position::Absolute,
            "overflow faces must be out of the count flex flow"
        );
        assert_eq!(
            crate::layout::dim_length_px(n.style.min_size.width),
            Some(0.0)
        );
    }
}

fn avatar_with_fallback(label: &str) -> Element {
    avatar().fallback(label).into()
}

#[test]
fn avatar_group_popover_is_offset_and_wraps_all_faces() {
    let (_owner, _) = crate::core::reactivity::create_root(|| {
        let faces = vec![extract_face_spec(&avatar_with_fallback("A")).unwrap(); 12];
        let panel = build_avatar_group_expand_panel(
            "group",
            "trigger",
            PopupPlacement::BottomStart,
            &faces,
            None,
            32.0,
        );
        assert_eq!(panel.children.len(), 12);
        assert_eq!(panel.style.flex_wrap, taffy::style::FlexWrap::Wrap);
        assert_eq!(panel.style.translate, [0.0; 2]);
        assert_eq!(
            panel
                .attrs
                .get(crate::utils::popup::ATTR_FLOATING_GAP)
                .unwrap(),
            "6"
        );
        assert!(!panel
            .attrs
            .contains_key(crate::utils::popup::ATTR_FLOATING_EXPAND_FROM_W));
        assert!(!panel
            .attrs
            .contains_key(crate::utils::popup::ATTR_FLOATING_EXPAND_FROM_H));
    });
}

#[test]
fn avatar_group_expand_panel_hugs_face_row_in_wide_parent() {
    use crate::core::reactivity::create_root;
    use crate::layout::layout_tree::LayoutTree;

    let (_owner, faces) = create_root(|| {
        ["A", "B", "C", "D", "E", "F"]
            .into_iter()
            .map(|label| extract_face_spec(&avatar_with_fallback(label)).expect("face"))
            .collect::<Vec<_>>()
    });
    let panel = build_avatar_group_expand_panel(
        "grp-hug",
        "fid",
        PopupPlacement::BottomStart,
        &faces,
        None,
        32.0,
    );
    let mut host = Element::new().relative().w(400.0).h(200.0).child(panel);
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut host, 400.0, 200.0, [-1.0, -1.0]);
    let w = host.children[0].layout_bounds[2];
    let face = IntoF32::into_f32(AvatarSize::Default.box_dim());
    let gap = IntoF32::into_f32(tokens::SPACE_1);
    let content = 6.0 * face + 5.0 * gap + 2.0 * super::expand_chrome_inset();
    assert!(
            (w - content).abs() < 12.0,
            "expand panel must hug the 6-face row ({content:.1}px), not stretch to parent 400, got {w:.1}"
        );
}

#[test]
fn avatar_group_expand_keeps_image_src_and_fallback_class() {
    use crate::core::reactivity::create_root;

    const SRC: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==";

    let (_owner, (image_spec, class_spec, img_el, class_el)) = create_root(|| {
        let with_img: Element = avatar().image(SRC).fallback("A").into();
        let with_class: Element = avatar().class("bg-red-500").fallback("B").into();
        let image_spec = extract_face_spec(&with_img).expect("image face");
        let class_spec = extract_face_spec(&with_class).expect("class face");
        // Same owner as the collapsed faces: expand rebuild must still seed
        // image_texture even when bind_prop skips a duplicate key.
        let img_el = rebuild_face(&image_spec);
        let class_el = rebuild_face(&class_spec);
        (image_spec, class_spec, img_el, class_el)
    });
    assert_eq!(
        image_spec.src.as_deref(),
        Some(SRC),
        "expand snapshot must keep AvatarImage src so A/B do not fall back to muted"
    );
    assert!(
        class_spec
            .class
            .as_deref()
            .is_some_and(|c| c.split_whitespace().any(|t| t == "bg-red-500")),
        "expand snapshot must keep author class colors, got {:?}",
        class_spec.class
    );

    let mut rebuilt_src = None;
    let mut rebuilt_tex = false;
    crate::core::tree::walk_elements(&img_el, |n| {
        if rebuilt_src.is_none() {
            if let Some(s) = n.attrs.get(ATTR_AVATAR_SRC).filter(|s| !s.is_empty()) {
                rebuilt_src = Some(s.clone());
            }
        }
        if n.image_texture.as_ref().is_some_and(|k| !k.is_empty()) {
            rebuilt_tex = true;
        }
    });
    assert_eq!(
        rebuilt_src.as_deref(),
        Some(SRC),
        "rebuild_face must emit AvatarImage with the original src"
    );
    assert!(
        rebuilt_tex,
        "expand rebuild must seed image_texture so colored faces do not paint as muted fallback"
    );
    let class_attr = class_el
        .attrs
        .get("jadix_utility_class")
        .map(|s| s.as_str())
        .unwrap_or("");
    assert!(
        class_attr.split_whitespace().any(|t| t == "bg-red-500"),
        "rebuild_face must keep bg-red-500, got {class_attr:?}"
    );
}

#[test]
fn avatar_group_open_does_not_reborrow_registry() {
    use crate::core::reactivity::create_root;

    let (_owner, _group) = create_root(|| {
        let mut frag = Element::new();
        frag.set_attr("jadix_fragment", "1");
        frag = frag.children(vec![
            avatar_with_fallback("A"),
            AvatarGroupCount(AvatarGroupCountProps {
                children: Some(Element::new().text("+1")),
                ..Default::default()
            }),
        ]);
        AvatarGroup(AvatarGroupProps {
            children: Some(frag),
            ..Default::default()
        })
    });
    let group_id = AVATAR_GROUPS.with(|m| {
        m.borrow()
            .keys()
            .next()
            .cloned()
            .expect("avatar group registry")
    });
    // Synchronous signal writes used to borrow_mut the registry and panic on re-enter.
    open_avatar_group_expand(&group_id);
    AVATAR_GROUPS.with(|m| {
        assert!(m.borrow().get(&group_id).unwrap().expanded.peek());
    });
}

#[test]
fn avatar_group_expand_anchor_lives_inside_trigger_shell() {
    use crate::app::reactive::REACTIVE_PLACEHOLDER_HASH;
    use crate::core::reactivity::create_root;

    fn find_group<'a>(el: &'a Element) -> Option<&'a Element> {
        if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_GROUP) {
            return Some(el);
        }
        el.children.iter().find_map(find_group)
    }

    fn find_placeholder<'a>(el: &'a Element) -> bool {
        el.template_hash
            .as_deref()
            .is_some_and(|h| h == REACTIVE_PLACEHOLDER_HASH)
            || el.children.iter().any(find_placeholder)
    }

    let (_owner, root) = create_root(|| {
        let mut frag = Element::new();
        frag.set_attr("jadix_fragment", "1");
        frag = frag.children(vec![
            avatar_with_fallback("A"),
            AvatarGroupCount(AvatarGroupCountProps {
                children: Some(Element::new().text("+1")),
                ..Default::default()
            }),
        ]);
        AvatarGroup(AvatarGroupProps {
            children: Some(frag),
            ..Default::default()
        })
    });
    let group = find_group(&root).expect("avatar group row");
    assert!(
        find_avatar_group_row(group)
            .unwrap()
            .children
            .last()
            .unwrap()
            .focus_id
            .is_some(),
        "+N carries focus_id for floating placement"
    );
    assert!(
        find_placeholder(group),
        "reactive expand anchor must stay inside the group shell like BubbleReactions"
    );
    assert_ne!(
        group.attrs.get("jadix_portal").map(String::as_str),
        Some("1"),
        "group shell itself is not a portal; only the expand panel hoists"
    );
}

#[test]
fn avatar_group_scroll_dismisses_expand() {
    use crate::core::reactivity::create_root;

    let (_owner, _group) = create_root(|| {
        let mut frag = Element::new();
        frag.set_attr("jadix_fragment", "1");
        frag = frag.children(vec![
            avatar_with_fallback("A"),
            AvatarGroupCount(AvatarGroupCountProps {
                children: Some(Element::new().text("+1")),
                ..Default::default()
            }),
        ]);
        AvatarGroup(AvatarGroupProps {
            children: Some(frag),
            ..Default::default()
        })
    });
    let group_id = AVATAR_GROUPS.with(|m| {
        m.borrow()
            .keys()
            .next()
            .cloned()
            .expect("avatar group registry")
    });
    open_avatar_group_expand(&group_id);
    assert!(super::dismiss_avatar_group_expands_on_scroll());
    AVATAR_GROUPS.with(|m| {
        assert!(!m.borrow().get(&group_id).unwrap().expanded.peek());
    });
}

#[test]
fn avatar_group_width_collapses_visible_faces() {
    use crate::core::reactivity::create_root;

    let (_owner, group) = create_root(|| {
        let mut frag = Element::new();
        frag.set_attr("jadix_fragment", "1");
        frag = frag.children(vec![
            avatar_with_fallback("A"),
            avatar_with_fallback("B"),
            avatar_with_fallback("C"),
            AvatarGroupCount(AvatarGroupCountProps {
                children: Some({
                    let mut f = Element::new();
                    f.set_attr("jadix_fragment", "1");
                    f = f.children(vec![
                        Element::new().text("+3"),
                        avatar_with_fallback("D"),
                        avatar_with_fallback("E"),
                        avatar_with_fallback("F"),
                    ]);
                    f
                }),
                ..Default::default()
            }),
        ]);
        AvatarGroup(AvatarGroupProps {
            children: Some(frag),
            ..Default::default()
        })
    });
    let group_id = AVATAR_GROUPS.with(|m| {
        m.borrow()
            .keys()
            .next()
            .cloned()
            .expect("avatar group registry")
    });
    let mut host = Element::new().w(72.0).child(group);
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let _ = tree.build_and_layout(&mut host, 72.0, 120.0, [-1.0, -1.0]);
    let mut impact = LayoutStatus::Clean;
    impact = impact.merge(crate::authoring::presentation::sync_tree(
        &mut host,
        &mut tree,
        crate::authoring::presentation::Context::new(
            crate::authoring::presentation::Phase::AfterGeometry,
            [72.0, 120.0],
        ),
    ));
    assert_ne!(impact, LayoutStatus::Clean);
    let rendered = AVATAR_GROUPS.with(|m| {
        m.borrow()
            .get(&group_id)
            .expect("entry")
            .rendered_visible
            .get()
    });
    assert!(
        rendered < 3,
        "narrow column must fold visible avatars into +N, got {rendered}"
    );
    let visible_faces = host
        .children
        .iter()
        .find(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_GROUP))
        .and_then(find_avatar_group_row)
        .map(|row| {
            row.children
                .iter()
                .filter(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_AVATAR))
                .count()
        })
        .expect("group row");
    assert_eq!(
        visible_faces, rendered,
        "collapsed row must rebuild to match rendered visible count"
    );
}

#[test]
fn avatar_group_max_visible_auto_splits_faces() {
    use crate::core::reactivity::create_root;

    let (_owner, group) = create_root(|| {
        let mut frag = Element::new();
        frag.set_attr("jadix_fragment", "1");
        frag = frag.children(vec![
            avatar_with_fallback("A"),
            avatar_with_fallback("B"),
            avatar_with_fallback("C"),
            avatar_with_fallback("D"),
        ]);
        AvatarGroup(AvatarGroupProps {
            max_visible: Some(2),
            children: Some(frag),
            ..Default::default()
        })
    });
    fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
        if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(slot) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_slot(c, slot))
    }
    let faces: Vec<_> = find_slot(&group, SLOT_GROUP)
        .into_iter()
        .flat_map(|g| g.children.iter())
        .flat_map(|row| row.children.iter())
        .filter(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_AVATAR))
        .collect();
    assert_eq!(faces.len(), 2, "max_visible=2 keeps two faces in the row");
    assert!(
        find_slot(&group, SLOT_GROUP_COUNT).is_some(),
        "overflow faces must fold into +N"
    );
}

#[test]
fn avatar_group_count_keeps_ring_after_interactive_wrap() {
    use crate::core::reactivity::create_root;
    use crate::core::tree::IntoElementList;

    fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
        if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(slot) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_slot(c, slot))
    }

    let (_owner, root) = create_root(|| {
        let mut frag = Element::new();
        frag.set_attr("jadix_fragment", "1");
        frag = frag.children(vec![
            avatar_with_fallback("A"),
            AvatarGroupCount(AvatarGroupCountProps {
                children: Some(Element::new().text("+1")),
                ..Default::default()
            }),
        ]);
        AvatarGroup(AvatarGroupProps {
            children: Some(frag),
            ..Default::default()
        })
    });
    let kids = root.into_element_list();
    let count = find_slot(&kids[0], SLOT_GROUP_COUNT).expect("count in group row");
    let face = find_slot(&kids[0], SLOT_AVATAR).expect("face in group row");
    assert!(
        (count.style.border_width - 2.0).abs() < 0.01,
        "wrapped +N must keep 2px ring, got {}",
        count.style.border_width
    );
    assert!(
        (face.style.border_width - count.style.border_width).abs() < 0.01,
        "face ring {} vs count ring {}",
        face.style.border_width,
        count.style.border_width
    );
}

#[test]
fn avatar_image_load_failure_falls_back() {
    use crate::core::reactivity::create_root;

    fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
        if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(slot) {
            return Some(el);
        }
        for c in &el.children {
            if let Some(hit) = find_slot(c, slot) {
                return Some(hit);
            }
        }
        None
    }

    let (_owner, el) = create_root(|| {
        avatar_scope("", AvatarSize::Default, AvatarShape::Circle, None, || {
            let mut frag = Element::new();
            frag.set_attr("jadix_fragment", "1");
            frag.children(vec![
                AvatarImage(AvatarImageProps {
                    id: String::new(),
                    src: Some("/nonexistent/jadix-avatar-fail-test.png".into()),
                    alt: None,
                    base: None,
                    children: None,
                    ..Default::default()
                }),
                AvatarFallback(AvatarFallbackProps {
                    id: String::new(),
                    children: Some(Element::new().text("FB")),
                    base: None,
                    ..Default::default()
                }),
            ])
        })
    });
    let img = find_slot(&el, SLOT_IMAGE).expect("avatar-image");
    assert_eq!(
        img.attrs.get(ATTR_LOAD_STATE).map(|s| s.as_str()),
        Some("failed")
    );
    assert!(img.image_texture.is_none());
    assert!((img.style.opacity - 0.0).abs() < f32::EPSILON);
    fn has_text(el: &Element, want: &str) -> bool {
        if el.text.as_deref() == Some(want) {
            return true;
        }
        el.children.iter().any(|c| has_text(c, want))
    }

    let fb = find_slot(&el, SLOT_FALLBACK).expect("avatar-fallback");
    assert!(has_text(fb, "FB"), "fallback caption must remain visible");
}

#[test]
fn avatar_fallback_applies_shape_radii() {
    let el: Element = avatar().fallback("ER").into();
    let fb = el
        .children
        .iter()
        .find(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_FALLBACK))
        .expect("fallback");
    assert!(
        fb.style.radii.iter().any(|r| *r < 0.0),
        "circle fallback must use rounded_full radii sentinel"
    );
}

#[test]
fn avatar_rounded_square_shape() {
    let el: Element = avatar().shape("rounded-square").fallback("RS").into();
    assert_eq!(
        el.attrs.get(ATTR_SHAPE).map(|s| s.as_str()),
        Some("rounded-square")
    );
    let fb = el
        .children
        .iter()
        .find(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_FALLBACK))
        .expect("fallback");
    assert!(
        fb.style.radii.iter().all(|r| *r > 0.0),
        "rounded-square fallback must use explicit corner radii"
    );
}

#[test]
fn avatar_fallback_icon_in_scope() {
    fn find_lucide(el: &Element) -> bool {
        if el.text_style.font_family.as_deref() == Some("lucide") {
            return true;
        }
        el.children.iter().any(find_lucide)
    }

    let el: Element = avatar().fallback_icon("user").into();
    let fb = el
        .children
        .iter()
        .find(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_FALLBACK))
        .expect("fallback slot");
    assert!(find_lucide(fb), "fallback icon must render lucide glyph");
}

#[test]
fn avatar_fallback_builder_image_fills_layer() {
    let el: Element = avatar()
            .fallback_image("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'><rect width='8' height='8' fill='red'/></svg>")
            .into();
    let fb = el
        .children
        .iter()
        .find(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_FALLBACK))
        .expect("fallback slot");
    fn find_load_layer(el: &Element) -> bool {
        if el.attrs.get(ATTR_LOAD_STATE).is_some() {
            return true;
        }
        el.children.iter().any(find_load_layer)
    }
    assert!(find_load_layer(fb), "fallback image layer must exist");
}

#[test]
fn avatar_fallback_builder_class_and_style() {
    fn has_text(el: &Element, want: &str) -> bool {
        if el.text.as_deref() == Some(want) {
            return true;
        }
        el.children.iter().any(|c| has_text(c, want))
    }

    let el: Element = avatar()
        .fallback_with(|fb| {
            fb.class("bg-accent")
                .style(|b| b.text_size(20.0))
                .text("AB")
        })
        .into();
    let fb = el
        .children
        .iter()
        .find(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_FALLBACK))
        .expect("fallback slot");
    assert!(
        fb.class_set
            .as_ref()
            .is_some_and(|cs| cs.base.iter().any(|t| t.contains("bg-accent")))
            || fb
                .attrs
                .get("jadix_utility_class")
                .is_some_and(|c| c.contains("bg-accent"))
    );
    assert_eq!(fb.text_style.size_resolved(), 20.0);
    assert!(has_text(fb, "AB"), "fallback caption must render");
}

#[test]
fn avatar_image_with_applies_chrome() {
    let el: Element = avatar()
        .image_with(|img| img.src("assets/user.png").class("opacity-80"))
        .into();
    let img = el
        .children
        .iter()
        .find(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_IMAGE))
        .expect("avatar-image");
    assert!(
        img.class_set
            .as_ref()
            .is_some_and(|cs| cs.base.iter().any(|t| t.contains("opacity-80")))
            || img
                .attrs
                .get("jadix_utility_class")
                .is_some_and(|c| c.contains("opacity-80"))
    );
}

#[test]
fn avatar_layers_paint_by_tree_order_without_z_index() {
    use crate::core::reactivity::create_root;
    use crate::core::tree::LayoutTree;

    let (_owner, face) = create_root(|| {
        avatar_scope("", AvatarSize::Lg, AvatarShape::Circle, None, || {
            let mut frag = Element::new();
            frag.set_attr("jadix_fragment", "1");
            frag.children(vec![
                AvatarImage(AvatarImageProps {
                    id: String::new(),
                    src: Some(
                        "data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg'/>".into(),
                    ),
                    alt: Some(String::new()),
                    base: None,
                    children: None,
                    ..Default::default()
                }),
                AvatarFallback(AvatarFallbackProps {
                    id: String::new(),
                    children: Some(Element::new().text("JD")),
                    base: None,
                    ..Default::default()
                }),
                build_avatar_badge(BadgeVariant::Default, Vec::new(), None),
            ])
        })
    });
    let slots: Vec<&str> = face
        .children
        .iter()
        .filter_map(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()))
        .collect();
    assert_eq!(
        slots,
        vec![SLOT_FALLBACK, SLOT_IMAGE, SLOT_BADGE],
        "image must paint after fallback, badge last — no z-index needed"
    );
    for c in &face.children {
        assert_eq!(
            c.style.z_index,
            0,
            "slot {:?} z-index would enable gallery page_stack_bands",
            c.attrs.get(ATTR_SLOT)
        );
    }

    let mut col = Element::new().flex_col().w(400.0).gap(8.0);
    col = col.child(Element::new().text("Fallback Image"));
    col = col.child(face);
    col = col.child(Element::new().text("Sizes"));
    let mut root = Element::new()
        .w(400.0)
        .h(280.0)
        .overflow_y("auto")
        .child(col);
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 400.0, 280.0, [-1.0, -1.0]);
    assert!(
        result.overlay_split.page_stack_bands.is_empty(),
        "avatar image/badge must not switch the page onto page_stack_bands, got {}",
        result.overlay_split.page_stack_bands.len()
    );
    root.scroll_offset = [0.0, 40.0];
    let scrolled = tree.build_and_layout_fast(&mut root, 400.0, 280.0, [-1.0, -1.0], true, false);
    assert!(
        scrolled.overlay_split.page_stack_bands.is_empty(),
        "scroll_only collect must not grow page_stack_bands from avatars, got {}",
        scrolled.overlay_split.page_stack_bands.len()
    );
    let y0 = result
        .text_nodes
        .iter()
        .find(|n| n.text.contains("Sizes"))
        .map(|n| n.y);
    let y1 = scrolled
        .text_nodes
        .iter()
        .find(|n| n.text.contains("Sizes"))
        .map(|n| n.y);
    if let (Some(before), Some(after)) = (y0, y1) {
        assert!(
            (after - (before - 40.0)).abs() < 0.5,
            "label glyphon y must track scroll (before={before:.2} after={after:.2})"
        );
    }
}

#[test]
fn dual_surface_compiles_shape() {
    let _b: Element = avatar().size("sm").fallback("AB").into();
    let _i: Element = avatar().fallback_icon("user").into();
    let _fb: Element = avatar_scope("", AvatarSize::Default, AvatarShape::Circle, None, || {
        avatar_fallback().text("X").into()
    });
    let el = Avatar(AvatarProps {
        id: String::new(),
        size: Some(AvatarSize::Default),
        shape: Some(AvatarShape::Circle),
        children: None,
        base: None,
        ..Default::default()
    });
    let _: Element = el;
}
