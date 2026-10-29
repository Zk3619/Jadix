use super::*;
use taffy::style::JustifyContent;

fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
    if slot_name(el) == Some(slot) {
        return Some(el);
    }
    el.children.iter().find_map(|c| find_slot(c, slot))
}

#[test]
fn message_default_align_start_row() {
    let el: Element = message().content_child(Element::new().text("Hello")).into();
    assert_eq!(slot_name(&el), Some(SLOT_ROOT));
    assert_eq!(el.attrs.get(ATTR_ALIGN).map(String::as_str), Some("start"));
    assert_eq!(el.style.flex_direction, FlexDirection::Column);
    let upper = find_slot(&el, SLOT_UPPER).expect("upper");
    assert_eq!(upper.style.flex_direction, FlexDirection::Row);
    assert!(el.template_id.is_none());
}

#[test]
fn message_start_surface_children_self_start() {
    let el: Element = message().content_child(Element::new().text("Hello")).into();
    let upper = find_slot(&el, SLOT_UPPER).expect("upper");
    let content = find_slot(upper, SLOT_CONTENT).expect("content");
    assert!(
        content
            .children
            .iter()
            .all(|c| c.style.align_self == Some(AlignSelf::FLEX_START)),
        "start: surface children hug; Taffy has no fit-content"
    );
}

#[test]
fn file_chip_in_start_message_hugs_not_full_width() {
    let chip = Element::from(
        crate::components::attachment::attachment()
            .media_icon(Element::new().w(16.0).h(16.0))
            .title_description("sales-dashboard.pdf", "PDF · 2.4 MB"),
    );
    let mut root: Element = crate::primitives::r#box::vstack()
        .w(400.0)
        .child(
            message()
                .align("start")
                .avatar(Element::new().text("CN").w(32.0).h(32.0))
                .content_child(chip),
        )
        .into();
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 400.0, 240.0, [-1.0, -1.0]);
    let msg = find_slot(&root, SLOT_ROOT).expect("message");
    let content = find_slot(msg, SLOT_CONTENT).expect("content");
    fn find_attachment<'a>(el: &'a Element) -> Option<&'a Element> {
        if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some("attachment") {
            return Some(el);
        }
        el.children.iter().find_map(find_attachment)
    }
    let att = find_attachment(content).expect("attachment");
    assert!(
        att.layout_bounds[2] > 80.0,
        "file chip should keep an intrinsic width (w={:.1})",
        att.layout_bounds[2]
    );
    assert!(
        att.layout_bounds[2] < content.layout_bounds[2] - 40.0,
        "file chip must hug, not fill the message column (att={:.1} content={:.1})",
        att.layout_bounds[2],
        content.layout_bounds[2]
    );
}

#[test]
fn message_align_end_reverses_row() {
    let el: Element = message()
        .align("end")
        .content_child(Element::new().text("On it."))
        .into();
    assert_eq!(el.attrs.get(ATTR_ALIGN).map(String::as_str), Some("end"));
    assert_eq!(el.style.flex_direction, FlexDirection::Column);
    let upper = find_slot(&el, SLOT_UPPER).expect("upper");
    assert_eq!(upper.style.flex_direction, FlexDirection::RowReverse);
}

fn rect_right(el: &Element) -> f32 {
    el.layout_bounds[0] + el.layout_bounds[2]
}

/// `align=end`: avatar on the trailing edge; bubble/header self-end against it
/// (content stays `w-full` like ShadCN, leftover sits inside the content column).
#[test]
fn message_align_end_packs_avatar_right_content_against_it() {
    let mut root: Element = crate::primitives::r#box::vstack()
        .w(400.0)
        .child(
            message()
                .align("end")
                .avatar(Element::new().text("ME").w(32.0).h(32.0))
                .header("You")
                .content_child(Element::new().text("Hi"))
                .footer("sent"),
        )
        .into();
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 400.0, 240.0, [-1.0, -1.0]);
    let msg = find_slot(&root, SLOT_ROOT).expect("message");
    let upper = find_slot(msg, SLOT_UPPER).expect("upper");
    let avatar = find_slot(upper, SLOT_AVATAR).expect("avatar");
    let content = find_slot(upper, SLOT_CONTENT).expect("content");
    let header = find_slot(content, SLOT_HEADER).expect("header");
    let msg_right = rect_right(msg);
    let av_x = avatar.layout_bounds[0];
    let av_right = rect_right(avatar);
    let co_x = content.layout_bounds[0];
    let co_right = rect_right(content);

    assert!(
        av_x > co_x + 1.0,
        "end: avatar must sit to the right of content (av_x={av_x:.1} co_x={co_x:.1})"
    );
    assert!(
            (msg_right - av_right).abs() < 3.0,
            "end: avatar must sit on the trailing edge (av_right={av_right:.1} msg_right={msg_right:.1} msg_w={:.1} av_x={av_x:.1} co_x={co_x:.1} co_w={:.1})",
            msg.layout_bounds[2],
            content.layout_bounds[2]
        );
    assert!(
        (av_x - co_right).abs() < 12.0,
        "end: content fills up to the avatar (gap={:.1} av_x={av_x:.1} co_right={co_right:.1})",
        av_x - co_right
    );
    assert!(
        content.layout_bounds[2] > msg.layout_bounds[2] * 0.5,
        "end: content stays w-full (content_w={:.1} msg_w={:.1})",
        content.layout_bounds[2],
        msg.layout_bounds[2]
    );
    assert!(
        (rect_right(header) - co_right).abs() < 4.0,
        "end: header box self-ends against the content trailing edge"
    );
}

#[test]
fn message_align_end_bind_moves_avatar_to_trailing_edge() {
    let mut root: Element = crate::primitives::r#box::vstack()
        .w(400.0)
        .child(
            message()
                .avatar(Element::new().text("ME").w(32.0).h(32.0))
                .content_child(Element::new().text("Hi")),
        )
        .into();
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 400.0, 240.0, [-1.0, -1.0]);
    let status = apply_bound_message_align(root.children.first_mut().expect("child"), "end");
    assert_eq!(
        status,
        LayoutStatus::StructureChanged,
        "align bind must full-rebuild Taffy so descendant row-reverse is applied"
    );
    let _ = tree.build_and_layout(&mut root, 400.0, 240.0, [-1.0, -1.0]);
    let msg = find_slot(&root, SLOT_ROOT).expect("message after bind");
    let upper = find_slot(msg, SLOT_UPPER).expect("upper");
    let avatar = find_slot(upper, SLOT_AVATAR).expect("avatar");
    let content = find_slot(upper, SLOT_CONTENT).expect("content");
    assert_eq!(upper.style.flex_direction, FlexDirection::RowReverse);
    assert!(
        (rect_right(msg) - rect_right(avatar)).abs() < 3.0,
        "bind end: avatar on trailing edge (av_right={:.1} msg_right={:.1})",
        rect_right(avatar),
        rect_right(msg)
    );
    assert!(
        avatar.layout_bounds[0] > content.layout_bounds[0] + 1.0,
        "bind end: avatar right of content"
    );
}

#[test]
fn message_align_end_with_bubble_keeps_avatar_on_trailing_edge() {
    let bubble_el: Element = crate::components::bubble::bubble()
        .variant("default")
        .align("end")
        .child(crate::components::bubble::bubble_content().text("It's a one-line change."))
        .into();
    let mut root: Element = crate::primitives::r#box::vstack()
        .w(448.0)
        .child(
            message()
                .align("end")
                .avatar(Element::new().text("ME").w(32.0).h(32.0))
                .content_child(bubble_el)
                .footer("Delivered"),
        )
        .into();
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 448.0, 240.0, [-1.0, -1.0]);
    let msg = find_slot(&root, SLOT_ROOT).expect("message");
    let upper = find_slot(msg, SLOT_UPPER).expect("upper");
    let avatar = find_slot(upper, SLOT_AVATAR).expect("avatar");
    let content = find_slot(upper, SLOT_CONTENT).expect("content");
    assert!(
            (rect_right(msg) - rect_right(avatar)).abs() < 3.0,
            "gallery-like end: avatar on trailing edge (av_right={:.1} msg_right={:.1} co_x={:.1} co_w={:.1})",
            rect_right(avatar),
            rect_right(msg),
            content.layout_bounds[0],
            content.layout_bounds[2]
        );
    assert!(
        (avatar.layout_bounds[0] - rect_right(content)).abs() < 12.0,
        "gallery-like end: content fills up to the avatar"
    );
    let bubble = content
        .children
        .iter()
        .find(|c| c.attrs.get("data-slot").map(String::as_str) == Some("bubble"))
        .expect("bubble");
    assert!(
        (rect_right(bubble) - rect_right(content)).abs() < 4.0,
        "gallery-like end: bubble self-ends against the avatar"
    );
}

#[test]
fn message_footer_justifies_end_when_align_end() {
    let el: Element = message()
        .align("end")
        .content_child(Element::new().text("Ping"))
        .footer("Delivered")
        .into();
    let footer = find_slot(&el, SLOT_FOOTER).expect("footer");
    assert_eq!(footer.style.justify_content, Some(JustifyContent::FLEX_END));
    let header = find_slot(&el, SLOT_HEADER);
    assert!(
        header.is_none() || header.unwrap().style.justify_content != Some(JustifyContent::FLEX_END)
    );
}

#[test]
fn message_header_stays_start_justified_on_end() {
    let el: Element = message()
        .align("end")
        .header("Oliver")
        .content_child(Element::new().text("Hey"))
        .into();
    let header = find_slot(&el, SLOT_HEADER).expect("header");
    assert_eq!(
        header.style.align_self,
        Some(taffy::style::AlignSelf::END),
        "header box self-ends with the bubble (*:data-slot:self-end)"
    );
    assert_ne!(
        header.style.justify_content,
        Some(JustifyContent::FLEX_END),
        "header content stays start-justified"
    );
    let leaf = header
        .children
        .iter()
        .find(|c| c.text.as_deref() == Some("Oliver"))
        .expect("header copy");
    assert_eq!(leaf.text_style.size_resolved(), HEADER_SIZE_PX);
}

#[test]
fn empty_avatar_reserves_default_size() {
    let el: Element = message_avatar().into();
    assert_eq!(slot_name(&el), Some(SLOT_AVATAR));
    assert!(el.children.is_empty());
    assert_eq!(el.style.align_self, Some(taffy::style::AlignSelf::END));
}

#[test]
fn apply_bound_message_align_restyles_in_place() {
    let mut el: Element = message()
        .header("A")
        .content_child(Element::new().text("body"))
        .footer("sent")
        .into();
    assert_eq!(el.style.flex_direction, FlexDirection::Column);
    let status = apply_bound_message_align(&mut el, "end");
    assert_eq!(
        status,
        LayoutStatus::StructureChanged,
        "live align change must StructureChanged (LayoutChanged only dirties the column root)"
    );
    assert_eq!(el.style.flex_direction, FlexDirection::Column);
    let upper = find_slot(&el, SLOT_UPPER).expect("upper");
    assert_eq!(upper.style.flex_direction, FlexDirection::RowReverse);
    let footer = find_slot(&el, SLOT_FOOTER).expect("footer");
    assert_eq!(footer.style.justify_content, Some(JustifyContent::FLEX_END));
}

#[test]
fn footer_row_keeps_avatar_off_actions() {
    let el: Element = message()
        .avatar(Element::new().text("OL"))
        .header("Oliver")
        .content_child(Element::new().text("Send the report."))
        .footer("Copy")
        .into();
    let upper = find_slot(&el, SLOT_UPPER).expect("upper");
    assert!(find_slot(upper, SLOT_AVATAR).is_some());
    assert!(find_slot(upper, SLOT_FOOTER).is_none());
    let footer_row = find_slot(&el, SLOT_FOOTER_ROW).expect("footer row");
    assert!(find_slot(footer_row, SLOT_FOOTER).is_some());
    assert!(find_slot(footer_row, "message-avatar-ghost").is_none());
    assert!(find_slot(footer_row, SLOT_AVATAR).is_none());
    assert_eq!(footer_row.children.len(), 1);
}

#[test]
fn footer_indent_follows_the_avatar_column_after_align_changes() {
    let mut root: Element = message()
        .avatar(Element::new().text("OL"))
        .content_child(Element::new().text("Send the report."))
        .footer("Copy")
        .into();
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    for align in ["start", "end", "start"] {
        apply_bound_message_align(&mut root, align);
        tree.build_and_layout(&mut root, 400.0, 240.0, [-1.0; 2]);
        let content = find_slot(&root, SLOT_CONTENT).expect("content");
        let footer = find_slot(&root, SLOT_FOOTER).expect("footer");
        let delta = if align == "end" {
            rect_right(content) - rect_right(footer)
        } else {
            content.layout_bounds[0] - footer.layout_bounds[0]
        };
        assert!(
            delta.abs() < 0.1,
            "{align}: content={:?}, footer={:?}",
            content.layout_bounds,
            footer.layout_bounds
        );
    }
}

#[test]
fn caption_preserves_bind_identity_and_skips_controls() {
    let mut bound = Element::new().text("align=start");
    bound.template_id = Some("gal-msg-align".into());
    let styled = caption_from_children(vec![bound], &header_text_config());
    assert_eq!(styled[0].template_id.as_deref(), Some("gal-msg-align"));
    assert_eq!(styled[0].text.as_deref(), Some("align=start"));

    let mut btn = Element::new();
    btn.children.push(Element::new().text("copy"));
    let kept = caption_from_children(vec![btn], &footer_text_config());
    assert_eq!(kept[0].children.len(), 1);
    assert!(kept[0].text.is_none());
}

#[test]
fn builder_header_class_and_style() {
    let el: Element = message_header()
        .class("x")
        .style(|b| b.text_size(12.0))
        .text("Hi")
        .into();
    assert_eq!(slot_name(&el), Some(SLOT_HEADER));
    assert_eq!(el.text_style.size_resolved(), 12.0);
}

#[test]
fn group_and_builder_assemble_slots() {
    let el: Element = message_group()
        .child(
            message()
                .avatar(Element::new().text("CN"))
                .header("Oliver")
                .content_child(Element::new().text("Hi"))
                .footer("Delivered"),
        )
        .into();
    assert_eq!(slot_name(&el), Some(SLOT_GROUP));
    let row = find_slot(&el, SLOT_ROOT).expect("row");
    assert!(find_slot(row, SLOT_AVATAR).is_some());
    assert!(find_slot(row, SLOT_CONTENT).is_some());
    assert!(find_slot(row, SLOT_HEADER).is_some());
    assert!(find_slot(row, SLOT_FOOTER).is_some());
}
