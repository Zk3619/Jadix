#![cfg(all(windows, feature = "windows"))]

use jadix_native::{host::NativeRoot, presentation::*, windows::WindowsHost};
use windows_sys::Win32::{
    Foundation::HWND,
    Graphics::Gdi::*,
    UI::{
        Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
        WindowsAndMessaging::*,
    },
};

fn palette(window: HWND) -> (u32, u32) {
    unsafe {
        let device = GetDC(window);
        assert!(!device.is_null());
        let brush = SendMessageW(
            GetParent(window),
            WM_CTLCOLOREDIT,
            device as usize,
            window as isize,
        ) as HBRUSH;
        let foreground = GetTextColor(device);
        let mut description = LOGBRUSH::default();
        assert_ne!(
            GetObjectW(
                brush,
                std::mem::size_of::<LOGBRUSH>() as i32,
                (&mut description as *mut LOGBRUSH).cast()
            ),
            0
        );
        ReleaseDC(window, device);
        (foreground, description.lbColor)
    }
}

#[test]
fn native_edit_palette_inherits_background_retains_handles_and_respects_high_contrast() {
    let mut root = NativeRoot::new(WindowsHost::new("Jadix native palette", [320, 240]).unwrap());
    let identifier = root.model().id();
    let mut roots = vec![PresentationNode {
        id: NodeId::new(identifier),
        descriptor: NodeDescriptor {
            content: NodeContent::Container,
            frame: Rect::default(),
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            interaction: Default::default(),
            keyboard: Default::default(),
            navigation_keys: Vec::new(),
            accessibility: Default::default(),
            accessibility_relations: Default::default(),
            accessible_description: None,
            accessible_name: None,
            style: style::NodeStyle {
                background: [10, 20, 30, 255],
                width: Some(300.0),
                height: Some(160.0),
                ..Default::default()
            },
        },
        children: vec![],
    }];
    roots[0].children.push(PresentationNode {
        id: NodeId::new(identifier),
        descriptor: NodeDescriptor {
            content: NodeContent::Input {
                max_length: None,
                value: "retained".into(),
                placeholder: String::new(),
                enabled: true,
                password: false,
                revision: 0,
            },
            frame: Rect::default(),
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            interaction: Default::default(),
            keyboard: Default::default(),
            navigation_keys: Vec::new(),
            accessibility: Default::default(),
            accessibility_relations: Default::default(),
            accessible_description: None,
            accessible_name: None,
            style: style::NodeStyle {
                foreground: [230, 231, 232, 255],
                width: Some(260.0),
                height: Some(40.0),
                ..Default::default()
            },
        },
        children: vec![],
    });
    root.present(&mut roots, [320.0, 240.0], 0).unwrap();
    let input = roots[0].children[0].id;
    let window = root.host().control_identity(input).unwrap() as HWND;
    let mut settings = HIGHCONTRASTW {
        cbSize: std::mem::size_of::<HIGHCONTRASTW>() as u32,
        ..Default::default()
    };
    assert_ne!(
        unsafe {
            SystemParametersInfoW(
                SPI_GETHIGHCONTRAST,
                settings.cbSize,
                (&mut settings as *mut HIGHCONTRASTW).cast(),
                0,
            )
        },
        0
    );
    let high_contrast = settings.dwFlags & HCF_HIGHCONTRASTON != 0;
    let expected = if high_contrast {
        unsafe { (GetSysColor(COLOR_WINDOWTEXT), GetSysColor(COLOR_WINDOW)) }
    } else {
        (230 | (231 << 8) | (232 << 16), 10 | (20 << 8) | (30 << 16))
    };
    assert_eq!(palette(window), expected);
    roots[0].descriptor.style.background = [30, 40, 50, 255];
    root.present(&mut roots, [320.0, 240.0], 0).unwrap();
    assert_eq!(root.host().control_identity(input), Some(window as usize));
    let expected_background = if high_contrast {
        unsafe { GetSysColor(COLOR_WINDOW) }
    } else {
        30 | (40 << 8) | (50 << 16)
    };
    assert_eq!(palette(window).1, expected_background);
    assert_eq!(root.host().text(input).unwrap(), "retained");
}
