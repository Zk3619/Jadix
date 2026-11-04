use jadix_splash::prelude::*;

#[test]
fn builder_and_rsx_share_the_same_controlled_splash() {
    let snapshot = StartupSnapshot {
        epoch: 42,
        phase: StartupPhase::Preparing,
        stage: "Loading fonts".into(),
        progress: Some((1, 4)),
    };
    let builder: Element = splash()
        .title("Application")
        .description("Starting")
        .state(snapshot.clone())
        .child(text("Offline assets"))
        .into();
    let rsx = ui! {
        <Splash title="Application" description="Starting" state={snapshot}>
            <Text>"Offline assets"</Text>
        </Splash>
    };
    assert_eq!(builder.get_attr("data-slot"), rsx.get_attr("data-slot"));
    assert_eq!(builder.get_attr("role").map(String::as_str), Some("status"));
    fn texts(element: &Element) -> Vec<String> {
        let mut result = Vec::new();
        jadix::core::tree::walk_elements(element, |child| {
            if let Some(text) = &child.text {
                result.push(text.clone());
            }
        });
        result
    }
    assert_eq!(texts(&builder), texts(&rsx));
    assert!(texts(&builder).contains(&"25%".into()));
}

#[test]
fn progress_is_bounded_and_zero_total_is_indeterminate() {
    for (completed, total, expected) in [
        (u64::MAX, u64::MAX, Some("100")),
        (8, 4, Some("100")),
        (0, 0, None),
    ] {
        let root: Element = splash()
            .state(StartupSnapshot {
                epoch: 1,
                phase: StartupPhase::Preparing,
                stage: "Loading".into(),
                progress: Some((completed, total)),
            })
            .into();
        let mut values = Vec::new();
        jadix::core::tree::walk_elements(&root, |element| {
            if let Some(value) = element.get_attr("aria-valuenow") {
                values.push(value.clone());
            }
        });
        assert_eq!(
            values,
            expected.into_iter().map(str::to_owned).collect::<Vec<_>>()
        );
    }
}

#[test]
fn custom_backdrop_is_transparent_decorative_and_below_interactive_content() {
    let builder: Element = splash()
        .title("Application")
        .background("transparent")
        .backdrop(text("Decorative media"))
        .child(text("Foreground"))
        .into();
    let rsx = ui! {
        <Splash title="Application" background="transparent" backdrop={Element::from(text("Decorative media"))}>
            <Text>"Foreground"</Text>
        </Splash>
    };
    for root in [builder, rsx] {
        assert_eq!(
            root.style.bg_color,
            jadix::core::tree::parse_color("transparent")
        );
        assert_eq!(root.children.len(), 2);
        let backdrop = &root.children[0];
        assert_eq!(
            backdrop.get_attr("data-slot").map(String::as_str),
            Some("splash-backdrop")
        );
        assert_eq!(
            backdrop.get_attr("aria-hidden").map(String::as_str),
            Some("true")
        );
        assert!(!jadix::core::tree::pointer_events_targets_self(backdrop));
        assert!(!jadix::core::tree::pointer_events_targets_children(
            backdrop
        ));
        let content = &root.children[1];
        assert_eq!(
            content.get_attr("data-slot").map(String::as_str),
            Some("splash-content")
        );
        assert!(jadix::core::tree::pointer_events_targets_children(content));
    }
}
