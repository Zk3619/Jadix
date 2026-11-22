use super::*;
use crate::linux::wake::Wake;
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
struct Daemon(Child);
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn spin(context: &glib::MainContext, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < deadline, "private portal deadline");
        while context.pending() {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn private_portal_accepts_early_responses_typed_bindings_activation_and_teardown() {
    let context = glib::MainContext::default();
    let _guard = context.acquire().unwrap();
    let mut daemon = Daemon(
        Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut address = String::new();
    BufReader::new(daemon.0.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    let connect = || {
        gio::DBusConnection::for_address_sync(
            address.trim(),
            gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
                | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
            None,
            gio::Cancellable::NONE,
        )
        .unwrap()
    };
    let server = connect();
    let client = connect();
    server.set_exit_on_close(false);
    client.set_exit_on_close(false);
    server
        .call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "RequestName",
            Some(&(PORTAL, 0u32).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            3000,
            gio::Cancellable::NONE,
        )
        .unwrap();
    let xml = r#"<node><interface name="org.freedesktop.portal.GlobalShortcuts"><method name="CreateSession"><arg type="a{sv}" direction="in"/><arg type="o" direction="out"/></method><method name="BindShortcuts"><arg type="o" direction="in"/><arg type="a(sa{sv})" direction="in"/><arg type="s" direction="in"/><arg type="a{sv}" direction="in"/><arg type="o" direction="out"/></method><signal name="Activated"><arg type="o"/><arg type="s"/><arg type="t"/><arg type="a{sv}"/></signal></interface></node>"#;
    let info = gio::DBusNodeInfo::for_xml(xml).unwrap();
    let seen = Rc::new(RefCell::new(Vec::<String>::new()));
    let calls = seen.clone();
    let registration = server
        .register_object(PATH, &info.interfaces()[0])
        .method_call(move |bus, sender, _, _, method, args, invocation| {
            calls.borrow_mut().push(args.type_().as_str().into());
            let options =
                glib::VariantDict::new(Some(&args.child_value(if method == "CreateSession" {
                    0
                } else {
                    3
                })));
            let token = options.lookup::<String>("handle_token").unwrap().unwrap();
            let sender = sender.unwrap();
            let request_path = format!(
                "{PATH}/request/{}/{}",
                sender.trim_start_matches(':').replace('.', "_"),
                token
            );
            let result = if method == "CreateSession" {
                assert!(options
                    .lookup::<String>("session_handle_token")
                    .unwrap()
                    .is_some());
                BTreeMap::from([(
                    "session_handle",
                    "/org/freedesktop/portal/desktop/session/test".to_variant(),
                )])
            } else {
                let shortcuts = args
                    .child_value(1)
                    .get::<Vec<(String, BTreeMap<String, glib::Variant>)>>()
                    .unwrap();
                assert_eq!(
                    shortcuts[0].1["preferred_trigger"].get::<String>().unwrap(),
                    "CTRL+ALT+K"
                );
                BTreeMap::from([("shortcuts", shortcuts.to_variant())])
            };
            // Emitting before returning the request path exercises the portal race.
            bus.emit_signal(
                Some(sender),
                &request_path,
                "org.freedesktop.portal.Request",
                "Response",
                Some(&(0u32, result).to_variant()),
            )
            .unwrap();
            invocation.return_value(Some(
                &(ObjectPath::try_from(request_path).unwrap(),).to_variant(),
            ));
        })
        .build()
        .unwrap();
    let wake = Wake::new().unwrap();
    let service = DesktopServices::default();
    let shortcut = GlobalShortcut {
        key: "K".into(),
        control: true,
        option: true,
        shift: false,
        command: false,
    };
    let handle = service.register_shortcut(shortcut.clone()).unwrap();
    service.take_commands();
    let shortcuts = Shortcuts::new(service, wake.handle.clone());
    *shortcuts.bus.borrow_mut() = Some(client.clone());
    shortcuts.register(handle.id(), shortcut, None);
    spin(&context, || handle.reply().is_some());
    assert!(
        matches!(handle.reply(), Some(ServiceReply::Complete { error: None })),
        "{:?}",
        handle.reply()
    );
    assert_eq!(&*seen.borrow(), &["(a{sv})", "(oa(sa{sv})sa{sv})"]);
    assert!(shortcuts.registrations.borrow()[&handle.id()]
        .session
        .is_some());
    let empty = BTreeMap::<String, glib::Variant>::new();
    for session in [
        "/wrong/session",
        "/org/freedesktop/portal/desktop/session/test",
    ] {
        server
            .emit_signal(
                None,
                PATH,
                INTERFACE,
                "Activated",
                Some(
                    &(
                        ObjectPath::try_from(session).unwrap(),
                        handle.id().to_string(),
                        0u64,
                        empty.clone(),
                    )
                        .to_variant(),
                ),
            )
            .unwrap();
    }
    let received = Cell::new(false);
    spin(&context, || {
        if !handle.events().is_empty() {
            received.set(true);
        }
        received.get()
    });
    assert!(
        handle.events().is_empty(),
        "wrong-session activation leaked"
    );
    shortcuts.unregister(handle.id());
    assert!(shortcuts.registrations.borrow().is_empty());
    shortcuts.dispose();
    assert!(shortcuts.bus.borrow().is_none());
    server.unregister_object(registration).unwrap();
    let _ = server.close_sync(gio::Cancellable::NONE);
}
