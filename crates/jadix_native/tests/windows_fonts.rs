#![cfg(all(windows, feature = "windows"))]

use jadix_native::{host::NativeRoot, presentation::*, windows::WindowsHost};
use windows_sys::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_GETFONT};

#[test]
fn private_font_registration_remeasures_without_replacing_editors_and_refuses_early_removal() {
    let bytes = include_bytes!("../../jadix_cosmic_text/fonts/FiraMono-Medium.ttf");
    let mut root = NativeRoot::new(WindowsHost::new("Native font lease test", [420, 200]).unwrap());
    let editor = NodeId::new(root.model().id());
    let mut nodes = [PresentationNode {
        id: editor,
        descriptor: NodeDescriptor {
            content: NodeContent::Input {
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
