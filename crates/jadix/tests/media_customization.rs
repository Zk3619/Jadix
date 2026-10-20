use jadix::{core::reactivity::create_root, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

fn slot<'a>(root: &'a Element, name: &str) -> Option<&'a Element> {
    if root.attrs.get("data-slot").map(String::as_str) == Some(name) {
        return Some(root);
    }
    root.children.iter().find_map(|child| slot(child, name))
}

fn controller() -> MediaController {
    MediaController::new(
        Signal::new(MediaSnapshot {
            state: PlaybackState::Ready,
            duration: Some(Duration::from_secs(100)),
            capabilities: MediaCapabilities {
                seek: true,
                play: true,
                ..Default::default()
            },
            ..Default::default()
        }),
        |_| Ok(()),
    )
}

#[test]
fn slots_are_lazy_ordered_and_keep_unmodified_shadcn_controls() {
    let (_owner, ()) = create_root(|| {
        let calls = Rc::new(Cell::new(0));
        let captured = calls.clone();
        let slots = MediaPlayerSlots::new()
            .replace(MediaPlayerSlot::Header, || {
                panic!("replaced factory must not execute")
            })
            .replace(MediaPlayerSlot::Header, move || {
                captured.set(captured.get() + 1);
                assert_eq!(use_media_player().id, "custom-player");
                Element::new().text("Title")
            })
            .before(MediaPlayerSlot::Header, || Element::new().text("Before"))
            .after(MediaPlayerSlot::Header, || Element::new().text("After"))
            .style(MediaPlayerSlot::Header, |chrome| chrome.class("gap-4"))
            .hide(MediaPlayerSlot::Footer);
        assert_eq!(calls.get(), 0);
        let player: Element = video_player()
            .id("custom-player")
            .controller(controller())
            .slots(slots)
            .into();
        assert_eq!(calls.get(), 1);
        let header = slot(&player, "media-header").unwrap();
        let labels: Vec<_> = header
            .children
            .iter()
            .map(|element| element.text.as_deref().unwrap())
            .collect();
        assert_eq!(labels, ["Before", "Title", "After"]);
        assert!(slot(&player, "media-play").is_some());
        assert!(slot(&player, "media-viewport").is_some());
        assert!(slot(&player, "media-footer").is_none());
        assert!(slot(&player, "media-overlays").is_none());
        assert!(slot(&player, "media-sidebar").is_none());
    });
}

#[test]
fn custom_body_has_precedence_and_hidden_defaults_are_not_instantiated() {
    let (_owner, ()) = create_root(|| {
        let slots =
            MediaPlayerSlots::new().replace(MediaPlayerSlot::Viewport, || panic!("body wins"));
        let player = ui! { <VideoPlayer controller={controller()} slots={slots}>
            <MediaTimeDisplay />
        </VideoPlayer> };
        assert!(slot(&player, "media-time").is_some());
        assert!(slot(&player, "media-viewport").is_none());
        let player: Element = video_player()
            .controller(controller())
            .slots(MediaPlayerSlots::new().hide(MediaPlayerSlot::ControlsStart))
            .into();
        assert!(slot(&player, "media-play").is_none());
        assert!(slot(&player, "media-controls-start").is_none());
        assert!(slot(&player, "media-mute").is_some());
    });
}

#[test]
fn cloned_slots_resolve_the_current_player_and_layers_do_not_capture_the_surface() {
    let (_owner, ()) = create_root(|| {
        let slots = MediaPlayerSlots::new()
            .replace(MediaPlayerSlot::Header, || Element::new().text(use_media_player().id))
            .after(MediaPlayerSlot::Overlay, || {
                jadix::components::base::packed_children(vec![
                    ui! { <MediaLayer><Label>"Captions"</Label></MediaLayer> },
                    ui! { <MediaLayer interactive={true}><Button>"Open transcript"</Button></MediaLayer> },
                ]).unwrap()
            })
            .replace(MediaPlayerSlot::Sidebar, || Element::new().text("Episodes"));
        let first: Element = video_player()
            .id("first")
            .controller(controller())
            .slots(slots.clone())
            .into();
        let second = ui! { <VideoPlayer id="second" controller={controller()} slots={slots} /> };
        assert_eq!(
            slot(&first, "media-header").unwrap().children[0]
                .text
                .as_deref(),
            Some("first")
        );
        assert_eq!(
            slot(&second, "media-header").unwrap().children[0]
                .text
                .as_deref(),
            Some("second")
        );
        let overlay = slot(&first, "media-overlays").unwrap();
        let pointer_mode = |element: &Element| element.attrs.get("jadix_pointer_events").cloned();
        assert_eq!(pointer_mode(overlay).as_deref(), Some("box-none"));
        assert_eq!(pointer_mode(&overlay.children[0]).as_deref(), Some("none"));
        assert_eq!(
            pointer_mode(&overlay.children[1]).as_deref(),
            Some("box-none")
        );
        assert!(slot(&first, "media-sidebar").is_some());
        let interactive = media_player_scope(
            "layer-builder",
            controller(),
            MediaKind::Video,
            false,
            || {
                media_layer()
                    .interactive(true)
                    .child(Element::new().text("Action"))
                    .into()
            },
        );
        assert_eq!(
            pointer_mode(slot(&interactive, "media-layer").unwrap()).as_deref(),
            Some("box-none")
        );
    });
}

#[test]
fn choices_request_without_optimistic_selection_and_guard_latest_session() {
    let (_owner, ()) = create_root(|| {
        let controller = controller();
        let state = Signal::new(MediaChoiceState {
            selected: Some("720p".into()),
            ..MediaChoiceState::new(0)
        });
        let requests = Rc::new(RefCell::new(Vec::new()));
        let recorded = requests.clone();
        let root = media_player_scope(
            "choice",
            controller.clone(),
            MediaKind::Video,
            false,
            || {
                media_choice("1080p", state)
                    .on_select(move |request| recorded.borrow_mut().push(request))
                    .child(Element::new().text("Full HD"))
                    .into()
            },
        );
        let choice = slot(&root, "media-choice").unwrap();
        assert_eq!(choice.attrs["aria-pressed"], "false");
        choice.on_click.as_ref().unwrap().borrow_mut()();
        assert_eq!(state.peek().selected.as_deref(), Some("720p"));
        assert_eq!(
            requests.borrow().as_slice(),
            &[MediaChoiceRequest {
                load_session: 0,
                value: "1080p".into()
            }]
        );
        state.update(|state| state.enabled = false);
        choice.on_click.as_ref().unwrap().borrow_mut()();
        assert_eq!(requests.borrow().len(), 1);
        state.update(|state| state.enabled = true);
        controller
            .load("next", MediaLoadOptions::default())
            .unwrap();
        choice.on_click.as_ref().unwrap().borrow_mut()();
        assert_eq!(requests.borrow().len(), 1);
        state.set(MediaChoiceState::new(1));
        choice.on_click.as_ref().unwrap().borrow_mut()();
        assert_eq!(requests.borrow().last().unwrap().load_session, 1);
    });
}

#[test]
fn episode_sidebar_wraps_without_collapsing_the_video_on_narrow_layouts() {
    let (_owner, ()) = create_root(|| {
        let mut root: Element = vstack()
            .w("240px")
            .child(
                video_player().controller(controller()).slots(
                    MediaPlayerSlots::new()
                        .replace(MediaPlayerSlot::Sidebar, || Element::new().text("Episodes")),
                ),
            )
            .into();
        let mut layout = jadix::core::tree::LayoutTree::new();
        layout.build_and_layout(&mut root, 240.0, 1000.0, [-1.0, -1.0]);
        let viewport = slot(&root, "media-viewport").unwrap().layout_bounds;
        let sidebar = slot(&root, "media-sidebar").unwrap().layout_bounds;
        assert!(viewport[2] > 180.0, "video collapsed: {viewport:?}");
        assert!(
            viewport[3] > 90.0,
            "video lost its aspect ratio: {viewport:?}"
        );
        assert!(
            sidebar[1] >= viewport[1] + viewport[3],
            "sidebar did not wrap: {sidebar:?}"
        );
        assert!(
            sidebar[0] + sidebar[2] <= 241.0,
            "sidebar overflowed: {sidebar:?}"
        );
    });
}

#[test]
fn choices_require_a_sink_and_inherit_player_disabled_state() {
    let (_owner, ()) = create_root(|| {
        let state = Signal::new(MediaChoiceState::new(0));
        let root = media_player_scope("no-sink", controller(), MediaKind::Video, false, || {
            media_choice("auto", state).into()
        });
        assert!(slot(&root, "media-choice")
            .unwrap()
            .attrs
            .contains_key("disabled"));
        let on_select: Rc<dyn Fn(MediaChoiceRequest)> =
            Rc::new(|_| panic!("disabled must not dispatch"));
        let root = media_player_scope(
            "disabled-choice",
            controller(),
            MediaKind::Video,
            true,
            || {
                ui! { <MediaChoice value="auto" state={state} on_select={on_select}>"Auto"</MediaChoice> }
            },
        );
        let choice = slot(&root, "media-choice").unwrap();
        assert!(!choice.focusable);
        choice.on_click.as_ref().unwrap().borrow_mut()();
    });
}

#[test]
fn timeline_preview_is_not_a_seek_and_clears_on_leave_capability_loss_and_load() {
    let (_owner, ()) = create_root(|| {
        let controller = controller();
        let preview = Signal::new(None::<MediaTimelinePreview>);
        let player = ui! { <VideoPlayer controller={controller.clone()}>
            <MediaSeekSlider preview={preview} />
        </VideoPlayer> };
        let slider = slot(&player, "media-seek-slider").unwrap();
        let mut event = jadix::PointerEvent::new(60.0, 4.0, winit::event::MouseButton::Left, 1);
        event.current_bounds = [10.0, 0.0, 100.0, 20.0];
        slider.on_mousemove.as_ref().unwrap().borrow_mut()(&event);
        assert_eq!(preview.peek().unwrap().position, Duration::from_secs(50));
        assert_eq!(controller.current().position, Duration::ZERO);
        slider.on_mouseout.as_ref().unwrap().borrow_mut()(&event);
        assert!(preview.peek().is_none());
        event.x = f32::NAN;
        slider.on_mousemove.as_ref().unwrap().borrow_mut()(&event);
        assert!(preview.peek().is_none());
        event.x = 60.0;
        slider.on_mousemove.as_ref().unwrap().borrow_mut()(&event);
        controller.publish(MediaEvent::new(
            0,
            MediaEventKind::DurationChanged(Some(Duration::from_secs(200))),
        ));
        assert!(preview.peek().is_none());
        slider.on_mousemove.as_ref().unwrap().borrow_mut()(&event);
        controller.publish(MediaEvent::new(
            0,
            MediaEventKind::CapabilitiesChanged(MediaCapabilities::default()),
        ));
        assert!(preview.peek().is_none());
        controller.publish(MediaEvent::new(
            0,
            MediaEventKind::CapabilitiesChanged(MediaCapabilities {
                seek: true,
                ..Default::default()
            }),
        ));
        slider.on_mousemove.as_ref().unwrap().borrow_mut()(&event);
        controller.publish(MediaEvent::new(
            0,
            MediaEventKind::SeekableChanged(Some(
                MediaTimeRange::new(Duration::from_secs(300), Duration::from_secs(360)).unwrap(),
            )),
        ));
        assert!(preview.peek().is_none());
        event.x = 110.0;
        slider.on_mousemove.as_ref().unwrap().borrow_mut()(&event);
        assert_eq!(preview.peek().unwrap().position, Duration::from_secs(360));
        event.x = -10.0;
        slider.on_mousemove.as_ref().unwrap().borrow_mut()(&event);
        assert_eq!(preview.peek().unwrap().position, Duration::from_secs(300));
        event.current_bounds[2] = 0.0;
        slider.on_mousemove.as_ref().unwrap().borrow_mut()(&event);
        assert!(preview.peek().is_none());
        event.current_bounds[2] = 100.0;
        event.x = 110.0;
        controller.publish(MediaEvent::new(
            0,
            MediaEventKind::SeekableChanged(Some(
                MediaTimeRange::new(Duration::ZERO, Duration::MAX).unwrap(),
            )),
        ));
        slider.on_mousemove.as_ref().unwrap().borrow_mut()(&event);
        assert_eq!(preview.peek().unwrap().position, Duration::MAX);
        controller
            .load("another", MediaLoadOptions::default())
            .unwrap();
        assert!(preview.peek().is_none());
    });
}
