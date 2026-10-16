//! AvatarGroup label-to-row gap must stay stable when only scroll offset changes.

use crate::app::reactive::reset_registered_anchors;
use crate::components::avatar::{
    AvatarGroup, AvatarGroupCount, AvatarGroupCountProps, AvatarGroupProps,
};
use crate::core::reactivity::create_root;
use crate::core::scroll_viewport::{activate_scroll_viewport, SCROLL_VIEWPORT_FOCUS_PREFIX};
use crate::core::tree::{Element, IntoElementList, LayoutTree};
use crate::layout::layout_tree::hoist_portal_subtrees;
use crate::renderer::font_loader;
use crate::text::install_shared_font_system;

fn avatar_with_fallback(label: &str) -> Element {
    crate::components::avatar::avatar().fallback(label).into()
}

fn build_scrolled_avatar_section() -> Element {
    let mut section = Element::new().flex_col().w(400.0).gap(4.0);
    section = section.child(
        Element::new()
            .text("AvatarGroup + Count")
            .text_size(12.0)
            .font_weight(700)
            .w(400.0),
    );
    let mut desc = Element::new()
        .text("悬停 / 点击 +N：原地向下盖住展开（expand_placement / expand_class 可配）；Count 内可嵌隐藏头像参与展开列表")
        .text_size(11.0)
        .w(400.0);
    desc.set_attr("data-test-desc", "1");
    section = section.child(desc);
    let mut frag = Element::new();
    frag.set_attr("jadix_fragment", "1");
    frag = frag.children(vec![
        avatar_with_fallback("A"),
        avatar_with_fallback("B"),
        avatar_with_fallback("C"),
        AvatarGroupCount(AvatarGroupCountProps {
            children: Some(Element::new().text("+3")),
            ..Default::default()
        }),
    ]);
    for kid in AvatarGroup(AvatarGroupProps {
        children: Some(frag),
        ..Default::default()
    })
    .into_element_list()
    {
        section = section.child(kid);
    }

    let mut tall = Element::new().flex_col().w(400.0).gap(16.0);
    for _ in 0..8 {
        tall = tall.child(
            Element::new()
                .w(400.0)
                .h(80.0)
                .flex_shrink(0.0)
                .bg([0.95, 0.95, 0.95, 1.0]),
        );
    }
    tall = tall.child(section);
    for _ in 0..4 {
        tall = tall.child(
            Element::new()
                .w(400.0)
                .h(80.0)
                .flex_shrink(0.0)
                .bg([0.92, 0.92, 0.92, 1.0]),
        );
    }

    let viewport = activate_scroll_viewport(
        Element::new()
            .overflow_y("auto")
            .overflow_x("hidden")
            .w(400.0)
            .h(280.0)
            .child(tall),
        "avatar-scroll-test",
        "hidden",
        "auto",
        "jump",
        "auto",
    );
    Element::new().w(400.0).h(280.0).child(viewport)
}

fn find_viewport_mut<'a>(el: &'a mut Element) -> Option<&'a mut Element> {
    if el
        .focus_id
        .as_deref()
        .is_some_and(|f| f == format!("{SCROLL_VIEWPORT_FOCUS_PREFIX}avatar-scroll-test"))
    {
        return Some(el);
    }
    for child in &mut el.children {
        if let Some(hit) = find_viewport_mut(child) {
            return Some(hit);
        }
    }
    None
}

fn set_scroll_y(root: &mut Element, y: f32) {
    let vp = find_viewport_mut(root).expect("scroll viewport");
    vp.scroll_offset = [0.0, y];
    use crate::core::reactivity::use_signal;
    use crate::core::scroll_viewport::scroll_offset_y_signal_key;
    use_signal(
        scroll_offset_y_signal_key("avatar-scroll-test").as_str(),
        || 0.0_f32,
    )
    .set(y);
}

fn find_desc<'a>(el: &'a Element) -> Option<&'a Element> {
    if el.attrs.get("data-test-desc").map(|s| s.as_str()) == Some("1") {
        return Some(el);
    }
    el.children.iter().find_map(find_desc)
}

fn find_group_row<'a>(el: &'a Element) -> Option<&'a Element> {
    if el.attrs.get("data-slot").map(|s| s.as_str()) == Some("avatar-group") {
        return Some(el);
    }
    el.children.iter().find_map(find_group_row)
}

/// Paint-space gap: row top minus description layout bottom, both minus viewport scroll.
fn gap_label_to_avatar(root: &Element, scroll_y: f32) -> Option<f32> {
    let desc = find_desc(root)?;
    let row = find_group_row(root)?;
    let text_bottom = desc.layout_bounds[1] + desc.layout_bounds[3] - scroll_y;
    let avatar_top = row.layout_bounds[1] - scroll_y;
    Some(avatar_top - text_bottom)
}

#[test]
fn avatar_group_label_gap_stable_on_scroll_only_collect() {
    reset_registered_anchors();
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let (_owner, mut root) = create_root(|| build_scrolled_avatar_section());
    let mut tree = LayoutTree::new();
    let _ = hoist_portal_subtrees(&mut root);
    let _ = tree.build_and_layout(&mut root, 400.0, 280.0, [-1.0, -1.0]);

    set_scroll_y(&mut root, 520.0);
    let _ = hoist_portal_subtrees(&mut root);
    let _ = tree.build_and_layout(&mut root, 400.0, 280.0, [-1.0, -1.0]);
    let gap0 = gap_label_to_avatar(&root, 520.0).expect("baseline gap at scroll=520");

    set_scroll_y(&mut root, 560.0);
    let _ = hoist_portal_subtrees(&mut root);
    let _ = tree.build_and_layout_fast(&mut root, 400.0, 280.0, [-1.0, -1.0], true, false);
    let gap1 = gap_label_to_avatar(&root, 560.0).expect("scrolled gap at scroll=560");

    assert!(
        (gap0 - gap1).abs() < 1.0,
        "label-to-avatar gap must not drift on scroll-only collect (before={gap0:.2} after={gap1:.2})"
    );
}
