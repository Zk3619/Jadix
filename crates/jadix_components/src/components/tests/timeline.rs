use super::*;
use crate::core::reactivity::create_root;

#[test]
fn discs_center_on_complete_copy_including_wrapped_descriptions() {
    for width in [160.0, 320.0] {
        let (_owner, ()) = create_root(|| {
            let specs = [
                TimelineItemSpec {
                    title: "Placed".into(),
                    ..Default::default()
                },
                TimelineItemSpec {
                    index: 1,
                    title: "Packed".into(),
                    description: "Left the warehouse.".into(),
                    ..Default::default()
                },
                TimelineItemSpec {
                    index: 2,
                    title: "Delivered".into(),
                    description: "Your package was left safely at the front door.".into(),
                    ..Default::default()
                },
            ];
            let mut root = Element::new().w(width).h(600.0).child(paint_timeline(
                &specs,
                1,
                DEFAULT_BULLET_PX,
                DEFAULT_LINE_PX,
                None,
            ));
            let mut layout = crate::core::tree::LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, width, 600.0, [-1.0, -1.0]);
            for row in &root.children[0].children {
                let disc = &row.children[0].children[1];
                let copy = &row.children[1];
                let first = copy.children.first().unwrap().layout_bounds;
                let last = copy.children.last().unwrap().layout_bounds;
                let copy_center = (first[1] + last[1] + last[3]) / 2.0;
                let disc_center = disc.layout_bounds[1] + disc.layout_bounds[3] / 2.0;
                assert!(
                    (copy_center - disc_center).abs() < 1.5,
                    "width={width}, copy center={copy_center}, disc center={disc_center}"
                );
            }
        });
    }
}

#[test]
fn compound_icon_defaults_do_not_override_state_color() {
    let (_owner, ()) = create_root(|| {
        provide_context(
            timeline_context(),
            TimelineCtx {
                active: crate::core::reactivity::create_signal(-1),
                bullet_size: DEFAULT_BULLET_PX,
                line_size: DEFAULT_LINE_PX,
            },
        );
        push_collect();
        let bullet = TimelineItemBullet(TimelineItemBulletProps {
            children: Some(build_icon_named("circle-check")),
            base: None,
        });
        TimelineItem(TimelineItemProps {
            title: Some("Delivered".into()),
            description: None,
            bullet: None,
            children: Some(bullet),
            base: None,
        });
        let specs = pop_collect();
        for (active, expected) in [(-1, "border"), (0, "primary")] {
            let row = paint_item(&specs[0], active, true, DEFAULT_BULLET_PX, DEFAULT_LINE_PX);
            let disc = &row.children[0].children[1];
            assert_eq!(
                disc.children[0].text_style.color,
                Some(parse_text_color(expected))
            );
        }
    });
}

#[test]
fn bullet_glyph_px_scales_with_disc() {
    assert!((bullet_glyph_px(20.0) - 13.0).abs() < f32::EPSILON);
    assert!((bullet_glyph_px(8.0) - BULLET_GLYPH_MIN_PX).abs() < f32::EPSILON);
}

#[test]
fn active_tables_match_solid_ui() {
    assert!(!timeline_bullet_active(-1, 0));
    assert!(!timeline_line_active(-1, 0, false));
    assert!(timeline_bullet_active(0, 0));
    assert!(!timeline_bullet_active(0, 1));
    assert!(!timeline_line_active(0, 0, false));
    assert!(timeline_line_active(1, 0, false));
    assert!(timeline_bullet_active(1, 1));
    assert!(!timeline_line_active(2, 2, true));
}

#[test]
fn entry_from_str_and_pair() {
    let a = TimelineEntry::from("Shipped");
    assert_eq!(a.title, "Shipped");
    assert!(a.description.is_empty());
    let b = TimelineEntry::from(("Packed", "Warehouse")).bullet("package");
    assert_eq!(b.title, "Packed");
    assert_eq!(b.description, "Warehouse");
    assert_eq!(b.bullet.as_deref(), Some("package"));
}

#[test]
fn empty_id_does_not_write_empty_template() {
    let (_owner, el): (_, Element) =
        create_root(|| timeline().item("A").item("B").active_item(0).into());
    let tid = el.template_id.as_deref().unwrap_or("");
    assert_ne!(tid, "");
}

#[test]
fn builder_sugar_sets_instance_template() {
    let (_owner, el): (_, Element) = create_root(|| {
        timeline()
            .id("tl-sugar")
            .active_item(1)
            .items(["A", "B", "C"])
            .into()
    });
    assert_eq!(el.template_id.as_deref(), Some("tl-sugar__timeline"));
}

#[test]
fn builder_item_bullet_keeps_template() {
    let (_owner, el): (_, Element) = create_root(|| {
        timeline()
            .id("tl-icon")
            .active_item(2)
            .item(TimelineEntry::from(("Packed", "Warehouse")).bullet("package"))
            .item("Shipped")
            .into()
    });
    assert_eq!(el.template_id.as_deref(), Some("tl-icon__timeline"));
}

#[test]
fn dual_surface_compiles_shape() {
    let (_owner, _): (_, Element) = create_root(|| {
        let sig = use_signal("tl-dual", || 1i32);
        let _b: Element = timeline()
            .active_item_signal(sig)
            .bullet_size(16.0)
            .line_size(2.0)
            .item(("A", "one"))
            .item("B")
            .into();
        Timeline(TimelineProps {
            id: "tl-rsx".into(),
            active: Some(sig),
            active_item: None,
            active_item_signal: None,
            default_active: Some(-1),
            bullet_size: Some(16.0),
            line_size: Some(2.0),
            children: None,
            base: None,
        })
    });
}

#[test]
fn builder_item_class_reaches_painted_item() {
    let class = timeline_item()
        .title("Packed")
        .description("Warehouse")
        .class("border-primary")
        .chrome
        .class;
    let spec = TimelineItemSpec {
        index: 0,
        title: "Packed".into(),
        description: "Warehouse".into(),
        bullet: None,
        chrome: SlotChrome::default().class(class),
        ..Default::default()
    };
    let el = paint_item(&spec, 0, false, DEFAULT_BULLET_PX, DEFAULT_LINE_PX);
    assert!(
        el.class_set
            .as_ref()
            .is_some_and(|cs| cs.base.iter().any(|t| t.contains("border-primary")))
            || el
                .attrs
                .get("jadix_utility_class")
                .is_some_and(|c| c.contains("border-primary"))
    );
}

#[test]
fn builder_items_sugar_chrome_reaches_item() {
    let s = timeline_entry("Packed").class("border-primary");
    let chrome = SlotChrome::default().overlay(s.chrome);
    let spec = TimelineItemSpec {
        index: 0,
        title: s.title,
        description: s.description,
        bullet: s.bullet,
        chrome,
        ..Default::default()
    };
    let el = paint_item(&spec, 0, false, DEFAULT_BULLET_PX, DEFAULT_LINE_PX);
    assert!(
        el.class_set
            .as_ref()
            .is_some_and(|cs| cs.base.iter().any(|t| t.contains("border-primary")))
            || el
                .attrs
                .get("jadix_utility_class")
                .is_some_and(|c| c.contains("border-primary")),
        "timeline_entry(\"Packed\").class must land on the painted item"
    );
}

#[test]
fn builder_title_style() {
    let title: Element = timeline_item_title()
        .style(|b| b.text_size(20.0))
        .text("Packed")
        .into();
    let title_slot = SlotAuthor::from_el(&title);
    assert_eq!(title_slot.size, Some(20.0));
    let spec = TimelineItemSpec {
        index: 0,
        title: "Packed".into(),
        title_slot,
        ..Default::default()
    };
    let el = paint_item(&spec, 0, false, DEFAULT_BULLET_PX, DEFAULT_LINE_PX);
    fn find_size(el: &Element, want: f32) -> bool {
        el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
            || (el.text_style.size_resolved() - want).abs() < 0.01
            || el.children.iter().any(|c| find_size(c, want))
    }
    assert!(
        find_size(&el, 20.0),
        "timeline_item_title .style(|b| b.text_size(20)) must land on the painted title"
    );
}
