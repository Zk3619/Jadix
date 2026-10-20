use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use webview2_com::{
    AcceleratorKeyPressedEventHandler, CapturePreviewCompletedHandler,
    CreateCoreWebView2CompositionControllerCompletedHandler,
    CreateCoreWebView2EnvironmentCompletedHandler, Microsoft::Web::WebView2::Win32::*,
    NavigationCompletedEventHandler, NewWindowRequestedEventHandler,
};
use windows::core::{Interface, PCWSTR, PWSTR};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice, IDCompositionDevice, IDCompositionTarget, IDCompositionVisual,
};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::System::Com::{IStream, STREAM_SEEK_END, STREAM_SEEK_SET};
use windows::Win32::UI::Shell::SHCreateMemStream;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

use super::{collect_specs, webview_status, WebViewSpec, WebViewStatus};
use crate::core::tree::Element;

struct Browser {
    composition: ICoreWebView2CompositionController,
    controller: ICoreWebView2Controller,
    webview: ICoreWebView2,
    visual: IDCompositionVisual,
}

impl Drop for Browser {
    fn drop(&mut self) {
        unsafe {
            let _ = self.controller.Close();
        }
    }
}

struct Entry {
    browser: RefCell<Option<Browser>>,
    source: RefCell<(String, Option<String>)>,
    spec: RefCell<WebViewSpec>,
    attached: Cell<bool>,
    covered: Cell<bool>,
    capture_pending: Cell<bool>,
    snapshot: crate::core::reactivity::Signal<bool>,
}

struct ComApartment;

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

pub(crate) struct NativeWebViews {
    entries: HashMap<String, Rc<Entry>>,
    root: IDCompositionVisual,
    gpu_visual: IDCompositionVisual,
    _target: IDCompositionTarget,
    device: IDCompositionDevice,
    hwnd: HWND,
    hole_pipeline: Option<wgpu::RenderPipeline>,
    scale: f32,
    pointer: [f32; 2],
    captured: Option<String>,
    hovered: Option<String>,
    mouse_keys: u32,
    _apartment: ComApartment,
}

impl Drop for NativeWebViews {
    fn drop(&mut self) {
        self.entries.clear();
    }
}

impl NativeWebViews {
    pub(crate) fn new(window: &dyn Window) -> windows::core::Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        }
        let apartment = ComApartment;
        let handle = window
            .window_handle()
            .map_err(|_| windows::core::Error::from_thread())?;
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            unreachable!()
        };
        let hwnd = HWND(handle.hwnd.get() as *mut _);
        let device: IDCompositionDevice = unsafe { DCompositionCreateDevice(None)? };
        let target = unsafe { device.CreateTargetForHwnd(hwnd, true)? };
        let root = unsafe { device.CreateVisual()? };
        let gpu_visual = unsafe { device.CreateVisual()? };
        unsafe {
            root.AddVisual(&gpu_visual, true, None)?;
            target.SetRoot(&root)?;
            device.Commit()?;
        }
        Ok(Self {
            entries: HashMap::new(),
            root,
            gpu_visual,
            _target: target,
            device,
            hwnd,
            hole_pipeline: None,
            scale: 1.0,
            pointer: [-1.0; 2],
            captured: None,
            hovered: None,
            mouse_keys: 0,
            _apartment: apartment,
        })
    }

    pub(crate) fn visual_ptr(&self) -> *mut std::ffi::c_void {
        self.gpu_visual.as_raw()
    }

    pub(crate) fn commit(&self) {
        unsafe {
            let _ = self.device.Commit();
        }
    }

    pub(crate) fn configure(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("WebView composition aperture"),
            source: wgpu::ShaderSource::Wgsl(r#"
                @vertex fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
                    let points = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
                    return vec4(points[index], 0.0, 1.0);
                }
                @fragment fn fs() -> @location(0) vec4<f32> { return vec4(0.0); }
            "#.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        self.hole_pipeline = Some(
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("WebView composition aperture"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            }),
        );
    }

    pub(crate) fn sync(
        &mut self,
        window: &Arc<dyn Window>,
        root: &Element,
        scale: f32,
        viewport: [f32; 4],
    ) {
        self.scale = scale;
        let specs = collect_specs(root, viewport);
        self.entries.retain(|id, entry| {
            let keep = specs.iter().any(|spec| &spec.id == id);
            if !keep && entry.attached.get() {
                if let Some(browser) = entry.browser.borrow().as_ref() {
                    unsafe {
                        let _ = self.root.RemoveVisual(&browser.visual);
                    }
                }
            }
            keep
        });
        for spec in specs {
            if !self.entries.contains_key(&spec.id) && spec.visible {
                let entry = Rc::new(Entry {
                    browser: RefCell::new(None),
                    source: RefCell::new((spec.url.clone(), spec.html.clone())),
                    spec: RefCell::new(spec.clone()),
                    attached: Cell::new(false),
                    covered: Cell::new(false),
                    capture_pending: Cell::new(false),
                    snapshot: super::webview_snapshot(&spec.id),
                });
                self.entries.insert(spec.id.clone(), entry.clone());
                if let Err(error) = self.create_browser(window, &entry) {
                    webview_status(&spec.id).try_set(WebViewStatus::Failed(error.to_string()));
                }
            }
            if let Some(entry) = self.entries.get(&spec.id) {
                *entry.spec.borrow_mut() = spec.clone();
                if let Some(browser) = entry.browser.borrow().as_ref() {
                    let covered = spec.visible
                        && (crate::app::portal_overlay::top_stacked_panel_z().is_some()
                            || crate::utils::drag::is_active()
                            || crate::components::resizable::is_resizable_dragging());
                    if !covered {
                        entry.snapshot.try_set(false);
                    }
                    let was_covered = entry.covered.replace(covered);
                    if covered && !was_covered && !entry.capture_pending.get() {
                        if let Err(error) = capture_snapshot(entry, browser, window) {
                            eprintln!("WebView snapshot: {error}");
                        }
                    }
                    if let Err(error) = self.update_browser(entry, browser, &spec) {
                        webview_status(&spec.id).try_set(WebViewStatus::Failed(error.to_string()));
                    }
                }
            }
        }
        self.commit();
    }

    fn create_browser(
        &self,
        window: &Arc<dyn Window>,
        entry: &Rc<Entry>,
    ) -> windows::core::Result<()> {
        let weak = Rc::downgrade(entry);
        let hwnd = self.hwnd;
        let device = self.device.clone();
        let window = window.clone();
        let scope = crate::app::window_scope::active();
        let handler = CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(
            move |result, environment| {
                let Some(entry) = weak.upgrade() else {
                    return Ok(());
                };
                let status = webview_status(&entry.spec.borrow().id);
                let result = result.and_then(|_| environment.ok_or_else(windows::core::Error::from_thread)).and_then(|environment| {
                let environment: ICoreWebView2Environment3 = environment.cast()?;
                let weak = Rc::downgrade(&entry);
                let window = window.clone();
                let handler = CreateCoreWebView2CompositionControllerCompletedHandler::create(Box::new(move |result, composition| {
                    let Some(entry) = weak.upgrade() else {
                        if let Some(composition) = composition { unsafe { let _ = composition.cast::<ICoreWebView2Controller>()?.Close(); } }
                        return Ok(());
                    };
                    let created = result.and_then(|_| composition.ok_or_else(windows::core::Error::from_thread)).and_then(|composition| unsafe {
                        let controller: ICoreWebView2Controller = composition.cast()?;
                        let controller3: ICoreWebView2Controller3 = controller.cast()?;
                        controller3.SetBoundsMode(COREWEBVIEW2_BOUNDS_MODE_USE_RAW_PIXELS)?;
                        let webview = controller.CoreWebView2()?;
                        let visual = device.CreateVisual()?;
                        composition.SetRootVisualTarget(&visual)?;
                        controller.SetIsVisible(false)?;
                        let settings = webview.Settings()?;
                        settings.SetIsWebMessageEnabled(false)?;
                        let mut token = 0;
                        webview.add_NewWindowRequested(&NewWindowRequestedEventHandler::create(Box::new(move |sender, args| {
                            if let Some(args) = args {
                                args.SetHandled(true)?;
                                let mut uri = PWSTR::null();
                                args.Uri(&mut uri)?;
                                let uri = webview2_com::take_pwstr(uri);
                                if let Some(sender) = sender {
                                    if !uri.is_empty() && uri != "about:blank" {
                                        sender.Navigate(PCWSTR(wide(&uri).as_ptr()))?;
                                    }
                                }
                            }
                            Ok(())
                        })), &mut token)?;
                        let status = webview_status(&entry.spec.borrow().id);
                        let ready_window = window.clone();
                        webview.add_NavigationCompleted(&NavigationCompletedEventHandler::create(Box::new(move |_, args| {
                            let mut success = windows::core::BOOL(0);
                            if let Some(args) = args { args.IsSuccess(&mut success)?; }
                            status.try_set(if success.as_bool() { WebViewStatus::Ready } else { WebViewStatus::Failed("Website navigation failed".into()) });
                            ready_window.request_redraw();
                            Ok(())
                        })), &mut token)?;
                        controller.add_AcceleratorKeyPressed(&AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
                            if let Some(args) = args {
                                let mut key = 0;
                                let mut kind = COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN;
                                args.VirtualKey(&mut key)?;
                                args.KeyEventKind(&mut kind)?;
                                if key == 27 && kind == COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN {
                                    let _scope = crate::app::window_scope::enter(scope);
                                    if crate::app::portal_overlay::try_close_top() || crate::app::navigator::try_pop_active() {
                                        args.SetHandled(true)?;
                                    }
                                }
                            }
                            Ok(())
                        })), &mut token)?;
                        let spec = entry.spec.borrow();
                        if let Some(html) = &spec.html { webview.NavigateToString(PCWSTR(wide(html).as_ptr()))?; }
                        else { webview.Navigate(PCWSTR(wide(&spec.url).as_ptr()))?; }
                        Ok(Browser { composition, controller, webview, visual })
                    });
                    match created {
                        Ok(browser) => *entry.browser.borrow_mut() = Some(browser),
                        Err(error) => { webview_status(&entry.spec.borrow().id).try_set(WebViewStatus::Failed(error.to_string())); }
                    }
                    window.request_redraw();
                    Ok(())
                }));
                unsafe { environment.CreateCoreWebView2CompositionController(hwnd, &handler) }
            });
                if let Err(error) = result {
                    status.try_set(WebViewStatus::Failed(error.to_string()));
                    window.request_redraw();
                }
                Ok(())
            },
        ));
        let user_data = std::env::var_os("LOCALAPPDATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("Jadix/WebView2");
        unsafe {
            CreateCoreWebView2EnvironmentWithOptions(
                PCWSTR::null(),
                PCWSTR(wide(&user_data.to_string_lossy()).as_ptr()),
                None,
                &handler,
            )
        }
    }

    fn update_browser(
        &self,
        entry: &Entry,
        browser: &Browser,
        spec: &WebViewSpec,
    ) -> windows::core::Result<()> {
        unsafe {
            let source = (spec.url.clone(), spec.html.clone());
            if *entry.source.borrow() != source {
                if let Some(html) = &spec.html {
                    browser
                        .webview
                        .NavigateToString(PCWSTR(wide(html).as_ptr()))?;
                } else {
                    browser.webview.Navigate(PCWSTR(wide(&spec.url).as_ptr()))?;
                }
                *entry.source.borrow_mut() = source;
                webview_status(&spec.id).try_set(WebViewStatus::Loading);
            }
            if !entry.attached.replace(true) {
                self.root
                    .AddVisual(&browser.visual, false, &self.gpu_visual)?;
            }
            let [x, y, width, height] = spec.bounds.map(|value| value * self.scale);
            let clip = spec.clip.map(|value| value * self.scale);
            // Bounds carries the host-client origin too: native tooltips, status bubbles
            // and context menus use it when converting WebView coordinates to screen.
            // The visual and the controller both need the origin: one places the
            // pixels, the other positions native browser UI relative to the HWND.
            browser.controller.SetBounds(RECT {
                left: x.round() as i32,
                top: y.round() as i32,
                right: (x + width).round() as i32,
                bottom: (y + height).round() as i32,
            })?;
            browser.visual.SetOffsetX2(x)?;
            browser.visual.SetOffsetY2(y)?;
            browser.visual.SetClip2(&D2D_RECT_F {
                left: clip[0] - x,
                top: clip[1] - y,
                right: clip[0] + clip[2] - x,
                bottom: clip[1] + clip[3] - y,
            })?;
            browser
                .controller
                .SetIsVisible(spec.visible && !entry.snapshot.peek())?;
        }
        Ok(())
    }

    pub(crate) fn has_visible(&self) -> bool {
        self.entries
            .values()
            .any(|entry| entry.spec.borrow().visible && entry.browser.borrow().is_some())
    }

    pub(crate) fn clear_apertures<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        width: u32,
        height: u32,
    ) {
        let Some(pipeline) = self.hole_pipeline.as_ref() else {
            return;
        };
        pass.set_pipeline(pipeline);
        for entry in self.entries.values() {
            let spec = entry.spec.borrow();
            if !spec.visible || entry.snapshot.peek() || entry.browser.borrow().is_none() {
                continue;
            }
            let [x, y, clip_width, clip_height] = spec.clip.map(|value| value * self.scale);
            let left = x.floor().max(0.0) as u32;
            let top = y.floor().max(0.0) as u32;
            let right = (x + clip_width).ceil().min(width as f32) as u32;
            let bottom = (y + clip_height).ceil().min(height as f32) as u32;
            if right > left && bottom > top {
                pass.set_scissor_rect(left, top, right - left, bottom - top);
                pass.draw(0..3, 0..1);
            }
        }
        pass.set_scissor_rect(0, 0, width, height);
    }

    pub(crate) fn handle_event(&mut self, event: &WindowEvent) -> bool {
        if let WindowEvent::PointerMoved { position, .. }
        | WindowEvent::PointerButton { position, .. } = event
        {
            self.pointer = [position.x as f32, position.y as f32];
        }
        if matches!(
            event,
            WindowEvent::Moved(_) | WindowEvent::SurfaceResized(_)
        ) {
            for entry in self.entries.values() {
                if let Some(browser) = entry.browser.borrow().as_ref() {
                    unsafe {
                        let _ = browser.controller.NotifyParentWindowPositionChanged();
                    }
                }
            }
        }
        let blocked = blocked();
        let target = self.captured.clone().or_else(|| {
            self.entries.iter().find_map(|(id, entry)| {
                let spec = entry.spec.borrow();
                let rect = spec.clip.map(|value| value * self.scale);
                (spec.visible
                    && self.pointer[0] >= rect[0]
                    && self.pointer[1] >= rect[1]
                    && self.pointer[0] < rect[0] + rect[2]
                    && self.pointer[1] < rect[1] + rect[3])
                    .then(|| id.clone())
            })
        });
        let target = if blocked || matches!(event, WindowEvent::PointerLeft { .. }) {
            None
        } else {
            target
        };
        if self.hovered != target {
            if let Some(old) = self.hovered.take().and_then(|id| self.entries.get(&id)) {
                if let Some(browser) = old.browser.borrow().as_ref() {
                    unsafe {
                        let _ = browser.composition.SendMouseInput(
                            COREWEBVIEW2_MOUSE_EVENT_KIND_LEAVE,
                            COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS(0),
                            0,
                            POINT::default(),
                        );
                    }
                }
            }
            self.hovered = target.clone();
        }
        if blocked || target.is_none() {
            if matches!(
                event,
                WindowEvent::PointerButton {
                    state: ElementState::Pressed,
                    ..
                }
            ) {
                unsafe {
                    windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(self.hwnd.0);
                }
            }
            return false;
        }
        let target = target.unwrap();
        let entry = &self.entries[&target];
        let browser = entry.browser.borrow();
        let Some(browser) = browser.as_ref() else {
            return false;
        };
        let mut data = 0;
        let kind = match event {
            WindowEvent::PointerMoved { .. } => COREWEBVIEW2_MOUSE_EVENT_KIND_MOVE,
            WindowEvent::PointerButton { state, button, .. } => {
                let Some(button) = button.clone().mouse_button() else {
                    return false;
                };
                let pressed = *state == ElementState::Pressed;
                let (key, down, up) = match button {
                    MouseButton::Left => (
                        1,
                        COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_DOWN,
                        COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_UP,
                    ),
                    MouseButton::Right => (
                        2,
                        COREWEBVIEW2_MOUSE_EVENT_KIND_RIGHT_BUTTON_DOWN,
                        COREWEBVIEW2_MOUSE_EVENT_KIND_RIGHT_BUTTON_UP,
                    ),
                    MouseButton::Middle => (
                        16,
                        COREWEBVIEW2_MOUSE_EVENT_KIND_MIDDLE_BUTTON_DOWN,
                        COREWEBVIEW2_MOUSE_EVENT_KIND_MIDDLE_BUTTON_UP,
                    ),
                    _ => return false,
                };
                if pressed {
                    self.mouse_keys |= key;
                    self.captured = Some(target);
                    unsafe {
                        let _ = browser
                            .controller
                            .MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC);
                        windows_sys::Win32::UI::Input::KeyboardAndMouse::SetCapture(self.hwnd.0);
                    }
                    down
                } else {
                    self.mouse_keys &= !key;
                    if self.mouse_keys == 0 {
                        self.captured = None;
                        unsafe {
                            windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
                        }
                    }
                    up
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (horizontal, vertical) = match delta {
                    MouseScrollDelta::LineDelta(horizontal, vertical) => {
                        (*horizontal * 120.0, *vertical * 120.0)
                    }
                    MouseScrollDelta::PixelDelta(position) => {
                        (position.x as f32, position.y as f32)
                    }
                    _ => return false,
                };
                if vertical.abs() >= horizontal.abs() {
                    data = vertical as i32 as u32;
                    COREWEBVIEW2_MOUSE_EVENT_KIND_WHEEL
                } else {
                    data = horizontal as i32 as u32;
                    COREWEBVIEW2_MOUSE_EVENT_KIND_HORIZONTAL_WHEEL
                }
            }
            _ => return false,
        };
        let spec = entry.spec.borrow();
        let point = POINT {
            x: (self.pointer[0] - spec.bounds[0] * self.scale).round() as i32,
            y: (self.pointer[1] - spec.bounds[1] * self.scale).round() as i32,
        };
        unsafe {
            let _ = browser.composition.SendMouseInput(
                kind,
                COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS(self.mouse_keys as i32),
                data,
                point,
            );
        }
        !matches!(event, WindowEvent::PointerMoved { .. })
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn blocked() -> bool {
    crate::app::portal_overlay::top_stacked_panel_z().is_some()
        || crate::utils::drag::is_dragging()
        || crate::components::resizable::is_resizable_dragging()
}

fn read_snapshot(stream: &IStream) -> windows::core::Result<Vec<u8>> {
    unsafe {
        let mut length = 0;
        stream.Seek(0, STREAM_SEEK_END, Some(&mut length))?;
        if length == 0 || length > 64 * 1024 * 1024 {
            return Err(windows::core::Error::from_hresult(
                windows::Win32::Foundation::E_FAIL,
            ));
        }
        stream.Seek(0, STREAM_SEEK_SET, None)?;
        let mut bytes = vec![0; length as usize];
        let mut read = 0;
        stream
            .Read(
                bytes.as_mut_ptr().cast(),
                bytes.len() as u32,
                Some(&mut read),
            )
            .ok()?;
        if read as usize != bytes.len() {
            return Err(windows::core::Error::from_hresult(
                windows::Win32::Foundation::E_FAIL,
            ));
        }
        Ok(bytes)
    }
}

fn capture_snapshot(
    entry: &Rc<Entry>,
    browser: &Browser,
    window: &Arc<dyn Window>,
) -> windows::core::Result<()> {
    let stream =
        unsafe { SHCreateMemStream(None) }.ok_or_else(windows::core::Error::from_thread)?;
    let captured_stream = stream.clone();
    let weak = Rc::downgrade(entry);
    let window = window.clone();
    entry.capture_pending.set(true);
    let result = unsafe {
        browser.webview.CapturePreview(
            COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG,
            &stream,
            &CapturePreviewCompletedHandler::create(Box::new(move |result| {
                let Some(entry) = weak.upgrade() else {
                    return Ok(());
                };
                entry.capture_pending.set(false);
                result?;
                if !entry.covered.get() {
                    return Ok(());
                }
                let bytes = read_snapshot(&captured_stream)?;
                if let Ok(image) = image::load_from_memory(&bytes) {
                    let image = image.into_rgba8();
                    crate::queue_image_rgba8(
                        format!("{}__webview_snapshot_texture", entry.spec.borrow().id),
                        image.width(),
                        image.height(),
                        image.into_raw(),
                    );
                    entry.snapshot.try_set(true);
                    window.request_redraw();
                }
                Ok(())
            })),
        )
    };
    if result.is_err() {
        entry.capture_pending.set(false);
    }
    result
}
