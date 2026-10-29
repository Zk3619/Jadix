use super::*;
use crate::app::portal_overlay::{clear_overlay_stack, portal_sync, OverlayKind, OverlayMode};
use crate::core::reactivity::{create_root, Signal};
use crate::layout::layout_tree::LayoutTree;
fn find_desc_attr<'a>(el: &'a Element, attr: &str) -> Option<&'a Element> {
    crate::core::tree::find_in_tree(el, |n| n.attrs.contains_key(attr))
}

#[test]
fn bubble_variants_set_slot_and_paint_key() {
    for name in [
        "default",
        "secondary",
        "muted",
        "tinted",
        "outline",
        "ghost",
        "destructive",
    ] {
        let el: Element = bubble()
            .variant(name)
            .child(bubble_content().text(name))
            .into();
        assert_eq!(
            el.attrs.get(ATTR_SLOT).map(|s| s.as_str()),
            Some(SLOT_BUBBLE)
        );
        assert_eq!(el.attrs.get(ATTR_VARIANT).map(|s| s.as_str()), Some(name));
        let content = el
            .children
            .iter()
            .find(|c| is_bubble_content(c))
            .expect("bubble-content");
        assert_eq!(
            content.attrs.get(ATTR_BUBBLE_VARIANT).map(|s| s.as_str()),
            Some(name)
        );
    }
}

#[test]
fn bubble_align_end_sets_data_align() {
    let el: Element = bubble()
        .align("end")
        .child(bubble_content().text("hi"))
        .into();
    assert_eq!(el.attrs.get(ATTR_ALIGN).map(|s| s.as_str()), Some("end"));
}

#[test]
fn builder_content_class_and_style() {
    let el: Element = bubble()
        .content_with(|c| c.class("px-2").style(|b| b.text_size(20.0)).text("Hi"))
        .into();
    let content = el
        .children
        .iter()
        .find(|c| is_bubble_content(c))
        .expect("bubble-content");
    fn find_size(el: &Element, want: f32) -> bool {
        el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
            || (el.text_style.size_resolved() - want).abs() < 0.01
            || el.children.iter().any(|c| find_size(c, want))
    }
    assert!(
        find_size(content, 20.0),
        "content .style(|b| b.text_size(20)) must land on the content tree"
    );
}

#[test]
fn bubble_group_sets_slot() {
    let el: Element = bubble_group()
        .child(bubble().child(bubble_content().text("a")))
        .into();
    assert_eq!(
        el.attrs.get(ATTR_SLOT).map(|s| s.as_str()),
        Some(SLOT_GROUP)
    );
}

#[test]
fn bubble_reactions_side_and_align_attrs() {
    let el: Element = bubble()
        .child(bubble_content().text("x"))
        .child(
            bubble_reactions()
                .side("top")
                .align("start")
                .child(Element::new().text("👍")),
        )
        .into();
    let reactions = el
        .children
        .iter()
        .find(|c| {
            c.attrs
                .get(ATTR_SLOT)
                .map(|s| s.as_str() == SLOT_REACTIONS)
                .unwrap_or(false)
        })
        .expect("reactions");
    assert_eq!(
        reactions.attrs.get(ATTR_SIDE).map(|s| s.as_str()),
        Some("top")
    );
    assert_eq!(
        reactions.attrs.get(ATTR_ALIGN).map(|s| s.as_str()),
        Some("start")
    );
    assert_eq!(
        reactions.attrs.get(ATTR_LAYOUT).map(|s| s.as_str()),
        Some("row"),
        "layout defaults to independent row"
    );
}

#[test]
fn apply_bound_bubble_variant_updates_content() {
    let mut el: Element = bubble()
        .variant("default")
        .child(bubble_content().text("Hi"))
        .into();
    let status = apply_bound_bubble_variant(&mut el, "outline");
    assert_ne!(status, LayoutStatus::Clean);
    assert_eq!(
        el.attrs.get(ATTR_VARIANT).map(|s| s.as_str()),
        Some("outline")
    );
    let content = el.children.iter().find(|c| is_bubble_content(c)).unwrap();
    assert!(content.style.border_width > 0.5);
}

#[test]
fn bubble_reactions_paint_text_xs_children() {
    let mut el: Element = bubble()
        .child(bubble_content().text("x"))
        .child(bubble_reactions().emoji("👍").emoji("+2"))
        .into();
    let reactions = el
        .children
        .iter()
        .find(|c| {
            c.attrs
                .get(ATTR_SLOT)
                .map(|s| s.as_str() == SLOT_REACTIONS)
                .unwrap_or(false)
        })
        .expect("reactions");
    let inner = find_desc_attr(reactions, ATTR_REACTIONS_INNER).expect("inner bar");
    assert_eq!(
        inner.children.len(),
        1,
        "+N badge is stripped; only 👍 remains"
    );
    let want = IntoF32::into_f32(REACTION_EMOJI_SIZE);
    let chip = &inner.children[0];
    let leaf = chip
        .children
        .iter()
        .find(|c| c.text.as_deref() == Some("👍"))
        .expect("emoji label leaf inside chip");
    assert!(
        (leaf.text_style.size_resolved() - want).abs() < 0.5,
        "reaction should use text-xs (one step below bubble text-sm), got {}",
        leaf.text_style.size_resolved()
    );
    assert!(
        leaf.text_style.size_resolved() < IntoF32::into_f32("0.875rem") - 1.0,
        "reaction glyph must be smaller than bubble body text-sm"
    );
    assert!(
        inner.style.outline_width > 2.5,
        "ring-card outline expected"
    );
    assert!(
        inner.style.radii[0] > 0.0,
        "bar radius is a fixed px value, not rounded_full"
    );
    assert_eq!(
        crate::layout::lp_kind(inner.style.padding.left),
        crate::layout::lp_kind(inner.style.padding.right)
    );
    assert_eq!(
        crate::layout::lp_kind(inner.style.padding.left),
        crate::layout::lp_kind(inner.style.padding.top)
    );
    assert!(
        crate::renderer::element_wants_sticky_xform(chip),
        "reaction chip should sticky-bake so hover:scale uses the Twemoji Bake path"
    );
    assert!(
        !crate::renderer::element_wants_sticky_xform(leaf),
        "emoji leaf itself stays Coverage unless the chip's transform applies"
    );
    // Resting scale must stay 1.0 — animated scale comes from hover utility, not a baked CSS scale.
    assert!(
        (leaf.style.scale[0] - 1.0).abs() < 1e-4 && (leaf.style.scale[1] - 1.0).abs() < 1e-4,
        "emoji leaf must rest at scale 1 (hover applies via class transition)"
    );
    assert_eq!(
        leaf.text_style.line_height_scale,
        Some(1.0),
        "reaction emoji uses leading-none so the glyph sits in the chip"
    );
    assert_eq!(
        leaf.text_style.align_resolved(),
        crate::style::TextAlign::Center,
        "emoji leaf must text-align center so ink sits in the chip square"
    );

    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut el, 400.0, 200.0, [-1.0, -1.0]);
    let inner = el
        .children
        .iter()
        .find(|c| is_bubble_reactions(c))
        .and_then(|rx| find_desc_attr(rx, ATTR_REACTIONS_INNER))
        .expect("inner after layout");
    assert!(
        (inner.layout_bounds[2] - inner.layout_bounds[3]).abs() < 2.0,
        "single-emoji bar must be square so padding reads even ({}×{})",
        inner.layout_bounds[2],
        inner.layout_bounds[3]
    );
    let chip = &inner.children[0];
    let chip_side = IntoF32::into_f32(REACTION_CHIP_SIZE);
    assert!(
        (chip.layout_bounds[2] - chip_side).abs() < 1.0
            && (chip.layout_bounds[3] - chip_side).abs() < 1.0,
        "reaction chip is text-xs line-height square ({}×{}, want {chip_side})",
        chip.layout_bounds[2],
        chip.layout_bounds[3]
    );
    let leaf = chip
        .children
        .iter()
        .find(|c| c.text.as_deref() == Some("👍"))
        .expect("emoji leaf after layout");
    assert!(
        (leaf.layout_bounds[2] - chip.layout_bounds[2]).abs() < 0.75,
        "emoji leaf must fill chip width so text-align can center the glyph box \
             (leaf_w={} chip_w={})",
        leaf.layout_bounds[2],
        chip.layout_bounds[2]
    );
    let chip_cy = chip.layout_bounds[1] + chip.layout_bounds[3] * 0.5;
    let leaf_cy = leaf.layout_bounds[1] + leaf.layout_bounds[3] * 0.5;
    assert!(
        (chip_cy - leaf_cy).abs() < 1.0,
        "emoji leaf must be vertically centered in the chip (chip_cy={chip_cy} leaf_cy={leaf_cy})"
    );
    assert!(
        leaf.layout_bounds[3] < chip.layout_bounds[3] - 1.0,
        "leaf height follows the leading-none line box, not the chip square \
             (leaf_h={} chip_h={})",
        leaf.layout_bounds[3],
        chip.layout_bounds[3]
    );
}

#[test]
fn bubble_reactions_end_stays_in_flow_on_bubble_right() {
    let bubble_el: Element = bubble()
        .align("end")
        .child(bubble_content().text("Reactions"))
        .child(
            bubble_reactions()
                .side("bottom")
                .align("end")
                .emoji("👍")
                .emoji("😮"),
        )
        .into();
    let reactions = bubble_el
        .children
        .iter()
        .find(|c| is_bubble_reactions(c))
        .expect("reactions");
    assert_ne!(
        reactions.style.position,
        taffy::style::Position::Absolute,
        "collapsed reaction bar must occupy flow height"
    );
    assert!(
        reactions.style.translate_pct[0].abs() < 1e-4
            && reactions.style.translate_pct[1].abs() < 1e-4,
        "in-flow bar must not use translate_pct (scroll desync), got {:?}",
        reactions.style.translate_pct
    );
    assert_eq!(
        reactions.style.align_self,
        Some(taffy::style::AlignSelf::FLEX_END),
        "end-aligned reactions sit on the bubble's bottom-right"
    );

    // Root `width: auto` fills the viewport up to max-w 80%; w-fit only applies
    // as a flex child of BubbleGroup (`items: start`).
    let mut group: Element = bubble_group().child(bubble_el).into();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut group, 400.0, 200.0, [-1.0, -1.0]);
    let el = group
        .children
        .iter()
        .find(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_BUBBLE))
        .expect("bubble");
    let bubble_right = el.layout_bounds[0] + el.layout_bounds[2];
    let reactions = el
        .children
        .iter()
        .find(|c| is_bubble_reactions(c))
        .expect("reactions after layout");
    let rx_right = reactions.layout_bounds[0] + reactions.layout_bounds[2];
    assert!(
            rx_right <= bubble_right + 1.0,
            "end-aligned reactions must not extend past the bubble (rx_right={rx_right:.1} bubble_right={bubble_right:.1})"
        );
    assert!(
        reactions.layout_bounds[2] > 8.0,
        "reaction bar must have real width so emoji are not crushed, got {}",
        reactions.layout_bounds[2]
    );
    assert!(
        reactions.layout_bounds[3] > 8.0,
        "reaction bar must occupy height in flow, got {}",
        reactions.layout_bounds[3]
    );
    assert!(
        el.layout_bounds[2] < 200.0,
        "bubble must be w-fit (not stretched to the group), got {}",
        el.layout_bounds[2]
    );
}

#[test]
fn bubble_reactions_inherit_inner_corner() {
    let mut group: Element = bubble_group()
        .child(
            bubble()
                .align("start")
                .child(bubble_content().text("It worked yesterday."))
                .child(bubble_reactions().emoji("👍").emoji("😮")),
        )
        .child(
            bubble()
                .align("end")
                .child(bubble_content().text("You tell me!"))
                .child(bubble_reactions().emoji("👍")),
        )
        .into();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut group, 400.0, 300.0, [-1.0, -1.0]);
    let bubbles: Vec<&Element> = group
        .children
        .iter()
        .filter(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_BUBBLE))
        .collect();
    assert_eq!(bubbles.len(), 2);
    let left = bubbles[0];
    let left_rx = left
        .children
        .iter()
        .find(|c| is_bubble_reactions(c))
        .expect("left reactions");
    assert_eq!(
        left_rx.attrs.get(ATTR_ALIGN).map(|s| s.as_str()),
        Some("end"),
        "left bubble defaults to inner (right) reactions"
    );
    assert_eq!(
        left_rx.style.align_self,
        Some(taffy::style::AlignSelf::FLEX_END)
    );
    let left_right = left.layout_bounds[0] + left.layout_bounds[2];
    let left_rx_right = left_rx.layout_bounds[0] + left_rx.layout_bounds[2];
    let inset = left_right - left_rx_right;
    assert!(
            inset > 4.0 && inset < 16.0,
            "left-bubble reactions inset from the right edge (inset={inset:.1} rx_right={left_rx_right:.1} bubble_right={left_right:.1})"
        );

    let right = bubbles[1];
    let right_rx = right
        .children
        .iter()
        .find(|c| is_bubble_reactions(c))
        .expect("right reactions");
    assert_eq!(
        right_rx.attrs.get(ATTR_ALIGN).map(|s| s.as_str()),
        Some("start"),
        "right bubble defaults to inner (left) reactions"
    );
    assert_eq!(
        right_rx.style.align_self,
        Some(taffy::style::AlignSelf::FLEX_START)
    );
    let start_inset = right_rx.layout_bounds[0] - right.layout_bounds[0];
    assert!(
            start_inset > 4.0 && start_inset < 16.0,
            "right-bubble reactions inset from the left (inset={start_inset:.1} rx_x={:.1} bubble_x={:.1})",
            right_rx.layout_bounds[0],
            right.layout_bounds[0]
        );
}

#[test]
fn bubble_reactions_center_row_overrides_bubble_align() {
    let el: Element = bubble()
        .align("end")
        .child(bubble_content().text("Centered"))
        .child(bubble_reactions().align("center").emoji("👍"))
        .into();
    let rx = el
        .children
        .iter()
        .find(|c| is_bubble_reactions(c))
        .expect("reactions");
    assert_eq!(rx.attrs.get(ATTR_ALIGN).map(|s| s.as_str()), Some("center"));
    assert_eq!(rx.style.align_self, Some(taffy::style::AlignSelf::CENTER));
    assert_ne!(rx.style.position, taffy::style::Position::Absolute);
}

#[test]
fn bubble_reactions_overlay_nips_bubble_edge_in_flow() {
    let mut group: Element = bubble_group()
        .child(
            bubble()
                .align("start")
                .child(bubble_content().text("Overlay pin"))
                .child(
                    bubble_reactions()
                        .layout("overlay")
                        .align("start")
                        .emoji("👍")
                        .emoji("😮"),
                ),
        )
        .child(
            bubble()
                .align("start")
                .child(bubble_content().text("Next line")),
        )
        .into();
    let rx = group
        .children
        .iter()
        .find(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_BUBBLE))
        .and_then(|b| b.children.iter().find(|c| is_bubble_reactions(c)))
        .expect("reactions");
    assert_eq!(
        rx.attrs.get(ATTR_LAYOUT).map(|s| s.as_str()),
        Some("overlay")
    );
    assert_ne!(
        rx.style.position,
        taffy::style::Position::Absolute,
        "overlay must stay in-flow so scroll keeps text glued to the bubble"
    );
    assert_eq!(
        rx.style.z_index, 0,
        "idle overlay must not create a stacking context (page_stack_bands desyncs glyphs)"
    );
    assert!(
        rx.style.translate_pct[0].abs() < 1e-4 && rx.style.translate_pct[1].abs() < 1e-4,
        "overlay must not use translate_pct"
    );

    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut group, 400.0, 300.0, [-1.0, -1.0]);
    let bubbles: Vec<&Element> = group
        .children
        .iter()
        .filter(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_BUBBLE))
        .collect();
    assert_eq!(bubbles.len(), 2);
    let first = bubbles[0];
    let content = first
        .children
        .iter()
        .find(|c| is_bubble_content(c))
        .expect("content");
    let rx = first
        .children
        .iter()
        .find(|c| is_bubble_reactions(c))
        .expect("reactions after layout");
    let content_bottom = content.layout_bounds[1] + content.layout_bounds[3];
    let visual_top = rx.layout_bounds[1] + rx.style.translate[1];
    let visual_bottom = rx.layout_bounds[1] + rx.layout_bounds[3] + rx.style.translate[1];
    assert!(
        rx.layout_bounds[2] > 8.0 && rx.layout_bounds[3] > 8.0,
        "overlay bar must still have real size"
    );
    assert!(
        rx.style.translate[1] < -1.0,
        "bottom overlay must pixel-translate up onto the bubble, got {}",
        rx.style.translate[1]
    );
    assert!(
            visual_top < content_bottom - 2.0,
            "overlay must nip the bubble edge (visual_top={visual_top:.1} content_bottom={content_bottom:.1})"
        );
    assert!(
            visual_bottom > content_bottom + 2.0,
            "overlay must hang outside the bubble (visual_bottom={visual_bottom:.1} content_bottom={content_bottom:.1})"
        );
    assert!(
            visual_top >= content_bottom - 16.0,
            "overlay should only nibble padding, not cover text (visual_top={visual_top:.1} content_bottom={content_bottom:.1})"
        );
    assert!(
        rx.layout_bounds[0] > first.layout_bounds[0] + 4.0,
        "overlay must inset from the bubble edge (rx_x={:.1} bubble_x={:.1})",
        rx.layout_bounds[0],
        first.layout_bounds[0]
    );
    assert!(
        rx.layout_bounds[0] < first.layout_bounds[0] + 16.0,
        "overlay inset should stay modest (rx_x={:.1} bubble_x={:.1})",
        rx.layout_bounds[0],
        first.layout_bounds[0]
    );
    let next_top = bubbles[1].layout_bounds[1];
    let first_bottom = first.layout_bounds[1] + first.layout_bounds[3];
    assert!(
        first.layout_bounds[3] > 8.0,
        "in-flow overlay must keep the bubble height, got {}",
        first.layout_bounds[3]
    );
    assert!(
            next_top >= visual_bottom - 2.0,
            "in-flow hang must keep the next row below the overlay \
             (next_top={next_top:.1} visual_bottom={visual_bottom:.1} first_bottom={first_bottom:.1})"
        );
}

#[test]
fn bubble_reactions_overlay_top_paints_over_content() {
    let el: Element = bubble()
        .align("start")
        .child(bubble_content().text("Top overlay"))
        .child(
            bubble_reactions()
                .side("top")
                .align("start")
                .layout("overlay")
                .emoji("👀"),
        )
        .into();
    assert_eq!(
        el.style.flex_direction,
        taffy::style::FlexDirection::ColumnReverse,
        "top overlay reverses the column so the pill stays last in tree order"
    );
    let rx_idx = el
        .children
        .iter()
        .position(|c| is_bubble_reactions(c))
        .expect("reactions");
    let content_idx = el
        .children
        .iter()
        .position(|c| is_bubble_content(c))
        .expect("content");
    assert!(
            rx_idx > content_idx,
            "overlay top reactions stay after content so they paint on top (rx={rx_idx} content={content_idx})"
        );
    let rx = &el.children[rx_idx];
    assert!(
        rx.style.translate[1] > 1.0,
        "top overlay translates down onto the bubble, got {}",
        rx.style.translate[1]
    );
    assert_eq!(rx.style.z_index, 0, "must not idle-stack via z-index");
}

#[test]
fn bubble_reactions_occupy_flow_so_next_row_is_below() {
    let mut group: Element = bubble_group()
        .child(
            bubble()
                .variant("destructive")
                .align("start")
                .child(bubble_content().text("Failed to run the command."))
                .child(bubble_reactions().align("start").emoji("👍").emoji("😮")),
        )
        .child(
            bubble()
                .variant("ghost")
                .align("start")
                .child(bubble_content().text("Ghost bubbles work for assistant text.")),
        )
        .into();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut group, 400.0, 400.0, [-1.0, -1.0]);
    let bubbles: Vec<&Element> = group
        .children
        .iter()
        .filter(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_BUBBLE))
        .collect();
    assert_eq!(bubbles.len(), 2);
    let rx = bubbles[0]
        .children
        .iter()
        .find(|c| is_bubble_reactions(c))
        .expect("reactions");
    let first_bottom = bubbles[0].layout_bounds[1] + bubbles[0].layout_bounds[3];
    let rx_bottom = rx.layout_bounds[1] + rx.layout_bounds[3];
    let next_top = bubbles[1].layout_bounds[1];
    assert!(
        bubbles[0].layout_bounds[3] > 8.0,
        "bubble with reactions must have height, got {}",
        bubbles[0].layout_bounds[3]
    );
    assert!(
            next_top >= first_bottom - 1.0,
            "next bubble must sit below the previous bubble (next_top={next_top:.1} first_bottom={first_bottom:.1})"
        );
    assert!(
            next_top >= rx_bottom - 1.0,
            "next bubble must sit below the in-flow reaction bar (next_top={next_top:.1} rx_bottom={rx_bottom:.1})"
        );
    assert!(
        rx.layout_bounds[3] > 8.0,
        "reaction bar must have real height, got {}",
        rx.layout_bounds[3]
    );
}

#[test]
fn bubble_reactions_max_visible_is_configurable() {
    let el: Element = bubble()
        .child(bubble_content().text("x"))
        .child(
            bubble_reactions()
                .max_visible(2)
                .emoji("👍")
                .emoji("🔥")
                .emoji("👀")
                .emoji("🎉"),
        )
        .into();
    let reactions = el.children.iter().find(|c| is_bubble_reactions(c)).unwrap();
    assert_eq!(
        reactions.attrs.get(ATTR_MAX_VISIBLE).map(|s| s.as_str()),
        Some("2")
    );
    let inner = find_desc_attr(reactions, ATTR_REACTIONS_INNER).expect("inner");
    assert_eq!(inner.children.len(), 3, "2 visible + +2 badge");
}

#[test]
fn bubble_reactions_width_collapses_visible_chips() {
    let el: Element = bubble()
        .child(bubble_content().text("x"))
        .child(
            bubble_reactions()
                .max_visible(5)
                .emoji("👍")
                .emoji("🔥")
                .emoji("👀")
                .emoji("🎉")
                .emoji("👏"),
        )
        .into();
    let reactions_id = el
        .children
        .iter()
        .find(|c| is_bubble_reactions(c))
        .and_then(|rx| rx.attrs.get(ATTR_REACTIONS_ID).cloned())
        .expect("reactions id");
    let mut host = Element::new().w(44.0).child(el);
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut host, 44.0, 120.0, [-1.0, -1.0]);
    let mut impact = LayoutStatus::Clean;
    impact = impact.merge(crate::authoring::presentation::sync_tree(
        &mut host,
        &mut tree,
        crate::authoring::presentation::Context::new(
            crate::authoring::presentation::Phase::AfterGeometry,
            [44.0, 120.0],
        ),
    ));
    assert_ne!(impact, LayoutStatus::Clean);
    let rendered = BUBBLE_REACTIONS.with(|m| {
        m.borrow()
            .get(&reactions_id)
            .expect("registry")
            .rendered_visible
            .get()
    });
    assert!(
        rendered < 5,
        "tight bubble width must reduce visible reactions, got {rendered}"
    );
    let reactions = host
        .children
        .iter()
        .flat_map(|c| c.children.iter())
        .find(|c| is_bubble_reactions(c))
        .expect("reactions");
    let inner = find_desc_attr(reactions, ATTR_REACTIONS_INNER).expect("inner");
    assert!(
        inner.children.len() <= rendered + 1,
        "collapsed bar must rebuild (chips={}, rendered={rendered})",
        inner.children.len()
    );
}

#[test]
fn bubble_reactions_overflow_keeps_collapsed_bar_and_opens_overlay() {
    let mut el: Element = bubble()
        .child(bubble_content().text("x"))
        .child(
            bubble_reactions()
                .emoji("👍")
                .emoji("🔥")
                .emoji("👀")
                .emoji("🎉")
                .emoji("👏"),
        )
        .into();
    let reactions = el
        .children
        .iter()
        .find(|c| is_bubble_reactions(c))
        .expect("reactions");
    let id = reactions
        .attrs
        .get(ATTR_REACTIONS_ID)
        .expect("reactions id")
        .clone();
    let inner = find_desc_attr(reactions, ATTR_REACTIONS_INNER).expect("inner");
    assert_eq!(inner.children.len(), 4, "3 visible + +2 badge");
    let badge = find_desc_attr(reactions, ATTR_OVERFLOW_CHIP).expect("+N circle");
    assert!(
        badge.style.radii[0] < 0.0,
        "+N badge must be rounded-full so it reads as a circle beside emoji"
    );
    assert!(
        inner.children.iter().any(|c| {
            c.text.as_deref() == Some("+2")
                || c.children.iter().any(|n| n.text.as_deref() == Some("+2"))
                || find_desc_attr(c, ATTR_OVERFLOW_CHIP).is_some()
        }),
        "last chip should be +2"
    );
    assert!(
        find_desc_attr(reactions, ATTR_REACTIONS_OVERLAY).is_none(),
        "expand panel lives on the root portal, not inside the bubble"
    );
    assert_eq!(
        reactions.style.z_index, 0,
        "idle reactions row must not create a stacking context"
    );
    assert_eq!(el.style.z_index, 0, "idle bubble must not lift z-index");

    BUBBLE_REACTIONS.with(|m| {
        let binding = m.borrow();
        let entry = binding.get(&id).expect("registry");
        entry.expanded.set(true);
    });
    let mut impact = LayoutStatus::Clean;
    let mut tree = LayoutTree::new();
    sync_bubble_reactions_subtree(&mut el, &mut tree, &mut impact, 400.0);
    let reactions = el.children.iter().find(|c| is_bubble_reactions(c)).unwrap();
    assert_eq!(
        reactions.attrs.get("data-state").map(|s| s.as_str()),
        Some("open")
    );
    let inner = find_desc_attr(reactions, ATTR_REACTIONS_INNER).expect("inner");
    assert_eq!(
        inner.children.len(),
        4,
        "collapsed bar width must not change when the overlay opens"
    );
    assert_eq!(
        el.style.z_index, 0,
        "open bubble must not lift z-index; the portal layer stacks the panel"
    );
    assert_eq!(
        inner.style.display,
        taffy::style::Display::Flex,
        "collapsed bar stays in flow so the float does not collapse the row"
    );
    assert!(
        inner.style.radii[0] > 0.0,
        "reaction bar uses a fixed radius, not rounded_full sentinel"
    );
    assert_eq!(
        crate::layout::lp_kind(inner.style.padding.left),
        crate::layout::lp_kind(inner.style.padding.right),
        "reaction bar left/right padding must match"
    );
    assert_eq!(
        crate::layout::lp_kind(inner.style.padding.left),
        crate::layout::lp_kind(inner.style.padding.top),
        "reaction bar padding must be equal on all sides so a single emoji stays centered"
    );

    let overlay = build_reaction_expand_panel(
        &id,
        "trigger",
        BubbleReactionsSide::Bottom,
        BubbleReactionsAlign::End,
        overlay_reaction_chips(&["👍", "🔥", "👀", "🎉", "👏"].map(str::to_string)),
        DEFAULT_MAX_VISIBLE_REACTIONS,
        None,
    );
    assert_eq!(overlay.children.len(), 5, "overlay shows all reactions");
    assert_eq!(
        overlay.style.flex_wrap,
        taffy::style::FlexWrap::Wrap,
        "bottom-side expand wraps extra chips downward"
    );
    assert_eq!(
        overlay.style.justify_content,
        Some(taffy::style::JustifyContent::FLEX_START),
        "expand chips pack from the start so left/right padding stays even"
    );
    assert_eq!(
        overlay
            .attrs
            .get(ATTR_FLOATING_EXPAND_FROM_H)
            .and_then(|s| s.parse::<f32>().ok()),
        Some(reaction_collapsed_bar_height()),
        "expand enter/exit clip against collapsed bar height, not 0"
    );
    assert_eq!(
        overlay
            .attrs
            .get(ATTR_FLOATING_EXPAND_FROM_W)
            .and_then(|s| s.parse::<f32>().ok()),
        Some(collapsed_reaction_bar_width(
            5,
            DEFAULT_MAX_VISIBLE_REACTIONS
        )),
        "expand must pin width to the collapsed pill so extra chips wrap instead of popping in"
    );
    assert_eq!(
        overlay.style.radii, inner.style.radii,
        "expanded overlay radius must match the collapsed bar"
    );
    assert_eq!(
        inner.style.bg_color, overlay.style.bg_color,
        "expand overlay chrome matches the reaction bar"
    );
    assert_eq!(
        inner.style.overflow.x, overlay.style.overflow.x,
        "collapsed bar must share the expand chrome overflow clip box \
             (hover:scale inside the +N panel has no AABB crop)"
    );
    assert_eq!(
        inner.style.overflow.y, overlay.style.overflow.y,
        "collapsed bar must share the expand chrome overflow clip box \
             (hover:scale inside the +N panel has no AABB crop)"
    );
    assert_eq!(
        inner.style.align_items, overlay.style.align_items,
        "collapsed bar must share expand chrome items (flex-start), not items_center"
    );
    assert!(
        overlay.style.bg_color[3] > 0.99,
        "expand overlay must be opaque so bubble text cannot show through"
    );
}

#[test]
fn bubble_reactions_expand_top_grows_up_not_over_bubble() {
    let overlay = build_reaction_expand_panel(
        "rx-top",
        "trigger",
        BubbleReactionsSide::Top,
        BubbleReactionsAlign::End,
        overlay_reaction_chips(&["👍", "🔥", "👀", "🎉", "👏"].map(str::to_string)),
        DEFAULT_MAX_VISIBLE_REACTIONS,
        None,
    );
    assert_eq!(
        overlay.style.flex_wrap,
        taffy::style::FlexWrap::WrapReverse,
        "top-side extra rows stack upward"
    );
    assert_eq!(
        overlay.style.justify_content,
        Some(taffy::style::JustifyContent::FLEX_START),
        "top-side expand also left-aligns wrapped rows"
    );
    assert_eq!(
        overlay.style.overflow.y,
        taffy::style::Overflow::Hidden,
        "top expand clips extra rows at the top so wrap-reverse unveils upward"
    );
}

#[test]
#[cfg(feature = "animation-base")]
fn overlay_reaction_chip_hover_scale_tweens_from_rest() {
    // +N hoist builds chips under the pointer (or reveals a wrapped row onto it).
    // First live `:hover` captures rest as the transition baseline *before*
    // overlaying scale-125. Tick or_inserts from current; an empty snapshot
    // after the hover write would snap 1.0 → 1.25.
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let mut chips = overlay_reaction_chips(&["👍".to_string()]);
    let mut chip = chips.pop().expect("chip");
    chip.layout_bounds = [10.0, 10.0, 16.0, 16.0];
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    crate::style::sync_class_variants_subtree(
        &mut chip,
        [18.0, 18.0],
        "",
        false,
        400.0,
        200.0,
        &mut tree,
        &mut impact,
    );
    assert!(
        chip.transition_snapshot.has_active_tweens(),
        "fresh overlay chip hover must start a transform tween, scale={}",
        chip.style.scale[0]
    );
    assert!(
        (chip.style.scale[0] - 1.0).abs() < 0.05,
        "t=0 must hold rest scale, not snap to 1.25, got {}",
        chip.style.scale[0]
    );
    crate::style::advance_css_transition_timeline_ms(75.0);
    impact = LayoutStatus::Clean;
    crate::style::sync_class_variants_subtree(
        &mut chip,
        [18.0, 18.0],
        "",
        false,
        400.0,
        200.0,
        &mut tree,
        &mut impact,
    );
    assert!(
        chip.style.scale[0] > 1.05 && chip.style.scale[0] < 1.24,
        "75ms of 150ms hover:scale-125 must be mid-tween, got {}",
        chip.style.scale[0]
    );
    crate::style::set_css_transition_virtual_timeline(false);
}

fn find_reaction_chip_with_emoji<'a>(el: &'a Element, emoji: &str) -> Option<&'a Element> {
    let is_chip = el
        .class_set
        .as_ref()
        .is_some_and(crate::style::ClassSet::has_pointer_hover_linear_xform);
    if is_chip && el.children.iter().any(|c| c.text.as_deref() == Some(emoji)) {
        return Some(el);
    }
    el.children
        .iter()
        .find_map(|c| find_reaction_chip_with_emoji(c, emoji))
}

fn reaction_xform_bake<'a>(
    result: &'a crate::layout::layout_tree::RenderResult,
    dest_hint: [f32; 4],
) -> &'a crate::renderer::TexturedDraw {
    let bakes: Vec<_> = result
        .textured_draws
        .iter()
        .filter(|d| {
            d.texture_key
                .starts_with(crate::utils::text_texture::TEXT_XFORM_TEX_PREFIX)
        })
        .collect();
    bakes
        .iter()
        .copied()
        .min_by(|a, b| {
            let da = (a.rect[0] + a.rect[2] * 0.5 - (dest_hint[0] + dest_hint[2] * 0.5)).abs()
                + (a.rect[1] + a.rect[3] * 0.5 - (dest_hint[1] + dest_hint[3] * 0.5)).abs();
            let db = (b.rect[0] + b.rect[2] * 0.5 - (dest_hint[0] + dest_hint[2] * 0.5)).abs()
                + (b.rect[1] + b.rect[3] * 0.5 - (dest_hint[1] + dest_hint[3] * 0.5)).abs();
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or_else(|| {
            panic!(
                "expected Bake for reaction emoji; draws={:?}",
                result
                    .textured_draws
                    .iter()
                    .map(|d| (d.texture_key.as_str(), d.rect, d.uv_scale, d.clip_size))
                    .collect::<Vec<_>>()
            )
        })
}

fn xywh_intersect(a: [f32; 4], b: [f32; 4]) -> Option<[f32; 4]> {
    crate::app::damage::intersect_rects(a, b)
}

fn map_tex_ink_to_dest(
    dest: [f32; 4],
    uv_offset: [f32; 2],
    uv_scale: [f32; 2],
    ink: [u32; 4],
    tw: u32,
    th: u32,
) -> [f32; 4] {
    let tw = tw.max(1) as f32;
    let th = th.max(1) as f32;
    let us = uv_scale[0].max(1e-6);
    let vs = uv_scale[1].max(1e-6);
    let ix0 = ink[0] as f32 / tw;
    let iy0 = ink[1] as f32 / th;
    let ix1 = (ink[0] + ink[2]) as f32 / tw;
    let iy1 = (ink[1] + ink[3]) as f32 / th;
    let x0 = dest[0] + (ix0 - uv_offset[0]) / us * dest[2];
    let y0 = dest[1] + (iy0 - uv_offset[1]) / vs * dest[3];
    let x1 = dest[0] + (ix1 - uv_offset[0]) / us * dest[2];
    let y1 = dest[1] + (iy1 - uv_offset[1]) / vs * dest[3];
    [x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0)]
}

fn bake_gpu_clip(d: &crate::renderer::TexturedDraw) -> Option<[f32; 4]> {
    if d.clip_size[0] < 0.0 || d.clip_size[1] < 0.0 {
        None
    } else {
        Some([d.clip_pos[0], d.clip_pos[1], d.clip_size[0], d.clip_size[1]])
    }
}

/// Fraction of each ink-bbox edge that is opaque. An unclipped circle only
/// touches the midpoints (low); an AABB chord (screenshot) fills the edge.
fn ink_aabb_chord_fill(rgba: &[u8], tw: u32, th: u32, ink: [u32; 4]) -> [f32; 4] {
    let opaque = |x: u32, y: u32| -> bool {
        if x >= tw || y >= th {
            return false;
        }
        rgba[((y * tw + x) * 4 + 3) as usize] >= 24
    };
    let [x, y, w, h] = ink;
    if w < 2 || h < 2 {
        return [1.0; 4];
    }
    let mut l = 0u32;
    let mut r = 0u32;
    let mut t = 0u32;
    let mut b = 0u32;
    for i in 0..h {
        if opaque(x, y + i) {
            l += 1;
        }
        if opaque(x + w - 1, y + i) {
            r += 1;
        }
    }
    for i in 0..w {
        if opaque(x + i, y) {
            t += 1;
        }
        if opaque(x + i, y + h - 1) {
            b += 1;
        }
    }
    [
        l as f32 / h as f32,
        t as f32 / w as f32,
        r as f32 / h as f32,
        b as f32 / w as f32,
    ]
}

/// Matches `encode_node_present` for a Node dirty (ui_scale = 1).
fn encode_node_present_scissor(
    dirty: &crate::app::damage::PaintDirty,
    dirties: &[crate::app::damage::PaintDirty],
    prev_slots: &[(u64, [f32; 4])],
    result: &crate::layout::layout_tree::RenderResult,
) -> [f32; 4] {
    let mut scissor = crate::app::draw_cull::node_present_scissor_logical(
        dirty.kind,
        dirty.key,
        dirty.bounds,
        prev_slots,
        &result.node_paints,
        crate::app::draw_cull::node_present_scissor_pad(dirty.kind),
    );
    if dirty.kind == crate::app::damage::PaintDirtyKind::Node {
        if crate::app::damage::page_scissor_excluding_overlay(scissor, None).is_none() {
            return [0.0, 0.0, 0.0, 0.0];
        }
    }
    let layered = crate::app::draw_cull::layered_overlay_slot_bounds(
        dirty.key,
        dirty.bounds,
        &result.node_paints,
    )
    .is_some();
    let skip_wipe = dirty.kind == crate::app::damage::PaintDirtyKind::Node
        && crate::app::damage::paint_dirty_skips_node_dest_reset(dirty, dirties, layered)
        && !crate::app::draw_cull::slot_visual_exceeds_rest(dirty.key, &result.node_paints);
    if !skip_wipe && dirty.kind == crate::app::damage::PaintDirtyKind::Node {
        let scaled_tex = crate::renderer::scale_textured_draws(&result.textured_draws, 1.0);
        let prev =
            crate::app::draw_cull::last_present_prev_for_dirty(dirty.kind, dirty.key, prev_slots);
        if let Some(reset) = crate::app::draw_cull::node_present_dest_reset_full(
            dirty.key,
            dirty.bounds,
            &result.node_paints,
            &result.instances,
            &scaled_tex,
            &[],
            1.0,
            prev,
        ) {
            scissor = crate::app::damage::union_rects(scissor, reset.rect);
        }
    }
    scissor
}

#[test]
fn collapsed_reaction_chip_hover_scale_face_is_not_square_cropped() {
    // Gallery collapsed bar (row, 👍😮, no +N). Screenshot: axis-aligned square
    // crop of the Twemoji face on all four sides, capsule intact. That is the
    // dest-mapped 1.10em ink AABB cut by a rest-sized clip (leaf / chip layout
    // or overflow clip that cancels the 8px node-present pad).
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let mut el: Element = bubble()
        .child(bubble_content().text("It worked yesterday."))
        .child(bubble_reactions().side("bottom").emoji("👍").emoji("😮"))
        .into();
    let mut tree = LayoutTree::new();
    let rest_result = tree.build_and_layout(&mut el, 400.0, 200.0, [-1.0, -1.0]);
    let chip = find_reaction_chip_with_emoji(&el, "😮").expect("😮 chip");
    let rest_layout = chip.layout_bounds;
    let rest_bake = reaction_xform_bake(&rest_result, rest_layout).clone();
    let cx = rest_layout[0] + rest_layout[2] * 0.5;
    let cy = rest_layout[1] + rest_layout[3] * 0.5;
    let mut impact = LayoutStatus::Clean;
    crate::style::sync_class_variants_subtree(
        &mut el,
        [cx, cy],
        "",
        false,
        400.0,
        200.0,
        &mut tree,
        &mut impact,
    );
    crate::style::advance_css_transition_timeline_ms(200.0);
    impact = LayoutStatus::Clean;
    crate::style::sync_class_variants_subtree(
        &mut el,
        [cx, cy],
        "",
        false,
        400.0,
        200.0,
        &mut tree,
        &mut impact,
    );
    let scale = find_reaction_chip_with_emoji(&el, "😮")
        .expect("chip after hover")
        .style
        .scale[0];
    assert!(
        (scale - 1.25).abs() < 0.02,
        "200ms hover:scale-125 must settle at 1.25 (screenshot is the enlarged face), got {scale}"
    );
    let result = tree.build_and_layout(&mut el, 400.0, 200.0, [cx, cy]);
    let chip = find_reaction_chip_with_emoji(&el, "😮").expect("chip after collect");
    let vis = crate::core::tree::visual_paint_aabb(chip, chip.paint_scroll_offset());
    let leaf = chip
        .children
        .iter()
        .find(|c| c.text.as_deref() == Some("😮"))
        .expect("emoji leaf");
    let leaf_vis = crate::core::tree::visual_paint_aabb(leaf, leaf.paint_scroll_offset());
    let bake = reaction_xform_bake(&result, vis).clone();
    let bar = find_desc_attr(&el, ATTR_REACTIONS_INNER);
    crate::app::damage::take_frame_paint_nodes();
    crate::app::damage::push_frame_paint_element(chip, chip.paint_scroll_offset());
    if let Some(bar) = bar {
        crate::app::damage::push_frame_paint_element(bar, bar.paint_scroll_offset());
    }
    let mut dirties = crate::app::damage::take_frame_paint_nodes();
    let rest_slots = crate::app::draw_cull::last_present_visual_slots(
        &rest_result.node_paints,
        &rest_result.instances,
        &crate::renderer::scale_textured_draws(&rest_result.textured_draws, 1.0),
        &rest_result.text_nodes,
        &rest_result.input_chrome_instances,
    );
    let hover_slots = crate::app::draw_cull::last_present_visual_slots(
        &result.node_paints,
        &result.instances,
        &crate::renderer::scale_textured_draws(&result.textured_draws, 1.0),
        &result.text_nodes,
        &result.input_chrome_instances,
    );
    crate::app::damage::expand_paint_dirties_for_translated_slots(
        &mut dirties,
        &hover_slots,
        &rest_slots,
    );
    let chip_key = crate::app::damage::paint_element_key(chip);
    let dirty = dirties
        .iter()
        .find(|d| d.key == chip_key)
        .or_else(|| dirties.first())
        .unwrap_or_else(|| panic!("hover:scale must node-present dirty, dirties={dirties:?}"));
    let scissor = encode_node_present_scissor(dirty, &dirties, &rest_slots, &result);
    let hover_tex = crate::utils::text_texture::cache_peek(&bake.texture_key);
    let Some((tw, th, rgba)) = hover_tex.as_ref() else {
        crate::style::set_css_transition_virtual_timeline(false);
        return;
    };
    let Some(ink) = crate::utils::text_texture::opaque_ink_rect(rgba, *tw, *th) else {
        panic!("😮 Bake has no opaque Twemoji ink");
    };
    let dest_ink = map_tex_ink_to_dest(bake.rect, bake.uv_offset, bake.uv_scale, ink, *tw, *th);
    let bake_idx = result
        .textured_draws
        .iter()
        .position(|d| d.rect == bake.rect && d.texture_key == bake.texture_key)
        .expect("bake index");
    let mut present_cut = dest_ink;
    if let Some(c) = bake_gpu_clip(&bake) {
        present_cut = xywh_intersect(present_cut, c).unwrap_or([0.0, 0.0, 0.0, 0.0]);
    }
    let mut dirty_scissors = Vec::new();
    for d in &dirties {
        let sc = encode_node_present_scissor(d, &dirties, &rest_slots, &result);
        let draws_bake = result
            .node_paints
            .iter()
            .any(|p| p.key == d.key && p.textured.0 <= bake_idx && bake_idx < p.textured.1);
        dirty_scissors.push((d.key, d.bounds, sc, draws_bake, d.kind));
        if draws_bake {
            present_cut = xywh_intersect(present_cut, sc).unwrap_or([0.0, 0.0, 0.0, 0.0]);
        }
    }
    let mut visible = present_cut;
    let overflow_clips: Vec<_> = result
        .node_paints
        .iter()
        .map(|p| (p.key, p.bounds, p.overflow_clip, p.textured))
        .collect();
    let coverage_clips: Vec<_> = result
        .text_nodes
        .iter()
        .map(|n| {
            (
                n.text.clone(),
                n.clip_bounds,
                n.paint_scale,
                n.width,
                n.height,
                n.x,
                n.y,
                n.glyph_raster,
            )
        })
        .collect();
    for (text, clip, _, _, _, _, _, _) in &coverage_clips {
        if text.contains('😮') || text.contains(crate::renderer::emoji_layout::EMOJI_LAYOUT_SLOT)
        {
            if let Some(c) = clip {
                visible = xywh_intersect(visible, *c).unwrap_or([0.0, 0.0, 0.0, 0.0]);
            }
        }
    }
    let font = leaf.text_style.size_resolved();
    let expected = crate::renderer::emoji_layout::EMOJI_INK_EM * font * scale;
    let leaf_box = leaf.layout_bounds;
    let hover_chords = ink_aabb_chord_fill(rgba, *tw, *th, ink);
    let rest_chords = crate::utils::text_texture::cache_peek(&rest_bake.texture_key).and_then(
        |(rw, rh, rrgba)| {
            crate::utils::text_texture::opaque_ink_rect(&rrgba, rw, rh)
                .map(|rink| ink_aabb_chord_fill(&rrgba, rw, rh, rink))
        },
    );
    let dump = format!(
            "scale={scale} expected_face={expected} rest_layout={rest_layout:?} vis={vis:?} \
             leaf_layout={leaf_box:?} leaf_vis={leaf_vis:?} dest_ink={dest_ink:?} visible={visible:?} \
             bake={:?} uv={:?} dirty={:?} scissor={:?} overflow_clips={overflow_clips:?} \
             coverage={coverage_clips:?} dirties={dirties:?} dirty_scissors={dirty_scissors:?} \
             tex_ink={ink:?} rt={tw}x{th} hover_chords={hover_chords:?} rest_chords={rest_chords:?}",
            bake.rect,
            bake.uv_scale,
            dirty,
            scissor,
        );
    assert!(
        leaf.style.overflow.x == taffy::style::Overflow::Visible
            && leaf.style.overflow.y == taffy::style::Overflow::Visible,
        "emoji leaf must stay overflow:visible (hidden strut is the screenshot AABB); \
             overflow={:?}/{:?} {dump}",
        leaf.style.overflow.x,
        leaf.style.overflow.y,
    );
    // Screenshot: yellow 😮 shaved by an axis-aligned square (opaque chords on
    // all four sides of the face). A circle only kisses the ink-bbox midpoints.
    assert!(
        hover_chords.iter().all(|f| *f < 0.55),
        "Twemoji Bake is AABB-cropped (screenshot chords); {dump}"
    );
    let cut_l = visible[0] - dest_ink[0];
    let cut_t = visible[1] - dest_ink[1];
    let cut_r = dest_ink[0] + dest_ink[2] - (visible[0] + visible[2]);
    let cut_b = dest_ink[1] + dest_ink[3] - (visible[1] + visible[3]);
    assert!(
            cut_l < 0.5 && cut_t < 0.5 && cut_r < 0.5 && cut_b < 0.5,
            "present clip square-crops the face (screenshot); cut LTRB=({cut_l},{cut_t},{cut_r},{cut_b}) {dump}"
        );
    let vis_cut_l = leaf_vis[0] - dest_ink[0];
    let vis_cut_t = leaf_vis[1] - dest_ink[1];
    let vis_cut_r = dest_ink[0] + dest_ink[2] - (leaf_vis[0] + leaf_vis[2]);
    let vis_cut_b = dest_ink[1] + dest_ink[3] - (leaf_vis[1] + leaf_vis[3]);
    assert!(
        vis_cut_l < 0.5 && vis_cut_t < 0.5 && vis_cut_r < 0.5 && vis_cut_b < 0.5,
        "leading-none leaf vis AABB square-crops dest-mapped ink; \
             cut LTRB=({vis_cut_l},{vis_cut_t},{vis_cut_r},{vis_cut_b}) {dump}"
    );
    // Screenshot geometry: 1.10em square on the leaf vis center (paint box),
    // not the trimmed opaque bbox (that sits slightly above optical center).
    let cx = leaf_vis[0] + leaf_vis[2] * 0.5;
    let cy = leaf_vis[1] + leaf_vis[3] * 0.5;
    let expected_box = [cx - expected * 0.5, cy - expected * 0.5, expected, expected];
    let e_cut_l = leaf_vis[0] - expected_box[0];
    let e_cut_t = leaf_vis[1] - expected_box[1];
    let e_cut_r = expected_box[0] + expected_box[2] - (leaf_vis[0] + leaf_vis[2]);
    let e_cut_b = expected_box[1] + expected_box[3] - (leaf_vis[1] + leaf_vis[3]);
    assert!(
        e_cut_l < 0.5 && e_cut_t < 0.5 && e_cut_r < 0.5 && e_cut_b < 0.5,
        "1.10em Twemoji square is clipped by the leading-none leaf vis (screenshot AABB); \
             cut LTRB=({e_cut_l},{e_cut_t},{e_cut_r},{e_cut_b}) expected={expected_box:?} {dump}"
    );
    let mut face_visible = expected_box;
    if let Some(c) = bake_gpu_clip(&bake) {
        face_visible = xywh_intersect(face_visible, c).unwrap_or([0.0, 0.0, 0.0, 0.0]);
    }
    for d in &dirties {
        let sc = encode_node_present_scissor(d, &dirties, &rest_slots, &result);
        let draws_bake = result
            .node_paints
            .iter()
            .any(|p| p.key == d.key && p.textured.0 <= bake_idx && bake_idx < p.textured.1);
        if draws_bake {
            face_visible = xywh_intersect(face_visible, sc).unwrap_or([0.0, 0.0, 0.0, 0.0]);
        }
    }
    let p_cut_l = face_visible[0] - expected_box[0];
    let p_cut_t = face_visible[1] - expected_box[1];
    let p_cut_r = expected_box[0] + expected_box[2] - (face_visible[0] + face_visible[2]);
    let p_cut_b = expected_box[1] + expected_box[3] - (face_visible[1] + face_visible[3]);
    assert!(
        p_cut_l < 0.5 && p_cut_t < 0.5 && p_cut_r < 0.5 && p_cut_b < 0.5,
        "present clip square-crops the 1.10em face (screenshot dest-space chords); \
             cut LTRB=({p_cut_l},{p_cut_t},{p_cut_r},{p_cut_b}) expected={expected_box:?} {dump}"
    );
    let dest_cut_l = bake.rect[0] - expected_box[0];
    let dest_cut_t = bake.rect[1] - expected_box[1];
    let dest_cut_r = expected_box[0] + expected_box[2] - (bake.rect[0] + bake.rect[2]);
    let dest_cut_b = expected_box[1] + expected_box[3] - (bake.rect[1] + bake.rect[3]);
    assert!(
        dest_cut_l < 0.5 && dest_cut_t < 0.5 && dest_cut_r < 0.5 && dest_cut_b < 0.5,
        "Bake dest UV-crops the 1.10em face (screenshot); \
             cut LTRB=({dest_cut_l},{dest_cut_t},{dest_cut_r},{dest_cut_b}) \
             expected={expected_box:?} {dump}"
    );
    crate::style::set_css_transition_virtual_timeline(false);
}

#[test]
fn hover_scale_overflow_hidden_must_clip_to_vis_not_rest_layout() {
    // Screenshot geometry: overflow clip stayed on the rest 16×16 layout
    // border while scale¹ dest grew to ~20×20. CSS overflow:hidden clips the
    // transformed padding box (vis), not the Taffy strut.
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let mut chip = reaction_chip_shell("😮");
    chip.style.overflow.x = taffy::style::Overflow::Hidden;
    chip.style.overflow.y = taffy::style::Overflow::Hidden;
    let mut el = Element::new().w(400.0).h(200.0).p(40.0).child(chip);
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut el, 400.0, 200.0, [-1.0, -1.0]);
    let chip = find_reaction_chip_with_emoji(&el, "😮").expect("😮 chip");
    let rest_layout = chip.layout_bounds;
    let cx = rest_layout[0] + rest_layout[2] * 0.5;
    let cy = rest_layout[1] + rest_layout[3] * 0.5;
    let mut impact = LayoutStatus::Clean;
    crate::style::sync_class_variants_subtree(
        &mut el,
        [cx, cy],
        "",
        false,
        400.0,
        200.0,
        &mut tree,
        &mut impact,
    );
    crate::style::advance_css_transition_timeline_ms(200.0);
    impact = LayoutStatus::Clean;
    crate::style::sync_class_variants_subtree(
        &mut el,
        [cx, cy],
        "",
        false,
        400.0,
        200.0,
        &mut tree,
        &mut impact,
    );
    let scale = find_reaction_chip_with_emoji(&el, "😮")
        .expect("chip after hover")
        .style
        .scale[0];
    assert!(
        (scale - 1.25).abs() < 0.02,
        "hover:scale-125 must settle, got {scale}"
    );
    let result = tree.build_and_layout(&mut el, 400.0, 200.0, [cx, cy]);
    let chip = find_reaction_chip_with_emoji(&el, "😮").expect("chip after collect");
    let vis = crate::core::tree::visual_paint_aabb(chip, chip.paint_scroll_offset());
    let leaf = chip
        .children
        .iter()
        .find(|c| c.text.as_deref() == Some("😮"))
        .expect("emoji leaf");
    let bake = reaction_xform_bake(&result, vis).clone();
    let font = leaf.text_style.size_resolved();
    let expected = crate::renderer::emoji_layout::EMOJI_INK_EM * font * scale;
    let dest_ink =
        crate::utils::text_texture::cache_peek(&bake.texture_key).and_then(|(tw, th, rgba)| {
            crate::utils::text_texture::opaque_ink_rect(&rgba, tw, th).map(|ink| {
                map_tex_ink_to_dest(bake.rect, bake.uv_offset, bake.uv_scale, ink, tw, th)
            })
        });
    let Some(_dest_ink) = dest_ink else {
        crate::style::set_css_transition_virtual_timeline(false);
        return;
    };
    let cx = vis[0] + vis[2] * 0.5;
    let cy = vis[1] + vis[3] * 0.5;
    let expected_box = [cx - expected * 0.5, cy - expected * 0.5, expected, expected];
    let rest = chip.layout_bounds;
    let rest_cut_l = rest[0] - vis[0];
    let rest_cut_t = rest[1] - vis[1];
    let rest_cut_r = vis[0] + vis[2] - (rest[0] + rest[2]);
    let rest_cut_b = vis[1] + vis[3] - (rest[1] + rest[3]);
    assert!(
        rest_cut_l > 0.5 || rest_cut_t > 0.5 || rest_cut_r > 0.5 || rest_cut_b > 0.5,
        "documents the screenshot: rest layout {rest:?} is smaller than vis {vis:?}"
    );
    let vis_cut_l = vis[0] - expected_box[0];
    let vis_cut_t = vis[1] - expected_box[1];
    let vis_cut_r = expected_box[0] + expected_box[2] - (vis[0] + vis[2]);
    let vis_cut_b = expected_box[1] + expected_box[3] - (vis[1] + vis[3]);
    assert!(
        vis_cut_l < 0.5 && vis_cut_t < 0.5 && vis_cut_r < 0.5 && vis_cut_b < 0.5,
        "overflow:hidden hover:scale must clip to vis, not rest layout; \
             vis_cut LTRB=({vis_cut_l},{vis_cut_t},{vis_cut_r},{vis_cut_b}) \
             rest_cut LTRB=({rest_cut_l},{rest_cut_t},{rest_cut_r},{rest_cut_b}) \
             dest={:?} uv={:?} vis={vis:?} rest={rest:?} expected={expected_box:?}",
        bake.rect,
        bake.uv_scale,
    );
    let dest = bake.rect;
    let dest_cut_l = dest[0] - expected_box[0];
    let dest_cut_t = dest[1] - expected_box[1];
    let dest_cut_r = expected_box[0] + expected_box[2] - (dest[0] + dest[2]);
    let dest_cut_b = expected_box[1] + expected_box[3] - (dest[1] + dest[3]);
    assert!(
        dest_cut_l < 0.5 && dest_cut_t < 0.5 && dest_cut_r < 0.5 && dest_cut_b < 0.5,
        "Bake dest UV-cropped to rest layout (screenshot chords); \
             cut LTRB=({dest_cut_l},{dest_cut_t},{dest_cut_r},{dest_cut_b}) \
             dest={dest:?} uv={:?} rest={rest:?} expected={expected_box:?}",
        bake.uv_scale,
    );
    crate::style::set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn collapsed_reaction_chip_hover_scale_mid_tween_is_not_square_cropped() {
    // Screenshot is the 150ms tween: yellow 😮 AABB-cropped on all four sides,
    // gray capsule intact. Full `build_and_layout` FACE/mid-tween stayed green
    // while live ViewportsOnly dest-reset / skip-blob still clips to the rest
    // strut. Drive that present path.
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let bubble_el: Element = bubble()
        .child(bubble_content().text("It worked yesterday."))
        .child(bubble_reactions().side("bottom").emoji("👍").emoji("😮"))
        .into();
    let mut el: Element = r#box()
        .overflow("hidden")
        .w(400.0)
        .h(200.0)
        .child(bubble_el)
        .into();
    let mut tree = LayoutTree::new();
    let rest_result = tree.build_and_layout_fast(&mut el, 400.0, 200.0, [-1.0, -1.0], false, false);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut el, 400.0, 200.0, [-1.0, -1.0], true, false);
    let chip = find_reaction_chip_with_emoji(&el, "😮").expect("😮 chip");
    let rest_layout = chip.layout_bounds;
    let cx = rest_layout[0] + rest_layout[2] * 0.5;
    let cy = rest_layout[1] + rest_layout[3] * 0.5;
    crate::app::damage::take_frame_paint_nodes();
    let mut impact = LayoutStatus::Clean;
    crate::style::sync_class_variants_subtree(
        &mut el,
        [cx, cy],
        "",
        false,
        400.0,
        200.0,
        &mut tree,
        &mut impact,
    );
    crate::style::advance_css_transition_timeline_ms(75.0);
    impact = LayoutStatus::Clean;
    crate::style::sync_class_variants_subtree(
        &mut el,
        [cx, cy],
        "",
        false,
        400.0,
        200.0,
        &mut tree,
        &mut impact,
    );
    let scale = find_reaction_chip_with_emoji(&el, "😮")
        .expect("chip after hover")
        .style
        .scale[0];
    assert!(
        scale > 1.05 && scale < 1.24,
        "75ms of 150ms hover:scale-125 must be mid-tween, got {scale}"
    );
    let mut dirties = crate::app::damage::take_frame_paint_nodes();
    let result = tree.build_and_layout_fast(&mut el, 400.0, 200.0, [cx, cy], true, false);
    let chip = find_reaction_chip_with_emoji(&el, "😮").expect("chip after collect");
    let vis = crate::core::tree::visual_paint_aabb(chip, chip.paint_scroll_offset());
    let leaf = chip
        .children
        .iter()
        .find(|c| c.text.as_deref() == Some("😮"))
        .expect("emoji leaf");
    let leaf_vis = crate::core::tree::visual_paint_aabb(leaf, leaf.paint_scroll_offset());
    let bake = reaction_xform_bake(&result, vis).clone();
    let font = leaf.text_style.size_resolved();
    let expected = crate::renderer::emoji_layout::EMOJI_INK_EM * font * scale;
    let cx = leaf_vis[0] + leaf_vis[2] * 0.5;
    let cy = leaf_vis[1] + leaf_vis[3] * 0.5;
    let expected_box = [cx - expected * 0.5, cy - expected * 0.5, expected, expected];
    let rest_slots = crate::app::draw_cull::last_present_visual_slots(
        &rest_result.node_paints,
        &rest_result.instances,
        &crate::renderer::scale_textured_draws(&rest_result.textured_draws, 1.0),
        &rest_result.text_nodes,
        &rest_result.input_chrome_instances,
    );
    let hover_slots = crate::app::draw_cull::last_present_visual_slots(
        &result.node_paints,
        &result.instances,
        &crate::renderer::scale_textured_draws(&result.textured_draws, 1.0),
        &result.text_nodes,
        &result.input_chrome_instances,
    );
    crate::app::damage::expand_paint_dirties_for_translated_slots(
        &mut dirties,
        &hover_slots,
        &rest_slots,
    );
    let bake_idx = result
        .textured_draws
        .iter()
        .position(|d| d.rect == bake.rect && d.texture_key == bake.texture_key)
        .expect("bake index");
    let mut visible = expected_box;
    if let Some(c) = bake_gpu_clip(&bake) {
        visible = xywh_intersect(visible, c).unwrap_or([0.0, 0.0, 0.0, 0.0]);
    }
    let mut dirty_scissors = Vec::new();
    for d in &dirties {
        let sc = encode_node_present_scissor(d, &dirties, &rest_slots, &result);
        let draws_bake = result
            .node_paints
            .iter()
            .any(|p| p.key == d.key && p.textured.0 <= bake_idx && bake_idx < p.textured.1);
        dirty_scissors.push((d.key, d.bounds, sc, draws_bake, d.kind));
        if draws_bake {
            visible = xywh_intersect(visible, sc).unwrap_or([0.0, 0.0, 0.0, 0.0]);
        }
    }
    let coverage: Vec<_> = result
        .text_nodes
        .iter()
        .filter(|n| {
            n.text.contains('😮')
                || n.text
                    .contains(crate::renderer::emoji_layout::EMOJI_LAYOUT_SLOT)
        })
        .map(|n| (n.clip_bounds, n.paint_scale, n.width, n.height))
        .collect();
    let overflow_clips: Vec<_> = result
        .node_paints
        .iter()
        .map(|p| (p.key, p.bounds, p.overflow_clip, p.textured))
        .collect();
    let inst_clips: Vec<_> = result
        .instances
        .iter()
        .map(|i| (i.rect_pos, i.rect_size, i.clip_pos, i.clip_size))
        .collect();
    let dump = format!(
        "scale={scale} rest_layout={rest_layout:?} vis={vis:?} leaf_vis={leaf_vis:?} \
             dest={:?} uv={:?} gpu={:?} expected={expected_box:?} visible={visible:?} \
             coverage={coverage:?} dirties={dirties:?} dirty_scissors={dirty_scissors:?} \
             overflow_clips={overflow_clips:?} inst_clips={inst_clips:?}",
        bake.rect,
        bake.uv_scale,
        bake_gpu_clip(&bake),
    );
    assert!(
        !dirties.is_empty(),
        "hover:scale tween must node-present dirty (empty dirties skip occupancy); {dump}"
    );
    assert!(
        dirty_scissors.iter().any(|s| s.3),
        "a node-present dirty must own the emoji Bake; {dump}"
    );
    let hover_tex = crate::utils::text_texture::cache_peek(&bake.texture_key);
    let Some((tw, th, rgba)) = hover_tex.as_ref() else {
        panic!("😮 Bake texture missing (cannot check screenshot chords); {dump}");
    };
    let Some(ink) = crate::utils::text_texture::opaque_ink_rect(rgba, *tw, *th) else {
        panic!("😮 Bake has no opaque Twemoji ink; {dump}");
    };
    let hover_chords = ink_aabb_chord_fill(rgba, *tw, *th, ink);
    assert!(
        hover_chords.iter().all(|f| *f < 0.55),
        "Twemoji Bake RT is AABB-cropped (screenshot chords={hover_chords:?}); {dump}"
    );
    assert!(
        coverage.is_empty(),
        "mid-tween hover:scale emoji must Bake, not Coverage TextBounds; {dump}"
    );
    assert!(
        bake.uv_scale[0] > 0.97 && bake.uv_scale[1] > 0.97,
        "rest parent_clip UV-cropped the scaled dest (screenshot chords); {dump}"
    );
    let dest_cut_l = bake.rect[0] - expected_box[0];
    let dest_cut_t = bake.rect[1] - expected_box[1];
    let dest_cut_r = expected_box[0] + expected_box[2] - (bake.rect[0] + bake.rect[2]);
    let dest_cut_b = expected_box[1] + expected_box[3] - (bake.rect[1] + bake.rect[3]);
    assert!(
            dest_cut_l < 0.5 && dest_cut_t < 0.5 && dest_cut_r < 0.5 && dest_cut_b < 0.5,
            "mid-tween Bake dest square-crops 1.10em; cut LTRB=({dest_cut_l},{dest_cut_t},{dest_cut_r},{dest_cut_b}) {dump}"
        );
    if let Some(c) = bake_gpu_clip(&bake) {
        let g_cut_l = c[0] - expected_box[0];
        let g_cut_t = c[1] - expected_box[1];
        let g_cut_r = expected_box[0] + expected_box[2] - (c[0] + c[2]);
        let g_cut_b = expected_box[1] + expected_box[3] - (c[1] + c[3]);
        assert!(
                g_cut_l < 0.5 && g_cut_t < 0.5 && g_cut_r < 0.5 && g_cut_b < 0.5,
                "GPU overflow clip square-crops 1.10em mid-tween; cut LTRB=({g_cut_l},{g_cut_t},{g_cut_r},{g_cut_b}) {dump}"
            );
    }
    let p_cut_l = visible[0] - expected_box[0];
    let p_cut_t = visible[1] - expected_box[1];
    let p_cut_r = expected_box[0] + expected_box[2] - (visible[0] + visible[2]);
    let p_cut_b = expected_box[1] + expected_box[3] - (visible[1] + visible[3]);
    assert!(
        p_cut_l < 0.5 && p_cut_t < 0.5 && p_cut_r < 0.5 && p_cut_b < 0.5,
        "node-present scissor square-crops 1.10em mid-tween (screenshot); \
             cut LTRB=({p_cut_l},{p_cut_t},{p_cut_r},{p_cut_b}) {dump}"
    );
    crate::style::set_css_transition_virtual_timeline(false);
}

#[test]
fn collapsed_reaction_chip_hover_scale_tween_must_not_crop_top() {
    // Live: upper face clipped during the 150ms tween, intact after settle.
    // Drive consecutive ViewportsOnly frames (hash_bust only on enter) for
    // both row and overlay layouts — overlay nips content, so occupancy
    // foreign-sibling clip is the top-only present path.
    crate::style::set_css_transition_virtual_timeline(true);
    for layout in ["row", "overlay"] {
        crate::style::set_css_transition_timeline_ms(0.0);
        let bubble_el: Element = bubble()
            .child(bubble_content().text("It worked yesterday."))
            .child(
                bubble_reactions()
                    .side("bottom")
                    .layout(layout)
                    .emoji("👍")
                    .emoji("😮"),
            )
            .into();
        let mut el: Element = r#box()
            .overflow("hidden")
            .w(400.0)
            .h(200.0)
            .child(bubble_el)
            .into();
        let mut tree = LayoutTree::new();
        let rest_result =
            tree.build_and_layout_fast(&mut el, 400.0, 200.0, [-1.0, -1.0], false, false);
        tree.set_collect_skip_allowed(true);
        let _ = tree.build_and_layout_fast(&mut el, 400.0, 200.0, [-1.0, -1.0], true, false);
        let chip = find_reaction_chip_with_emoji(&el, "😮").expect("😮 chip");
        let leaf_vis = {
            let leaf = chip
                .children
                .iter()
                .find(|c| c.text.as_deref() == Some("😮"))
                .expect("emoji leaf");
            crate::core::tree::visual_paint_aabb(leaf, leaf.paint_scroll_offset())
        };
        let mx = leaf_vis[0] + leaf_vis[2] * 0.5;
        let my = leaf_vis[1] + leaf_vis[3] * 0.5;
        let mut prev_slots = crate::app::draw_cull::last_present_visual_slots(
            &rest_result.node_paints,
            &rest_result.instances,
            &crate::renderer::scale_textured_draws(&rest_result.textured_draws, 1.0),
            &rest_result.text_nodes,
            &rest_result.input_chrome_instances,
        );
        crate::app::damage::take_frame_paint_nodes();
        let mut impact = LayoutStatus::Clean;
        crate::style::sync_class_variants_subtree(
            &mut el,
            [mx, my],
            "",
            false,
            400.0,
            200.0,
            &mut tree,
            &mut impact,
        );
        let mut elapsed = 0.0_f32;
        for _ in 0..12 {
            crate::style::advance_css_transition_timeline_ms(16.0);
            elapsed += 16.0;
            impact = LayoutStatus::Clean;
            crate::style::sync_class_variants_subtree(
                &mut el,
                [mx, my],
                "",
                false,
                400.0,
                200.0,
                &mut tree,
                &mut impact,
            );
            let scale = find_reaction_chip_with_emoji(&el, "😮")
                .expect("chip")
                .style
                .scale[0];
            let mut dirties = crate::app::damage::take_frame_paint_nodes();
            let result = tree.build_and_layout_fast(&mut el, 400.0, 200.0, [mx, my], true, false);
            let chip = find_reaction_chip_with_emoji(&el, "😮").expect("chip after collect");
            let vis = crate::core::tree::visual_paint_aabb(chip, chip.paint_scroll_offset());
            let leaf = chip
                .children
                .iter()
                .find(|c| c.text.as_deref() == Some("😮"))
                .expect("emoji leaf");
            let leaf_vis = crate::core::tree::visual_paint_aabb(leaf, leaf.paint_scroll_offset());
            let inner = find_desc_attr(&el, ATTR_REACTIONS_INNER).expect("inner");
            let bar_vis = crate::core::tree::visual_paint_aabb(inner, inner.paint_scroll_offset());
            let bake = reaction_xform_bake(&result, vis).clone();
            let hover_slots = crate::app::draw_cull::last_present_visual_slots(
                &result.node_paints,
                &result.instances,
                &crate::renderer::scale_textured_draws(&result.textured_draws, 1.0),
                &result.text_nodes,
                &result.input_chrome_instances,
            );
            crate::app::damage::expand_paint_dirties_for_translated_slots(
                &mut dirties,
                &hover_slots,
                &prev_slots,
            );
            let bake_idx = result
                .textured_draws
                .iter()
                .position(|d| d.rect == bake.rect && d.texture_key == bake.texture_key)
                .expect("bake index");
            let hover_tex =
                crate::utils::text_texture::cache_peek(&bake.texture_key).expect("😮 Bake texture");
            let (tw, th, rgba) = hover_tex;
            let ink =
                crate::utils::text_texture::opaque_ink_rect(&rgba, tw, th).expect("Twemoji ink");
            let dest_ink =
                map_tex_ink_to_dest(bake.rect, bake.uv_offset, bake.uv_scale, ink, tw, th);
            let mut visible = dest_ink;
            if let Some(c) = bake_gpu_clip(&bake) {
                visible = xywh_intersect(visible, c).unwrap_or([0.0, 0.0, 0.0, 0.0]);
            }
            let mut dirty_scissors = Vec::new();
            for d in &dirties {
                let sc = encode_node_present_scissor(d, &dirties, &prev_slots, &result);
                let draws_bake = result
                    .node_paints
                    .iter()
                    .any(|p| p.key == d.key && p.textured.0 <= bake_idx && bake_idx < p.textured.1);
                dirty_scissors.push((d.key, d.bounds, sc, draws_bake, d.kind));
                if draws_bake {
                    visible = xywh_intersect(visible, sc).unwrap_or([0.0, 0.0, 0.0, 0.0]);
                }
            }
            let content = find_desc_attr(&el, ATTR_SLOT).and_then(|_| {
                fn find_content<'a>(el: &'a Element) -> Option<&'a Element> {
                    if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_CONTENT) {
                        return Some(el);
                    }
                    el.children.iter().find_map(find_content)
                }
                find_content(&el)
            });
            let content_bounds = content.map(|c| c.layout_bounds);
            let overflow_clip = result.node_paints.iter().find_map(|p| {
                let draws_bake = p.textured.0 <= bake_idx && bake_idx < p.textured.1;
                draws_bake.then_some((p.bounds, p.overflow_clip))
            });
            let top_cut = visible[1] - dest_ink[1];
            let dump = format!(
                "layout={layout} t={elapsed}ms scale={scale} dest_ink={dest_ink:?} \
                     visible={visible:?} top_cut={top_cut} bar_vis={bar_vis:?} \
                     chip_vis={vis:?} leaf_vis={leaf_vis:?} dest={:?} gpu={:?} \
                     bar_overflow={:?}/{:?} content_bounds={content_bounds:?} \
                     overflow_clip={overflow_clip:?} dirties={dirties:?} \
                     dirty_scissors={dirty_scissors:?}",
                bake.rect,
                bake_gpu_clip(&bake),
                inner.style.overflow.x,
                inner.style.overflow.y,
            );
            assert!(
                top_cut < 0.5,
                "tween crops emoji top (live: upper face clipped until settle); {dump}"
            );
            let bar_top_cut = bar_vis[1] - dest_ink[1];
            assert!(
                bar_top_cut < 0.5,
                "bar overflow clip box sits below dest_ink top; {dump} bar_top_cut={bar_top_cut}"
            );
            prev_slots = hover_slots;
        }
    }
    crate::style::set_css_transition_virtual_timeline(false);
}

#[test]
fn collapsed_reaction_chip_hover_scale_must_enlarge_visible_dest_on_scroll_only() {
    // Live: collapsed bar (no +N), hover the glyph — scale tween and the
    // enlarged face both gone. Isolated FACE used the chip Taffy center and
    // skipped GPU clip; this drives the emoji ink box (what the cursor is
    // on) inside a gallery-like overflow clip, then asserts the *visible*
    // Bake (dest ∩ GPU clip) is larger than rest — not rest-sized discard.
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let bubble_el: Element = bubble()
        .child(bubble_content().text("It worked yesterday."))
        .child(bubble_reactions().side("bottom").emoji("👍").emoji("😮"))
        .into();
    let mut el: Element = r#box()
        .overflow("hidden")
        .w(400.0)
        .h(200.0)
        .child(bubble_el)
        .into();
    let mut tree = LayoutTree::new();
    // Live gallery hover is StyleChanged + ViewportsOnly: `scroll_only` stamp
    // `reuse_idle`s idle wrappers (hash_paint_self was ~9ms). Full
    // `build_and_layout` / `scroll_only=false` rehash the chip and dest-growth
    // stays green while live skip-replays the rest Bake.
    let rest_result = tree.build_and_layout_fast(&mut el, 400.0, 200.0, [-1.0, -1.0], false, false);
    tree.set_collect_skip_allowed(true);
    let _ = tree.build_and_layout_fast(&mut el, 400.0, 200.0, [-1.0, -1.0], true, false);
    let chip = find_reaction_chip_with_emoji(&el, "😮").expect("😮 chip");
    let rest_layout = chip.layout_bounds;
    let leaf = chip
        .children
        .iter()
        .find(|c| c.text.as_deref() == Some("😮"))
        .expect("emoji leaf");
    let leaf_vis = crate::core::tree::visual_paint_aabb(leaf, leaf.paint_scroll_offset());
    let mx = leaf_vis[0] + leaf_vis[2] * 0.5;
    let my = leaf_vis[1] + leaf_vis[3] * 0.5;
    let rest_bake = reaction_xform_bake(&rest_result, rest_layout);
    let rest_visible = {
        let dest = rest_bake.rect;
        match bake_gpu_clip(rest_bake) {
            Some(c) => xywh_intersect(dest, c).unwrap_or([0.0, 0.0, 0.0, 0.0]),
            None => dest,
        }
    };
    let mut impact = LayoutStatus::Clean;
    crate::style::sync_class_variants_subtree(
        &mut el,
        [mx, my],
        "",
        false,
        400.0,
        200.0,
        &mut tree,
        &mut impact,
    );
    crate::style::advance_css_transition_timeline_ms(200.0);
    impact = LayoutStatus::Clean;
    crate::style::sync_class_variants_subtree(
        &mut el,
        [mx, my],
        "",
        false,
        400.0,
        200.0,
        &mut tree,
        &mut impact,
    );
    let scale = find_reaction_chip_with_emoji(&el, "😮")
        .expect("chip after hover")
        .style
        .scale[0];
    assert!(
        (scale - 1.25).abs() < 0.02,
        "hover on emoji ink must apply hover:scale-125, got {scale}"
    );
    let result = tree.build_and_layout_fast(&mut el, 400.0, 200.0, [mx, my], true, false);
    let chip = find_reaction_chip_with_emoji(&el, "😮").expect("chip after collect");
    let vis = crate::core::tree::visual_paint_aabb(chip, chip.paint_scroll_offset());
    let bake = reaction_xform_bake(&result, vis);
    let dest = bake.rect;
    let visible = match bake_gpu_clip(bake) {
        Some(c) => xywh_intersect(dest, c).unwrap_or([0.0, 0.0, 0.0, 0.0]),
        None => dest,
    };
    assert!(
        visible[2] > rest_visible[2] * 1.12 && visible[3] > rest_visible[3] * 1.12,
        "hover:scale visible dest stayed rest-sized (lost enlarge); \
             scale={scale} rest_layout={rest_layout:?} rest_visible={rest_visible:?} \
             dest={dest:?} visible={visible:?} gpu={:?} uv={:?}",
        bake_gpu_clip(bake),
        bake.uv_scale,
    );
    crate::style::set_css_transition_virtual_timeline(false);
}

#[test]
fn bubble_reactions_hover_leave_closes_expand() {
    let mut el: Element = bubble()
        .child(bubble_content().text("x"))
        .child(
            bubble_reactions()
                .emoji("👍")
                .emoji("🔥")
                .emoji("👀")
                .emoji("🎉")
                .emoji("👏"),
        )
        .into();
    let id = el
        .children
        .iter()
        .find(|c| is_bubble_reactions(c))
        .and_then(|rx| rx.attrs.get(ATTR_REACTIONS_ID).cloned())
        .expect("reactions id");
    BUBBLE_REACTIONS.with(|m| {
        let binding = m.borrow();
        let e = binding.get(&id).expect("registry");
        e.expanded.set(true);
        e.hover_armed.set(true);
        e.bar_hover.set(false);
        e.panel_hover.set(false);
    });
    let mut impact = LayoutStatus::Clean;
    let mut tree = LayoutTree::new();
    sync_bubble_reactions_subtree(&mut el, &mut tree, &mut impact, 400.0);
    BUBBLE_REACTIONS.with(|m| {
        let binding = m.borrow();
        let e = binding.get(&id).expect("registry");
        assert!(
            !e.expanded.peek(),
            "leaving the bar and the portal panel must collapse the expand"
        );
    });
    BUBBLE_REACTIONS.with(|m| {
        m.borrow().get(&id).expect("registry").expanded.set(true);
    });
    let mut impact = LayoutStatus::Clean;
    let mut tree = LayoutTree::new();
    sync_bubble_reactions_subtree(&mut el, &mut tree, &mut impact, 400.0);
    BUBBLE_REACTIONS.with(|m| {
        assert!(
            m.borrow().get(&id).expect("registry").expanded.peek(),
            "click/programmatic open without a hover-out must stay open"
        );
    });
}

#[test]
fn opening_one_reaction_expand_closes_the_other() {
    let (_owner, _) = create_root(|| {
        let a: Element = bubble()
            .child(bubble_content().text("a"))
            .child(
                bubble_reactions()
                    .emoji("👍")
                    .emoji("🔥")
                    .emoji("👀")
                    .emoji("🎉")
                    .emoji("👏"),
            )
            .into();
        let b: Element = bubble()
            .child(bubble_content().text("b"))
            .child(
                bubble_reactions()
                    .emoji("👍")
                    .emoji("🔥")
                    .emoji("👀")
                    .emoji("🎉")
                    .emoji("👏"),
            )
            .into();
        let id_a = a
            .children
            .iter()
            .find(|c| is_bubble_reactions(c))
            .and_then(|rx| rx.attrs.get(ATTR_REACTIONS_ID).cloned())
            .expect("a id");
        let id_b = b
            .children
            .iter()
            .find(|c| is_bubble_reactions(c))
            .and_then(|rx| rx.attrs.get(ATTR_REACTIONS_ID).cloned())
            .expect("b id");
        assert_ne!(id_a, id_b);
        BUBBLE_REACTIONS.with(|m| {
            let map = m.borrow();
            map.get(&id_a).expect("a").expanded.set(true);
            map.get(&id_a).expect("a").hover_armed.set(true);
        });
        open_reaction_expand(&id_b);
        BUBBLE_REACTIONS.with(|m| {
            let map = m.borrow();
            assert!(
                !map.get(&id_a).expect("a").expanded.peek(),
                "opening another reaction expand must close the first"
            );
            assert!(
                map.get(&id_b).expect("b").expanded.peek(),
                "the hovered reaction expand must stay open"
            );
        });
        let _keep = (a, b);
    });
}

#[test]
fn flush_reaction_hover_dismiss_closes_after_leave_batch() {
    let mut el: Element = bubble()
        .child(bubble_content().text("x"))
        .child(
            bubble_reactions()
                .emoji("👍")
                .emoji("🔥")
                .emoji("👀")
                .emoji("🎉")
                .emoji("👏"),
        )
        .into();
    let id = el
        .children
        .iter()
        .find(|c| is_bubble_reactions(c))
        .and_then(|rx| rx.attrs.get(ATTR_REACTIONS_ID).cloned())
        .expect("reactions id");
    let mut layout = LayoutTree::new();
    layout.build_and_layout(&mut el, 400.0, 300.0, [-1.0; 2]);
    let flush = |layout: &mut LayoutTree| {
        layout
            .deadlines
            .prepare(true)
            .into_iter()
            .map(|task| task.dispatch(std::time::Instant::now()).redraw)
            .fold(false, |changed, redraw| changed | redraw)
    };
    BUBBLE_REACTIONS.with(|m| {
        let e = m.borrow();
        let e = e.get(&id).expect("registry");
        e.expanded.set(true);
        e.hover_armed.set(true);
        e.bar_hover.set(true);
        e.panel_hover.set(false);
    });
    assert!(
        !flush(&mut layout),
        "still over the bar — must not close mid hover-transfer"
    );
    note_reactions_hover(&id, HoverPart::Bar, false);
    note_reactions_hover(&id, HoverPart::Panel, true);
    assert!(
        !flush(&mut layout),
        "pointer moved onto the portal panel — stay open"
    );
    note_reactions_hover(&id, HoverPart::Panel, false);
    assert!(
        flush(&mut layout),
        "left bar and panel — close in the same hover batch"
    );
    BUBBLE_REACTIONS.with(|m| {
        assert!(!m.borrow().get(&id).expect("registry").expanded.peek());
    });
    let _ = &mut el;
}

#[test]
fn bubble_reactions_expand_floats_and_may_cover_next_bubble() {
    let mut group: Element = bubble_group()
        .child(
            bubble()
                .align("end")
                .child(bubble_content().text("row · 8 reactions"))
                .child(
                    bubble_reactions()
                        .emoji("👍")
                        .emoji("🔥")
                        .emoji("👀")
                        .emoji("🎉")
                        .emoji("👏")
                        .emoji("❤️")
                        .emoji("🚀")
                        .emoji("✨"),
                ),
        )
        .child(
            bubble()
                .align("end")
                .child(bubble_content().text("Builder bubble")),
        )
        .into();

    let rx = group
        .children
        .iter()
        .find(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_BUBBLE))
        .and_then(|b| b.children.iter().find(|c| is_bubble_reactions(c)))
        .expect("reactions");
    assert!(
        find_desc_attr(rx, ATTR_REACTIONS_OVERLAY).is_none(),
        "portal panel must not occupy the bubble subtree"
    );

    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut group, 400.0, 400.0, [-1.0, -1.0]);
    let bubbles: Vec<&Element> = group
        .children
        .iter()
        .filter(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_BUBBLE))
        .collect();
    assert_eq!(bubbles.len(), 2);
    let first_bottom = bubbles[0].layout_bounds[1] + bubbles[0].layout_bounds[3];
    let next_top = bubbles[1].layout_bounds[1];
    assert!(
            next_top >= first_bottom - 1.0,
            "floating overlay must not push the next bubble in flow (next_top={next_top:.1} first_bottom={first_bottom:.1})"
        );

    let items: Vec<String> = ["👍", "🔥", "👀", "🎉", "👏", "❤️", "🚀", "✨"]
        .into_iter()
        .map(str::to_string)
        .collect();
    let bar_w = find_desc_attr(bubbles[0], ATTR_REACTIONS_INNER)
        .map(|inner| inner.layout_bounds[2])
        .unwrap_or(80.0)
        .max(40.0);
    let mut overlay = build_reaction_expand_panel(
        "rx-wrap",
        "trigger",
        BubbleReactionsSide::Bottom,
        BubbleReactionsAlign::Start,
        overlay_reaction_chips(&items),
        DEFAULT_MAX_VISIBLE_REACTIONS,
        None,
    );
    overlay = overlay.w(bar_w);
    let mut host = Element::new().w(400.0).child(overlay);
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut host, 400.0, 400.0, [-1.0, -1.0]);
    let overlay = &host.children[0];
    let max_w = IntoF32::into_f32(REACTION_EXPAND_MAX_W);
    assert!(
        overlay.layout_bounds[2] <= max_w + 1.0,
        "expanded overlay must wrap instead of spilling sideways (w={:.1} max={max_w:.1})",
        overlay.layout_bounds[2]
    );
    assert!(
        overlay.layout_bounds[3] > IntoF32::into_f32(REACTION_CHIP_SIZE) * 2.0,
        "8 chips must wrap onto a second row instead of overflowing (h={:.1})",
        overlay.layout_bounds[3]
    );
}

#[test]
fn reaction_expand_collapse_must_not_paint_wrap_emojis_outside_accordion() {
    // Live: +N overlay close leaves 👏🩷🚀 hanging below the shrinking capsule.
    // Accordion max-height is the clip; overflow-visible Twemoji must not skip
    // that ancestor clip (hover:scale still fits in the padded pill vis).
    let items: Vec<String> = ["👍", "🔥", "👀", "🎉", "👏", "🩷", "🚀"]
        .into_iter()
        .map(str::to_string)
        .collect();
    let mut overlay = build_reaction_expand_panel(
        "rx-collapse-ghost",
        "trigger",
        BubbleReactionsSide::Bottom,
        BubbleReactionsAlign::Start,
        overlay_reaction_chips(&items),
        DEFAULT_MAX_VISIBLE_REACTIONS,
        None,
    );
    overlay.set_attr(crate::layout::layout_tree::ATTR_HOISTED_PORTAL, "1");
    // Live +N `match_trigger_width` is the collapsed bar (3 chips + overflow),
    // not `REACTION_EXPAND_MAX_W` (12rem still fits 7 chips in one row).
    let bar_w = collapsed_reaction_bar_width_for(DEFAULT_MAX_VISIBLE_REACTIONS, items.len());
    overlay.style.size.width = taffy::style::Dimension::length(bar_w);
    overlay.style.max_size.width = taffy::style::Dimension::length(bar_w);
    let mut tree = LayoutTree::new();
    let _open = tree.build_and_layout(&mut overlay, 400.0, 400.0, [-1.0, -1.0]);
    let open_h = overlay.layout_bounds[3];
    let collapsed_h = reaction_collapsed_bar_height();
    assert!(
            open_h > collapsed_h * 1.8,
            "7 chips at trigger width must wrap (open_h={open_h:.1} collapsed={collapsed_h:.1} bar_w={bar_w:.1} layout_w={:.1})",
            overlay.layout_bounds[2]
        );
    let wrap_chip = find_reaction_chip_with_emoji(&overlay, "🚀").expect("🚀 wrap chip");
    let wrap_vis = crate::core::tree::visual_paint_aabb(wrap_chip, wrap_chip.paint_scroll_offset());
    assert!(
        wrap_vis[1] + wrap_vis[3] > overlay.layout_bounds[1] + collapsed_h + 8.0,
        "open wrap chip must sit below the collapsed capsule (wrap_vis={wrap_vis:?} \
             capsule={:.1} layout={:?})",
        overlay.layout_bounds[1] + collapsed_h,
        overlay.layout_bounds
    );

    overlay.style.max_size.height = taffy::style::Dimension::length(collapsed_h);
    let collapsed = tree.build_and_layout(&mut overlay, 400.0, 400.0, [-1.0, -1.0]);
    let clip_top = overlay.layout_bounds[1];
    let clip_h = crate::layout::dim_length_px(overlay.style.max_size.height)
        .unwrap_or(overlay.layout_bounds[3])
        .min(overlay.layout_bounds[3]);
    let clip_bottom = clip_top + clip_h;
    let mut leak: Vec<([f32; 4], Option<[f32; 4]>, f32)> = Vec::new();
    for d in &collapsed.textured_draws {
        if !d
            .texture_key
            .starts_with(crate::utils::text_texture::TEXT_XFORM_TEX_PREFIX)
        {
            continue;
        }
        let dest_bottom = d.rect[1] + d.rect[3];
        let gpu = bake_gpu_clip(d);
        let visible_bottom = gpu
            .and_then(|c| xywh_intersect(d.rect, c))
            .map(|v| v[1] + v[3])
            .unwrap_or(dest_bottom);
        if visible_bottom > clip_bottom + 1.0 {
            leak.push((d.rect, gpu, visible_bottom));
        }
    }
    let dump = format!(
        "clip=[{clip_top:.1}..{clip_bottom:.1}] layout={:?} max_h={clip_h:.1} \
             overflow={:?}/{:?} wrap_vis={wrap_vis:?} leaks={leak:?} bakes={}",
        overlay.layout_bounds,
        overlay.style.overflow.x,
        overlay.style.overflow.y,
        collapsed
            .textured_draws
            .iter()
            .filter(|d| d
                .texture_key
                .starts_with(crate::utils::text_texture::TEXT_XFORM_TEX_PREFIX))
            .map(|d| format!("{:?} gpu={:?}", d.rect, bake_gpu_clip(d)))
            .collect::<Vec<_>>()
            .join("; ")
    );
    assert!(
        leak.is_empty(),
        "collapse paints wrap-row emoji below the accordion capsule (live ghosts); {dump}"
    );
}

#[test]
fn bubble_reaction_expand_class_applies_to_panel_chrome() {
    let overlay = build_reaction_expand_panel(
        "rx-cls",
        "trigger",
        BubbleReactionsSide::Bottom,
        BubbleReactionsAlign::Start,
        overlay_reaction_chips(&["👍", "🔥"].map(str::to_string)),
        2,
        Some("bg-card shadow-md"),
    );
    assert!(
        overlay
            .class_set
            .as_ref()
            .map(|s| {
                s.base.iter().any(|c| c == "bg-card") && s.base.iter().any(|c| c == "shadow-md")
            })
            .unwrap_or(false),
        "expand_class must land on expand chrome class_set"
    );
    let default_overlay = build_reaction_expand_panel(
        "rx-def",
        "trigger",
        BubbleReactionsSide::Top,
        BubbleReactionsAlign::End,
        overlay_reaction_chips(&["👍"].map(str::to_string)),
        1,
        None,
    );
    assert_eq!(
        default_overlay.attrs.get(ATTR_SIDE).map(|s| s.as_str()),
        Some("top")
    );
}

#[test]
fn bubble_reaction_expand_is_exclusive_popover() {
    clear_overlay_stack();
    let (_owner, _) = create_root(|| {
        let menu = Signal::new(true);
        let rx = Signal::new(true);
        portal_sync("menu", menu, OverlayKind::Menu, OverlayMode::Stacked);
        portal_sync("rx", rx, OverlayKind::Popover, OverlayMode::Stacked);
        assert!(
            !menu.peek(),
            "opening a reaction popover must dismiss menus"
        );
        let other = Signal::new(true);
        portal_sync("rx-2", other, OverlayKind::Popover, OverlayMode::Stacked);
        assert!(
            !rx.peek(),
            "a second expand / popover must dismiss the first"
        );
    });
    clear_overlay_stack();
}

#[test]
fn bubble_group_has_vertical_gap() {
    let el: Element = bubble_group()
        .child(bubble().child(bubble_content().text("a")))
        .child(bubble().child(bubble_content().text("b")))
        .into();
    let zero = taffy::style::LengthPercentage::length(0.0);
    assert_ne!(
        el.style.gap.height, zero,
        "BubbleGroup must space bubbles vertically"
    );
    assert_eq!(
        el.style.align_items,
        Some(taffy::style::AlignItems::START),
        "group must not stretch bubbles to full width (ShadCN w-fit)"
    );
}

#[test]
fn bubble_group_keeps_end_bubble_content_sized() {
    let mut group: Element = bubble_group()
        .child(bubble().align("start").child(bubble_content().text("Hi")))
        .child(
            bubble()
                .align("end")
                .child(bubble_content().text("Yo"))
                .child(bubble_reactions().side("bottom").align("end").emoji("👍")),
        )
        .into();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut group, 400.0, 300.0, [-1.0, -1.0]);
    let bubbles: Vec<&Element> = group
        .children
        .iter()
        .filter(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_BUBBLE))
        .collect();
    assert_eq!(bubbles.len(), 2, "group should contain two bubbles");
    let start = bubbles[0];
    let end = bubbles[1];
    assert!(
        start.layout_bounds[2] < 120.0,
        "start bubble must be w-fit, got {}",
        start.layout_bounds[2]
    );
    assert!(
        end.layout_bounds[2] < 120.0,
        "end bubble must be w-fit (not 80% of the group), got {}",
        end.layout_bounds[2]
    );
    let end_right = end.layout_bounds[0] + end.layout_bounds[2];
    assert!(
        end_right > 280.0,
        "end bubble should sit on the right, right edge={end_right:.1}"
    );
    let rx = end
        .children
        .iter()
        .find(|c| is_bubble_reactions(c))
        .expect("reactions");
    let rx_right = rx.layout_bounds[0] + rx.layout_bounds[2];
    assert!(
            rx_right <= end_right + 2.0,
            "end reactions must stay on the bubble, not the group (rx_right={rx_right:.1} bubble_right={end_right:.1})"
        );
}

fn is_bubble_reactions(el: &Element) -> bool {
    el.attrs
        .get(ATTR_SLOT)
        .map(|s| s.as_str() == SLOT_REACTIONS)
        .unwrap_or(false)
}

#[test]
fn default_variant_text_uses_primary_foreground() {
    let el: Element = bubble()
        .variant("default")
        .align("end")
        .child(bubble_content().text("You tell me!"))
        .into();
    let content = el
        .children
        .iter()
        .find(|c| is_bubble_content(c))
        .expect("bubble-content");
    let want = parse_text_color("primary-foreground");
    let mut found = false;
    crate::core::tree::walk_elements(content, |n| {
        if n.text.as_deref() == Some("You tell me!") {
            found = true;
            assert_eq!(
                n.text_style.color,
                Some(want),
                "default bubble text must contrast with primary fill"
            );
        }
    });
    assert!(found, "default bubble must keep its text node");
}

#[test]
fn destructive_variant_uses_token_opacity_not_custom_mix() {
    let el: Element = bubble()
        .variant("destructive")
        .child(bubble_content().text("err"))
        .into();
    let content = el
        .children
        .iter()
        .find(|c| is_bubble_content(c))
        .expect("content");
    let want = parse_color(if is_dark_theme() {
        "destructive/20"
    } else {
        "destructive/10"
    });
    assert!(
        (0..4).all(|i| (content.style.bg_color[i] - want[i]).abs() < 1e-4),
        "destructive bubble bg must come from theme token opacity suffix"
    );
}

#[test]
fn bubble_content_sizes_to_text() {
    let mut el: Element = bubble()
        .variant("secondary")
        .child(bubble_content().text("Short"))
        .into();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut el, 400.0, 200.0, [-1.0, -1.0]);
    let w = el.layout_bounds[2];
    assert!(
        w < 360.0,
        "bubble should stay content-sized below 80% of 400px, got {w:.1}"
    );
}
