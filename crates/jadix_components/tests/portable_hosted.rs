#![cfg(all(not(feature = "gpu"), feature = "macros"))]

use jadix::{authoring::native::{ElementProjection, ProjectionError}, presentation::*};
use jadix_components::prelude::*;
use jadix_native::HeadlessRoot;
use std::{cell::RefCell, rc::Rc};

#[test]
fn hosted_controls_validate_events_and_preserve_identity() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let capture = events.clone();
    let mut element = segmented_control("mode", vec![
        HostedSegment { id: "a".into(), label: "A".into(), enabled: true, symbol: None, accessibility_label: None },
        HostedSegment { id: "b".into(), label: "B".into(), enabled: false, symbol: None, accessibility_label: None },
    ]).on_event(move |event, data| capture.borrow_mut().push((event.to_owned(), data.clone())))
        .build().unwrap();
    let mut model = HeadlessRoot::new();
    let projection = ElementProjection::with_lifetime(model.lifetime());
    let first = projection.snapshot(&mut element).unwrap();
    model.reconcile(std::slice::from_ref(&first)).unwrap();
    let event = |id| PlatformEvent { node: first.id,
        payload: EventPayload::Hosted { event: "select".into(), data: serde_json::json!(id) } };
    assert_eq!(projection.dispatch(&mut element, &event("b")), Err(ProjectionError::InvalidEvent));
    projection.dispatch(&mut element, &event("a")).unwrap();
    assert_eq!(&*events.borrow(), &[("select".into(), serde_json::json!("a"))]);
    assert_eq!(projection.snapshot(&mut element).unwrap().id, first.id);
    assert!(web_view("bad", "javascript:alert(1)").build().is_err());
    assert!(segmented_control("bad", Vec::new()).build().is_err());
}

#[cfg(feature = "i18n")]
#[test]
fn native_projection_uses_surface_locale_for_bindings_and_callbacks() {
    use jadix::i18n::{locale, LocaleContext};
    let french = Rc::new(LocaleContext::new());
    french.update_environment("fr-FR", Some("Europe/Paris")).unwrap();
    let japanese = Rc::new(LocaleContext::new());
    japanese.update_environment("ja-JP", Some("Asia/Tokyo")).unwrap();
    let observed = Rc::new(RefCell::new(String::new()));
    let capture = observed.clone();
    let (owner, mut element) = create_root(|| {
        Element::from(button("locale").child(jadix_components::app::reactive::bind_text("locale", || locale().to_string())))
            .on_click(move || *capture.borrow_mut() = locale().to_string())
    });
    let model = HeadlessRoot::new();
    let projection = ElementProjection::with_lifetime(model.lifetime()).with_locale_context(french);
    japanese.with(|| {
        let snapshot = projection.snapshot(&mut element).unwrap();
        // Explicit button content remains a styled subtree, rather than a
        // flattened native caption. Its binding still belongs to this surface.
        assert!(matches!(snapshot.children[0].descriptor.content, NodeContent::Text(ref text) if text == "fr-FR"));
        projection.dispatch(&mut element, &PlatformEvent { node: snapshot.id, payload: EventPayload::Activate }).unwrap();
        assert_eq!(&*observed.borrow(), "fr-FR");
        assert_eq!(locale().to_string(), "ja-JP");
    });
    owner.dispose();
}

#[test]
fn menu_content_is_a_retained_subtree_and_tabs_share_hosted_contract() {
    let mut element = menu_button("actions", "Fallback", vec![])
        .child(Element::new().id("caption").child(jadix_components::app::reactive::bind_text("caption-text", || "Custom".to_owned())))
        .build().unwrap();
    let model = HeadlessRoot::new();
    let projection = ElementProjection::with_lifetime(model.lifetime());
    let snapshot = projection.snapshot(&mut element).unwrap();
    assert_eq!(snapshot.children.len(), 1);
    assert!(snapshot.descriptor.content.accepts_children());
    let mut tabs = tab_bar("tabs", vec![HostedTab {id:"home".into(),label:"Home".into(),enabled:true,symbol:None,accessibility_label:None,badge:Some("1".into())}])
        .selected("home").build().unwrap();
    let snapshot = projection.snapshot(&mut tabs).unwrap();
    assert!(!snapshot.descriptor.content.accepts_children());
    assert!(matches!(snapshot.descriptor.content, NodeContent::Hosted {..}));
}
