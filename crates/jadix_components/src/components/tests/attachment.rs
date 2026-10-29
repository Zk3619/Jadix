use super::*;

#[test]
fn attachment_enums_parse() {
    assert_eq!(
        AttachmentState::from("uploading"),
        AttachmentState::Uploading
    );
    assert_eq!(AttachmentState::from("error"), AttachmentState::Destructive);
    assert_eq!(AttachmentState::from("warn"), AttachmentState::Warning);
    assert_eq!(AttachmentState::from("info"), AttachmentState::Info);
    assert_eq!(AttachmentSize::from("xs"), AttachmentSize::Xs);
    assert_eq!(
        AttachmentOrientation::from("vertical"),
        AttachmentOrientation::Vertical
    );
    assert_eq!(
        AttachmentMediaVariant::from("image"),
        AttachmentMediaVariant::Image
    );
}

#[test]
fn bound_state_keeps_mounted_identity_and_geometry() {
    let mut el: Element = attachment().state("done").into();
    el.template_id = Some("live-attachment".into());
    el = el.w("100%");
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    tree.build_and_layout(&mut el, 400.0, 100.0, [-1.0, -1.0]);
    let node = el.layout_node_id();
    let bounds = el.layout_bounds;
    let width_spec = el.style.unit_specs.clone();
    for state in ["success", "warning", "done"] {
        assert_ne!(
            apply_bound_attachment_state(&mut el, state),
            LayoutStatus::Clean
        );
        assert_eq!(el.template_id.as_deref(), Some("live-attachment"));
        assert_eq!(el.layout_node_id(), node);
        assert_eq!(el.layout_bounds, bounds);
        assert_eq!(
            el.style.unit_specs, width_spec,
            "state repaint must preserve the consumer's w-full style"
        );
        assert_eq!(
            attachment_state_from_attrs(&el.attrs),
            AttachmentState::from(state)
        );
    }
}

#[test]
fn attachment_paint_sets_slots() {
    let mut el = Element::new();
    assert_eq!(
        apply_attachment_paint(
            &mut el,
            AttachmentState::Destructive,
            AttachmentSize::Sm,
            AttachmentOrientation::Horizontal,
        ),
        LayoutStatus::LayoutChanged
    );
    assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
    assert_eq!(
        el.attrs.get(ATTR_STATE).map(String::as_str),
        Some("destructive")
    );
    assert_eq!(
        apply_attachment_paint(
            &mut el,
            AttachmentState::Destructive,
            AttachmentSize::Sm,
            AttachmentOrientation::Horizontal,
        ),
        LayoutStatus::Clean
    );
    assert_eq!(
        apply_attachment_paint(
            &mut el,
            AttachmentState::Idle,
            AttachmentSize::Sm,
            AttachmentOrientation::Horizontal,
        ),
        LayoutStatus::StyleChanged
    );
}

#[test]
fn attachment_group_slot() {
    let group = AttachmentGroup(AttachmentGroupProps {
        children: None,
        base: None,
        base_builder: None,
        ..Default::default()
    });
    assert_eq!(
        group.attrs.get(ATTR_SLOT).map(String::as_str),
        Some(SLOT_GROUP)
    );
}

#[test]
fn apply_bound_attachment_state_repaints_title_pulse() {
    fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
        if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(slot) {
            return Some(el);
        }
        for child in &el.children {
            if let Some(found) = find_slot(child, slot) {
                return Some(found);
            }
        }
        None
    }

    let root = Attachment(AttachmentProps {
        state: Some(AttachmentState::Done),
        size: Some(AttachmentSize::Default),
        orientation: Some(AttachmentOrientation::Horizontal),
        children: {
            let mut frag = Element::new();
            frag.set_attr("jadix_fragment", "1");
            Some(frag.children([AttachmentTitle(AttachmentTitleProps {
                children: Some(Element::new().text("file.txt")),
                base: None,
                base_builder: None,
                ..Default::default()
            })]))
        },
        base: None,
        base_builder: None,
        ..Default::default()
    });
    let mut el = root;
    assert_eq!(
        apply_bound_attachment_state(&mut el, "uploading"),
        LayoutStatus::StyleChanged
    );
    let title = find_slot(&el, SLOT_TITLE).expect("title slot");
    let has_pulse = title
        .attrs
        .get("jadix_utility_class")
        .map(|c| c.contains(PULSE_CLASS))
        .unwrap_or(false)
        || title
            .class_set
            .as_ref()
            .is_some_and(|cs| cs.base.iter().any(|t| t.contains(PULSE_CLASS)));
    assert!(has_pulse, "uploading should enable animate-pulse on title");
}

fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
    if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(slot) {
        return Some(el);
    }
    for child in &el.children {
        if let Some(found) = find_slot(child, slot) {
            return Some(found);
        }
    }
    None
}

#[test]
fn default_done_horizontal_paints_card_chrome() {
    let mut el = Element::new();
    assert_ne!(
        apply_attachment_paint(
            &mut el,
            AttachmentState::Done,
            AttachmentSize::Default,
            AttachmentOrientation::Horizontal,
        ),
        LayoutStatus::Clean
    );
    assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
    assert_eq!(
        crate::layout::dim_length_px(el.style.min_size.width),
        Some(0.0)
    );
    assert_eq!(el.style.flex_wrap, taffy::style::FlexWrap::NoWrap);
    assert_eq!(
        apply_attachment_paint(
            &mut el,
            AttachmentState::Done,
            AttachmentSize::Default,
            AttachmentOrientation::Horizontal,
        ),
        LayoutStatus::Clean
    );
}

#[test]
fn attachment_title_is_full_width_for_ellipsis() {
    let el = AttachmentTitle(AttachmentTitleProps {
        children: Some(Element::new().text("very-long-filename.pdf")),
        base: None,
        base_builder: None,
        ..Default::default()
    });
    assert!(
        matches!(
            crate::layout::dim_kind(el.style.size.width),
            crate::layout::DimKind::Percent(p) if (p - 1.0).abs() < 0.001
        ),
        "title must be w-full so a clamped chip can ellipsize"
    );
}

#[test]
fn builder_title_with_text_size() {
    fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
        if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(slot) {
            return Some(el);
        }
        for child in &el.children {
            if let Some(found) = find_slot(child, slot) {
                return Some(found);
            }
        }
        None
    }

    fn has_text(el: &Element, want: &str) -> bool {
        if el.text.as_deref() == Some(want) {
            return true;
        }
        el.children.iter().any(|c| has_text(c, want))
    }

    let el: Element = attachment()
        .title_with(|t| t.style(|b| b.text_size(18.0)).text("report.pdf"))
        .into();
    let title = find_slot(&el, SLOT_TITLE).expect("title slot");
    assert_eq!(title.text_style.size_resolved(), 18.0);
    assert!(has_text(title, "report.pdf"));
}

#[test]
fn vertical_image_preview_fills_media_and_uses_w24() {
    let mut img = Element::new();
    img.image_texture = Some("demo-checker".into());
    let root = Attachment(AttachmentProps {
        orientation: Some(AttachmentOrientation::Vertical),
        children: {
            let mut frag = Element::new();
            frag.set_attr("jadix_fragment", "1");
            Some(frag.children([AttachmentMedia(AttachmentMediaProps {
                variant: Some(AttachmentMediaVariant::Image),
                children: Some(img),
                base: None,
                base_builder: None,
                ..Default::default()
            })]))
        },
        base: None,
        base_builder: None,
        ..Default::default()
    });
    assert_eq!(
        crate::layout::dim_length_px(root.style.size.width),
        Some(96.0)
    );
    let media = find_slot(&root, SLOT_MEDIA).expect("media slot");
    assert!(
        matches!(
            crate::layout::dim_kind(media.style.size.width),
            crate::layout::DimKind::Percent(p) if (p - 1.0).abs() < 0.001
        ),
        "vertical image media should be w-full"
    );
    let photo = media.children.first().expect("image child");
    assert!(
        matches!(
            crate::layout::dim_kind(photo.style.size.width),
            crate::layout::DimKind::Percent(p) if (p - 1.0).abs() < 0.001
        ),
        "image child should fill the media well"
    );
}
