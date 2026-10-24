use crate::app::WindowOptions;
use winit::window::Window;

pub(crate) fn apply_created_window(
    window: &dyn Window,
    options: &WindowOptions,
    composition_swapchain: bool,
) {
    #[cfg(feature = "window-materials")]
    if apply_glass(window, options) {
        return;
    }
    crate::platform::desktop::apply_created_window(window, options, composition_swapchain);
}

#[cfg(feature = "window-materials")]
fn apply_glass(window: &dyn Window, options: &WindowOptions) -> bool {
    use objc2::{
        MainThreadMarker, msg_send,
        rc::Retained,
        runtime::{AnyClass, Bool},
    };
    use objc2_app_kit::{NSAutoresizingMaskOptions, NSView, NSWindowOrderingMode};
    use objc2_foundation::{NSString, ns_string};
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Some(_) = MainThreadMarker::new() else {
        return false;
    };
    let Ok(handle) = window.window_handle() else {
        return false;
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return false;
    };
    let view = unsafe { &*handle.ns_view.as_ptr().cast::<NSView>() };
    let Some(content) = view.window().and_then(|window| window.contentView()) else {
        return false;
    };
    let existing = content.subviews().iter().find_map(|child| {
        let identifier: Option<Retained<NSString>> = unsafe { msg_send![&child, identifier] };
        identifier
            .filter(|identifier| &**identifier == ns_string!("jadix.window.glass"))
            .map(|_| child.clone())
    });
    let enabled = options.follow_theme
        && options.blur
        && crate::style::theme_preset() == crate::style::ThemePreset::LiquidGlass;
    if !enabled {
        if let Some(existing) = existing {
            existing.removeFromSuperview();
        }
        return false;
    }
    let Some(class) = AnyClass::get(c"NSGlassEffectView") else {
        return false;
    };
    window.set_blur(false);
    let glass = existing.unwrap_or_else(|| {
        let glass: Retained<NSView> = unsafe { msg_send![class, new] };
        unsafe {
            let _: () = msg_send![&glass, setIdentifier: ns_string!("jadix.window.glass")];
        }
        content.addSubview_positioned_relativeTo(&glass, NSWindowOrderingMode::Below, Some(view));
        glass
    });
    glass.setFrame(content.bounds());
    glass.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    unsafe {
        let _: () = msg_send![&glass, setCornerRadius: 20.0_f64];
        let _: () = msg_send![&glass, setEffectIsInteractive: Bool::YES];
    }
    true
}
