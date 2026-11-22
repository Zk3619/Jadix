#![cfg(all(windows, feature = "windows"))]

use jadix_core::{hosted::HostedContent, web_evaluation::{WebEvaluation, WebEvaluationError}, web_resource::ResourceResponse};
use jadix_native::{host::NativeRoot, presentation::*, windows::WindowsHost};
use std::{future::Future, pin::Pin, task::{Context, Poll, Waker}, time::{Duration, Instant}};

fn tick(root: &mut NativeRoot<WindowsHost>) {
    root.host().pump();
    for batch in root.pump_web() { batch.wake(); }
    for work in root.take_resource_work() { work.run(); }
    root.poll_events().unwrap();
    std::thread::sleep(Duration::from_millis(10));
}

fn answer(root: &mut NativeRoot<WindowsHost>, mut evaluation: WebEvaluation) -> Result<serde_json::Value, WebEvaluationError> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        tick(root);
        if let Poll::Ready(value) = Pin::new(&mut evaluation).poll(&mut Context::from_waker(Waker::noop())) { return value; }
        assert!(Instant::now() < deadline, "WebView2 evaluation did not settle");
    }
}

#[test]
fn webview2_async_resources_promises_navigation_and_unmount_cancellation() {
    let mut host = WindowsHost::new("Jadix WebView2 desktop acceptance", [640, 400]).unwrap();
    let (pending_tx, pending_rx) = std::sync::mpsc::channel();
    host.register_web_resources("desktop", move |request, reply| {
        if request.url.ends_with("/pending") {
            pending_tx.send(reply).unwrap();
        } else {
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(30));
                let html = request.url.ends_with("/index.html");
                reply.complete(Ok(ResourceResponse { status: 200, mime: if html { "text/html; charset=utf-8" } else { "application/json" }.into(), bytes: if html { "<!doctype html><meta charset=utf-8><title>原生网页 😀</title><body>WebView2</body>".as_bytes().to_vec() } else { br#"{"value":42}"#.to_vec() } }));
            });
        }
    }).unwrap();
    let mut root = NativeRoot::new(host);
    let id = NodeId::new(root.model().id());
    let spec = HostedContent::Web { url: Some("app://desktop/index.html".into()), html: None, base_url: None, origins: vec![] };
    let mut nodes = vec![PresentationNode { id, descriptor: NodeDescriptor {
        content: NodeContent::Hosted { specification: spec.to_json().unwrap() }, frame: Rect::default(), style: Default::default(), accessible_role: None, accessible_selected: None, accessible_expanded: None, interaction: Default::default(), keyboard: Default::default(), navigation_keys: vec![], accessibility: Default::default(), accessibility_relations: Default::default(), accessible_description: None, accessible_name: None,
    }, children: vec![] }];
    root.present(&mut nodes, root.host().viewport(), root.host().environment()).unwrap();
    root.host().show();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        tick(&mut root);
        let evaluation = root.evaluate_web(id, "document.title");
        match answer(&mut root, evaluation) {
            Err(WebEvaluationError::NotReady) => assert!(Instant::now() < deadline, "WebView2 did not load"),
            result => { assert_eq!(result.unwrap(), "原生网页 😀"); break; }
        }
    }
    let evaluation = root.evaluate_web(id, "fetch('/answer').then(r => r.json()).then(v => ({answer:v.value, text:'中文 😀'}))");
    assert_eq!(answer(&mut root, evaluation).unwrap(), serde_json::json!({"answer":42,"text":"中文 😀"}));
    let evaluation = root.evaluate_web(id, "Promise.reject(new Error('desktop rejection'))");
    assert!(matches!(answer(&mut root, evaluation), Err(WebEvaluationError::Exception(message)) if message.contains("desktop rejection")));
    let evaluation = root.evaluate_web(id, "location.href='https://example.invalid/denied'; true");
    assert_eq!(answer(&mut root, evaluation).unwrap(), true);
    let evaluation = root.evaluate_web(id, "document.title");
    assert_eq!(answer(&mut root, evaluation).unwrap(), "原生网页 😀");
    let mut evaluation = root.evaluate_web(id, "fetch('/pending').then(r => r.text())");
    let pending = loop {
        tick(&mut root);
        if let Ok(reply) = pending_rx.try_recv() { break reply; }
        assert!(Instant::now() < deadline, "resource provider did not receive request");
    };
    assert!(pending.is_active());
    root.present(&mut [], root.host().viewport(), root.host().environment()).unwrap();
    tick(&mut root);
    assert!(!pending.is_active());
    assert!(matches!(Pin::new(&mut evaluation).poll(&mut Context::from_waker(Waker::noop())), Poll::Ready(Err(WebEvaluationError::Aborted))));
    let evaluation = root.evaluate_web(id, "1");
    assert_eq!(answer(&mut root, evaluation), Err(WebEvaluationError::Unavailable));
}
