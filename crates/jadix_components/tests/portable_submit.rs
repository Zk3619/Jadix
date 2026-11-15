#![cfg(all(not(feature = "gpu"), feature = "macros"))]
use jadix_components::{authoring::native::ElementProjection, prelude::*};
use jadix_native::presentation::*;
use std::{cell::RefCell, rc::Rc};

#[test]
fn portable_rsx_submit_reads_committed_unicode_and_rejects_replay_and_composition() {
    let submissions = Rc::new(RefCell::new(Vec::new()));
    let output = submissions.clone();
    let (_owner, (mut element, value)) = create_root(|| {
        let value = create_signal(String::new());
        let element = ui! { <Input id="submit" value={value} onSubmit={move || output.borrow_mut().push(value.get())} /> };
        (element, value)
    });
    let projection = ElementProjection::new(RootId::new());
    let node = projection.snapshot(&mut element).unwrap().id;
    let edit = |revision, composing| PlatformEvent {
        node,
        payload: EventPayload::Edit {
            value: "中😀".into(),
            revision,
            selection_utf16: [3, 3],
            composing_utf16: composing,
        },
    };
    let submit = |revision| PlatformEvent {
        node,
        payload: EventPayload::Submit { revision },
    };
    projection
        .dispatch(&mut element, &edit(1, Some([0, 3])))
        .unwrap();
    assert!(projection.dispatch(&mut element, &submit(2)).is_err());
    assert!(value.get().is_empty());
    projection.dispatch(&mut element, &edit(2, None)).unwrap();
    projection.dispatch(&mut element, &submit(3)).unwrap();
    assert_eq!(*submissions.borrow(), ["中😀"]);
    assert!(projection.dispatch(&mut element, &submit(3)).is_err());
    let snapshot = projection.snapshot(&mut element).unwrap();
    assert!(matches!(
        snapshot.descriptor.content,
        NodeContent::Input { revision: 3, .. }
    ));
    projection.dispose();
    assert!(projection.dispatch(&mut element, &submit(4)).is_err());
}

#[test]
fn portable_input_composition_callback_keeps_preedit_out_of_committed_value() {
    let changes = Rc::new(RefCell::new(Vec::new()));
    let output = changes.clone();
    let (_owner, (mut element, value)) = create_root(|| {
        let value = create_signal("seed".to_string());
        let element = ui! {
            <Input id="compose" value={value}
                onCompositionChange={move |value| output.borrow_mut().push(value)} />
        };
        (element, value)
    });
    let projection = ElementProjection::new(RootId::new());
    let snapshot = projection.snapshot(&mut element).unwrap();
    let node = snapshot.id;
    let first_revision = match snapshot.descriptor.content {
        NodeContent::Input { revision, .. } => revision + 1,
        _ => panic!("expected input"),
    };
    projection
        .dispatch(
            &mut element,
            &PlatformEvent {
                node,
                payload: EventPayload::Edit {
                    value: "seed拼".into(),
                    revision: first_revision,
                    selection_utf16: [5, 5],
                    composing_utf16: Some([4, 5]),
                },
            },
        )
        .unwrap();
    assert_eq!(value.get(), "seed");
    assert_eq!(*changes.borrow(), [Some("seed拼".into())]);
    projection
        .dispatch(
            &mut element,
            &PlatformEvent {
                node,
                payload: EventPayload::Edit {
                    value: "seed拼音".into(),
                    revision: first_revision + 1,
                    selection_utf16: [6, 6],
                    composing_utf16: None,
                },
            },
        )
        .unwrap();
    assert_eq!(value.get(), "seed拼音");
    assert_eq!(*changes.borrow(), [Some("seed拼".into()), None]);
}
