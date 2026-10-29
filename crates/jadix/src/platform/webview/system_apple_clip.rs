//! A native clipping parent keeps WKWebView's document viewport independent of
//! the visible region. No synthetic pointer forwarding or screen-coordinate math.
use super::super::super::WebViewSpec;
use objc2::{
    msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject},
};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use std::{ffi::c_void, ptr::NonNull};
use winit::{
    raw_window_handle::{HandleError, HasWindowHandle, RawWindowHandle, WindowHandle},
    window::Window,
};

pub struct ClipView {
    view: Retained<AnyObject>,
    raw: RawWindowHandle,
}
impl ClipView {
    pub fn new(window: &dyn Window) -> Result<Self, String> {
        let mut raw = window.window_handle().map_err(|e| e.to_string())?.as_raw();
        let parent: *mut AnyObject = match raw {
            RawWindowHandle::AppKit(h) => h.ns_view.as_ptr().cast(),
            RawWindowHandle::UiKit(h) => h.ui_view.as_ptr().cast(),
            _ => return Err("WKWebView requires an AppKit/UIKit view".into()),
        };
        unsafe {
            #[cfg(target_os = "macos")]
            let class = AnyClass::get(c"NSView").ok_or("NSView unavailable")?;
            #[cfg(target_os = "ios")]
            let class = AnyClass::get(c"UIView").ok_or("UIView unavailable")?;
            let view: *mut AnyObject = msg_send![class, alloc];
            let view: *mut AnyObject = msg_send![view, initWithFrame: NSRect::new(NSPoint::new(0.0,0.0), NSSize::new(1.0,1.0))];
            let view = Retained::from_raw(view).ok_or("create WebView clipping parent")?;
            let _: () = msg_send![parent, addSubview: &*view];
            #[cfg(target_os = "macos")]
            {
                let _: () = msg_send![&*view, setWantsLayer: true];
                let layer: *mut AnyObject = msg_send![&*view, layer];
                let _: () = msg_send![layer, setMasksToBounds: true];
            }
            #[cfg(target_os = "ios")]
            {
                let _: () = msg_send![&*view, setClipsToBounds: true];
            }
            let ptr = NonNull::new(Retained::as_ptr(&view) as *mut c_void).unwrap();
            match &mut raw {
                RawWindowHandle::AppKit(h) => h.ns_view = ptr,
                RawWindowHandle::UiKit(h) => h.ui_view = ptr,
                _ => unreachable!(),
            }
            Ok(Self { view, raw })
        }
    }
    pub fn visible(&self, visible: bool) {
        unsafe {
            let _: () = msg_send![&*self.view, setHidden: !visible];
        }
    }
    pub fn update(&self, browser: &wry::WebView, spec: &WebViewSpec) -> Result<(), String> {
        let [x, y, w, h] = spec.clip;
        unsafe {
            let yy = y as f64;
            #[cfg(target_os = "macos")]
            let yy = {
                let parent: *mut AnyObject = msg_send![&*self.view, superview];
                let frame: NSRect = msg_send![parent, bounds];
                let flipped: bool = msg_send![parent, isFlipped];
                if flipped {
                    yy
                } else {
                    frame.size.height - yy - h as f64
                }
            };
            let _: () = msg_send![&*self.view, setFrame: NSRect::new(NSPoint::new(x as f64, yy), NSSize::new(w.max(1.0) as f64,h.max(1.0) as f64))];
        }
        let [bx, by, bw, bh] = spec.bounds;
        #[cfg(target_os = "macos")]
        browser
            .set_bounds(wry::Rect {
                position: wry::dpi::LogicalPosition::new((bx - x) as f64, (by - y) as f64).into(),
                size: wry::dpi::LogicalSize::new(bw as f64, bh as f64).into(),
            })
            .map_err(|e| e.to_string())?;
        #[cfg(target_os = "ios")]
        unsafe {
            use wry::WebViewExtIOS;
            let view = browser.webview();
            let _: () = msg_send![&*view, setAutoresizingMask: 0usize];
            let _: () = msg_send![&*view, setFrame: NSRect::new(NSPoint::new((bx-x) as f64,(by-y) as f64), NSSize::new(bw as f64,bh as f64))];
        }
        Ok(())
    }
}
impl HasWindowHandle for ClipView {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        Ok(unsafe { WindowHandle::borrow_raw(self.raw) })
    }
}
impl Drop for ClipView {
    fn drop(&mut self) {
        unsafe {
            let _: () = msg_send![&*self.view, removeFromSuperview];
        }
    }
}
