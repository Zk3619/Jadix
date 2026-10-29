use super::*;

#[test]
fn builder_page_with_style() {
    let el: Element = breadcrumb()
        .link("Home", || ())
        .page_with(|p| p.style(|b| b.text_size(18.0)).text("Here"))
        .into();
    fn find_size(el: &Element, want: f32) -> bool {
        (el.text_style.size_resolved() - want).abs() < 0.01
            || el.children.iter().any(|c| find_size(c, want))
    }
    assert!(find_size(&el, 18.0));
}

#[test]
fn breadcrumb_root_slot_and_aria() {
    let el = Breadcrumb(BreadcrumbProps {
        id: String::new(),
        children: None,
        base: None,
        base_builder: None,
        ..Default::default()
    });
    assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
    assert_eq!(
        el.attrs.get("aria-label").map(String::as_str),
        Some("breadcrumb")
    );
    assert!(el.template_id.is_none());
}

#[test]
fn breadcrumb_page_aria_current() {
    let el = BreadcrumbPage(BreadcrumbPageProps {
        children: Some(Element::new().text("Current")),
        base: None,
        ..Default::default()
    });
    assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_PAGE));
    assert_eq!(
        el.attrs.get("aria-current").map(String::as_str),
        Some("page")
    );
}

#[test]
fn breadcrumb_link_hover_applies_foreground_and_underline() {
    use crate::components::button::sync_button_hover_paint_subtree;
    use crate::core::reactivity::create_root;
    use crate::core::tree::LayoutStatus;
    use crate::layout::layout_tree::LayoutTree;
    use crate::style::sync_class_variants_subtree;

    let muted = parse_text_color("muted-foreground");
    let fg = theme_fg("foreground");
    let (_h, mut el) = create_root(|| {
        build_breadcrumb_link("bc-hover-home", "Home", None, None, Handler::default())
    });
    el.layout_bounds = [0.0, 0.0, 80.0, 24.0];
    for c in &mut el.children {
        if c.text.is_some() {
            c.layout_bounds = [0.0, 0.0, 80.0, 24.0];
        }
    }
    let rest = el
        .children
        .iter()
        .find(|c| c.text.as_deref() == Some("Home"))
        .expect("caption");
    assert_eq!(rest.text_style.color_resolved(), muted);
    assert_ne!(rest.text_style.underline, Some(true));

    if let Some(hover) = el.on_hover.clone() {
        hover.borrow_mut()(true);
    }
    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_button_hover_paint_subtree(&mut el, &mut tree, &mut impact, 800.0, 600.0, [0.0, 0.0]);
    sync_class_variants_subtree(
        &mut el,
        [40.0, 12.0],
        "",
        false,
        800.0,
        600.0,
        &mut tree,
        &mut impact,
    );
    let hover = el
        .children
        .iter()
        .find(|c| c.text.as_deref() == Some("Home"))
        .expect("caption");
    assert_eq!(
        hover.text_style.color_resolved(),
        fg,
        "non-current BreadcrumbLink hover must lift muted to foreground"
    );
    assert_eq!(
        hover.text_style.underline,
        Some(true),
        "non-current BreadcrumbLink hover:underline must reach the caption"
    );
}

#[test]
fn breadcrumb_link_inline_button() {
    let el = build_breadcrumb_link("", "Home", None, None, Handler::default());
    assert_eq!(
        el.focus_id.as_deref(),
        Some("breadcrumb-link:Home"),
        "empty id must use stable breadcrumb focus_id, not auto btn-N"
    );
    assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_LINK));
    assert!(el
        .authoring_state
        .get::<crate::components::button::ButtonHoverPaint>()
        .is_some());
    assert_eq!(
        el.authoring_state
            .get::<crate::components::button::ButtonHoverPaint>()
            .map(|p| p.variant.as_str()),
        Some("link")
    );
    use crate::core::tree::PointerEvents;
    let caption = el
        .children
        .iter()
        .find(|c| c.text.is_some())
        .expect("caption");
    assert_eq!(caption.pointer_events_mode(), PointerEvents::None);
    assert_eq!(
        el.text_style.underline_offset,
        Some(2.0),
        "author underline-offset-[2px] on the Button root"
    );
    assert!(
        caption.text_style.underline_offset.is_none(),
        "caption inherits underline-offset from the Button root"
    );
}

#[test]
fn breadcrumb_page_uses_foreground() {
    let el = BreadcrumbPage(BreadcrumbPageProps {
        children: Some(Element::new().text("Current")),
        base: None,
        ..Default::default()
    });
    let text = el
        .children
        .iter()
        .find(|c| c.text.is_some())
        .expect("caption");
    assert_eq!(text.text_style.color_resolved(), theme_fg("foreground"));
    assert_eq!(text.text_style.size_resolved(), BREADCRUMB_TEXT_SIZE_PX);
    assert_eq!(text.text_style.weight_resolved(), BREADCRUMB_TEXT_WEIGHT);
}

#[test]
fn breadcrumb_link_and_page_typography_match() {
    let link = build_breadcrumb_link("", "Settings", None, None, Handler::default());
    let page = BreadcrumbPage(BreadcrumbPageProps {
        children: Some(Element::new().text("Profile")),
        base: None,
        ..Default::default()
    });
    let link_text = link
        .children
        .iter()
        .find(|c| c.text.is_some())
        .expect("link caption");
    let page_text = page
        .children
        .iter()
        .find(|c| c.text.is_some())
        .expect("page caption");
    assert_eq!(
        link_text.text_style.size_resolved(),
        page_text.text_style.size_resolved()
    );
    assert_eq!(
        link_text.text_style.weight_resolved(),
        page_text.text_style.weight_resolved()
    );
    assert_eq!(
        link_text.text_style.line_height_scale,
        page_text.text_style.line_height_scale
    );
    assert_ne!(
        link_text.text_style.line_height_scale,
        Some(1.0),
        "breadcrumb must not use leading-none (keep readable line box with Page)"
    );
}

#[test]
fn list_strips_trailing_separator() {
    let mut list_kids = vec![
        BreadcrumbItem(BreadcrumbItemProps {
            children: Some(BreadcrumbPage(BreadcrumbPageProps {
                children: Some(Element::new().text("Last")),
                base: None,
                ..Default::default()
            })),
            base: None,
            ..Default::default()
        }),
        BreadcrumbSeparator(BreadcrumbSeparatorProps {
            children: None,
            base: None,
            ..Default::default()
        }),
    ];
    list_kids = strip_trailing_separators(list_kids);
    assert_eq!(list_kids.len(), 1);
    assert_eq!(
        list_kids[0].attrs.get(ATTR_SLOT).map(String::as_str),
        Some(SLOT_ITEM)
    );
}

#[test]
fn builder_no_trailing_separator() {
    let el: Element = breadcrumb()
        .link("Home", || {})
        .link("Docs", || {})
        .page("Breadcrumb")
        .into();
    let list = el
        .children
        .iter()
        .find(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_LIST))
        .expect("list");
    assert_ne!(
        list.children
            .last()
            .and_then(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str())),
        Some(SLOT_SEPARATOR),
        "last trail segment must not be a separator"
    );
    let seps = list
        .children
        .iter()
        .filter(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_SEPARATOR))
        .count();
    assert_eq!(seps, 2, "Home / Docs / Breadcrumb → two separators");
}

#[test]
fn builder_assembles_trail() {
    let el: Element = breadcrumb()
        .link("Home", || {})
        .link("Docs", || {})
        .page("Breadcrumb")
        .into();
    assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
    let list = el
        .children
        .iter()
        .find(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_LIST))
        .or_else(|| el.children.first())
        .expect("list");
    assert!(
        list.children
            .iter()
            .any(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_SEPARATOR)),
        "builder inserts separators between segments"
    );
}

fn collect_link_focus_ids(el: &Element) -> Vec<String> {
    let mut ids = Vec::new();
    fn walk(el: &Element, ids: &mut Vec<String>) {
        if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_LINK) {
            if let Some(id) = el.focus_id.clone() {
                ids.push(id);
            }
        }
        for c in &el.children {
            walk(c, ids);
        }
    }
    walk(el, &mut ids);
    ids
}

fn hover_first_link_caption(el: &mut Element, label: &str) -> bool {
    if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_LINK)
        && el.children.iter().any(|c| c.text.as_deref() == Some(label))
    {
        if let Some(hover) = el.on_hover.clone() {
            hover.borrow_mut()(true);
        }
        return true;
    }
    el.children
        .iter_mut()
        .any(|c| hover_first_link_caption(c, label))
}

#[test]
fn builder_link_ids_are_stable() {
    fn trail() -> Element {
        breadcrumb()
            .link("Home", || {})
            .link("Docs", || {})
            .page("Breadcrumb")
            .into()
    }
    let ids = collect_link_focus_ids(&trail());
    assert_eq!(ids.len(), 2);
    assert_ne!(
        ids[0], ids[1],
        "segments in one trail must not share hover id"
    );
    assert!(ids[0].contains("Home"));
    assert!(ids[1].contains("Docs"));
    assert_eq!(
        collect_link_focus_ids(&trail()),
        ids,
        "same call site must keep focus_id (hover signal must survive rebuild)"
    );
}

#[test]
fn two_builder_homes_do_not_share_hover() {
    use crate::core::reactivity::{create_root, use_signal};
    use crate::primitives::r#box::vstack;

    // Gallery 1e: Builder Home and Ellipsis Home are both part_idx 0 "Home".
    let (_h, mut root) = create_root(|| {
        Element::from(
            vstack()
                .child(breadcrumb().link("Home", || {}).page("Breadcrumb"))
                .child(
                    breadcrumb()
                        .link("Home", || {})
                        .ellipsis()
                        .link("Settings", || {})
                        .page("Profile"),
                ),
        )
    });

    let ids = collect_link_focus_ids(&root);
    let homes: Vec<_> = ids.into_iter().filter(|id| id.contains("Home")).collect();
    assert_eq!(homes.len(), 2, "Builder Home + Ellipsis Home");
    assert_ne!(
        homes[0], homes[1],
        "two trails' first Home must not share breadcrumb-link:Home:0"
    );

    assert!(hover_first_link_caption(&mut root, "Home"), "builder Home");
    let hovered = use_signal(&format!("{}_hover", homes[0]), || false);
    let other = use_signal(&format!("{}_hover", homes[1]), || false);
    assert_eq!(hovered.try_peek(), Some(true), "hovered Home signal");
    assert_ne!(
        other.try_peek(),
        Some(true),
        "the other trail's Home must keep its own {{focus_id}}_hover"
    );
}

#[test]
fn dual_surface_compiles_shape() {
    let _b: Element = breadcrumb().page("Only").into();
    let el = BreadcrumbList(BreadcrumbListProps {
        children: None,
        base: None,
        ..Default::default()
    });
    let _: Element = el;
}
