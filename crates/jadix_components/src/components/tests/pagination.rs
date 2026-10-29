use super::*;

#[test]
fn range_small_lists_all_pages() {
    assert_eq!(
        pagination_range(1, 5, PaginationRangeOpts::new(1, 1, 1)),
        (1..=5).map(PaginationPage::Number).collect::<Vec<_>>()
    );
}

#[test]
fn range_near_start_expands_to_keep_stable_width() {
    // L=3,S=1,R=1 → 目标格子数 3+1+2+1+2=9；靠前扩展左段。
    // left_expanded = 3+3+1 = 7 → 1..7 … 20
    let r = pagination_range(1, 20, PaginationRangeOpts::new(3, 1, 1));
    assert_eq!(
        r,
        vec![
            PaginationPage::Number(1),
            PaginationPage::Number(2),
            PaginationPage::Number(3),
            PaginationPage::Number(4),
            PaginationPage::Number(5),
            PaginationPage::Number(6),
            PaginationPage::Number(7),
            PaginationPage::Ellipsis,
            PaginationPage::Number(20),
        ]
    );
}

#[test]
fn range_middle_with_single_boundaries() {
    // 1 … 5 6 7 … n
    let r = pagination_range(6, 20, PaginationRangeOpts::new(1, 1, 1));
    assert_eq!(
        r,
        vec![
            PaginationPage::Number(1),
            PaginationPage::Ellipsis,
            PaginationPage::Number(5),
            PaginationPage::Number(6),
            PaginationPage::Number(7),
            PaginationPage::Ellipsis,
            PaginationPage::Number(20),
        ]
    );
}

#[test]
fn range_three_segments_configurable() {
    // 1 2 3 … 6 7 8 9 10 … n-3 n-2 n-1 n
    let r = pagination_range(8, 30, PaginationRangeOpts::new(3, 2, 4));
    assert_eq!(
        r,
        vec![
            PaginationPage::Number(1),
            PaginationPage::Number(2),
            PaginationPage::Number(3),
            PaginationPage::Ellipsis,
            PaginationPage::Number(6),
            PaginationPage::Number(7),
            PaginationPage::Number(8),
            PaginationPage::Number(9),
            PaginationPage::Number(10),
            PaginationPage::Ellipsis,
            PaginationPage::Number(27),
            PaginationPage::Number(28),
            PaginationPage::Number(29),
            PaginationPage::Number(30),
        ]
    );
}

#[test]
fn range_item_count_stable_across_pages() {
    let opts = PaginationRangeOpts::new(3, 2, 4);
    let expected = range_item_budget(3, 2, 4); // 3+4+5+2 = 14
    for page in 1..=30 {
        let r = pagination_range(page, 30, opts);
        assert_eq!(
            r.len(),
            expected,
            "page {page}: expected stable len {expected}, got {:?} (len={})",
            r,
            r.len()
        );
    }
}

#[test]
fn range_max_items_compresses_caps() {
    let opts = PaginationRangeOpts::new(3, 2, 4).max_items(7);
    // 压到预算 ≤ 7：最终约 left=1,sibling=0,right=1 → 1+1+1+2=5，或类似。
    let mid = pagination_range(15, 30, opts);
    assert!(
        mid.len() <= 7,
        "max_items must cap grid count, got {:?}",
        mid
    );
    let start = pagination_range(1, 30, opts);
    let end = pagination_range(30, 30, opts);
    assert_eq!(start.len(), mid.len());
    assert_eq!(end.len(), mid.len());
}

#[test]
fn range_very_small_max_items_stays_compact() {
    let r = pagination_range(8, 30, PaginationRangeOpts::new(3, 2, 4).max_items(3));
    assert!(r.len() <= 3, "got {:?}", r);
    assert!(r.contains(&PaginationPage::Number(1)));
    assert!(r.contains(&PaginationPage::Number(30)));
}

#[test]
fn fit_width_compresses_and_may_hide_nav_text() {
    let (wide_slots, wide_compact) = fit_pagination_for_width(800.0, "上一页", "下一页");
    assert!(
        wide_slots >= 10,
        "wide should fit many slots, got {wide_slots}"
    );
    assert!(!wide_compact);

    let (narrow_slots, narrow_compact) = fit_pagination_for_width(280.0, "上一页", "下一页");
    assert!(narrow_slots < wide_slots);
    assert!(narrow_slots >= 1);
    // 中文 Prev/Next 较宽，窄屏应收起文案。
    assert!(narrow_compact);

    assert_eq!(
        fit_pagination_for_width(0.0, "Previous", "Next"),
        (0, false)
    );
}

#[test]
fn parse_jumper_page_accepts_integers_only() {
    assert_eq!(parse_jumper_page("12", 30), Some(12));
    assert_eq!(parse_jumper_page(" 30 ", 30), Some(30));
    assert_eq!(parse_jumper_page("99", 30), Some(30));
    assert_eq!(parse_jumper_page("0", 30), None);
    assert_eq!(parse_jumper_page("-1", 30), None);
    assert_eq!(parse_jumper_page("1.5", 30), None);
    assert_eq!(parse_jumper_page("abc", 30), None);
    assert_eq!(parse_jumper_page("", 30), None);
}

#[test]
fn jumper_input_wires_enter_to_jump() {
    let page = use_signal("pagination_jumper_enter_page", || 3usize);
    let el = paint_jumper("pg-enter", 3, 20, "Go", page, None);
    fn find_template<'a>(el: &'a Element, tid: &str) -> Option<&'a Element> {
        if el.template_id.as_deref() == Some(tid) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_template(c, tid))
    }
    let input = find_template(&el, "pg-enter__jump_input").expect("jump input");
    assert!(
        input.on_key_down.is_some(),
        "jump input must wire on_key_down for Enter"
    );
}

#[test]
fn builder_content_with_jumper_exposes_slot() {
    let page = use_signal("pagination_builder_jumper_page", || 3usize);
    let jumper = JumperPaint {
        button_text: "确定".into(),
    };
    let el = paint_builder_content(
        "pg-jump",
        3,
        20,
        PaginationRangeOpts::new(1, 1, 1),
        "上一页",
        "下一页",
        false,
        Some(&jumper),
        page,
        None,
        &PaginationPartChrome::default(),
    );
    fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
        if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(slot) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_slot(c, slot))
    }
    assert!(
        find_slot(&el, SLOT_JUMPER).is_some(),
        "jumper slot must be present"
    );
}

#[test]
fn jumper_chrome_reserves_width_for_fit() {
    let bare = fit_pagination_for_width(400.0, "上一页", "下一页").0;
    let chrome = estimate_jumper_chrome_px("确定", 30);
    assert!(chrome > 40.0);
    let with = fit_pagination_for_width_ex(400.0, "上一页", "下一页", chrome).0;
    assert!(with <= bare);
}

#[test]
fn rsx_root_exposes_slot() {
    let el = Pagination(PaginationProps {
        id: String::new(),
        children: Some(PaginationContent(PaginationContentProps {
            children: Some(PaginationItem(PaginationItemProps {
                children: Some(PaginationEllipsis(PaginationEllipsisProps {
                    children: None,
                    base: None,
                })),
                base: None,
            })),
            base: None,
        })),
        base: None,
    });
    assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
    assert!(el.template_id.is_none());
}

#[test]
fn link_active_sets_data_active() {
    let el = PaginationLink(PaginationLinkProps {
        id: String::new(),
        is_active: Some(true),
        size: Some("icon".into()),
        disabled: Some(false),
        on_click: Some(Handler::default()),
        children: Some(Element::new().text("3")),
        base: None,
    });
    assert_eq!(
        el.attrs.get("data-active").map(String::as_str),
        Some("true")
    );
    assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_LINK));
}

#[test]
fn pagination_link_wires_on_click() {
    use std::cell::Cell;
    let hit = Rc::new(Cell::new(0usize));
    let hit2 = Rc::clone(&hit);
    let el = PaginationLink(PaginationLinkProps {
        id: "pg-link-click".into(),
        is_active: Some(false),
        size: Some("icon".into()),
        disabled: Some(false),
        on_click: Some(Handler::new(move |_| hit2.set(hit2.get() + 1))),
        children: Some(Element::new().text("2")),
        base: None,
    });
    assert!(el.on_click.is_some(), "link must expose on_click");
    el.on_click.as_ref().unwrap().borrow_mut()();
    assert_eq!(hit.get(), 1);
}

#[test]
fn previous_next_wire_on_click_when_enabled() {
    use std::cell::Cell;
    let hit = Rc::new(Cell::new(0usize));
    let hit_p = Rc::clone(&hit);
    let prev = PaginationPrevious(PaginationPreviousProps {
        id: "pg-prev".into(),
        text: Some("上一页".into()),
        hide_text: Some(false),
        disabled: Some(false),
        on_click: Some(Handler::new(move |_| hit_p.set(hit_p.get() + 1))),
        children: None,
        base: None,
    });
    assert_eq!(
        prev.attrs.get(ATTR_SLOT).map(String::as_str),
        Some(SLOT_PREV)
    );
    assert!(prev.on_click.is_some(), "previous must expose on_click");
    prev.on_click.as_ref().unwrap().borrow_mut()();
    assert_eq!(hit.get(), 1);

    let hit_n = Rc::clone(&hit);
    let next = PaginationNext(PaginationNextProps {
        id: "pg-next".into(),
        text: Some("下一页".into()),
        hide_text: Some(false),
        disabled: Some(false),
        on_click: Some(Handler::new(move |_| hit_n.set(hit_n.get() + 10))),
        children: None,
        base: None,
    });
    assert!(next.on_click.is_some(), "next must expose on_click");
    next.on_click.as_ref().unwrap().borrow_mut()();
    assert_eq!(hit.get(), 11);
}

#[test]
fn previous_disabled_suppresses_on_click() {
    let prev = PaginationPrevious(PaginationPreviousProps {
        id: "pg-prev-dis".into(),
        text: Some("Prev".into()),
        hide_text: Some(false),
        disabled: Some(true),
        on_click: Some(Handler::new(move |_| panic!("disabled previous fired"))),
        children: None,
        base: None,
    });
    // Disabled buttons still expose a no-op interactive click that ignores the prop.
    if let Some(cb) = &prev.on_click {
        cb.borrow_mut()();
    }
}

#[test]
fn item_keeps_previous_button_shell_with_icon_and_caption() {
    let prev = PaginationPrevious(PaginationPreviousProps {
        id: "pg-prev-keep".into(),
        text: Some("上一页".into()),
        hide_text: Some(false),
        disabled: Some(false),
        on_click: Some(Handler::new(|_| {})),
        children: None,
        base: None,
    });
    assert!(prev.children.len() >= 2, "prev has icon + caption");
    let item = PaginationItem(PaginationItemProps {
        children: Some(prev),
        base: None,
    });
    fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
        if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(slot) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_slot(c, slot))
    }
    let kept = find_slot(&item, SLOT_PREV).expect("item must keep previous button shell");
    assert!(
        kept.on_click.is_some(),
        "unpacking icon+caption must not drop on_click"
    );
}

#[test]
#[cfg(feature = "i18n")]
fn default_navigation_and_jumper_labels_follow_locale_and_overlays() {
    crate::i18n::with_lock(|| {
        let (_handle, ()) = crate::core::reactivity::create_root(|| {
            for (locale, previous, next, go) in [
                ("en-US", "Previous", "Next", "Go"),
                ("zh-CN", "上一页", "下一页", "前往"),
            ] {
                crate::i18n::set_locale(locale);
                let defaults = pagination("pg-locale");
                assert_eq!(defaults.previous_text, previous);
                assert_eq!(defaults.next_text, next);
                assert_eq!(defaults.jumper_button_text, go);
                let prev = PaginationPrevious(PaginationPreviousProps {
                    id: "pg-locale-prev".into(),
                    ..Default::default()
                });
                let next_el = PaginationNext(PaginationNextProps {
                    id: "pg-locale-next".into(),
                    ..Default::default()
                });
                assert!(
                    crate::core::tree::find_in_tree(&prev, |el| el.text.as_deref()
                        == Some(previous))
                    .is_some()
                );
                assert!(
                    crate::core::tree::find_in_tree(&next_el, |el| el.text.as_deref()
                        == Some(next))
                    .is_some()
                );
            }
            crate::i18n::add_toml(
                "zh-CN",
                "[jadix.pagination]\nprevious = '往前'\nnext = '往后'\ngo = '跳转'",
            )
            .unwrap();
            let defaults = pagination("pg-overlay");
            assert_eq!(defaults.previous_text, "往前");
            assert_eq!(defaults.next_text, "往后");
            assert_eq!(defaults.jumper_button_text, "跳转");
            let custom = PaginationPrevious(PaginationPreviousProps {
                text: Some("Custom".into()),
                ..Default::default()
            });
            assert!(
                crate::core::tree::find_in_tree(&custom, |el| el.text.as_deref() == Some("Custom"))
                    .is_some()
            );
        });
    });
}

#[test]
fn navigation_hover_does_not_restart_when_page_changes() {
    use crate::core::reactivity::create_root;
    use crate::style::sync_class_variants_subtree;
    use crate::style::{
        set_css_transition_max_step_ms, set_css_transition_timeline_ms,
        set_css_transition_virtual_timeline,
    };

    set_css_transition_virtual_timeline(true);
    set_css_transition_max_step_ms(Some(1000.0));
    let (_handle, ()) = create_root(|| {
        let page = use_signal("pagination_hover_page", || 5usize);
        let paint = || {
            paint_builder_content(
                "pg-hover",
                page.peek(),
                12,
                PaginationRangeOpts::new(1, 1, 1),
                "上一页",
                "下一页",
                false,
                None,
                page,
                None,
                &PaginationPartChrome::default(),
            )
        };
        for slot in [SLOT_PREV, SLOT_NEXT] {
            set_css_transition_timeline_ms(0.0);
            let mut old = paint();
            let mut tree = LayoutTree::new();
            tree.build_and_layout(&mut old, 800.0, 120.0, [-1.0, -1.0]);
            let find = |root: &Element| {
                crate::core::tree::find_in_tree(root, |el| {
                    el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(slot)
                })
                .expect("navigation button")
                .layout_bounds
            };
            let [x, y, w, h] = find(&old);
            let mouse = [x + w * 0.5, y + h * 0.5];
            let mut impact = LayoutStatus::Clean;
            sync_class_variants_subtree(
                &mut old,
                [-1.0, -1.0],
                "",
                false,
                800.0,
                120.0,
                &mut tree,
                &mut impact,
            );
            let button = crate::core::tree::find_in_tree(&old, |el| {
                el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(slot)
            })
            .unwrap();
            button.on_hover.as_ref().unwrap().borrow_mut()(true);
            sync_class_variants_subtree(
                &mut old,
                mouse,
                "",
                false,
                800.0,
                120.0,
                &mut tree,
                &mut impact,
            );
            set_css_transition_timeline_ms(200.0);
            sync_class_variants_subtree(
                &mut old,
                mouse,
                "",
                false,
                800.0,
                120.0,
                &mut tree,
                &mut impact,
            );
            let button = crate::core::tree::find_in_tree(&old, |el| {
                el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(slot)
            })
            .unwrap();
            let hovered_bg = button.style.bg_color;
            assert!(hovered_bg[3] > 0.9, "hover must settle before clicking");
            button.on_click.as_ref().unwrap().borrow_mut()();
            let mut next = paint();
            let status = crate::app::test_support::reconcile_elements(&mut next, &mut old);
            assert_eq!(
                status,
                LayoutStatus::StructureChanged,
                "paging replaces page identities and must exercise the rebuild path"
            );
            tree.build_and_layout(&mut next, 800.0, 120.0, mouse);
            crate::app::tree_hit::sync_pointer_chrome_after_layout(
                &mut next, &mut tree, mouse, 800.0, 120.0,
            );
            let button = crate::core::tree::find_in_tree(&next, |el| {
                el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(slot)
            })
            .unwrap();
            assert_eq!(
                button.style.bg_color, hovered_bg,
                "{slot}: unchanged hover must survive the first frame after paging"
            );
        }
    });
    set_css_transition_max_step_ms(None);
    set_css_transition_virtual_timeline(false);
}

#[test]
fn builder_content_wires_prev_next_and_updates_page() {
    let page = use_signal("pagination_builder_nav_page", || 5usize);
    let el = paint_builder_content(
        "pg-nav",
        5,
        12,
        PaginationRangeOpts::new(1, 1, 1),
        "上一页",
        "下一页",
        false,
        None,
        page,
        None,
        &PaginationPartChrome::default(),
    );
    assert_eq!(
        el.attrs.get(ATTR_SLOT).map(String::as_str),
        Some(SLOT_CONTENT)
    );

    fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
        if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(slot) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_slot(c, slot))
    }
    let prev = find_slot(&el, SLOT_PREV).expect("prev");
    let next = find_slot(&el, SLOT_NEXT).expect("next");
    assert!(prev.on_click.is_some());
    assert!(next.on_click.is_some());
    prev.on_click.as_ref().unwrap().borrow_mut()();
    assert_eq!(page.peek(), 4);
    next.on_click.as_ref().unwrap().borrow_mut()();
    assert_eq!(page.peek(), 5);
}

#[test]
fn builder_content_wires_page_link_clicks() {
    let page = use_signal("pagination_builder_click_page", || 2usize);
    let el = paint_builder_content(
        "pg-links",
        2,
        5,
        PaginationRangeOpts::new(1, 1, 1),
        "Prev",
        "Next",
        false,
        None,
        page,
        None,
        &PaginationPartChrome::default(),
    );
    assert_eq!(
        el.attrs.get(ATTR_SLOT).map(String::as_str),
        Some(SLOT_CONTENT)
    );
    let mut found_click = false;
    fn walk(el: &Element, found: &mut bool) {
        if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_LINK) && el.on_click.is_some() {
            *found = true;
        }
        for c in &el.children {
            walk(c, found);
        }
    }
    walk(&el, &mut found_click);
    assert!(found_click, "builder page links must wire on_click");
}
