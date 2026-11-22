use jadix_components::{authoring::native::ElementProjection, prelude::*};
use jadix_core::reactivity::create_root;
use jadix_native::{
    host::{NativeRoot, PlatformHost},
    linux::{
        gtk4::{self as gtk, gio, glib, prelude::*},
        GtkHost,
    },
};
use std::{cell::RefCell, rc::Rc};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut host = GtkHost::new("@NAME@", [640, 480])?;
    let application = gtk::Application::builder()
        .application_id("@APPLICATION_ID@")
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();
    let window = host.window().downgrade();
    application.connect_activate(move |_| {
        if let Some(window) = window.upgrade() {
            window.present();
        }
    });
    application.register(None::<&gio::Cancellable>)?;
    let urls: Vec<_> = std::env::args().skip(1).collect();
    if application.is_remote() {
        if urls.is_empty() {
            application.activate();
        } else {
            application.open(
                &urls
                    .iter()
                    .map(|url| gio::File::for_uri(url))
                    .collect::<Vec<_>>(),
                "",
            );
        }
        return Ok(());
    }
    host.set_image_disk_cache(jadix_native::images::ImageDiskCache {
        directory: glib::user_cache_dir().join("@APPLICATION_ID@"),
        bytes: 16 * 1024 * 1024,
        entries: 64,
        age: std::time::Duration::from_secs(3600),
    })?;
    host.configure_deep_links(jadix_core::deep_link::DeepLinkConfig {
        schemes: std::collections::BTreeSet::from(["@NAME@".into()]),
        ..Default::default()
    })?;
    host.window().set_application(Some(&application));
    for url in urls {
        host.deliver_deep_link(jadix_core::deep_link::DeepLink {
            url,
            source: jadix_core::deep_link::DeepLinkSource::ColdStart,
        })?;
    }
    let mut root = NativeRoot::new(host);
    root.refresh_environment()?;
    let mut projection = ElementProjection::with_lifetime(root.model().lifetime())
        .with_locale_context(root.locale_context());
    let (owner, mut element) = root.with_locale(|| {
        create_root(|| {
            let count = create_signal(0usize);
            vstack()
                .p(16.)
                .gap(12.)
                .child(
                    jadix_components::app::reactive::bind_text("count", move || {
                        format!("Count: {}", count.get())
                    })
                    .h(32.),
                )
                .child(
                    button("increment")
                        .child(text("Continue"))
                        .on_click(move || count.set(count.get().saturating_add(1))),
                )
        })
    });
    let mut nodes = vec![projection
        .snapshot(&mut element)
        .map_err(|e| format!("{e:?}"))?];
    root.present(&mut nodes, [640., 480.], 0)?;
    let state = Rc::new(RefCell::new((root, projection, element)));
    let main_loop = glib::MainLoop::new(None, false);
    let quit = main_loop.clone();
    let weak = Rc::downgrade(&state);
    state.borrow().0.host().on_wake(move || {
        let Some(state) = weak.upgrade() else {
            quit.quit();
            return;
        };
        let mut state = state.borrow_mut();
        let (root, projection, element) = &mut *state;
        if !root.host().is_alive() {
            quit.quit();
            return;
        }
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            root.refresh_environment()?;
            for event in root.poll_events()? {
                projection
                    .dispatch(element, &event)
                    .map_err(|e| format!("{e:?}"))?;
            }
            let mut nodes = vec![projection.snapshot(element).map_err(|e| format!("{e:?}"))?];
            root.present(&mut nodes, root.host().viewport(), 0)?;
            Ok(())
        })();
        if let Err(error) = result {
            eprintln!("{error}");
            root.dispose();
            quit.quit();
        }
    });
    state.borrow().0.host().present();
    main_loop.run();
    state.borrow_mut().0.dispose();
    drop(state);
    drop(owner);
    Ok(())
}
