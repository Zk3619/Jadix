#![cfg(feature = "gpu")]
pub use jadix_components::{app, core, EventPhase};
use jadix_components::{core::tree::Element, MouseEvent};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[path = "../src/components/title_bar_drag.rs"]
mod drag;

#[test]
fn title_region_click_drag_and_double_click_keep_distinct_lifecycles() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let recorded = calls.clone();
    let region = drag::drag_region_handlers(
        Element::new().focus_id("title"),
        [4.0, 6.0],
        move |double| {
            recorded.borrow_mut().push(double);
        },
    );
    let mut event = MouseEvent::new(10.0, 10.0, winit::event::MouseButton::Left, 1);
    event.buttons = 1;
    let down = region.on_mousedown.as_ref().unwrap();
    let motion = region.on_mousemove.as_ref().unwrap();
    let up = region.on_mouseup.as_ref().unwrap();
    event.phase = EventPhase::Capture;
    (down.borrow_mut())(&event);
    assert_eq!(app::pointer_capture_id(), None);
    event.phase = EventPhase::Target;
    (down.borrow_mut())(&event);
    assert_eq!(app::pointer_capture_id().as_deref(), Some("title"));
    event.screen_x = 13.0;
    event.screen_y = 15.0;
    (motion.borrow_mut())(&event);
    assert!(calls.borrow().is_empty());
    (up.borrow_mut())(&event);
    assert_eq!(app::pointer_capture_id(), None);
    assert!(
        calls.borrow().is_empty(),
        "ordinary click must not enter the native move loop"
    );

    event.screen_x = 10.0;
    event.screen_y = 10.0;
    (down.borrow_mut())(&event);
    event.screen_y = 16.0;
    (motion.borrow_mut())(&event);
    (motion.borrow_mut())(&event);
    assert_eq!(*calls.borrow(), [false]);
    assert_eq!(app::pointer_capture_id(), None);

    event.detail = 2;
    (down.borrow_mut())(&event);
    assert_eq!(*calls.borrow(), [false, true]);
    event.detail = 3;
    (down.borrow_mut())(&event);
    event.buttons = 0;
    (motion.borrow_mut())(&event);
    assert_eq!(app::pointer_capture_id(), None);
    event.buttons = 1;
    event.screen_x += 100.0;
    (motion.borrow_mut())(&event);
    assert_eq!(
        *calls.borrow(),
        [false, true],
        "released button cancels the pending move"
    );

    let immediate_calls = Rc::new(Cell::new(0));
    let recorded = immediate_calls.clone();
    let immediate = drag::drag_region_handlers(Element::new(), [0.0; 2], move |double| {
        assert!(!double);
        recorded.set(recorded.get() + 1);
    });
    event.detail = 1;
    (immediate.on_mousedown.as_ref().unwrap().borrow_mut())(&event);
    assert_eq!(immediate_calls.get(), 1);
    assert_eq!(app::pointer_capture_id(), None);
}
