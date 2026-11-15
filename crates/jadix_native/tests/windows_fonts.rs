#![cfg(all(windows, feature = "windows"))]

use jadix_native::{host::NativeRoot, presentation::*, windows::WindowsHost};
use windows_sys::Win32::{
    Graphics::Gdi::{GetObjectW, LOGFONTW},
    UI::WindowsAndMessaging::{
        SendMessageW, SystemParametersInfoW, NONCLIENTMETRICSW, SPI_GETNONCLIENTMETRICS,
        WM_FONTCHANGE, WM_GETFONT, WM_SETTINGCHANGE,
    },
};

#[test]
fn system_font_changes_refresh_in_place_preserving_native_editor_state_and_authored_family() {
    let mut root =
        NativeRoot::new(WindowsHost::new("System font refresh test", [420, 200]).unwrap());
    let editor = NodeId::new(root.model().id());
    let mut nodes = [PresentationNode {
        id: editor,
        descriptor: NodeDescriptor {
            content: NodeContent::Input {
                max_length: None,
                value: "before".into(),
                placeholder: String::new(),
                enabled: true,
                password: false,
                revision: 0,
            },
            frame: Rect::default(),
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            accessibility_relations: Default::default(),
            accessible_description: None,
            accessible_name: None,
            style: style::NodeStyle {
                width: Some(360.0),
                height: Some(40.0),
                ..Default::default()
            },
        },
        children: vec![],
    }];
    let environment = root.host().environment();
    root.present(&mut nodes, [420.0, 200.0], environment)
        .unwrap();
    let identity = root.host().control_identity(editor).unwrap();
    let mut system = NONCLIENTMETRICSW::default();
    system.cbSize = std::mem::size_of::<NONCLIENTMETRICSW>() as u32;
    assert_ne!(
        unsafe {
            SystemParametersInfoW(
                SPI_GETNONCLIENTMETRICS,
                system.cbSize,
                std::ptr::addr_of_mut!(system).cast(),
                0,
            )
        },
        0
    );
    let read_font = || {
        let handle = unsafe { SendMessageW(identity as _, WM_GETFONT, 0, 0) };
        let mut font = LOGFONTW::default();
        assert_ne!(
            unsafe {
                GetObjectW(
                    handle as _,
                    std::mem::size_of::<LOGFONTW>() as i32,
                    std::ptr::addr_of_mut!(font).cast(),
                )
            },
            0
        );
        (handle, font)
    };
    assert_eq!(read_font().1.lfFaceName, system.lfMessageFont.lfFaceName);
    root.host().select_range(editor, [0, 6]).unwrap();
    root.host().replace_selection(editor, "edited 😀").unwrap();
    root.poll_events().unwrap();
    root.host().select_range(editor, [0, 6]).unwrap();
    for message in [WM_SETTINGCHANGE, WM_FONTCHANGE] {
        let old_font = read_font().0;
        let old_environment = root.host().environment();
        unsafe {
            SendMessageW(root.host().native_window() as _, message, 0, 0);
        }
        assert!(root.host().environment() > old_environment);
        root.poll_events().unwrap();
        assert_ne!(read_font().0, old_font);
        assert_eq!(root.host().control_identity(editor), Some(identity));
        assert_eq!(root.host().text(editor).unwrap(), "edited 😀");
        assert_eq!(root.host().selection(editor).unwrap(), [0, 6]);
    }
    assert!(root.host().undo(editor).unwrap());
    assert_eq!(root.host().text(editor).unwrap(), "before");
    let events = root.poll_events().unwrap();
    let revision = events
        .iter()
        .find_map(|event| match event.payload {
            EventPayload::Edit { revision, .. } if event.node == editor => Some(revision),
            _ => None,
        })
        .expect("native undo emits the latest edit revision");
    if let NodeContent::Input {
        revision: authored_revision,
        ..
    } = &mut nodes[0].descriptor.content
    {
        *authored_revision = revision;
    }
    nodes[0].descriptor.style.font_family = "Courier New".into();
    let environment = root.host().environment();
    root.present(&mut nodes, [420.0, 200.0], environment)
        .unwrap();
    unsafe {
        SendMessageW(root.host().native_window() as _, WM_SETTINGCHANGE, 0, 0);
    }
    root.poll_events().unwrap();
    let name = read_font().1.lfFaceName;
    let length = name.iter().position(|character| *character == 0).unwrap();
    assert_eq!(String::from_utf16(&name[..length]).unwrap(), "Courier New");
    assert_eq!(root.host().control_identity(editor), Some(identity));
}

#[test]
fn private_font_registration_remeasures_without_replacing_editors_and_refuses_early_removal() {
    let bytes = include_bytes!("../../jadix_cosmic_text/fonts/FiraMono-Medium.ttf");
    let mut root = NativeRoot::new(WindowsHost::new("Native font lease test", [420, 200]).unwrap());
    let editor = NodeId::new(root.model().id());
    let mut nodes = [PresentationNode {
        id: editor,
        descriptor: NodeDescriptor {
            content: NodeContent::Input {
                max_length: None,
                value: "WWW iii ffi 中😀".into(),
                placeholder: String::new(),
                enabled: true,
                password: false,
                revision: 0,
            },
            frame: Rect::default(),
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            accessibility_relations: Default::default(),
            accessible_description: None,
            accessible_name: None,
            style: style::NodeStyle {
                width: Some(360.0),
                height: Some(40.0),
                font_family: "Fira Mono Medium".into(),
                ..Default::default()
            },
        },
        children: vec![],
    }];
    let environment = root.host().environment();
    root.present(&mut nodes, [420.0, 200.0], environment)
        .unwrap();
    let identity = root.host().control_identity(editor).unwrap();
    let old_font = unsafe { SendMessageW(identity as _, WM_GETFONT, 0, 0) };
    let registration = root.host_mut().install_font(bytes).unwrap();
    assert!(registration.faces > 0);
    let environment = root.host().environment();
    let repeated = root.host_mut().install_font(bytes).unwrap();
    assert_eq!(registration, repeated);
    assert_eq!(root.host().environment(), environment);
    assert!(root.host_mut().install_font(b"not a font").is_err());
    assert_eq!(root.host().font_resources()["registrations"], 1);
    root.poll_events().unwrap();
    let current_font = unsafe { SendMessageW(identity as _, WM_GETFONT, 0, 0) };
    assert_ne!(old_font, current_font);
    nodes[0].descriptor.style.font_family = registration
        .families
        .iter()
        .find(|family| family.starts_with("Fira Mono"))
        .unwrap()
        .clone();
    let layout = root
        .present(&mut nodes, [420.0, 200.0], environment)
        .unwrap();
    assert!(layout.measurements > 0);
    assert_eq!(root.host().control_identity(editor), Some(identity));
    let selected = root.host().font_diagnostics(editor).unwrap();
    assert!(
        selected["face"].as_str().unwrap().starts_with("Fira Mono"),
        "{selected}"
    );
    assert!(root.host_mut().remove_font(registration.id).is_err());
    let mut other = WindowsHost::new("Other font owner", [100, 100]).unwrap();
    let foreign = other.install_font(bytes).unwrap();
    assert_ne!(foreign.id, registration.id);
    assert!(root.host_mut().remove_font(foreign.id).is_err());
    drop(other);
    nodes[0].descriptor.style.font_family = "system-ui".into();
    root.present(&mut nodes, [420.0, 200.0], environment)
        .unwrap();
    root.host_mut().remove_font(registration.id).unwrap();
    let environment = root.host().environment();
    assert!(
        root.present(&mut nodes, [420.0, 200.0], environment)
            .unwrap()
            .measurements
            > 0
    );
    assert_eq!(root.host().font_resources()["source_bytes"], 0);
    root.host_mut().install_font(bytes).unwrap();
    root.dispose();
    assert_eq!(root.host().font_resources()["registrations"], 0);
}
