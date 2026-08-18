//! WP-1.1：IME 光标区几何与诊断快照（可单测、可 `RUI_IME_DIAG=1` 观察）。

use std::cell::RefCell;

/// 文本编辑焦点：`input:{rest}` / `textarea:{rest}` → `rest`。
pub(crate) fn text_edit_rest(focus_id: &str) -> Option<&str> {
    crate::utils::text_field::text_edit_rest(focus_id)
}

pub(crate) fn is_text_edit_focus(focus_id: &str) -> bool {
    text_edit_rest(focus_id).is_some()
}

/// 上一次交给 OS 的 IME 光标区（物理像素），供回归断言 / 日志。
#[derive(Debug, Clone, PartialEq)]
pub struct ImeCursorSnapshot {
    pub focus_id: String,
    pub physical_x: i32,
    pub physical_y: i32,
    pub physical_w: u32,
    pub physical_h: u32,
    pub ui_scale: f32,
    pub visual_x: f32,
    pub visual_y: f32,
    pub caret_x_in_field: f32,
    /// `painted` = 本帧 `text_nodes`（已含滚动）；`focus_bounds` = 焦点盒 − scroll_acc。
    pub source: &'static str,
    pub scroll_acc_y: f32,
}

thread_local! {
    static LAST_IME_CURSOR: RefCell<Option<ImeCursorSnapshot>> = const { RefCell::new(None) };
    static LAST_IME_COMMIT: RefCell<Option<(String, String)>> = const { RefCell::new(None) };
    /// App-initiated composition end: ignore OS IME/key echoes until `Ime::Disabled`
    /// (or next `Ime::Enabled`). Event-driven drain — not a wall-clock suppress.
    static IME_END_DRAIN: RefCell<bool> = const { RefCell::new(false) };
}

pub(crate) fn begin_ime_end_drain() {
    IME_END_DRAIN.with(|c| *c.borrow_mut() = true);
}

pub(crate) fn clear_ime_end_drain() {
    IME_END_DRAIN.with(|c| *c.borrow_mut() = false);
}

/// True while OS teardown echoes after an app-initiated composition end must be ignored.
pub(crate) fn ime_end_drain_active() -> bool {
    IME_END_DRAIN.with(|c| *c.borrow())
}

pub fn take_last_ime_cursor_snapshot() -> Option<ImeCursorSnapshot> {
    LAST_IME_CURSOR.with(|c| c.borrow_mut().take())
}

pub fn peek_last_ime_cursor_snapshot() -> Option<ImeCursorSnapshot> {
    LAST_IME_CURSOR.with(|c| c.borrow().clone())
}

pub(crate) fn record_ime_cursor(snap: ImeCursorSnapshot) {
    if ime_diag_enabled() {
        log::info!(
            "RUI_IME_DIAG cursor fid={} src={} phys=({},{} {}x{}) scale={:.3} visual=({:.1},{:.1}) caret_x={:.1} scroll_acc_y={:.1}",
            snap.focus_id,
            snap.source,
            snap.physical_x,
            snap.physical_y,
            snap.physical_w,
            snap.physical_h,
            snap.ui_scale,
            snap.visual_x,
            snap.visual_y,
            snap.caret_x_in_field,
            snap.scroll_acc_y
        );
    }
    LAST_IME_CURSOR.with(|c| *c.borrow_mut() = Some(snap));
}

/// 本帧已绘制的文本叶视觉盒（`TextLayoutInfo.x/y` 已扣祖先滚动，与 caret 同空间）。
pub fn painted_text_edit_visual_bounds(
    text_nodes: &[crate::core::tree::TextLayoutInfo],
    focus_id: &str,
) -> Option<[f32; 4]> {
    let rest = text_edit_rest(focus_id)?;
    let tid = crate::utils::text_field::input_text_template_id(rest);
    let mut fallback = None;
    for n in text_nodes {
        if n.template_id.as_deref() != Some(tid.as_str()) {
            continue;
        }
        let bounds = [n.x, n.y, n.width, n.height];
        if n.style.input_caret.is_some() {
            return Some(bounds);
        }
        fallback = Some(bounds);
    }
    fallback
}

pub fn take_last_ime_commit() -> Option<(String, String)> {
    LAST_IME_COMMIT.with(|c| c.borrow_mut().take())
}

pub(crate) fn record_ime_commit(rest: &str, committed_text: &str) {
    if ime_diag_enabled() {
        log::info!(
            "RUI_IME_DIAG commit rest={} text={:?} chars={}",
            rest,
            committed_text,
            committed_text.chars().count()
        );
    }
    LAST_IME_COMMIT.with(|c| {
        *c.borrow_mut() = Some((rest.to_string(), committed_text.to_string()));
    });
}

pub(crate) fn ime_diag_enabled() -> bool {
    std::env::var("RUI_IME_DIAG")
        .map(|v| {
            let t = v.trim();
            t == "1" || t.eq_ignore_ascii_case("true")
        })
        .unwrap_or(false)
}

/// 将逻辑视觉盒 + 字段内 caret X 转为 `set_ime_cursor_area` 的物理矩形。
///
/// `visual_bounds`：已扣除祖先滚动的屏幕逻辑坐标 `[x,y,w,h]`。  
/// `caret_x_in_field`：相对字段左缘的逻辑偏移（可含 padding）。
pub fn physical_ime_cursor_area(
    visual_bounds: [f32; 4],
    caret_x_in_field: f32,
    ui_scale: f32,
) -> (i32, i32, u32, u32) {
    let scale = ui_scale.max(0.001);
    let field_w = visual_bounds[2].max(0.0);
    let caret_x = caret_x_in_field.clamp(0.0, field_w);
    let caret_w = (2.0_f32 * scale).max(1.0);
    let px = ((visual_bounds[0] + caret_x) * scale).round() as i32;
    let py = (visual_bounds[1] * scale).round() as i32;
    let ph = (visual_bounds[3] * scale).max(1.0).round() as u32;
    (px, py, caret_w as u32, ph.max(1))
}

/// 仅在 allowed 翻转时调用 winit，避免每帧 `ImmAssociateContextEx` 打断组字定位。
pub(crate) fn set_ime_allowed_latched(
    window: &winit::window::Window,
    allowed: bool,
    latched: &mut bool,
) {
    if *latched == allowed {
        return;
    }
    *latched = allowed;
    window.set_ime_allowed(allowed);
    #[cfg(target_os = "windows")]
    if !allowed {
        windows_release_ime_ui(window, true);
        win_ime::destroy_caret(); // also clears LAST_IME_FOCUS_ID
    }
}

/// Bind/release Windows IME HWND drive to the current text-edit focus id.
/// `None` / empty = no field owns the candidate window.
pub(crate) fn sync_ime_focus_owner(window: &winit::window::Window, focus_id: Option<&str>) {
    #[cfg(target_os = "windows")]
    win_ime::sync_focus_owner(window, focus_id.filter(|s| !s.is_empty()));
    #[cfg(not(target_os = "windows"))]
    let _ = (window, focus_id);
}

/// OS window deactivated: force-end composition (browser-like cancel). Keeps already
/// committed `value`; discards in-flight preedit (incl. IME syllable `'` marks).
/// Does **not** clear focus id.
pub(crate) fn on_os_focus_lost(window: &winit::window::Window) {
    let focused = crate::core::reactivity::use_signal("__rui_button_focus_id", String::new).peek();
    if is_text_edit_focus(&focused) {
        finish_composition_preserve_text(&focused);
    }
    // Composition already cancelled above; only release caret / HWND drive.
    #[cfg(target_os = "windows")]
    windows_release_ime_ui(window, false);
    #[cfg(not(target_os = "windows"))]
    let _ = window;
}

/// Clamp IME anchor Y into the window client (logical px), browser-like:
/// when the field scrolls above/below the viewport, the candidate sticks to the
/// top/bottom edge instead of following off-screen (and we do **not** blur).
pub(crate) fn clamp_ime_anchor_y(anchor_y: f32, line_h: f32, win_h: f32) -> f32 {
    if !win_h.is_finite() || win_h <= 0.0 {
        return anchor_y;
    }
    let h = line_h.max(1.0);
    let max_y = (win_h - h).max(0.0);
    anchor_y.clamp(0.0, max_y)
}

/// 文本编辑焦点盒是否已完全离开窗口客户区（逻辑坐标）。
pub(crate) fn visual_fully_outside_window(visual: [f32; 4], win_w: f32, win_h: f32) -> bool {
    let [x, y, w, h] = visual;
    w <= 0.0 || h <= 0.0 || x + w <= 0.0 || y + h <= 0.0 || x >= win_w || y >= win_h
}

/// Strip IME syllable-separator apostrophes from a preedit string before force-commit.
pub(crate) fn preedit_without_syllable_separators(preedit: &str) -> String {
    preedit.chars().filter(|c| *c != '\'').collect()
}

/// In-app / OS force-end: fold in-flight preedit into `value` after dropping syllable
/// separators (`'`), then cancel OS composition. Focus id unchanged.
///
/// OS `CPS_CANCEL` may echo Commit/Backspace; those are ignored until `Ime::Disabled`
/// via [`ime_end_drain_active`].
pub(crate) fn finish_composition_preserve_text(focus_id: &str) {
    let Some(rest) = text_edit_rest(focus_id) else {
        return;
    };
    let pk = crate::utils::text_field::input_preedit_signal_key(rest);
    let ak = crate::utils::text_field::input_ime_anchor_signal_key(rest);
    let vk = crate::utils::text_field::input_value_signal_key(rest);
    let ck = crate::utils::text_field::input_caret_signal_key(rest);
    let pre = crate::core::reactivity::use_signal(&pk, String::new).peek();
    let insert = preedit_without_syllable_separators(&pre);
    if !insert.is_empty() {
        let val_sig = crate::core::reactivity::use_signal(&vk, String::new);
        let v = val_sig.peek();
        let mut anchor = crate::core::reactivity::use_signal(&ak, || None::<usize>)
            .peek()
            .unwrap_or_else(|| {
                crate::core::reactivity::use_signal(&ck, || None::<usize>)
                    .peek()
                    .unwrap_or(v.len())
            });
        anchor = anchor.min(v.len());
        anchor = v.floor_char_boundary(anchor);
        let (new, new_caret) = apply_ime_commit(&v, anchor, &insert);
        crate::utils::text_field::commit_value_sig(rest, val_sig, new);
        crate::core::reactivity::use_signal(&ck, || None::<usize>).set(Some(new_caret));
        record_ime_commit(rest, &insert);
    }
    clear_text_edit_compose_state(focus_id);
    cancel_os_ime_composition();
}

/// Clear preedit / IME anchor / active flag (value untouched). Used after commit or
/// when discarding compose UI state.
pub(crate) fn clear_text_edit_compose_state(focus_id: &str) {
    let Some(rest) = text_edit_rest(focus_id) else {
        return;
    };
    let pk = crate::utils::text_field::input_preedit_signal_key(rest);
    let sk = crate::utils::text_field::input_preedit_sel_signal_key(rest);
    let ak = crate::utils::text_field::input_ime_anchor_signal_key(rest);
    crate::core::reactivity::use_signal(&pk, String::new).set(String::new());
    crate::core::reactivity::use_signal(&sk, || None::<(usize, usize)>).set(None);
    crate::core::reactivity::use_signal(&ak, || None::<usize>).set(None);
    crate::core::reactivity::use_signal(crate::components::IME_ACTIVE_SIGNAL, || false).set(false);
    on_ime_composition_ended();
}

/// Cancel in-flight OS composition (candidate / preedit). Safe from UI click handlers.
/// Enters end-drain so CPS_CANCEL echoes cannot mutate fields until `Ime::Disabled`.
pub(crate) fn cancel_os_ime_composition() {
    begin_ime_end_drain();
    #[cfg(target_os = "windows")]
    win_ime::cancel_active_composition();
}

/// Preedit cleared / commit finished — drop HWND candidate ownership so the next
/// composition can use Imm again.
pub(crate) fn on_ime_composition_ended() {
    #[cfg(target_os = "windows")]
    win_ime::end_composition_ui();
}

/// 交给 OS：组字锚点几何（逻辑坐标）+ Windows Imm / `WM_IME_REQUEST`。
///
/// `area_*`：**组字起点**（composition origin），不是插入符末端。
/// `composing`：当前有 preedit / IME active。
///
/// Windows 策略：
/// - **键入**：只维护 `IMR`（X=组字起点）；Imm 仅组字开始一次；**不**动候选 HWND（横向交给 TIP）。
/// - **滚动**：现代中文 TIP 常忽略组字中 Imm/IMR 的 Y 更新 → 仅此时对已闩锁候选 HWND
///   做 **ΔY** `SetWindowPos`（不改 X、不改尺寸、不每帧绝对钉死）。
/// - **空闲**：winit + Imm。不创建系统 caret。
pub(crate) fn apply_ime_cursor_area(
    window: &winit::window::Window,
    area_x: f32,
    area_y: f32,
    area_w: f32,
    area_h: f32,
    ui_scale: f32,
    scroll_acc: [f32; 2],
    composing: bool,
) {
    let scale = ui_scale.max(0.001);
    let compose_px = (area_x * scale).round() as i32;
    let py = (area_y * scale).round() as i32;
    let pw = (area_w * scale).max(1.0).round() as u32;
    let ph = (area_h * scale).max(1.0).round() as u32;
    {
        use std::cell::Cell;
        thread_local! {
            static LAST_Y: Cell<i32> = const { Cell::new(i32::MIN) };
            static LAST_LOG: Cell<Option<std::time::Instant>> = const { Cell::new(None) };
        }
        let prev = LAST_Y.get();
        if prev == i32::MIN || (py - prev).abs() >= 2 {
            let now = std::time::Instant::now();
            let allow = LAST_LOG
                .get()
                .is_none_or(|t| now.duration_since(t).as_millis() >= 80);
            if allow {
                log::warn!(
                    "RUI_IME_DIAG move visual_y={:.1} phys_y={} (dpy={}) composing={composing}",
                    area_y,
                    py,
                    if prev == i32::MIN { 0 } else { py - prev }
                );
                LAST_LOG.set(Some(now));
            }
            LAST_Y.set(py);
        }
    }
    #[cfg(target_os = "windows")]
    {
        let scrolling = win_ime::begin_scroll_delta(scroll_acc, scale);
        if composing {
            // No per-key winit/Imm/HWND. Scroll frames: ΔY HWND only.
            win_ime::drive_composing(window, compose_px, py, pw, ph, scrolling);
            return;
        }
        window.set_ime_cursor_area(
            winit::dpi::LogicalPosition::new(area_x as f64, area_y as f64),
            winit::dpi::LogicalSize::new(area_w.max(1.0) as f64, area_h.max(1.0) as f64),
        );
        win_ime::reinforce_imm(window, compose_px, py, pw, ph);
    }
    #[cfg(not(target_os = "windows"))]
    {
        window.set_ime_cursor_area(
            winit::dpi::LogicalPosition::new(area_x as f64, area_y as f64),
            winit::dpi::LogicalSize::new(area_w.max(1.0) as f64, area_h.max(1.0) as f64),
        );
        let _ = (compose_px, py, pw, ph, scroll_acc, composing);
    }
}

#[cfg(target_os = "windows")]
mod win_ime {
    use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, Ordering};
    use std::sync::Mutex;

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
    use windows_sys::Win32::UI::Input::Ime::{
        ImmGetContext, ImmGetDefaultIMEWnd, ImmNotifyIME, ImmReleaseContext,
        ImmSetCompositionWindow, CFS_POINT, COMPOSITIONFORM, CPS_CANCEL, IMC_SETCOMPOSITIONWINDOW,
        IMECHARPOSITION, IMR_QUERYCHARPOSITION, NI_COMPOSITIONSTR,
    };
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DestroyCaret, GetAncestor, GetClassNameW, GetClientRect, GetForegroundWindow,
        GetWindowRect, IsWindow, SendMessageW, SetWindowPos, WindowFromPoint, GA_ROOT, HWND_TOP,
        SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, WM_IME_CONTROL, WM_IME_REQUEST,
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
    static LAST_IMM_MS: AtomicI32 = AtomicI32::new(0);
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

    pub(super) fn hwnd_of(window: &winit::window::Window) -> Option<HWND> {
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let handle = window.window_handle().ok()?;
        match handle.as_raw() {
            RawWindowHandle::Win32(h) => Some(h.hwnd.get() as HWND),
            _ => None,
        }
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

    pub(super) fn release_ui_control(window: &winit::window::Window, cancel_comp: bool) {
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
        if let Ok(mut v) = TRACKED_IME_HWNDS.lock() {
            v.clear();
        }
    }

    pub(super) fn sync_focus_owner(window: &winit::window::Window, focus_id: Option<&str>) {
        let Ok(mut last) = LAST_IME_FOCUS_ID.lock() else {
            return;
        };
        let next = focus_id.unwrap_or("").to_string();
        if *last == next {
            return;
        }
        if !last.is_empty() {
            crate::app::ime_support::finish_composition_preserve_text(&last);
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

    pub(super) fn end_composition_ui() {
        clear_compose_session();
    }

    pub(super) fn cancel_active_composition() {
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
    pub(super) fn begin_scroll_delta(scroll_acc: [f32; 2], _ui_scale: f32) -> bool {
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
    pub(super) fn drive_composing(
        window: &winit::window::Window,
        compose_x: i32,
        y: i32,
        w: u32,
        h: u32,
        scrolling: bool,
    ) {
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
            LAST_IMM_MS.store(now_ms(), Ordering::Relaxed);
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

    /// Idle: Imm + IMR at composition origin. Never creates a system caret.
    pub(super) fn reinforce_imm(
        window: &winit::window::Window,
        compose_x: i32,
        y: i32,
        w: u32,
        h: u32,
    ) {
        let Some(hwnd) = hwnd_of(window) else {
            log::warn!("RUI_IME_DIAG windows_hwnd unavailable — Imm path skipped");
            return;
        };
        APP_HWND.store(hwnd as isize, Ordering::Relaxed);
        if unsafe { !is_app_foreground(hwnd) } {
            return;
        }
        store_compose_geom(compose_x, y, w, h);
        ensure_subclass(hwnd);
        let t = now_ms();
        let last_imm = LAST_IMM_MS.load(Ordering::Relaxed);
        let refresh_imm = last_imm == 0 || t.wrapping_sub(last_imm).abs() >= 32;
        if refresh_imm {
            LAST_IMM_MS.store(t, Ordering::Relaxed);
            apply_imm_compose_form(hwnd, compose_x, y, w, h);
        } else {
            unsafe {
                let _ = DestroyCaret();
            }
        }
    }

    pub(super) fn destroy_caret() {
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
}

#[cfg(target_os = "windows")]
fn windows_release_ime_ui(window: &winit::window::Window, cancel_comp: bool) {
    win_ime::release_ui_control(window, cancel_comp);
}

/// Commit 插入：在 `anchor` 处写入 `insert`，返回新串与新 caret 字节下标。
pub fn apply_ime_commit(value: &str, anchor: usize, insert: &str) -> (String, usize) {
    let mut a = anchor.min(value.len());
    a = value.floor_char_boundary(a);
    let mut out = String::with_capacity(value.len() + insert.len());
    out.push_str(&value[..a]);
    out.push_str(insert);
    out.push_str(&value[a..]);
    let new_caret = a + insert.len();
    (out, new_caret)
}

/// 组字开始时：若有选区则先删选区，返回 `(value, anchor)`。
pub fn prepare_compose_anchor(
    value: &str,
    caret: Option<usize>,
    selection: Option<(usize, usize)>,
) -> (String, usize) {
    if let Some((sa, sb)) = selection {
        let lo = sa.min(sb);
        let hi = sa.max(sb);
        if lo < hi && hi <= value.len() {
            let lo = value.floor_char_boundary(lo);
            let hi = value.floor_char_boundary(hi);
            if lo < hi {
                let mut v = value.to_string();
                v.replace_range(lo..hi, "");
                return (v, lo);
            }
        }
    }
    let mut a = caret.unwrap_or(value.len()).min(value.len());
    a = value.floor_char_boundary(a);
    (value.to_string(), a)
}

/// 估算 caret 相对字段左缘的逻辑 X（优先测宽；失败则用 padding 启发式）。
pub(crate) fn estimate_caret_x_in_field(
    font_system: Option<&mut glyphon::FontSystem>,
    display_prefix: &str,
    text_style: &crate::style::TextStyle,
    field_visual: [f32; 4],
    ui_scale: f32,
    pad_left_logical: f32,
) -> f32 {
    let scale = ui_scale.max(0.001);
    let content_w = (field_visual[2] - pad_left_logical).max(1.0);
    if let Some(fs) = font_system {
        if !display_prefix.is_empty() {
            let scaled_size = text_style.size * scale;
            let line_height = if let Some(lh) = text_style.line_height_px {
                lh * scale
            } else {
                let leading = text_style.line_height_scale.unwrap_or_else(|| {
                    crate::style::tailwind_default_leading_scale(text_style.size)
                });
                scaled_size * leading
            };
            let family = if let Some(ref f) = text_style.font_family {
                crate::utils::font::resolve_family(fs.db(), f)
            } else {
                glyphon::Family::SansSerif
            };
            let weight =
                crate::utils::font::normalize_weight_for_family(fs.db(), family, text_style.weight);
            let prefix_w_phys = crate::renderer::text_hit::measure_prefix_width(
                fs,
                display_prefix,
                scaled_size,
                line_height,
                weight,
                family,
                content_w * scale,
            );
            return pad_left_logical
                + ((prefix_w_phys / scale) - text_style.scroll_x.max(0.0)).clamp(0.0, content_w);
        }
    }
    pad_left_logical
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_edit_rest_input_and_textarea() {
        assert_eq!(text_edit_rest("input:foo"), Some("foo"));
        assert_eq!(text_edit_rest("textarea:bio"), Some("bio"));
        assert_eq!(text_edit_rest("button:x"), None);
    }

    #[test]
    fn visual_fully_outside_window_edges() {
        let win = (400.0, 300.0);
        assert!(!visual_fully_outside_window(
            [10.0, 10.0, 80.0, 24.0],
            win.0,
            win.1
        ));
        assert!(visual_fully_outside_window(
            [10.0, -30.0, 80.0, 24.0],
            win.0,
            win.1
        )); // above
        assert!(visual_fully_outside_window(
            [10.0, 300.0, 80.0, 24.0],
            win.0,
            win.1
        )); // below
        assert!(visual_fully_outside_window(
            [-90.0, 10.0, 80.0, 24.0],
            win.0,
            win.1
        )); // left
        assert!(visual_fully_outside_window(
            [400.0, 10.0, 80.0, 24.0],
            win.0,
            win.1
        )); // right
            // Still intersecting by 1px → not fully outside (focus stays either way).
        assert!(!visual_fully_outside_window(
            [10.0, -23.0, 80.0, 24.0],
            win.0,
            win.1
        ));
    }

    #[test]
    fn clamp_ime_anchor_y_sticks_to_viewport_edges() {
        assert_eq!(clamp_ime_anchor_y(-40.0, 20.0, 300.0), 0.0);
        assert_eq!(clamp_ime_anchor_y(10.0, 20.0, 300.0), 10.0);
        assert_eq!(clamp_ime_anchor_y(290.0, 20.0, 300.0), 280.0);
    }

    #[test]
    fn physical_area_tracks_scroll_and_scale() {
        // Field visually at (10, 40) after scroll; caret 24px into field; 2x scale.
        let (x, y, w, h) = physical_ime_cursor_area([10.0, 40.0, 200.0, 32.0], 24.0, 2.0);
        assert_eq!((x, y), (68, 80)); // (10+24)*2, 40*2
        assert!(w >= 1);
        assert_eq!(h, 64);
    }

    #[test]
    fn physical_area_clamps_caret_to_field_width() {
        let (x, _, _, _) = physical_ime_cursor_area([0.0, 0.0, 50.0, 20.0], 999.0, 1.0);
        assert_eq!(x, 50);
    }

    #[test]
    fn prepare_compose_deletes_selection() {
        let (v, a) = prepare_compose_anchor("hello world", Some(11), Some((6, 11)));
        assert_eq!(v, "hello ");
        assert_eq!(a, 6);
    }

    #[test]
    fn preedit_without_syllable_separators_strips_apostrophe() {
        assert_eq!(preedit_without_syllable_separators("shi'jie"), "shijie");
        assert_eq!(preedit_without_syllable_separators("你好"), "你好");
        assert_eq!(preedit_without_syllable_separators(""), "");
    }

    #[test]
    fn finish_composition_folds_preedit_stripping_apostrophe() {
        let (_owner, ()) = crate::core::reactivity::create_root(|| {
            let rest = "ime_finish_strip";
            let fid = format!("input:{rest}");
            let vk = crate::utils::text_field::input_value_signal_key(rest);
            let pk = crate::utils::text_field::input_preedit_signal_key(rest);
            let ak = crate::utils::text_field::input_ime_anchor_signal_key(rest);
            crate::core::reactivity::use_signal(&vk, String::new).set("已上屏".into());
            crate::core::reactivity::use_signal(&pk, String::new).set("shi'jie".into());
            crate::core::reactivity::use_signal(&ak, || None::<usize>).set(Some("已上屏".len()));
            finish_composition_preserve_text(&fid);
            assert_eq!(
                crate::core::reactivity::use_signal(&vk, String::new).peek(),
                "已上屏shijie",
                "preedit folds in; syllable `'` must be stripped"
            );
            assert!(
                crate::core::reactivity::use_signal(&pk, String::new)
                    .peek()
                    .is_empty(),
                "preedit signal must be cleared"
            );
            assert!(ime_end_drain_active());
            clear_ime_end_drain();
        });
    }

    #[test]
    fn apply_commit_mid_string() {
        let (v, c) = apply_ime_commit("ab中文", "ab中文".find('中').unwrap(), "好");
        assert_eq!(v, "ab好中文");
        assert_eq!(&v[..c], "ab好");
    }

    #[test]
    fn snapshot_roundtrip() {
        let snap = ImeCursorSnapshot {
            focus_id: "input:t".into(),
            physical_x: 1,
            physical_y: 2,
            physical_w: 3,
            physical_h: 4,
            ui_scale: 1.5,
            visual_x: 0.0,
            visual_y: 0.0,
            caret_x_in_field: 8.0,
            source: "painted",
            scroll_acc_y: 40.0,
        };
        record_ime_cursor(snap.clone());
        assert_eq!(take_last_ime_cursor_snapshot(), Some(snap));
        assert!(take_last_ime_cursor_snapshot().is_none());
    }

    #[test]
    fn painted_bounds_prefer_caret_node() {
        use crate::core::tree::TextLayoutInfo;
        use crate::style::TextStyle;
        let tid = crate::utils::text_field::input_text_template_id("gal");
        let mut style = TextStyle::default();
        style.input_caret = Some(crate::style::InputCaretDraw {
            caret_byte_index: 1,
            selection: None,
            selection_bg: [0; 4],
            overwrite_highlight: None,
        });
        let nodes = vec![
            TextLayoutInfo {
                text: "x".into(),
                shape_suffix: None,
                spans: None,
                template_id: Some(tid.clone()),
                x: 10.0,
                y: 500.0, // stale sibling without caret should lose
                width: 100.0,
                height: 20.0,
                clip_bounds: None,
                style: TextStyle::default(),
                prepared: None,
                glyph_fx: None,
            },
            TextLayoutInfo {
                text: "啊a".into(),
                shape_suffix: None,
                spans: None,
                template_id: Some(tid),
                x: 24.0,
                y: 80.0,
                width: 200.0,
                height: 32.0,
                clip_bounds: None,
                style,
                prepared: None,
                glyph_fx: None,
            },
        ];
        let b = painted_text_edit_visual_bounds(&nodes, "input:gal").expect("bounds");
        assert!((b[1] - 80.0).abs() < 0.5, "y={}", b[1]);
    }
}
