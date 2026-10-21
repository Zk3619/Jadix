//! Windows.UI.Composition host: backdrop recipe + wgpu composition surface.

use std::cell::RefCell;
use std::collections::HashMap;

use windows::Foundation::TimeSpan;
use windows::System::{DispatcherQueue, DispatcherQueueController};
use windows::UI::Color;
use windows::UI::Composition::Desktop::DesktopWindowTarget;
use windows::UI::Composition::{
    CompositionBrush, CompositionColorBrush, CompositionEasingFunction, Compositor,
    ContainerVisual, SpriteVisual,
};
use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::Graphics::DirectComposition::DCompositionCreateSurfaceHandle;
use windows::Win32::System::WinRT::Composition::{ICompositorDesktopInterop, ICompositorInterop};
use windows::Win32::System::WinRT::{
    CreateDispatcherQueueController, DQTAT_COM_NONE, DQTYPE_THREAD_CURRENT, DispatcherQueueOptions,
};
use windows::core::{HSTRING, Interface};
use windows_numerics::{Vector2, Vector3};

use crate::app::WindowBackdrop;
use crate::app::WindowOptions;
use crate::app::glass_params::aero_overlay;

use super::recipes::{create_recipe_brush, gaussian_host_brush};

thread_local! {
    static HOSTS: RefCell<HashMap<isize, DcompGlassHost>> = RefCell::new(HashMap::new());
}

static DISPATCHER: std::sync::OnceLock<DispatcherQueueController> = std::sync::OnceLock::new();

pub struct DcompGlassHost {
    _hwnd: isize,
    _target: DesktopWindowTarget,
    compositor: Compositor,
    _root: ContainerVisual,
    backdrop: SpriteVisual,
    tint_overlay: Option<SpriteVisual>,
    tint_brush: Option<CompositionColorBrush>,
    afterglow_overlay: Option<SpriteVisual>,
    afterglow_brush: Option<CompositionColorBrush>,
    reflection: Option<SpriteVisual>,
    _content: SpriteVisual,
    surface_handle: HANDLE,
    options: WindowOptions,
    active: bool,
    os_dark: bool,
    dark: bool,
    current_brush: Option<CompositionBrush>,
}

impl DcompGlassHost {
    fn surface_ptr(&self) -> *mut std::ffi::c_void {
        self.surface_handle.0
    }
}

impl Drop for DcompGlassHost {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.surface_handle);
        }
    }
}

fn glass_fail(msg: &str) {
    log::warn!("{msg}");
    eprintln!("jadix glass: {msg}");
}

fn ensure_dispatcher_queue() -> windows::core::Result<()> {
    if DispatcherQueue::GetForCurrentThread().is_ok() {
        return Ok(());
    }
    unsafe {
        let _ = windows::Win32::System::WinRT::RoInitialize(
            windows::Win32::System::WinRT::RO_INIT_SINGLETHREADED,
        );
    }
    if DispatcherQueue::GetForCurrentThread().is_ok() {
        return Ok(());
    }
    let opts = DispatcherQueueOptions {
        dwSize: std::mem::size_of::<DispatcherQueueOptions>() as u32,
        threadType: DQTYPE_THREAD_CURRENT,
        // Don't re-init COM: winit already OleInitialize'd this thread.
        apartmentType: DQTAT_COM_NONE,
    };
    match unsafe { CreateDispatcherQueueController(opts) } {
        Ok(c) => {
            let _ = DISPATCHER.set(c);
            Ok(())
        }
        Err(e) => {
            if DispatcherQueue::GetForCurrentThread().is_ok() {
                Ok(())
            } else {
                glass_fail(&format!("CreateDispatcherQueueController: {e}"));
                Err(e)
            }
        }
    }
}

#[cfg(test)]
#[test]
#[ignore = "requires Windows system composition effects; run explicitly with --ignored"]
fn theme_window_recipes_use_supported_sdk_effects_without_fallback() {
    ensure_dispatcher_queue().unwrap();
    let previous = crate::style::get_theme();
    let compositor = Compositor::new().unwrap();
    for preset in [
        crate::style::ThemePreset::Fluent,
        crate::style::ThemePreset::LiquidGlass,
    ] {
        crate::style::set_theme_preset(preset);
        for mode in [
            crate::style::ThemeMode::Light,
            crate::style::ThemeMode::Dark,
        ] {
            crate::style::set_theme_mode(mode);
            let options = WindowOptions::new().follow_theme(true).themed();
            create_recipe_brush(&compositor, &options, true, crate::style::is_dark_theme())
                .unwrap();
        }
    }
    crate::style::set_theme(previous);
}

fn wash_color(rgb: [u8; 3], opacity: f32) -> Color {
    Color {
        A: (opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
        R: rgb[0],
        G: rgb[1],
        B: rgb[2],
    }
}

fn make_color_overlay(
    compositor: &Compositor,
) -> windows::core::Result<(SpriteVisual, CompositionColorBrush)> {
    let vis = compositor.CreateSpriteVisual()?;
    vis.SetRelativeSizeAdjustment(Vector2 { X: 1.0, Y: 1.0 })?;
    vis.SetIsHitTestVisible(false)?;
    let brush = compositor.CreateColorBrushWithColor(Color {
        A: 0,
        R: 0,
        G: 0,
        B: 0,
    })?;
    vis.SetBrush(&brush)?;
    vis.SetOpacity(1.0)?;
    Ok((vis, brush))
}

fn stop_opacity_anim(visual: &SpriteVisual) {
    let _ = visual.StopAnimation(&HSTRING::from("Opacity"));
}

fn animate_opacity(compositor: &Compositor, visual: &SpriteVisual, to: f32) {
    let from = visual.Opacity().unwrap_or(to);
    let Ok(anim) = compositor.CreateScalarKeyFrameAnimation() else {
        let _ = visual.SetOpacity(to);
        return;
    };
    let _ = anim.InsertKeyFrame(0.0, from);
    if let Ok(bezier) = compositor
        .CreateCubicBezierEasingFunction(Vector2 { X: 0.5, Y: 0.0 }, Vector2 { X: 0.0, Y: 0.9 })
    {
        if let Ok(easing) = bezier.cast::<CompositionEasingFunction>() {
            let _ = anim.InsertKeyFrameWithEasingFunction(1.0, to, &easing);
        } else {
            let _ = anim.InsertKeyFrame(1.0, to);
        }
    } else {
        let _ = anim.InsertKeyFrame(1.0, to);
    }
    let _ = anim.SetDuration(TimeSpan { Duration: 220_000 });
    let _ = visual.StartAnimation(&HSTRING::from("Opacity"), &anim);
}

fn apply_aero_overlays(host: &DcompGlassHost, animate: bool) {
    let is_aero = host.options.blur && host.options.backdrop == WindowBackdrop::Aero;
    let layer = if is_aero {
        let tint = host.options.resolved_glass_tint_for(host.dark);
        let glow = host.options.resolved_aero_afterglow_for(host.dark);
        aero_overlay(
            tint,
            glow,
            host.options.resolved_aero_color_balance(),
            host.options.resolved_aero_afterglow_balance(),
            host.options.resolved_aero_reflection(),
            host.active,
            host.dark,
        )
    } else {
        let _ = host.tint_overlay.as_ref().map(|v| v.SetIsVisible(false));
        let _ = host
            .afterglow_overlay
            .as_ref()
            .map(|v| v.SetIsVisible(false));
        return;
    };
    if let Some(b) = host.tint_brush.as_ref() {
        let _ = b.SetColor(wash_color(layer.tint_rgb, layer.tint_opacity));
    }
    if let Some(b) = host.afterglow_brush.as_ref() {
        let _ = b.SetColor(wash_color(layer.glow_rgb, layer.glow_opacity));
    }
    if let Some(v) = host.tint_overlay.as_ref() {
        let _ = v.SetIsVisible(true);
        stop_opacity_anim(v);
        if animate {
            animate_opacity(&host.compositor, v, 1.0);
        } else {
            let _ = v.SetOpacity(1.0);
        }
    }
    if let Some(v) = host.afterglow_overlay.as_ref() {
        let visible = layer.glow_opacity > 0.01;
        let _ = v.SetIsVisible(visible);
        if visible {
            stop_opacity_anim(v);
            let _ = v.SetOpacity(1.0);
        }
    }
    if let Some(v) = host.reflection.as_ref() {
        let _ = v.SetIsVisible(layer.reflection > 0.001);
        stop_opacity_anim(v);
        if animate {
            animate_opacity(&host.compositor, v, layer.reflection);
        } else {
            let _ = v.SetOpacity(layer.reflection);
        }
    }
}

fn make_reflection_brush(
    compositor: &Compositor,
    intensity: f32,
) -> windows::core::Result<CompositionBrush> {
    let g = compositor.CreateRadialGradientBrush()?;
    // Light sits above the window so iso-contours are nearly horizontal (a top wash),
    // not a diagonal linear wedge in the corner.
    g.SetEllipseCenter(Vector2 { X: 0.5, Y: -0.2 })?;
    g.SetEllipseRadius(Vector2 { X: 1.55, Y: 0.95 })?;
    let peak = (intensity.clamp(0.0, 1.0) * 22.0).round() as u8;
    let stops = g.ColorStops()?;
    let mk = |off: f32, a: u8| {
        compositor.CreateColorGradientStopWithOffsetAndColor(
            off,
            Color {
                A: a,
                R: 255,
                G: 255,
                B: 255,
            },
        )
    };
    stops.Append(&mk(0.0, 0)?)?;
    stops.Append(&mk(0.52, peak)?)?;
    stops.Append(&mk(0.78, peak / 4)?)?;
    stops.Append(&mk(1.0, 0)?)?;
    Interface::cast(&g)
}

fn update_reflection(host: &DcompGlassHost) {
    let Some(visual) = host.reflection.as_ref() else {
        return;
    };
    if host.options.backdrop != WindowBackdrop::Aero
        || host.options.resolved_aero_reflection() <= 0.001
    {
        let _ = visual.SetIsVisible(false);
        return;
    }
    let _ = visual.SetOffset(Vector3 {
        X: 0.0,
        Y: 0.0,
        Z: 0.0,
    });
}

fn apply_brush(host: &mut DcompGlassHost, new_brush: CompositionBrush) {
    let _ = host.backdrop.SetBrush(&new_brush);
    host.current_brush = Some(new_brush);
}

fn recipe_or_fallback(
    compositor: &Compositor,
    options: &WindowOptions,
    active: bool,
    dark: bool,
) -> windows::core::Result<CompositionBrush> {
    let themed = options.themed();
    let options = &themed;
    if !options.blur {
        return Interface::cast(&compositor.CreateColorBrushWithColor(Color {
            A: 0,
            R: 0,
            G: 0,
            B: 0,
        })?);
    }
    match create_recipe_brush(compositor, options, active, dark) {
        Ok(b) => Ok(b),
        Err(e) => {
            glass_fail(&format!(
                "recipe failed ({e}); using Gaussian(HostBackdrop)"
            ));
            gaussian_host_brush(compositor, options.resolved_blur_amount()).or_else(|_| {
                Interface::cast(
                    &compositor
                        .CreateHostBackdropBrush()
                        .or_else(|_| compositor.CreateBackdropBrush())?,
                )
            })
        }
    }
}

fn try_new(
    hwnd: isize,
    options: &WindowOptions,
    os_dark: bool,
) -> windows::core::Result<DcompGlassHost> {
    ensure_dispatcher_queue()?;
    let compositor = Compositor::new()?;
    let desktop: ICompositorDesktopInterop = Interface::cast(&compositor)?;
    let target = unsafe { desktop.CreateDesktopWindowTarget(HWND(hwnd as *mut _), false) }?;
    let handle = unsafe { DCompositionCreateSurfaceHandle(0x000F_0003, None) }?;
    let interop: ICompositorInterop = Interface::cast(&compositor)?;
    let surface = unsafe { interop.CreateCompositionSurfaceForHandle(handle) }?;
    let root = compositor.CreateContainerVisual()?;
    root.SetRelativeSizeAdjustment(Vector2 { X: 1.0, Y: 1.0 })?;
    if let Ok(clip) = compositor.CreateInsetClip() {
        let _ = root.SetClip(&clip);
    }
    let backdrop = compositor.CreateSpriteVisual()?;
    backdrop.SetRelativeSizeAdjustment(Vector2 { X: 1.0, Y: 1.0 })?;
    let dark = if options.follow_theme {
        crate::style::is_dark_theme()
    } else {
        os_dark || crate::style::is_dark_theme()
    };
    let mut current_brush = None;
    if options.blur || options.follow_theme {
        let brush = recipe_or_fallback(&compositor, options, true, dark)?;
        backdrop.SetBrush(&brush)?;
        current_brush = Some(brush);
    }
    let is_aero = options.blur && options.backdrop == WindowBackdrop::Aero;
    let (tint_overlay, tint_brush, afterglow_overlay, afterglow_brush) = if is_aero {
        let (tv, tb) = make_color_overlay(&compositor)?;
        let (gv, gb) = make_color_overlay(&compositor)?;
        (Some(tv), Some(tb), Some(gv), Some(gb))
    } else {
        (None, None, None, None)
    };
    let reflection = if is_aero && options.resolved_aero_reflection() > 0.001 {
        let vis = compositor.CreateSpriteVisual()?;
        vis.SetRelativeSizeAdjustment(Vector2 { X: 1.0, Y: 1.0 })?;
        if let Ok(b) = make_reflection_brush(&compositor, options.resolved_aero_reflection()) {
            vis.SetBrush(&b)?;
        }
        vis.SetIsHitTestVisible(false)?;
        Some(vis)
    } else {
        None
    };
    let content = compositor.CreateSpriteVisual()?;
    content.SetRelativeSizeAdjustment(Vector2 { X: 1.0, Y: 1.0 })?;
    let surface_brush = compositor.CreateSurfaceBrushWithSurface(&surface)?;
    content.SetBrush(&surface_brush)?;
    let children = root.Children()?;
    if options.blur || options.follow_theme {
        children.InsertAtBottom(&backdrop)?;
        if let Some(ref vis) = tint_overlay {
            children.InsertAtTop(vis)?;
        }
        if let Some(ref vis) = afterglow_overlay {
            children.InsertAtTop(vis)?;
        }
        if let Some(ref vis) = reflection {
            children.InsertAtTop(vis)?;
        }
    }
    children.InsertAtTop(&content)?;
    target.SetRoot(&root)?;
    let host = DcompGlassHost {
        _hwnd: hwnd,
        _target: target,
        compositor,
        _root: root,
        backdrop,
        tint_overlay,
        tint_brush,
        afterglow_overlay,
        afterglow_brush,
        reflection,
        _content: content,
        surface_handle: handle,
        options: options.clone(),
        active: true,
        os_dark,
        dark,
        current_brush,
    };
    apply_aero_overlays(&host, false);
    update_reflection(&host);
    Ok(host)
}

fn refresh_brush(host: &mut DcompGlassHost, animate: bool) {
    let is_aero = host.options.blur && host.options.backdrop == WindowBackdrop::Aero;
    if is_aero {
        apply_aero_overlays(host, animate);
        update_reflection(host);
        return;
    }
    match recipe_or_fallback(&host.compositor, &host.options, host.active, host.dark) {
        Ok(brush) => apply_brush(host, brush),
        Err(e) => glass_fail(&format!("dcomp recipe refresh: {e}")),
    }
    update_reflection(host);
}

fn set_host_dark(host: &mut DcompGlassHost, os_dark: Option<bool>, animate: bool) {
    if let Some(os) = os_dark {
        host.os_dark = os;
    }
    let dark = if host.options.follow_theme {
        crate::style::is_dark_theme()
    } else {
        host.os_dark || crate::style::is_dark_theme()
    };
    let changed = host.dark != dark;
    host.dark = dark;
    if changed {
        refresh_brush(host, animate);
    } else {
        apply_aero_overlays(host, animate);
    }
}

pub fn try_init(hwnd: isize, options: &WindowOptions, os_dark: bool) -> bool {
    HOSTS.with(|hosts| {
        if hosts.borrow().contains_key(&hwnd) {
            return true;
        }
        match try_new(hwnd, options, os_dark) {
            Ok(host) => {
                log::info!("dcomp glass host attached hwnd={hwnd:#x}");
                hosts.borrow_mut().insert(hwnd, host);
                true
            }
            Err(e) => {
                glass_fail(&format!("dcomp glass host failed: {e}"));
                false
            }
        }
    })
}

pub fn has_host(hwnd: isize) -> bool {
    HOSTS.with(|h| h.borrow().contains_key(&hwnd))
}

pub fn surface_handle(hwnd: isize) -> Option<*mut std::ffi::c_void> {
    HOSTS.with(|h| h.borrow().get(&hwnd).map(|host| host.surface_ptr()))
}

pub fn detach(hwnd: isize) {
    HOSTS.with(|h| {
        h.borrow_mut().remove(&hwnd);
    });
}

pub fn refresh(hwnd: isize) {
    HOSTS.with(|h| {
        if let Some(host) = h.borrow_mut().get_mut(&hwnd) {
            set_host_dark(host, None, false);
            refresh_brush(host, false);
        }
    });
}

pub fn set_active(hwnd: isize, active: bool) {
    HOSTS.with(|h| {
        let mut map = h.borrow_mut();
        let Some(host) = map.get_mut(&hwnd) else {
            return;
        };
        set_host_dark(host, None, false);
        if host.active == active {
            return;
        }
        host.active = active;
        refresh_brush(host, true);
    });
}

pub fn on_moved(hwnd: isize) {
    HOSTS.with(|h| {
        if let Some(host) = h.borrow().get(&hwnd) {
            update_reflection(host);
        }
    });
}

pub fn set_os_dark_all(os_dark: bool) {
    HOSTS.with(|h| {
        let mut map = h.borrow_mut();
        for host in map.values_mut() {
            set_host_dark(host, Some(os_dark), true);
        }
    });
}

pub fn sync_jadix_theme() {
    HOSTS.with(|h| {
        let mut map = h.borrow_mut();
        for host in map.values_mut() {
            set_host_dark(host, None, false);
            if host.options.follow_theme {
                refresh_brush(host, false);
            }
        }
    });
}
