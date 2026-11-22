#![cfg(feature = "code")]

use jadix_native::{presentation::*, syntax::*, HeadlessRoot};
use std::{
    sync::mpsc::{channel, Receiver},
    time::{Duration, Instant},
};

fn setup() -> (
    HeadlessRoot,
    Vec<PresentationNode>,
    SyntaxWorker,
    Receiver<()>,
) {
    let mut model = HeadlessRoot::new();
    let nodes = vec![PresentationNode {
        id: NodeId::new(model.id()),
        descriptor: NodeDescriptor {
            content: NodeContent::Textarea {
                max_length: None,
                value: String::new(),
                placeholder: String::new(),
                enabled: true,
                revision: 0,
            },
            frame: Rect::default(),
            style: Default::default(),
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            interaction: Default::default(),
            keyboard: Default::default(),
            navigation_keys: Vec::new(),
            accessibility: Default::default(),
            accessibility_relations: Default::default(),
            accessible_description: None,
            accessible_name: None,
        },
        children: vec![],
    }];
    model.reconcile(&nodes).unwrap();
    let (send, receive) = channel();
    let worker = SyntaxWorker::new(&model, nodes[0].id, move || {
        let _ = send.send(());
    })
    .unwrap();
    (model, nodes, worker, receive)
}

fn update(model: &mut HeadlessRoot, nodes: &mut [PresentationNode], source: &str, revision: u64) {
    nodes[0].descriptor.content = NodeContent::Textarea {
        max_length: None,
        value: source.into(),
        placeholder: String::new(),
        enabled: true,
        revision,
    };
    model.reconcile(nodes).unwrap();
}

fn wait(worker: &mut SyntaxWorker, model: &HeadlessRoot, receive: &Receiver<()>) -> SyntaxResult {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(result) = worker.poll(model).unwrap() {
            return result;
        }
        receive
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
    }
}

#[test]
fn syntax_uses_existing_snapshot_identity_and_preserves_unicode_source_bytes() {
    let (mut model, mut nodes, mut worker, receive) = setup();
    let source = "/* 中文\r\n comment */\r\nfn main() { let text = \"😀\"; }\r\n";
    update(&mut model, &mut nodes, source, 1);
    let document = TextDocument::new(source);
    let request = worker
        .request(&model, document.clone(), "rs", false)
        .unwrap();
    assert_eq!(
        worker
            .request(&model, document.clone(), "rs", false)
            .unwrap(),
        request
    );
    let result = wait(&mut worker, &model, &receive);
    assert_eq!(result.request, request);
    assert_eq!(result.document_revision, document.revision());
    assert_eq!(
        result
            .spans
            .iter()
            .map(|span| span.text.as_str())
            .collect::<String>(),
        source
    );
    assert!(result
        .spans
        .windows(2)
        .any(|spans| spans[0].color != spans[1].color));
    assert!(worker.poll(&model).unwrap().is_none());
    assert_eq!(
        receive.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    );
}

#[test]
fn only_latest_request_can_publish_and_edit_revision_mismatch_discards_old_analysis() {
    let (mut model, mut nodes, mut worker, receive) = setup();
    for revision in 1..=100 {
        let source = format!("fn value_{revision}() {{ println!(\"中文😀\"); }}");
        update(&mut model, &mut nodes, &source, revision);
        worker
            .request(&model, TextDocument::new(source), "rs", true)
            .unwrap();
    }
    let result = wait(&mut worker, &model, &receive);
    assert_eq!(result.request, 100);
    assert!(result
        .spans
        .iter()
        .map(|span| span.text.as_str())
        .collect::<String>()
        .contains("value_100"));
    worker
        .request(&model, TextDocument::new("stale"), "rs", false)
        .unwrap();
    loop {
        receive.recv_timeout(Duration::from_secs(5)).unwrap();
        if worker.poll(&model).unwrap().is_none() {
            let source = "fn newest() {}";
            update(&mut model, &mut nodes, source, 101);
            worker
                .request(&model, TextDocument::new(source), "rs", false)
                .unwrap();
            assert!(wait(&mut worker, &model, &receive)
                .spans
                .iter()
                .any(|span| span.text.contains("newest")));
            break;
        }
    }
}

#[test]
fn cancellation_node_removal_foreign_roots_and_owner_disposal_reject_late_results() {
    let (mut model, nodes, mut worker, receive) = setup();
    worker
        .request(&model, TextDocument::new("fn cancelled() {}"), "rs", true)
        .unwrap();
    worker.cancel();
    assert!(worker.poll(&model).unwrap().is_none());
    assert!(matches!(
        worker.request(&HeadlessRoot::new(), TextDocument::new(""), "rs", false),
        Err(SyntaxError::Node)
    ));
    model.reconcile(&[]).unwrap();
    assert!(matches!(
        worker.request(&model, TextDocument::new(""), "rs", false),
        Err(SyntaxError::Node)
    ));
    assert!(matches!(worker.poll(&model), Err(SyntaxError::Disposed)));
    let (model, _, mut worker, receive2) = setup();
    model.lifetime().dispose();
    assert!(matches!(
        worker.request(&model, TextDocument::new(""), "rs", false),
        Err(SyntaxError::Disposed)
    ));
    drop(nodes);
    drop(receive);
    drop(receive2);
}

#[test]
fn document_line_language_and_output_budgets_do_not_expand_native_nodes() {
    let (mut model, mut nodes, mut worker, receive) = setup();
    for source in ["a".repeat(64 * 1024 + 1), "\n".repeat(2 * 1024 * 1024 + 1)] {
        assert!(matches!(
            worker.request(&model, TextDocument::new(source), "rs", false),
            Err(SyntaxError::Limits)
        ));
    }
    assert!(matches!(
        worker.request(&model, TextDocument::new(""), "../rs", false),
        Err(SyntaxError::Limits)
    ));
    let source = "let value = 10;\n".repeat(1000);
    update(&mut model, &mut nodes, &source, 1);
    worker
        .request(&model, TextDocument::new(source.clone()), "rs", false)
        .unwrap();
    let result = wait(&mut worker, &model, &receive);
    assert!(result.plain_fallback);
    assert_eq!(result.spans.len(), 1);
    assert_eq!(result.spans[0].text, source);
    assert_eq!(model.roots().len(), 1);
}

#[test]
fn visible_syntax_keeps_large_documents_colored_without_expanding_native_nodes() {
    let (mut model, mut nodes, mut worker, receive) = setup();
    let source = "let value = \"中文🙂\";\r\n".repeat(10_000);
    update(&mut model, &mut nodes, &source, 1);
    worker
        .request_visible(
            &model,
            TextDocument::new(source.clone()),
            "rs",
            false,
            0..20,
        )
        .unwrap();
    let result = wait(&mut worker, &model, &receive);
    assert!(!result.plain_fallback);
    assert!(result.spans.iter().any(|span| span.color.is_some()));
    assert_eq!(
        result
            .spans
            .iter()
            .map(|span| span.text.as_str())
            .collect::<String>(),
        source
    );
    assert!(result.spans.len() < 2048);
    assert_eq!(model.roots().len(), 1);
}
