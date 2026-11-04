use jadix::app::startup::{build_root, StartupHandle};
use jadix::core::tree::Element;
use jadix_splash::{StartupConfig, StartupController, StartupPhase};
use std::{cell::RefCell, rc::Rc};

#[test]
fn standalone_provider_drives_runtime_frames_and_retry() {
    let controllers = Rc::new(RefCell::new(Vec::<StartupController>::new()));
    let attempts = controllers.clone();
    let startup: StartupHandle =
        StartupConfig::component(|snapshot, _| Element::new().text(snapshot.stage))
            .prepare(move |controller| attempts.borrow_mut().push(controller))
            .into();
    startup.borrow_mut().attach();
    startup.borrow_mut().start();
    let first_epoch = startup.borrow().epoch();
    let root = build_root(Some(startup.as_ref()), || panic!("content before ready"));
    let (stamp, content) = startup.borrow().frame(&root).unwrap();
    assert!(!content);
    assert!(startup.borrow_mut().presented(stamp, content));
    assert!(!startup.borrow().content_presented());
    controllers.borrow()[0].cancel();
    assert!(startup.borrow_mut().retry(first_epoch));
    startup.borrow_mut().start();
    assert_ne!(startup.borrow().epoch(), first_epoch);
    assert!(!startup.borrow_mut().retry(first_epoch));
    controllers.borrow()[0].ready();
    assert!(!startup.borrow().content_ready());
    controllers.borrow()[1].ready();
    let root = build_root(Some(startup.as_ref()), || Element::new().text("content"));
    let (stamp, content) = startup.borrow().frame(&root).unwrap();
    assert!(content);
    assert!(startup.borrow_mut().presented(stamp, content));
    assert!(startup.borrow().content_presented());
    startup.borrow_mut().detach();
    assert!(!startup.borrow_mut().presented(stamp, content));
}

#[test]
fn shutdown_cancels_preparation_through_runtime_contract() {
    let controller = Rc::new(RefCell::new(None));
    let captured = controller.clone();
    let startup: StartupHandle = StartupConfig::system()
        .prepare(move |value| *captured.borrow_mut() = Some(value))
        .into();
    startup.borrow_mut().start();
    startup.borrow().cancel();
    let captured = controller.borrow();
    let controller: &StartupController = captured.as_ref().unwrap();
    assert_eq!(controller.snapshot().phase, StartupPhase::Cancelled);
    assert!(controller.cancellation().is_cancelled());
}

#[test]
fn application_builder_accepts_external_startup_provider() {
    let _application = jadix::app::app().startup(StartupConfig::system());
}

#[test]
fn dynamic_business_root_replacement_keeps_runtime_frame_identity() {
    let startup: StartupHandle = StartupConfig::system()
        .prepare(|controller| controller.ready())
        .into();
    startup.borrow_mut().attach();
    startup.borrow_mut().start();
    let mut root = build_root(Some(startup.as_ref()), || {
        let mut navigation = Element::new().text("first page");
        navigation.template_id = Some("navigation-host".into());
        navigation
    });
    assert!(root.template_id.is_none());
    assert_eq!(
        root.children[0].template_id.as_deref(),
        Some("navigation-host")
    );
    root.children[0] = Element::new().text("replaced page");
    let (stamp, content) = startup.borrow().frame(&root).unwrap();
    assert!(content);
    assert!(startup.borrow_mut().presented(stamp, content));
    assert!(startup.borrow().content_presented());
}

#[test]
fn unconfigured_runtime_does_not_wrap_business_root() {
    let mut content = Element::new().text("plain");
    content.template_id = Some("business-root".into());
    let root = build_root(None, || content);
    assert_eq!(root.template_id.as_deref(), Some("business-root"));
    assert!(root.get_attr("data-startup-epoch").is_none());
}
