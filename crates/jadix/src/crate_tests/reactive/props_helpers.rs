//! B2：`merge_props` / `split_props!` / `children()`.
#![allow(non_snake_case)]

use crate::app::props::{children, merge_props, resolve_children};
use crate::core::reactivity::{create_root, create_signal};
use crate::core::tree::Element;
use jadix_macros::component;

#[component(crate_path = "crate")]
fn MergeProbe(id: String, title: String, class: String, count: i32) -> Element {
    let _ = (id, class, count);
    Element::new().text(title)
}

#[test]
fn merge_props_later_some_wins_none_does_not_clobber() {
    let defaults = MergeProbeProps {
        id: "keep".into(),
        title: Some("Untitled".into()),
        class: Some("p-4".into()),
        count: None,
        children: None,
        base: None,
    };
    let incoming = MergeProbeProps {
        id: String::new(),
        title: Some("Hello".into()),
        class: None,
        count: Some(3),
        children: Some(Element::new().text("kid")),
        base: None,
    };
    let merged = merge_props(defaults, incoming);
    assert_eq!(merged.id, "keep");
    assert_eq!(merged.title.as_deref(), Some("Hello"));
    assert_eq!(merged.class.as_deref(), Some("p-4"));
    assert_eq!(merged.count, Some(3));
    assert_eq!(
        merged.children.as_ref().and_then(|c| c.text.as_deref()),
        Some("kid")
    );
}

#[test]
fn merge_props_macro_folds_three_sources() {
    let a = MergeProbeProps {
        title: Some("a".into()),
        class: Some("one".into()),
        ..Default::default()
    };
    let b = MergeProbeProps {
        title: Some("b".into()),
        ..Default::default()
    };
    let c = MergeProbeProps {
        count: Some(9),
        ..Default::default()
    };
    let merged = crate::merge_props!(a, b, c);
    assert_eq!(merged.title.as_deref(), Some("b"));
    assert_eq!(merged.class.as_deref(), Some("one"));
    assert_eq!(merged.count, Some(9));
}

#[test]
fn split_props_takes_fields_and_leaves_rest() {
    let props = MergeProbeProps {
        id: "x".into(),
        title: Some("Hello".into()),
        class: Some("p-4".into()),
        count: Some(1),
        children: None,
        base: None,
    };
    let ((title, class), rest) = crate::split_props!(props, title, class);
    assert_eq!(title.as_deref(), Some("Hello"));
    assert_eq!(class.as_deref(), Some("p-4"));
    assert!(rest.title.is_none());
    assert!(rest.class.is_none());
    assert_eq!(rest.id, "x");
    assert_eq!(rest.count, Some(1));
}

#[test]
fn resolve_children_and_pack_roundtrip() {
    let mut frag = Element::new();
    frag.set_attr("jadix_fragment", "1");
    let frag = frag
        .child(Element::new().text("a"))
        .child(Element::new().text("b"));
    let resolved = resolve_children(Some(frag));
    assert_eq!(resolved.len(), 2);
    let packed = resolved.into_element().expect("frag");
    let again = resolve_children(Some(packed));
    assert_eq!(again.len(), 2);
}

#[test]
fn children_accessor_tracks_signal() {
    create_root(|| {
        let n = create_signal(1_i32);
        let acc = children(move || Some(Element::new().text(format!("n={}", n.get()))));
        assert_eq!(acc.len(), 1);
        assert_eq!(
            acc.to_element().and_then(|e| e.text.clone()),
            Some("n=1".to_string())
        );
        n.set(2);
        assert_eq!(
            acc.to_element().and_then(|e| e.text.clone()),
            Some("n=2".to_string())
        );
    });
}

#[test]
fn wrapper_merges_defaults_then_forwards_rest() {
    let defaults = MergeProbeProps {
        class: Some("card".into()),
        title: Some("Untitled".into()),
        ..Default::default()
    };
    let incoming = MergeProbeProps {
        title: Some("Hello".into()),
        children: Some(Element::new().text("body")),
        ..Default::default()
    };
    let props = merge_props(defaults, incoming);
    let ((title,), rest) = crate::split_props!(props, title);
    assert_eq!(title.as_deref(), Some("Hello"));
    assert_eq!(rest.class.as_deref(), Some("card"));
    let kids = resolve_children(rest.children);
    assert_eq!(kids.len(), 1);
    assert_eq!(kids.as_slice()[0].text.as_deref(), Some("body"));
}
