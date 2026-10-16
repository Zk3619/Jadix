//! AppKit extensions. Native traffic lights, tabs and materials remain AppKit-owned.
pub use winit_appkit::*;

use crate::chrome::WindowExtJadix;
use crate::error::{NotSupportedError, RequestError};
use crate::raw_window_handle::HasWindowHandle;
use crate::window::{Window as CoreWindow, WindowButtons};
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSView, NSWindow, NSWindowButton};
use objc2_foundation::{NSPoint, NSString, NSUserDefaults};

fn native_window(window: &dyn CoreWindow) -> Result<Retained<NSWindow>, RequestError> {
    // Raw AppKit handles are valid only on the main thread. Never send an NSView
    // or NSWindow through the generic Send + Sync Window trait.
    let _mtm = MainThreadMarker::new()
        .ok_or_else(|| NotSupportedError::new("AppKit chrome requires the main thread"))?;
    let handle = window
        .window_handle()
        .map_err(|_| NotSupportedError::new("AppKit window handle is unavailable"))?;
    let crate::raw_window_handle::RawWindowHandle::AppKit(raw) = handle.as_raw() else {
        return Err(NotSupportedError::new("window is not an AppKit window").into());
    };
    // SAFETY: the borrowed Winit Window owns this NSView, the handle is live,
    // and MainThreadMarker verified AppKit's thread affinity above.
    let view = unsafe { raw.ns_view.cast::<NSView>().as_ref() };
    view.window()
        .ok_or_else(|| NotSupportedError::new("NSView is detached from its window").into())
}

/// Respect the system title-bar preference. `Fill` uses public `zoom:` as a
/// fallback; the private Fill selector is only used with private-apple-apis.
pub fn titlebar_double_click(window: &dyn CoreWindow) -> Result<(), RequestError> {
    let native = native_window(window)?;
    let state = window.chrome_state();
    if state.fullscreen || window.simple_fullscreen() {
        return Err(NotSupportedError::new("fullscreen title bars do not zoom or minimize").into());
    }
    let action = NSUserDefaults::standardUserDefaults()
        .stringForKey(&NSString::from_str("AppleActionOnDoubleClick"))
        .map(|s| s.to_string());
    match action.as_deref() {
        Some("None") => Ok(()),
        Some("Minimize") if state.buttons.contains(WindowButtons::MINIMIZE) => {
            native.miniaturize(None);
            Ok(())
        }
        Some("Minimize") => {
            Err(NotSupportedError::new("window cannot be minimized by its title bar").into())
        }
        _ if state.can_maximize() => {
            #[cfg(feature = "private-apple-apis")]
            if action.as_deref() == Some("Fill") {
                use objc2::runtime::NSObjectProtocol;
                use objc2::{msg_send, sel};
                if native.respondsToSelector(sel!(_zoomFill:)) {
                    unsafe {
                        let _: () = msg_send![&native, _zoomFill: Option::<&objc2::runtime::AnyObject>::None];
                    }
                    return Ok(());
                }
            }
            native.zoom(None);
            Ok(())
        }
        _ => Err(NotSupportedError::new("window cannot be maximized by its title bar").into()),
    }
}

/// Position native traffic lights by a top-left logical offset in their AppKit
/// container. Reapply after a title-bar layout/fullscreen transition. The caller
/// must reserve content space for these native controls; no buttons are emulated.
pub fn set_traffic_light_position(
    window: &dyn CoreWindow,
    position: crate::dpi::LogicalPosition<f64>,
) -> Result<(), RequestError> {
    if ![position.x, position.y]
        .iter()
        .all(|v| v.is_finite() && *v >= 0.0)
    {
        return Err(NotSupportedError::new(
            "traffic-light position must be finite and nonnegative",
        )
        .into());
    }
    let native = native_window(window)?;
    let buttons = [
        NSWindowButton::CloseButton,
        NSWindowButton::MiniaturizeButton,
        NSWindowButton::ZoomButton,
    ]
    .map(|kind| native.standardWindowButton(kind));
    let Some(close) = &buttons[0] else {
        return Err(NotSupportedError::new("window has no native traffic lights").into());
    };
    // SAFETY: retained native button, accessed on the verified AppKit main thread.
    let container = unsafe { close.superview() }
        .ok_or_else(|| NotSupportedError::new("traffic lights have no container"))?;
    let close_origin = close.frame().origin;
    for button in buttons.into_iter().flatten() {
        let frame = button.frame();
        let y = if container.isFlipped() {
            position.y
        } else {
            container.bounds().size.height - position.y - frame.size.height
        };
        button.setFrameOrigin(NSPoint::new(
            position.x + frame.origin.x - close_origin.x,
            y,
        ));
    }
    Ok(())
}
