use jadix_components::prelude::*;
use std::{cell::RefCell, rc::Rc};

#[test]
fn rsx_window_round_trips_native_state_and_host_closure_without_echo_requests() {
    let requests = Rc::new(RefCell::new(Vec::new()));
    let captured = requests.clone();
    let windows = Rc::new(RefCell::new(WindowRequests::default()));
    windows.borrow_mut().set_request_handler(move |request| {
        let mut requests = captured.borrow_mut();
        requests.push(request.clone());
        Ok(requests.len() as u64)
    });
    let service = WindowRequests::declarations(&windows);
    let (owner, (_element, open, attributes, state)) = create_root(|| {
        provide_window_declarations(service);
        let open = create_signal(true);
        let attributes = create_signal(WindowAttributes {
            mode: Some(WindowMode::Normal),
            visible: Some(true),
            position: Some(WindowPosition { x: 0.0, y: 0.0 }),
            content_size: Some(WindowSize {
                width: 400,
                height: 300,
            }),
            ..Default::default()
        });
        let state = create_signal(None);
        let element =
            ui! { <Window id="editor" open={open} attributes={attributes} state={state} /> };
        (element, open, attributes, state)
    });
    assert!(
        matches!(&requests.borrow()[0], WindowRequest::OpenDeclared { id, .. } if id == "editor")
    );
    batch(|| windows.borrow_mut().complete(1, Ok(())).unwrap());
    let observed = WindowState {
        id: Some("editor".into()),
        frame: WindowFrame {
            x: 30.0,
            y: 40.0,
            width: 800.0,
            height: 600.0,
        },
        content_width: 800.0,
        content_height: 578.0,
        backing_scale: 2.0,
        mode: WindowMode::Normal,
        key: true,
        visible: true,
        occluded: false,
        normal_frame: None,
        display: None,
        title_bar_insets: Default::default(),
    };
    batch(|| {
        windows
            .borrow_mut()
            .child_state("editor", Some(observed.clone()))
    });
    assert_eq!(state.get(), Some(observed));
    assert_eq!(
        attributes.get().position,
        Some(WindowPosition { x: 30.0, y: 40.0 })
    );
    assert_eq!(
        attributes.get().content_size,
        Some(WindowSize {
            width: 800,
            height: 578
        })
    );
    assert_eq!(
        requests.borrow().len(),
        1,
        "OS observations must not echo an update"
    );
    batch(|| windows.borrow_mut().child_destroyed("editor"));
    assert!(!open.get());
    assert_eq!(state.get(), None);
    assert_eq!(
        requests.borrow().len(),
        1,
        "host closure must not issue another close"
    );
    open.set(true);
    assert_eq!(requests.borrow().len(), 2);
    batch(|| windows.borrow_mut().complete(2, Ok(())).unwrap());
    drop(owner);
    assert!(matches!(&requests.borrow()[2], WindowRequest::CloseDeclared { id } if id == "editor"));
}
