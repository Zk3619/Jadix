use super::*;

#[test]
fn shortcut_letters_and_numbers() {
    assert_eq!(
        assign_shortcut(QuestionnaireShortcuts::Letters, 0).as_deref(),
        Some("A")
    );
    assert_eq!(
        assign_shortcut(QuestionnaireShortcuts::Letters, 25).as_deref(),
        Some("Z")
    );
    assert_eq!(assign_shortcut(QuestionnaireShortcuts::Letters, 26), None);
    assert_eq!(
        assign_shortcut(QuestionnaireShortcuts::Numbers, 0).as_deref(),
        Some("1")
    );
    assert_eq!(assign_shortcut(QuestionnaireShortcuts::Numbers, 9), None);
    assert_eq!(assign_shortcut(QuestionnaireShortcuts::None, 0), None);
}

#[test]
fn enabled_names_skip_disabled() {
    let order = vec![
        QuestionnaireItemMeta {
            name: "a".into(),
            required: true,
            disabled: false,
            multiple: false,
        },
        QuestionnaireItemMeta {
            name: "b".into(),
            required: false,
            disabled: true,
            multiple: false,
        },
        QuestionnaireItemMeta {
            name: "c".into(),
            required: true,
            disabled: false,
            multiple: false,
        },
    ];
    assert_eq!(enabled_item_names(&order), vec!["a", "c"]);
    assert_eq!(progress_position(&order, "c"), (2, 2));
    assert!(is_first_enabled(&order, "a"));
    assert!(is_last_enabled(&order, "c"));
    assert_eq!(neighbor_name(&order, "a", 1).as_deref(), Some("c"));
    assert_eq!(neighbor_name(&order, "a", -1), None);
}

#[test]
fn validation_requires_answer_or_skip() {
    assert!(!item_has_valid_answer(
        true,
        QuestionnaireItemStatus::Unanswered,
        &[],
        ""
    ));
    assert!(item_has_valid_answer(
        true,
        QuestionnaireItemStatus::Answered,
        &["x".into()],
        ""
    ));
    assert!(item_has_valid_answer(
        false,
        QuestionnaireItemStatus::Skipped,
        &[],
        ""
    ));
    assert!(!item_has_valid_answer(
        false,
        QuestionnaireItemStatus::Unanswered,
        &[],
        ""
    ));
    assert!(item_has_valid_answer(
        true,
        QuestionnaireItemStatus::Answered,
        &[],
        "typed"
    ));
}

#[test]
fn answers_get_and_get_all() {
    let a = QuestionnaireAnswers {
        current: "signals".into(),
        items: vec![
            QuestionnaireItemAnswer {
                name: "direction".into(),
                status: QuestionnaireItemStatus::Answered,
                values: vec!["tool-calls".into()],
            },
            QuestionnaireItemAnswer {
                name: "signals".into(),
                status: QuestionnaireItemStatus::Answered,
                values: vec!["progress".into(), "risks".into()],
            },
        ],
    };
    assert_eq!(a.get("direction"), Some("tool-calls"));
    assert_eq!(a.get_all("signals"), vec!["progress", "risks"]);
    assert_eq!(a.get("missing"), None);
}

#[test]
fn slot_class_joins_parts_then_local() {
    assert_eq!(merge_slot_class(None, None), None);
    assert_eq!(
        merge_slot_class(Some("gap-8"), None).as_deref(),
        Some("gap-8")
    );
    assert_eq!(merge_slot_class(None, Some("p-2")).as_deref(), Some("p-2"));
    assert_eq!(
        merge_slot_class(Some("gap-8"), Some("p-2")).as_deref(),
        Some("gap-8 p-2")
    );
    assert_eq!(merge_slot_class(Some("  "), Some("")), None);
}

#[test]
fn question_text_keeps_line_width() {
    use crate::core::reactivity::create_root;
    use crate::core::tree::LayoutTree;
    use crate::renderer::font_loader;
    use crate::text::install_shared_font_system;

    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);
    let (_owner, mut root): (_, Element) = create_root(|| {
        let q: Element = questionnaire("q-width")
            .shortcuts(QuestionnaireShortcuts::Letters)
            .question("direction", "What should the agent build next?", |q| {
                q.description("Choose a direction or describe another task.")
                    .choice_with(
                        "tool-calls",
                        "Tool call timeline",
                        "Show what the agent ran and what came back.",
                    )
            })
            .question(
                "signals",
                "What should every progress update include?",
                |q| {
                    q.description("Select all that apply, or skip this question.")
                        .choice("progress", "Progress")
                },
            )
            .into();
        Element::new().w(448.0).flex_col().child(
            Element::new()
                .w("100%")
                .max_w("28rem")
                .min_w(0.0)
                .flex_col()
                .child(q),
        )
    });
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 448.0, 900.0, [-1.0, -1.0]);
    let mut impact = crate::core::tree::LayoutStatus::Clean;
    crate::core::tree::sync_reactive_elements_subtree(
        &mut root,
        &mut tree,
        &mut impact,
        448.0,
        900.0,
        [0.0, 0.0],
    );
    let painted = tree.build_and_layout(&mut root, 448.0, 900.0, [-1.0, -1.0]);
    let column: Vec<_> = painted
        .text_nodes
        .iter()
        .filter(|t| !t.text.is_empty() && (t.width < 1.0 || t.height > 200.0))
        .map(|t| {
            format!(
                "{:?} w={:.1} h={:.1} y={:.1}",
                t.text, t.width, t.height, t.y
            )
        })
        .collect();
    let title = painted
        .text_nodes
        .iter()
        .find(|t| t.text.contains("What should the agent"));
    assert!(
        column.is_empty(),
        "hidden steps must not shape a glyph column:\n{}",
        column.join("\n")
    );
    let title = title.expect("visible title");
    assert!(
        title.width > 120.0 && title.height < 40.0,
        "visible title should stay one line, got w={} h={}",
        title.width,
        title.height
    );
    assert!(
        painted
            .text_nodes
            .iter()
            .all(|t| !t.text.contains("progress update")),
        "inactive question must not be painted"
    );
}

#[test]
fn builder_title_style() {
    use crate::core::reactivity::create_root;
    let (_owner, el): (_, Element) = create_root(|| {
        questionnaire("q-chrome").child(|| {
            questionnaire_item_scope("q1", false, false, false, false, None, || {
                questionnaire_title()
                    .style(|b| b.text_size(20.0))
                    .text("Hello")
                    .into()
            })
        })
    });
    fn find_size(el: &Element, want: f32) -> bool {
        el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
            || (el.text_style.size_resolved() - want).abs() < 0.01
            || el.children.iter().any(|c| find_size(c, want))
    }
    assert!(
        find_size(&el, 20.0),
        "questionnaire_title .style(|b| b.text_size(20)) must land on the title"
    );
}

#[test]
fn builder_slot_and_parts_paint_classes() {
    use crate::core::reactivity::create_root;
    let (_owner, el): (_, Element) = create_root(|| {
        questionnaire("q-slots")
            .slot(SLOT_TITLE, |c| c.class("text-lg"))
            .parts(|p| p.choices("gap-2"))
            .question("n", "Hello", |q| q.choice("a", "A"))
            .into()
    });
    fn slot_has_class(el: &Element, slot: &str, needle: &str) -> bool {
        let is_slot = el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(slot);
        let has = crate::components::base::author_class(el).contains(needle)
            || el
                .class_set
                .as_ref()
                .is_some_and(|cs| cs.base.iter().any(|t| t.contains(needle)));
        (is_slot && has) || el.children.iter().any(|c| slot_has_class(c, slot, needle))
    }
    assert!(
        slot_has_class(&el, SLOT_TITLE, "text-lg"),
        "`.slot(questionnaire-title)` must land on the title"
    );
    assert!(
        slot_has_class(&el, SLOT_CHOICES, "gap-2"),
        "`.parts().choices` must fill SlotMap questionnaire-choices"
    );
}

#[test]
fn action_buttons_do_not_flash_flex_after_class_variant_restore() {
    use crate::core::reactivity::create_root;
    use crate::style::sync_class_variants_subtree;

    fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
        if el.attrs.get("data-slot").map(String::as_str) == Some(slot) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_slot(c, slot))
    }
    fn slot_display(root: &Element, slot: &str) -> Display {
        find_slot(root, slot)
            .unwrap_or_else(|| panic!("missing slot {slot}"))
            .style
            .display
    }

    let (_owner, mut root): (_, Element) = create_root(|| {
        questionnaire("q-action-flash")
            .default_item("first")
            .question("first", "First?", |q| q.required(true).choice("a", "A"))
            .question("second", "Second?", |q| q.choice("b", "B"))
            .into()
    });

    assert_eq!(slot_display(&root, SLOT_PREVIOUS), Display::None);
    assert_eq!(slot_display(&root, SLOT_SKIP), Display::None);
    assert_eq!(slot_display(&root, SLOT_NEXT), Display::Flex);
    assert_eq!(slot_display(&root, SLOT_SUBMIT), Display::None);

    let mut layout = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    crate::core::tree::sync_reactive_elements_subtree(
        &mut root,
        &mut layout,
        &mut impact,
        800.0,
        600.0,
        [0.0, 0.0],
    );
    let mut impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut root,
        [-1.0e9, -1.0e9],
        "",
        false,
        800.0,
        600.0,
        &mut layout,
        &mut impact,
    );

    assert_eq!(
        slot_display(&root, SLOT_PREVIOUS),
        Display::None,
        "Previous must stay hidden after class-variant restore"
    );
    assert_eq!(
        slot_display(&root, SLOT_SKIP),
        Display::None,
        "Skip must stay hidden on a required first item after class-variant restore"
    );
    assert_eq!(slot_display(&root, SLOT_NEXT), Display::Flex);
    assert_eq!(
        slot_display(&root, SLOT_SUBMIT),
        Display::None,
        "Submit must stay hidden while not on the last item after class-variant restore"
    );
}
