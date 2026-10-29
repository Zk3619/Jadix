use super::*;
use crate::core::reactivity::create_root;

#[test]
fn orientation_parse_falls_back_to_horizontal() {
    assert_eq!(
        StepperOrientation::parse("vertical"),
        StepperOrientation::Vertical
    );
    assert_eq!(
        StepperOrientation::parse("column"),
        StepperOrientation::Vertical
    );
    assert_eq!(
        StepperOrientation::parse("nope"),
        StepperOrientation::Horizontal
    );
}

#[test]
fn step_from_str_and_pair() {
    let a = StepperStep::from("Account");
    assert_eq!(a.title, "Account");
    assert!(a.description.is_empty());
    let b = StepperStep::from(("Profile", "Details"));
    assert_eq!(b.title, "Profile");
    assert_eq!(b.description, "Details");
}

#[test]
fn empty_id_does_not_write_empty_template() {
    let (_owner, el): (_, Element) = create_root(|| stepper().steps(["A", "B"]).into());
    let tid = el.template_id.as_deref().unwrap_or("");
    assert_ne!(tid, "");
}

#[test]
fn builder_sugar_sets_instance_template() {
    let (_owner, el): (_, Element) = create_root(|| {
        stepper()
            .id("st-sugar")
            .steps(["Account", "Profile", "Confirm"])
            .default_step(1)
            .into()
    });
    assert_eq!(el.template_id.as_deref(), Some("st-sugar__stepper"));
}

#[test]
fn dual_surface_compiles_shape() {
    let (_owner, _): (_, Element) = create_root(|| {
        let sig = use_signal("st-dual", || 0usize);
        let _b: Element = stepper()
            .current_signal(sig)
            .orientation("vertical")
            .steps([("A", "one"), ("B", "two")])
            .into();
        Stepper(StepperProps {
            id: "st-rsx".into(),
            current: Some(sig),
            current_signal: None,
            default_step: Some(0),
            orientation: Some("horizontal".into()),
            clickable: Some(true),
            children: None,
            base: None,
        })
    });
}

#[test]
fn step_state_table() {
    assert_eq!(StepState::from_index(0, 1), StepState::Completed);
    assert_eq!(StepState::from_index(1, 1), StepState::Active);
    assert_eq!(StepState::from_index(2, 1), StepState::Inactive);
}

/// Clicking a step rebuilds the island; titles must stay one line so the last
/// letter cannot drop onto the description (glyphon wrap when the box is 1px
/// short of intrinsic, or when weight 400→500 grows inside a stale width).
#[test]
fn titles_stay_single_line_when_current_changes() {
    use crate::app::reactive::reset_registered_anchors;
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::core::reactivity::{create_effect, create_root};
    use crate::layout::layout_tree::LayoutTree;
    use crate::renderer::font_loader;
    use crate::text::{install_shared_font_system, intrinsic_width};
    use std::cell::RefCell;
    use std::rc::Rc;

    reset_registered_anchors();
    clear_test_reconcile_root();
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let tree: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&tree));

    let (handle, current) = create_root(|| {
        let current = use_signal("st-title-wrap", || 0usize);
        let tree_e = Rc::clone(&tree);
        let cur = current;
        create_effect(move || {
            let el: Element = stepper()
                .id("st-title-wrap")
                .current_signal(cur)
                .steps([
                    ("Account", "Email and password"),
                    ("Profile", "Your details"),
                    ("Confirm", "Review and submit"),
                ])
                .into();
            *tree_e.borrow_mut() = el;
        });
        current
    });

    fn assert_titles(tree: &mut Element, label: &str) {
        let mut layout = LayoutTree::new();
        let result = layout.build_and_layout(tree, 360.0, 240.0, [-1.0, -1.0]);
        for title in ["Account", "Profile", "Confirm"] {
            let tn = result
                .text_nodes
                .iter()
                .find(|n| n.text == title)
                .unwrap_or_else(|| panic!("{label}: missing title {title:?}"));
            let prepared = tn.prepared.as_ref().expect("prepared");
            let intrinsic = intrinsic_width(prepared);
            let metrics = crate::text::layout(prepared, tn.width.max(1.0));
            assert_eq!(
                metrics.line_count, 1,
                "{label}: {title:?} wrapped into {} lines box_w={:.3} intrinsic={:.3}",
                metrics.line_count, tn.width, intrinsic
            );
            assert!(
                tn.width + 0.05 >= intrinsic,
                "{label}: {title:?} box_w {:.3} < intrinsic {:.3}",
                tn.width,
                intrinsic
            );
            assert_eq!(
                tn.style.white_space,
                crate::style::WhiteSpace::Nowrap,
                "{label}: {title:?} must nowrap so a short box cannot drop the last letter"
            );
            let size = tn.style.size_resolved();
            assert!(
                tn.height < size * 1.85,
                "{label}: {title:?} height {:.3} looks like a second line (size={size})",
                tn.height
            );
        }
        let acc = result
            .text_nodes
            .iter()
            .find(|n| n.text == "Account")
            .expect("Account");
        let desc = result
            .text_nodes
            .iter()
            .find(|n| n.text == "Email and password")
            .expect("description");
        assert!(
                acc.y + acc.height <= desc.y + 1.0,
                "{label}: Account last-line mixes with description acc=({:.1},{:.1}+{:.1}) desc.y={:.1}",
                acc.x,
                acc.y,
                acc.height,
                desc.y
            );
    }

    {
        let mut t = tree.borrow_mut();
        assert_titles(&mut t, "current=0");
    }
    current.set(1);
    {
        let mut t = tree.borrow_mut();
        assert_titles(&mut t, "current=1");
    }
    current.set(2);
    {
        let mut t = tree.borrow_mut();
        assert_titles(&mut t, "current=2");
    }

    handle.dispose();
    clear_test_reconcile_root();
    reset_registered_anchors();
}

#[test]
fn builder_item_class_reaches_painted_item() {
    let (_owner, el) = create_root(|| {
        let class = stepper_item()
            .title("Profile")
            .description("Your details")
            .class("ring-2")
            .chrome
            .class;
        let current = use_signal("st-item-class", || 0usize);
        let spec = StepperItemSpec {
            index: 0,
            title: "Profile".into(),
            description: "Your details".into(),
            disabled: false,
            chrome: SlotChrome::default().class(class),
            ..Default::default()
        };
        paint_item(
            &spec,
            current,
            StepState::Active,
            StepperOrientation::Horizontal,
            true,
        )
    });
    assert!(
        el.class_set
            .as_ref()
            .is_some_and(|cs| cs.base.iter().any(|t| t.contains("ring-2")))
            || el
                .attrs
                .get("jadix_utility_class")
                .is_some_and(|c| c.contains("ring-2"))
    );
}

#[test]
fn builder_steps_item_chrome_applies_to_each_item() {
    let (_owner, ok) = create_root(|| {
        let class = SlotChrome::default().class("step-highlight").class;
        let current = use_signal("st-steps-chrome", || 0usize);
        [0usize, 1usize]
            .into_iter()
            .zip(["A", "B"])
            .all(|(index, title)| {
                let spec = StepperItemSpec {
                    index,
                    title: title.into(),
                    description: String::new(),
                    disabled: false,
                    chrome: SlotChrome::default().class(class.clone()),
                    ..Default::default()
                };
                let el = paint_item(
                    &spec,
                    current,
                    StepState::Inactive,
                    StepperOrientation::Horizontal,
                    true,
                );
                el.class_set
                    .as_ref()
                    .is_some_and(|cs| cs.base.iter().any(|t| t.contains("step-highlight")))
                    || el
                        .attrs
                        .get("jadix_utility_class")
                        .is_some_and(|c| c.contains("step-highlight"))
            })
    });
    assert!(ok, "each step item should carry item_chrome class");
}

#[test]
fn builder_steps_sugar_chrome_reaches_item() {
    let (_owner, ok) = create_root(|| {
        let s = stepper_step("A").class("ring-2");
        let chrome = SlotChrome::default().overlay(s.chrome);
        let current = use_signal("st-sugar-chrome", || 0usize);
        let spec = StepperItemSpec {
            index: 0,
            title: s.title,
            description: s.description,
            disabled: false,
            chrome,
            ..Default::default()
        };
        let el = paint_item(
            &spec,
            current,
            StepState::Inactive,
            StepperOrientation::Horizontal,
            true,
        );
        el.class_set
            .as_ref()
            .is_some_and(|cs| cs.base.iter().any(|t| t.contains("ring-2")))
            || el
                .attrs
                .get("jadix_utility_class")
                .is_some_and(|c| c.contains("ring-2"))
    });
    assert!(
        ok,
        "stepper_step(\"A\").class must land on the painted item"
    );
}

#[test]
fn builder_title_style() {
    let (_owner, el): (_, Element) = create_root(|| {
        let title: Element = stepper_title()
            .style(|b| b.text_size(20.0))
            .text("Account")
            .into();
        let title_slot = SlotAuthor::from_el(&title);
        assert_eq!(title_slot.size, Some(20.0));
        let current = use_signal("st-title-style", || 0usize);
        let spec = StepperItemSpec {
            index: 0,
            title: "Account".into(),
            title_slot,
            ..Default::default()
        };
        paint_item(
            &spec,
            current,
            StepState::Active,
            StepperOrientation::Horizontal,
            true,
        )
    });
    fn find_size(el: &Element, want: f32) -> bool {
        el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
            || (el.text_style.size_resolved() - want).abs() < 0.01
            || el.children.iter().any(|c| find_size(c, want))
    }
    assert!(
        find_size(&el, 20.0),
        "stepper_title .style(|b| b.text_size(20)) must land on the painted title"
    );
}

#[test]
fn builder_title_color_padding_survive_paint() {
    const RED: [u8; 4] = [255, 0, 0, 255];
    let (_owner, el): (_, Element) = create_root(|| {
        let title: Element = stepper_title()
            .style(|b| b.text_size(20.0).text_color(RED).p(8.0))
            .text("Account")
            .into();
        let title_slot = SlotAuthor::from_el(&title);
        assert_eq!(title_slot.size, Some(20.0));
        assert_eq!(title_slot.color, Some(RED));
        assert!(title_slot.padding.is_some());
        let current = use_signal("st-title-chrome", || 0usize);
        let spec = StepperItemSpec {
            index: 0,
            title: "Account".into(),
            title_slot,
            ..Default::default()
        };
        paint_item(
            &spec,
            current,
            StepState::Active,
            StepperOrientation::Horizontal,
            true,
        )
    });
    fn find_color(el: &Element, want: [u8; 4]) -> bool {
        el.text_style.color == Some(want) || el.children.iter().any(|c| find_color(c, want))
    }
    fn find_pad(el: &Element, want: f32) -> bool {
        crate::layout::lp_length_px(el.style.padding.left).is_some_and(|v| (v - want).abs() < 0.01)
            || el.children.iter().any(|c| find_pad(c, want))
    }
    assert!(
        find_color(&el, RED),
        "title text_color must survive paint_*"
    );
    assert!(find_pad(&el, 8.0), "title padding must survive paint_*");
}
