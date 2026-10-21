//! Native theme regression: actual primary/secondary trees and retained pixels.
use super::*;
use crate::core::reactivity::{create_effect, create_root};
use crate::renderer::offscreen::readback_texture;
use std::{cell::RefCell, rc::Rc};

#[test]
#[ignore = "requires a native Windows event loop and GPU surface"]
fn primary_startup_presents_real_content_before_show() {
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    use winit::window::WindowId;

    struct Harness;
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            crate::init();
            let (tx, rx) = std::sync::mpsc::channel();
            crate::app::ui_bridge::install_proxy(event_loop.create_proxy(), tx);
            let mut app = crate::app::lifecycle::create_app(
                crate::app::WindowOptions::new().size(480.0, 320.0),
                || {
                    Element::new()
                        .w("100%")
                        .h("100%")
                        .bg("background")
                        .child(crate::prelude::text("Ready"))
                },
                event_loop.create_proxy(),
                rx,
            );
            crate::app::with_ui_build_stack(|| app.can_create_surfaces(event_loop));
            let state = app.state.borrow();
            let state = state.as_ref().unwrap();
            assert!(state.window.is_visible().unwrap_or(false));
            assert!(
                state.has_presented_frame,
                "first show must have a presented UI frame"
            );
            assert!(!AppState::is_bootstrap_root(&state.root_element));
            assert!(
                !state.text_nodes.is_empty(),
                "startup must paint real content"
            );
            event_loop.exit();
        }
        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(Harness).unwrap();
    crate::app::runtime::clear_app_runtime();
    crate::app::reactive::reset_registered_anchors();
}

#[cfg(feature = "svg")]
#[test]
#[ignore = "requires a native Windows event loop and GPU surface"]
fn gallery_forms_hover_receives_scheduled_redraw() {
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    use winit::window::WindowId;

    struct Harness {
        app: Option<crate::app::App>,
        step: usize,
        presented: bool,
        deadline: Instant,
        forms_idle_until: Option<Instant>,
    }
    const TARGETS: [&str; 5] = [
        "gal-nav-forms",
        "gal-nav-events",
        "gal-nav-basics",
        "gal-nav-theme",
        "gal-nav-data",
    ];
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            #[cfg(feature = "runtime-logging")]
            crate::logging::init_with(crate::logging::InitOptions {
                default_filter:
                    "info,wgpu_core=warn,wgpu_hal=warn,wgpu_core::device::resource=off,fontdb=error",
            });
            crate::init();
            let theme = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/gallery/theme.toml");
            crate::style::load_and_install_theme_overrides(theme).unwrap();
            crate::core::reactivity::use_signal("gal_nav_section", String::new)
                .set("basics".into());
            let options = crate::app::WindowOptions::new().size(869.0, 600.0);
            #[cfg(all(debug_assertions, feature = "devtools"))]
            let options = options.devtools(true);
            let (tx, rx) = std::sync::mpsc::channel();
            crate::app::ui_bridge::install_proxy(event_loop.create_proxy(), tx);
            let mut app = crate::app::lifecycle::create_app(
                options,
                crate::gallery_app::build_gallery_app,
                event_loop.create_proxy(),
                rx,
            );
            crate::app::with_ui_build_stack(|| app.can_create_surfaces(event_loop));
            let window = app.state.borrow().as_ref().unwrap().window.clone();
            if std::env::var_os("JADIX_TEST_FULLSCREEN").is_some() {
                window.set_fullscreen(Some(winit::monitor::Fullscreen::Borderless(None)));
            } else {
                window.set_maximized(true);
            }
            self.app = Some(app);
        }
        fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
            self.app.as_mut().unwrap().proxy_wake_up(event_loop);
        }
        fn window_event(
            &mut self,
            event_loop: &dyn ActiveEventLoop,
            id: WindowId,
            event: WindowEvent,
        ) {
            let redraw = matches!(event, WindowEvent::RedrawRequested);
            crate::app::with_ui_build_stack(|| {
                self.app
                    .as_mut()
                    .unwrap()
                    .window_event(event_loop, id, event)
            });
            if redraw {
                self.presented = true;
            }
        }
        fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
            assert!(
                Instant::now() < self.deadline,
                "hover redraw did not arrive at step {}",
                self.step
            );
            let app = self.app.as_mut().unwrap();
            if self.step == 1 && self.presented {
                let until = *self
                    .forms_idle_until
                    .get_or_insert_with(|| Instant::now() + std::time::Duration::from_secs(3));
                if Instant::now() < until {
                    app.about_to_wait(event_loop);
                    if !matches!(
                        event_loop.control_flow(),
                        winit::event_loop::ControlFlow::Poll
                    ) {
                        event_loop
                            .set_control_flow(winit::event_loop::ControlFlow::WaitUntil(until));
                    }
                    return;
                }
            }
            if self.presented {
                self.presented = false;
                if self.step > 1 {
                    let id = TARGETS[self.step - 1];
                    let state = app.state.borrow();
                    let s = state.as_ref().unwrap();
                    let node = crate::core::tree::find_by_template_id(&s.root_element, id).unwrap();
                    assert!(
                        crate::core::reactivity::use_signal(&format!("{id}_hover"), || false)
                            .peek(),
                        "scheduled redraw must keep {id} hovered"
                    );
                    assert_eq!(
                        node.style.bg_color,
                        crate::style::parse_color("sidebar-accent")
                    );
                    let b = node.layout_bounds;
                    let scale = s.ui_scale();
                    let image = readback_texture(
                        &s.device,
                        &s.queue,
                        s.present_retain.color_texture_for_test().unwrap(),
                        s.config.width,
                        s.config.height,
                        s.device.create_command_encoder(&Default::default()),
                    )
                    .unwrap();
                    let pixel = image
                        .pixel(
                            ((b[0] + b[2] - 12.0) * scale) as u32,
                            ((b[1] + b[3] * 0.5) * scale) as u32,
                        )
                        .unwrap();
                    let expected = crate::style::parse_text_color("sidebar-accent");
                    assert!(
                        (0..3).all(|c| pixel[c].abs_diff(expected[c]) < 3),
                        "{id}: scheduled hover pixel={pixel:?} expected={expected:?} bounds={b:?} scale={scale} surface={}x{} inner={:?}",
                        s.config.width,
                        s.config.height,
                        s.window.surface_size()
                    );
                }
                if self.step == TARGETS.len() {
                    event_loop.exit();
                    return;
                }
                let (window, pointer) = {
                    let state = app.state.borrow();
                    let s = state.as_ref().unwrap();
                    let b =
                        crate::core::tree::find_by_template_id(&s.root_element, TARGETS[self.step])
                            .unwrap()
                            .layout_bounds;
                    (
                        s.window.clone(),
                        winit::dpi::PhysicalPosition::new(
                            ((b[0] + b[2] * 0.5) * s.ui_scale()) as f64,
                            ((b[1] + b[3] * 0.5) * s.ui_scale()) as f64,
                        ),
                    )
                };
                app.with_primary_input(window.id(), |app| {
                    app.handle_cursor_moved(window.id(), pointer, event_loop)
                });
                if self.step == 0 {
                    for state in [
                        winit::event::ElementState::Pressed,
                        winit::event::ElementState::Released,
                    ] {
                        app.with_primary_input(window.id(), |app| {
                            app.handle_mouse_button(
                                window.id(),
                                state,
                                winit::event::MouseButton::Left,
                                Some(event_loop),
                            )
                        });
                    }
                    assert_eq!(
                        crate::core::reactivity::use_signal("gal_nav_section", String::new).peek(),
                        "forms"
                    );
                }
                self.step += 1;
            }
            app.about_to_wait(event_loop);
        }
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder
        .build()
        .unwrap()
        .run_app(Harness {
            app: None,
            step: 0,
            presented: false,
            deadline: Instant::now() + std::time::Duration::from_secs(20),
            forms_idle_until: None,
        })
        .unwrap();
    crate::app::runtime::clear_app_runtime();
    crate::app::reactive::reset_registered_anchors();
}

#[test]
#[ignore = "requires a native Windows GPU surface"]
fn theme_switch_updates_primary_secondary_and_retained_pixels() {
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    use winit::window::{WindowAttributes, WindowId};

    struct Harness;
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            let make_window = || {
                Arc::<dyn Window>::from(
                    event_loop
                        .create_window(
                            WindowAttributes::default()
                                .with_visible(false)
                                .with_surface_size(winit::dpi::LogicalSize::new(320.0, 200.0)),
                        )
                        .unwrap(),
                )
            };
            run_frames(make_window(), make_window());
            event_loop.exit();
        }
        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(Harness).unwrap();
}

fn themed_tree() -> Element {
    Element::new()
        .w("100%")
        .h("100%")
        .child(crate::app::reactive::reactive_view(
            "theme:native:island",
            || {
                Element::new()
                    .w(120.0)
                    .h(80.0)
                    .bg("card")
                    .child(crate::components::button("theme:native:button").text("Theme"))
            },
        ))
}

fn run_frames(primary: Arc<dyn Window>, secondary: Arc<dyn Window>) {
    use crate::app::runtime::{clear_app_runtime, set_app_runtime, AppRuntime};
    use crate::app::secondary_window::SecondaryWindowState;
    use crate::app::window_scope;
    use crate::style::{set_theme_mode, ThemeMode};

    let prior = crate::style::get_theme();
    let prior_config = crate::style::theme_transition_config();
    crate::style::set_theme_transition_config(crate::style::ThemeTransitionConfig {
        enabled: true,
        duration_ms: 180.0,
        easing: "linear".into(),
    });
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    crate::app::reactive::reset_registered_anchors();
    let options = crate::app::WindowOptions::new();
    let state = Rc::new(RefCell::new(Some(crate::utils::block_on::block_on(
        AppState::new_bootstrap(primary, options.clone()),
    ))));
    let sec_id = secondary.id();
    let sec = {
        let state = state.borrow();
        let p = state.as_ref().unwrap();
        SecondaryWindowState::new(
            secondary,
            p.device.clone(),
            p.queue.clone(),
            &p.instance,
            &p.adapter,
            &p.config,
            Element::new(),
            options,
        )
        .unwrap()
    };
    let secondaries = Rc::new(RefCell::new(HashMap::from([(sec_id, sec)])));
    set_app_runtime(AppRuntime::new(
        Rc::downgrade(&state),
        Rc::downgrade(&secondaries),
    ));
    let live = state.clone();
    let (owner, _) = create_root(move || {
        create_effect(move || {
            crate::style::track_style_changes();
            live.borrow_mut()
                .as_mut()
                .unwrap()
                .rebuild_ui(themed_tree());
        });
    });
    let live = secondaries.clone();
    let (sec_owner, _) = create_root(move || {
        crate::core::reactivity::bind_current_owner_window_key_scope(
            window_scope::token_for_secondary(sec_id),
        );
        create_effect(move || {
            let _scope = window_scope::enter_secondary_id(sec_id);
            crate::style::track_style_changes();
            live.borrow_mut()
                .get_mut(&sec_id)
                .unwrap()
                .replace_root(themed_tree());
        });
    });

    let mut clock_ms = 0.0;
    for mode in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::Light] {
        set_theme_mode(mode);
        crate::style::flush_style_rebuild();
        crate::app::reactive::drain_deferred_view_remounts();
        {
            let _scope = window_scope::enter_primary();
            let mut state = state.borrow_mut();
            let p = state.as_mut().unwrap();
            p.render(
                [-1.0; 2],
                ScrollbarUpdateMode::ViewportsOnly,
                None,
                false,
                false,
                false,
            )
            .unwrap();
            if mode == ThemeMode::Dark {
                assert!(
                    p.theme_transition.needs_frame(),
                    "primary crossfade must start"
                );
                let texture = p
                    .theme_transition
                    .presented_texture_for_test()
                    .expect("primary faded image");
                let image = readback_texture(
                    &p.device,
                    &p.queue,
                    texture,
                    p.config.width,
                    p.config.height,
                    p.device.create_command_encoder(&Default::default()),
                )
                .unwrap();
                let pixel = image
                    .pixel(p.config.width - 10, p.config.height - 10)
                    .unwrap();
                assert!(
                    pixel[0] > 240,
                    "first dark frame retains light source: {pixel:?}"
                );
            }
            let texture = p.present_retain.color_texture_for_test().unwrap();
            let image = readback_texture(
                &p.device,
                &p.queue,
                texture,
                p.config.width,
                p.config.height,
                p.device.create_command_encoder(&Default::default()),
            )
            .unwrap();
            let pixel = image
                .pixel(p.config.width - 10, p.config.height - 10)
                .unwrap();
            let expected = crate::style::parse_text_color("background");
            assert!(
                (0..3).all(|i| pixel[i].abs_diff(expected[i]) < 3),
                "{mode:?}: background {pixel:?}"
            );
            assert!(
                crate::core::tree::find_in_tree(&p.root_element, |el| el.focus_id.as_deref()
                    == Some("theme:native:button"))
                .is_some()
            );
        }
        {
            let _scope = window_scope::enter_secondary_id(sec_id);
            let mut secs = secondaries.borrow_mut();
            let sec = secs.get_mut(&sec_id).unwrap();
            sec.render([-1.0; 2]).unwrap();
            if mode == ThemeMode::Dark {
                assert!(
                    sec.theme_transition.needs_frame(),
                    "secondary crossfade must start"
                );
                assert!(sec.theme_transition.presented_texture_for_test().is_some());
            }
            let node =
                crate::core::tree::find_by_template_id(&sec.root_element, "theme:native:island")
                    .unwrap();
            assert_eq!(node.style.bg_color, crate::style::parse_color("card"));
            assert!(crate::core::tree::find_in_tree(&sec.root_element, |el| el
                .focus_id
                .as_deref()
                == Some("theme:native:button"))
            .is_some());
        }
        clock_ms += 180.0;
        crate::style::set_css_transition_timeline_ms(clock_ms);
        {
            let _scope = window_scope::enter_primary();
            let mut live = state.borrow_mut();
            let p = live.as_mut().unwrap();
            p.render(
                [-1.0; 2],
                ScrollbarUpdateMode::ViewportsOnly,
                None,
                false,
                false,
                false,
            )
            .unwrap();
            assert!(
                !p.theme_transition.needs_frame(),
                "primary fade must settle even when the content fingerprint is unchanged"
            );
        }
        {
            let _scope = window_scope::enter_secondary_id(sec_id);
            let mut secs = secondaries.borrow_mut();
            let sec = secs.get_mut(&sec_id).unwrap();
            sec.render([-1.0; 2]).unwrap();
            assert!(
                !sec.theme_transition.needs_frame(),
                "secondary fade must settle"
            );
        }
    }
    drop(sec_owner);
    drop(owner);
    clear_app_runtime();
    window_scope::forget_secondary(sec_id);
    crate::app::reactive::reset_registered_anchors();
    crate::style::set_css_transition_virtual_timeline(false);
    crate::style::set_theme(prior);
    crate::style::set_theme_transition_config(prior_config);
}
