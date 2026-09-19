//! Windows Imm / IMR / candidate HWND drive (WP-1.1).
//!
//! Policy (compose origin, preedit fold) lives in `crate::app::ime_support`.
//! This module only talks to Win32; field-switch fold is injected via callback.

use winit::window::Window;

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, Ordering};
use std::sync::Mutex;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
use windows_sys::Win32::UI::Input::Ime::{
    ImmGetContext, ImmGetDefaultIMEWnd, ImmNotifyIME, ImmReleaseContext, ImmSetCompositionWindow,
    CFS_POINT, COMPOSITIONFORM, CPS_CANCEL, IMC_SETCOMPOSITIONWINDOW, IMECHARPOSITION,
    IMR_QUERYCHARPOSITION, NI_COMPOSITIONSTR,
};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DestroyCaret, GetAncestor, GetClassNameW, GetClientRect, GetForegroundWindow, GetWindowRect,
    IsWindow, SendMessageW, SetWindowPos, WindowFromPoint, GA_ROOT, HWND_TOP, SWP_NOACTIVATE,
    SWP_NOSIZE, SWP_NOZORDER, WM_IME_CONTROL, WM_IME_REQUEST,
};

const SUBCLASS_ID: usize = 0x5255_4949; // 'RUII'

static CARET_X: AtomicI32 = AtomicI32::new(0);
static CARET_Y: AtomicI32 = AtomicI32::new(0);
static CARET_W: AtomicI32 = AtomicI32::new(1);
static CARET_H: AtomicI32 = AtomicI32::new(16);
/// Composition-string origin (client px) for `IMR_QUERYCHARPOSITION`.
static COMPOSE_ORIGIN_X: AtomicI32 = AtomicI32::new(0);
static SUBCLASSED_HWNDS: Mutex<Vec<isize>> = Mutex::new(Vec::new());
static LAST_SCROLL_ACC_X: AtomicI32 = AtomicI32::new(0);
static LAST_SCROLL_ACC_Y: AtomicI32 = AtomicI32::new(0);
static HAS_SCROLL_ACC: AtomicBool = AtomicBool::new(false);
/// Imm already fired once for the current composition (keystrokes do not re-Imm).
static COMPOSE_IMM_SENT: AtomicBool = AtomicBool::new(false);
/// After first Imm, IMR Y is frozen so scroll HWND ΔY is not doubled by TIP IMR deltas.
static IMR_FROZEN: AtomicBool = AtomicBool::new(false);
static LAST_ANCHOR_SCR_Y: AtomicI32 = AtomicI32::new(0);
static HAS_ANCHOR_SCR: AtomicBool = AtomicBool::new(false);
static LAST_IDLE_COMPOSE_X: AtomicI32 = AtomicI32::new(i32::MIN);
static LAST_IDLE_COMPOSE_Y: AtomicI32 = AtomicI32::new(i32::MIN);
static LAST_IDLE_COMPOSE_W: AtomicI32 = AtomicI32::new(0);
static LAST_IDLE_COMPOSE_H: AtomicI32 = AtomicI32::new(0);
static LAST_RELOCATE_LOG_MS: AtomicI32 = AtomicI32::new(0);
static TRACKED_IME_HWNDS: Mutex<Vec<isize>> = Mutex::new(Vec::new());
static LAST_IME_FOCUS_ID: Mutex<String> = Mutex::new(String::new());
static APP_HWND: AtomicIsize = AtomicIsize::new(0);

fn now_ms() -> i32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| (d.as_millis() % i32::MAX as u128) as i32)
        .unwrap_or(0)
}

fn hwnd_of(window: &dyn Window) -> Option<HWND> {
    super::window::hwnd(window)
}

unsafe fn is_app_foreground(app: HWND) -> bool {
    let fg = unsafe { GetForegroundWindow() };
    if fg.is_null() {
        return false;
    }
    if fg == app {
        return true;
    }
    let root = unsafe { GetAncestor(fg, GA_ROOT) };
    !root.is_null() && root == app
}

unsafe fn cancel_composition(hwnd: HWND) {
    let himc = unsafe { ImmGetContext(hwnd) };
    if himc.is_null() {
        return;
    }
    let _ = unsafe { ImmNotifyIME(himc, NI_COMPOSITIONSTR, CPS_CANCEL, 0) };
    let _ = unsafe { ImmReleaseContext(hwnd, himc) };
}

pub(crate) fn release_ui_control(window: &dyn Window, cancel_comp: bool) {
    clear_compose_session();
    let Some(hwnd) = hwnd_of(window) else {
        unsafe {
            let _ = DestroyCaret();
        }
        return;
    };
    unsafe {
        if cancel_comp {
            cancel_composition(hwnd);
        }
        let _ = DestroyCaret();
    }
}

fn clear_compose_session() {
    HAS_SCROLL_ACC.store(false, Ordering::Relaxed);
    COMPOSE_IMM_SENT.store(false, Ordering::Relaxed);
    IMR_FROZEN.store(false, Ordering::Relaxed);
    HAS_ANCHOR_SCR.store(false, Ordering::Relaxed);
    LAST_IDLE_COMPOSE_X.store(i32::MIN, Ordering::Relaxed);
    LAST_IDLE_COMPOSE_Y.store(i32::MIN, Ordering::Relaxed);
    if let Ok(mut v) = TRACKED_IME_HWNDS.lock() {
        v.clear();
    }
}

pub(crate) fn sync_focus_owner(
    window: &dyn Window,
    focus_id: Option<&str>,
    on_leave_field: impl FnOnce(&str),
) {
    let Ok(mut last) = LAST_IME_FOCUS_ID.lock() else {
        return;
    };
    let next = focus_id.unwrap_or("").to_string();
    if *last == next {
        return;
    }
    if !last.is_empty() {
        on_leave_field(&last);
        // finish_* already cancelled OS composition; do not CPS_CANCEL twice.
        release_ui_control(window, false);
        log::warn!(
            "RUI_IME_DIAG focus switch {} -> {} (fold preedit sans ', end compose)",
            last,
            if next.is_empty() {
                "(none)"
            } else {
                next.as_str()
            }
        );
    } else if next.is_empty() {
        release_ui_control(window, true);
    }
    *last = next;
}

pub(crate) fn end_composition_ui() {
    clear_compose_session();
}

pub(crate) fn cancel_active_composition() {
    let raw = APP_HWND.load(Ordering::Relaxed);
    if raw == 0 {
        end_composition_ui();
        return;
    }
    let hwnd = raw as HWND;
    unsafe {
        cancel_composition(hwnd);
        let _ = DestroyCaret();
    }
    end_composition_ui();
    log::warn!("RUI_IME_DIAG cancel_os_composition (clear / explicit)");
}

/// True when ancestor `scroll_acc` changed this frame.
fn begin_scroll_delta(scroll_acc: [f32; 2], _ui_scale: f32) -> bool {
    let sx100 = (scroll_acc[0] * 100.0).round() as i32;
    let sy100 = (scroll_acc[1] * 100.0).round() as i32;
    let prev_sx = LAST_SCROLL_ACC_X.load(Ordering::Relaxed);
    let prev_sy = LAST_SCROLL_ACC_Y.load(Ordering::Relaxed);
    let has_scroll = HAS_SCROLL_ACC.load(Ordering::Relaxed);
    LAST_SCROLL_ACC_X.store(sx100, Ordering::Relaxed);
    LAST_SCROLL_ACC_Y.store(sy100, Ordering::Relaxed);
    HAS_SCROLL_ACC.store(true, Ordering::Relaxed);
    has_scroll && (sx100 != prev_sx || sy100 != prev_sy)
}

fn store_compose_geom(compose_x: i32, y: i32, w: u32, h: u32) {
    let width = (w as i32).max(1);
    let height = (h as i32).max(1);
    // IMR X always composition origin — never the advancing insert caret.
    CARET_X.store(compose_x, Ordering::Relaxed);
    CARET_Y.store(y, Ordering::Relaxed);
    CARET_W.store(width, Ordering::Relaxed);
    CARET_H.store(height, Ordering::Relaxed);
    COMPOSE_ORIGIN_X.store(compose_x, Ordering::Relaxed);
}

fn apply_imm_compose_form(hwnd: HWND, compose_x: i32, y: i32, w: u32, h: u32) {
    let width = (w as i32).max(1);
    let height = (h as i32).max(1);
    let rc = RECT {
        left: compose_x,
        top: y,
        right: compose_x + width,
        bottom: y + height,
    };
    // CFS_POINT only (same as winit) — no CFS_FORCE_POSITION fighting the TIP.
    let composition_point = COMPOSITIONFORM {
        dwStyle: CFS_POINT,
        ptCurrentPos: POINT {
            x: compose_x,
            y: y + height,
        },
        rcArea: rc,
    };
    unsafe {
        let ime_wnd = ImmGetDefaultIMEWnd(hwnd);
        let himc = ImmGetContext(hwnd);
        if !himc.is_null() {
            let _ = ImmSetCompositionWindow(himc, &composition_point);
            let _ = ImmReleaseContext(hwnd, himc);
        }
        if !ime_wnd.is_null() {
            let _ = SendMessageW(
                ime_wnd,
                WM_IME_CONTROL,
                IMC_SETCOMPOSITIONWINDOW as WPARAM,
                &composition_point as *const COMPOSITIONFORM as LPARAM,
            );
        }
        let _ = DestroyCaret();
    }
}

fn compose_anchor_screen(hwnd: HWND, compose_x: i32, y: i32, h: u32) -> POINT {
    let mut screen = POINT {
        x: compose_x,
        y: y + (h as i32).max(1),
    };
    unsafe {
        let _ = ClientToScreen(hwnd, &mut screen);
    }
    screen
}

fn looks_like_ime_class(class: &str) -> bool {
    let c = class.to_ascii_lowercase();
    c.contains("ime")
        || c.contains("candidate")
        || c.contains("cicero")
        || c.contains("msctf")
        || c.contains("ctfmon")
        || c.contains("sopy")
        || c.contains("sogou")
        || c.contains("qqpinyin")
        || c.contains("textinput")
        || c.contains("applicationframe")
        || c.contains("corewindow")
}

fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if n <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..n as usize])
}

/// Latch nearby IME candidate HWNDs once (scroll-follow only; never resize / never pin X).
unsafe fn latch_candidate_near(app: HWND, ax: i32, ay: i32) {
    let Ok(mut tracked) = TRACKED_IME_HWNDS.lock() else {
        return;
    };
    if !tracked.is_empty() {
        return;
    }
    let probes = [
        (ax, ay),
        (ax, ay + 12),
        (ax, ay + 28),
        (ax + 80, ay + 16),
        (ax + 160, ay + 16),
    ];
    for (px, py) in probes {
        let hit = unsafe { WindowFromPoint(POINT { x: px, y: py }) };
        if hit.is_null() || hit == app {
            continue;
        }
        let root = unsafe { GetAncestor(hit, GA_ROOT) };
        let target = if root.is_null() { hit } else { root };
        if target == app || unsafe { IsWindow(target) } == 0 {
            continue;
        }
        let class = class_name(hit);
        let root_class = class_name(target);
        if !(looks_like_ime_class(&class) || looks_like_ime_class(&root_class)) {
            continue;
        }
        let mut rc = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if unsafe { GetWindowRect(target, &mut rc) } == 0 {
            continue;
        }
        let w = rc.right - rc.left;
        let h = rc.bottom - rc.top;
        if w < 20 || h < 8 || w > 2400 || h > 900 {
            continue;
        }
        let raw = target as isize;
        if tracked.iter().any(|&t| t == raw) {
            continue;
        }
        tracked.push(raw);
        log::warn!(
            "RUI_IME_DIAG scroll-latch class={:?} root={:?} rect=({},{} {}x{})",
            class,
            root_class,
            rc.left,
            rc.top,
            w,
            h
        );
        if tracked.len() >= 4 {
            break;
        }
    }
}

/// Scroll-only: move latched candidate HWNDs by ΔY (keep X). No Imm / no IMR update.
unsafe fn follow_scroll_y(app: HWND, compose_x: i32, y: i32, h: u32) {
    let anchor = compose_anchor_screen(app, compose_x, y, h);
    let prev_y = if HAS_ANCHOR_SCR.load(Ordering::Relaxed) {
        LAST_ANCHOR_SCR_Y.load(Ordering::Relaxed)
    } else {
        anchor.y
    };
    let dy = anchor.y - prev_y;
    // Latch while candidate is still at the pre-scroll screen Y, then apply ΔY.
    latch_candidate_near(app, anchor.x, prev_y);
    latch_candidate_near(app, anchor.x, anchor.y);
    LAST_ANCHOR_SCR_Y.store(anchor.y, Ordering::Relaxed);
    HAS_ANCHOR_SCR.store(true, Ordering::Relaxed);
    if dy == 0 {
        return;
    }
    let Ok(mut tracked) = TRACKED_IME_HWNDS.lock() else {
        return;
    };
    let mut moved = 0u32;
    tracked.retain(|&raw| {
        let hwnd = raw as HWND;
        if unsafe { IsWindow(hwnd) } == 0 {
            return false;
        }
        let mut cur = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if unsafe { GetWindowRect(hwnd, &mut cur) } == 0 {
            return false;
        }
        let ok = unsafe {
            SetWindowPos(
                hwnd,
                HWND_TOP,
                cur.left,
                cur.top + dy,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
        };
        if ok != 0 {
            moved += 1;
        }
        true
    });
    if moved > 0 {
        let t = now_ms();
        let last = LAST_RELOCATE_LOG_MS.load(Ordering::Relaxed);
        if last == 0 || t.wrapping_sub(last).abs() >= 80 {
            LAST_RELOCATE_LOG_MS.store(t, Ordering::Relaxed);
            log::warn!(
                "RUI_IME_DIAG scroll-follow-y n={moved} dy={dy} anchor_y={}",
                anchor.y
            );
        }
    }
}

/// Composing: Imm once + freeze IMR; scroll frames → HWND ΔY only (no X / no Imm).
fn drive_composing(window: &dyn Window, compose_x: i32, y: i32, w: u32, h: u32, scrolling: bool) {
    let Some(hwnd) = hwnd_of(window) else {
        return;
    };
    APP_HWND.store(hwnd as isize, Ordering::Relaxed);
    if unsafe { !is_app_foreground(hwnd) } {
        return;
    }
    ensure_subclass(hwnd);
    unsafe {
        let _ = DestroyCaret();
    }

    let first = !COMPOSE_IMM_SENT.swap(true, Ordering::AcqRel);
    if first {
        store_compose_geom(compose_x, y, w, h);
        apply_imm_compose_form(hwnd, compose_x, y, w, h);
        IMR_FROZEN.store(true, Ordering::Release);
        let anchor = compose_anchor_screen(hwnd, compose_x, y, h);
        LAST_ANCHOR_SCR_Y.store(anchor.y, Ordering::Relaxed);
        HAS_ANCHOR_SCR.store(true, Ordering::Relaxed);
        // Candidate may appear a moment later — try a quiet latch.
        unsafe {
            latch_candidate_near(hwnd, anchor.x, anchor.y);
        }
        return;
    }

    if scrolling {
        // Do not Imm / do not refresh IMR Y (would 2× with HWND ΔY or yank on next key).
        unsafe {
            follow_scroll_y(hwnd, compose_x, y, h);
        }
        return;
    }

    // Typing: keep frozen IMR (compose-origin X). Touch nothing else.
    let _ = (compose_x, y, w, h, IMR_FROZEN.load(Ordering::Relaxed));
}

fn last_idle_cursor_unchanged(compose_x: i32, y: i32, w: u32, h: u32) -> bool {
    let width = (w as i32).max(1);
    let height = (h as i32).max(1);
    let same = LAST_IDLE_COMPOSE_X.load(Ordering::Relaxed) == compose_x
        && LAST_IDLE_COMPOSE_Y.load(Ordering::Relaxed) == y
        && LAST_IDLE_COMPOSE_W.load(Ordering::Relaxed) == width
        && LAST_IDLE_COMPOSE_H.load(Ordering::Relaxed) == height;
    if !same {
        LAST_IDLE_COMPOSE_X.store(compose_x, Ordering::Relaxed);
        LAST_IDLE_COMPOSE_Y.store(y, Ordering::Relaxed);
        LAST_IDLE_COMPOSE_W.store(width, Ordering::Relaxed);
        LAST_IDLE_COMPOSE_H.store(height, Ordering::Relaxed);
    }
    same
}

fn destroy_caret() {
    clear_compose_session();
    if let Ok(mut last) = LAST_IME_FOCUS_ID.lock() {
        last.clear();
    }
    unsafe {
        let _ = DestroyCaret();
    }
}

fn ensure_subclass(hwnd: HWND) {
    let raw = hwnd as isize;
    let Ok(mut installed) = SUBCLASSED_HWNDS.lock() else {
        return;
    };
    if installed.iter().any(|&h| h == raw) {
        return;
    }
    unsafe {
        let ok = SetWindowSubclass(hwnd, Some(ime_subclass_proc), SUBCLASS_ID, 0);
        if ok == 0 {
            log::warn!("RUI_IME_DIAG SetWindowSubclass failed");
        } else {
            installed.push(raw);
            log::info!("RUI_IME_DIAG installed WM_IME_REQUEST subclass hwnd={raw}");
        }
    }
}

unsafe extern "system" fn ime_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _uid: usize,
    _data: usize,
) -> LRESULT {
    if msg == WM_IME_REQUEST && wparam == IMR_QUERYCHARPOSITION as WPARAM {
        let pos = lparam as *mut IMECHARPOSITION;
        if !pos.is_null() {
            let origin_x = COMPOSE_ORIGIN_X.load(Ordering::Relaxed);
            let y = CARET_Y.load(Ordering::Relaxed);
            let h = CARET_H.load(Ordering::Relaxed).max(1);
            // Always composition origin for X (TIP dwCharPos follows insert caret otherwise).
            let _char_pos = unsafe { (*pos).dwCharPos };
            let mut pt = POINT { x: origin_x, y };
            unsafe {
                let _ = ClientToScreen(hwnd, &mut pt);
                (*pos).pt = pt;
                (*pos).cLineHeight = h as u32;
                let mut doc = RECT {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                };
                let _ = GetClientRect(hwnd, &mut doc);
                let mut tl = POINT {
                    x: doc.left,
                    y: doc.top,
                };
                let mut br = POINT {
                    x: doc.right,
                    y: doc.bottom,
                };
                let _ = ClientToScreen(hwnd, &mut tl);
                let _ = ClientToScreen(hwnd, &mut br);
                (*pos).rcDocument = RECT {
                    left: tl.x,
                    top: tl.y,
                    right: br.x,
                    bottom: br.y,
                };
            }
            return 1;
        }
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

/// Idle vs composing cursor drive. See `docs/engine/platform.md` P2 / `docs/ops/ime.md`.
pub(crate) fn apply_ime_cursor_area(
    window: &dyn Window,
    area_x: f32,
    area_y: f32,
    area_w: f32,
    area_h: f32,
    ui_scale: f32,
    scroll_acc: [f32; 2],
    composing: bool,
    field_in_overflow_clip: bool,
) {
    let scale = ui_scale.max(0.001);
    let compose_px = (area_x * scale).round() as i32;
    let py = (area_y * scale).round() as i32;
    let pw = (area_w * scale).max(1.0).round() as u32;
    let ph = (area_h * scale).max(1.0).round() as u32;
    let scrolling = begin_scroll_delta(scroll_acc, scale);
    if composing {
        // Same hang as idle sliver Imm. Focus can already be the in-view sibling
        // (Search) while `text_nodes` still carry the previous field's caret —
        // composing must not Imm until that sibling paints a caret in clip.
        if !field_in_overflow_clip {
            return;
        }
        drive_composing(window, compose_px, py, pw, ph, scrolling);
        return;
    }
    // Idle: never ImmSetCompositionWindow. Compose owns Imm (once + HWND ΔY on scroll).
    // Reinforcing Imm every paint after the focused field moves hangs winit WaitUntil —
    // clicks/keys sit in the OS queue until the next wheel or a click that cancels IME.
    // A half-clipped leaf (gallery forms card 2 wrap Inputs) is the same hang: winit's
    // set_ime_cursor_area is ImmSetCompositionWindow. 2b stays fully in the ScrollArea
    // clip so it may still push. Keep last in-view IMR geom; do not store the sliver.
    if scrolling || !field_in_overflow_clip {
        if field_in_overflow_clip {
            store_compose_geom(compose_px, py, pw, ph);
        }
        return;
    }
    if last_idle_cursor_unchanged(compose_px, py, pw, ph) {
        return;
    }
    window.set_ime_cursor_area(
        winit::dpi::Position::Logical(winit::dpi::LogicalPosition::new(
            area_x as f64,
            area_y as f64,
        )),
        winit::dpi::Size::Logical(winit::dpi::LogicalSize::new(
            area_w.max(1.0) as f64,
            area_h.max(1.0) as f64,
        )),
    );
}

pub(crate) fn ime_disallow(window: &dyn Window) {
    release_ui_control(window, true);
    destroy_caret();
}
