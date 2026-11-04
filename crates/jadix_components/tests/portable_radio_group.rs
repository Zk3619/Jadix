#![cfg(all(not(feature = "gpu"), feature = "macros"))]

use jadix::{authoring::native::ElementProjection, presentation::*};
use jadix_components::prelude::*;
use jadix_native::HeadlessRoot;

fn radios(node: &PresentationNode) -> Vec<&PresentationNode> {
    let mut result = Vec::new();
    if matches!(
        node.descriptor.accessible_role,
        Some(AccessibleRole::RadioItem { .. })
    ) {
        result.push(node);
    }
    for child in &node.children {
        result.extend(radios(child));
    }
    result
}

#[test]
fn compound_radio_navigation_wraps_skips_disabled_and_retains_native_identity() {
    let (_owner, (mut element, selected)) = create_root(|| {
        let selected = create_signal("a".to_string());
        let element = ui! {
            <RadioGroup id="delivery" value={selected}>
                <RadioGroupItem value="a">"Standard"</RadioGroupItem>
                <RadioGroupItem value="blocked" disabled={true}>"Unavailable"</RadioGroupItem>
                <RadioGroupItem value="b">"Express"</RadioGroupItem>
            </RadioGroup>
        };
        (element, selected)
    });
    let model = HeadlessRoot::new();
    let projection = ElementProjection::with_lifetime(model.lifetime());
    let first = projection.snapshot(&mut element).unwrap();
    assert_eq!(
        first.descriptor.accessible_role,
        Some(AccessibleRole::RadioGroup)
    );
    let entries = radios(&first);
    assert_eq!(entries.len(), 3);
    let identities: Vec<_> = entries.iter().map(|entry| entry.id).collect();
    assert_eq!(entries[0].descriptor.accessible_selected, Some(true));
    assert_eq!(
        entries[0].descriptor.accessible_role,
        Some(AccessibleRole::RadioItem { tab_stop: true })
    );
    assert_eq!(projection.navigation_nodes(&element).len(), 2);
    for (index, key, expected, focus_index) in [
        (0, NavigationKey::Right, "b", 2),
        (2, NavigationKey::Down, "a", 0),
        (0, NavigationKey::Left, "b", 2),
        (2, NavigationKey::Home, "a", 0),
        (0, NavigationKey::End, "b", 2),
    ] {
        projection
            .dispatch(
                &mut element,
                &PlatformEvent {
                    node: identities[index],
                    payload: EventPayload::Navigate {
                        key,
                        shift: false,
                        control: false,
                    },
                },
            )
            .unwrap();
        assert_eq!(selected.get(), expected);
        let current = projection.snapshot(&mut element).unwrap();
        assert_eq!(
            projection.take_focus_request(&element),
            Some(identities[focus_index])
        );
        let entries = radios(&current);
        assert_eq!(
            entries.iter().map(|entry| entry.id).collect::<Vec<_>>(),
            identities
        );
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.descriptor.accessible_role
                    == Some(AccessibleRole::RadioItem { tab_stop: true }))
                .count(),
            1
        );
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.descriptor.accessible_selected == Some(true))
                .count(),
            1
        );
    }
    assert!(
        projection
            .dispatch(
                &mut element,
                &PlatformEvent {
                    node: identities[1],
                    payload: EventPayload::Activate,
                },
            )
            .is_err()
    );
    assert_eq!(selected.get(), "b");
}

#[test]
fn unselected_controlled_radio_keeps_external_state_until_activation() {
    let (_owner, (mut element, selected)) = create_root(|| {
        let selected = create_signal(String::new());
        let element: Element = radio_group("optional")
            .value_signal(selected)
            .options([
                radio_option("blocked", "Blocked").disabled(true),
                radio_option("a", "Available"),
            ])
            .into();
        (element, selected)
    });
    let model = HeadlessRoot::new();
    let projection = ElementProjection::with_lifetime(model.lifetime());
    let first = projection.snapshot(&mut element).unwrap();
    assert_eq!(selected.get(), "");
    let entries = radios(&first);
    assert!(
        entries
            .iter()
            .all(|entry| entry.descriptor.accessible_selected == Some(false))
    );
    assert_eq!(
        entries[1].descriptor.accessible_role,
        Some(AccessibleRole::RadioItem { tab_stop: true })
    );
    projection
        .dispatch(
            &mut element,
            &PlatformEvent {
                node: entries[1].id,
                payload: EventPayload::Activate,
            },
        )
        .unwrap();
    assert_eq!(selected.get(), "a");
}

#[test]
fn duplicate_radio_values_fail_projection_instead_of_aliasing_identity() {
    let (_owner, mut element) = create_root(|| {
        radio_group("duplicate")
            .options([("same", "First"), ("same", "Second")])
            .into()
    });
    let model = HeadlessRoot::new();
    let projection = ElementProjection::with_lifetime(model.lifetime());
    assert!(projection.snapshot(&mut element).is_err());
}
