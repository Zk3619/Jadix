#[cfg(target_os = "linux")]
fn main() -> Result<(), String> {
    use jadix_native::{
        host::PlatformHost,
        linux::{
            gtk4::{glib, prelude::*},
            GtkHost, WindowApplication,
        },
    };
    use std::{cell::RefCell, rc::Rc};
    let mut host = GtkHost::new("Jadix · Linux native controls", [900, 700])?;
    use jadix_native::linux::gtk4::gio;
    let links: Vec<_> = std::env::args()
        .skip(1)
        .map(|arg| {
            let link = jadix_core::deep_link::DeepLink {
                url: arg,
                source: jadix_core::deep_link::DeepLinkSource::ColdStart,
            };
            link.validate(&std::collections::BTreeSet::from(["jadix".into()]))
                .map_err(str::to_owned)?;
            Ok::<_, String>(link)
        })
        .collect::<Result<_, _>>()?;
    let application = jadix_native::linux::gtk4::Application::builder()
        .application_id("org.jadix.NativeControls")
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();
    let window = host.window().downgrade();
    application.connect_activate(move |_| {
        if let Some(window) = window.upgrade() {
            window.present();
        }
    });
    application
        .register(None::<&jadix_native::linux::gtk4::gio::Cancellable>)
        .map_err(|e| e.to_string())?;
    if application.is_remote() {
        if links.is_empty() {
            application.activate();
        } else {
            application.open(
                &links
                    .iter()
                    .map(|link| gio::File::for_uri(&link.url))
                    .collect::<Vec<_>>(),
                "",
            );
        }
        return Ok(());
    }
    host.window().set_application(Some(&application));
    host.configure_deep_links(jadix_core::deep_link::DeepLinkConfig {
        schemes: std::collections::BTreeSet::from(["jadix".into()]),
        ..Default::default()
    })?;
    for link in links {
        host.deliver_deep_link(link)?;
    }
    jadix_native_controls::NativeApp::register_linux_windows(&mut host)?;
    let app = Rc::new(RefCell::new(jadix_native_controls::NativeApp::new(
        host,
        [900., 700.],
        0,
        "中文 🦀 Linux".into(),
    )?));
    let main_loop = glib::MainLoop::new(None, false);
    let quit = main_loop.clone();
    let weak = Rc::downgrade(&app);
    app.borrow().root().host().on_wake(move || {
        let Some(app) = weak.upgrade() else {
            quit.quit();
            return;
        };
        let mut app = app.borrow_mut();
        if !app.host().is_alive() {
            quit.quit();
            return;
        }
        if let Err(error) = app.tick() {
            eprintln!("{error}");
            app.root_mut().dispose();
            quit.quit();
        }
    });
    app.borrow().root().host().present();
    main_loop.run();
    app.borrow_mut().root_mut().dispose();
    Ok(())
}
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("native-linux requires Linux with GTK 4.14 or later");
}
