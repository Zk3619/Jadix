#![cfg(not(feature = "gpu"))]
use jadix::{authoring::native::ElementProjection, portable::*, presentation::*};
use jadix_components::portable::{text_view, TextDocument};

fn nodes(root: &PresentationNode) -> Vec<&PresentationNode> {
    let mut nodes = vec![root];
    let mut index = 0;
    while index < nodes.len() {
        nodes.extend(&nodes[index].children);
        index += 1;
    }
    nodes
}

#[test]
fn large_document_bounds_mounts_scrolls_to_distant_lines_and_replaces_source() {
    let mut source = (0..200_000).map(|i| format!("line {i}\r\n")).collect::<String>();
    source.push_str(&"界".repeat(1000));
    let (_owner, (mut view, document)) = create_root(|| {
        let document = create_signal(TextDocument::new(source));
        (text_view("source").document_signal(document).line_numbers(true)
            .viewport_h(220.).row_height(22.).build(), document)
    });
    let projection = ElementProjection::new(RootId::new());
    let first = projection.snapshot(&mut view).unwrap();
    assert!(nodes(&first).len() < 150);
    assert!(nodes(&first).iter().any(|n| matches!(&n.descriptor.content, NodeContent::Text(value) if value == "line 0")));
    assert!(first.children[0].descriptor.style.min_width >= 42_000.);
    projection.dispatch(&mut view, &PlatformEvent { node: first.id,
        payload: EventPayload::ScrollViewport { x: 0., y: 199_990. * 22., width: 300., height: 220. }
    }).unwrap();
    let distant = projection.snapshot(&mut view).unwrap();
    assert!(nodes(&distant).len() < 150);
    assert!(nodes(&distant).iter().any(|n| matches!(&n.descriptor.content, NodeContent::Text(value) if value == "line 199990")));
    document.set(TextDocument::new("替换👩‍💻\r\n末行"));
    let replaced = projection.snapshot(&mut view).unwrap();
    let text: Vec<_> = nodes(&replaced).iter().filter_map(|n| match &n.descriptor.content {
        NodeContent::Text(value) => Some(value.as_str()), _ => None,
    }).collect();
    assert!(text.contains(&"替换👩‍💻") && text.contains(&"末行"));
    assert!(!text.iter().any(|value| value.contains('\r')));
    assert!(nodes(&replaced).iter().filter(|n| matches!(n.descriptor.content, NodeContent::Text(_)))
        .any(|n| n.descriptor.style.text.as_ref().is_some_and(|style| style.selectable == Some(true))));
}

#[test]
fn long_utf8_lines_split_at_boundaries_without_losing_text() {
    let source = "界🙂".repeat(200_000);
    let (_owner, mut view) = create_root(|| text_view("long").document(source.clone()).selectable(false).build());
    let projection = ElementProjection::new(RootId::new());
    let tree = projection.snapshot(&mut view).unwrap();
    let leaves: Vec<_> = nodes(&tree).into_iter().filter_map(|node| match &node.descriptor.content {
        NodeContent::Text(text) => Some((node, text)), _ => None,
    }).collect();
    assert!(leaves.len() > 1 && leaves.len() < 5);
    assert_eq!(leaves.iter().map(|(_, text)| text.as_str()).collect::<String>(), source);
    for (node, text) in leaves {
        assert!(text.len() <= 1024 * 1024);
        assert_eq!(node.descriptor.style.text.as_ref().unwrap().selectable, Some(false));
    }
}
