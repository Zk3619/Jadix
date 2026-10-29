//! WP-1.1：IME 光标区几何与诊断快照（可单测、可 `JADIX_IME_DIAG=1` 观察）。

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
            "JADIX_IME_DIAG cursor fid={} src={} phys=({},{} {}x{}) scale={:.3} visual=({:.1},{:.1}) caret_x={:.1} scroll_acc_y={:.1}",
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

/// 本帧已绘制的文本编辑叶（`x/y` 已扣祖先滚动；`paint_transform` 与 glyphon/Bake 同轴）。
pub fn painted_text_edit_node<'a>(
    text_nodes: &'a [crate::core::tree::TextLayoutInfo],
    focus_id: &str,
) -> Option<&'a crate::core::tree::TextLayoutInfo> {
    let rest = text_edit_rest(focus_id)?;
    let tid = crate::utils::text_field::input_text_template_id(rest);
    let mut fallback = None;
    for n in text_nodes {
        if n.template_id.as_deref() != Some(tid.as_str()) {
            continue;
        }
        if n.style.input_caret.is_some() {
            return Some(n);
        }
        fallback = Some(n);
    }
    fallback
}

/// 本帧已绘制的文本叶视觉盒（`TextLayoutInfo.x/y` 已扣祖先滚动，与 caret 同空间）。
pub fn painted_text_edit_visual_bounds(
    text_nodes: &[crate::core::tree::TextLayoutInfo],
    focus_id: &str,
) -> Option<[f32; 4]> {
    painted_text_edit_node(text_nodes, focus_id).map(|n| [n.x, n.y, n.width, n.height])
}

/// Layout-space caret origin → paint space (center-pivot 2×2, same as VS / glyphon).
pub fn paint_space_ime_origin(
    layout_xy: [f32; 2],
    paint_linear: &crate::style::Mat2d,
    paint_origin: [f32; 2],
) -> [f32; 2] {
    if crate::style::mat2d_is_identity(paint_linear) {
        return layout_xy;
    }
    let dx = layout_xy[0] - paint_origin[0];
    let dy = layout_xy[1] - paint_origin[1];
    let (rx, ry) = crate::style::mat2d_transform_vec(paint_linear, dx, dy);
    [paint_origin[0] + rx, paint_origin[1] + ry]
}

pub fn take_last_ime_commit() -> Option<(String, String)> {
    LAST_IME_COMMIT.with(|c| c.borrow_mut().take())
}

pub(crate) fn record_ime_commit(rest: &str, committed_text: &str) {
    let input_type = crate::core::reactivity::use_signal(
        &crate::utils::text_field::input_type_signal_key(rest),
        String::new,
    )
    .peek();
    if crate::utils::text_edit::field::InputType::parse(&input_type).is_password() {
        LAST_IME_COMMIT.with(|last| *last.borrow_mut() = None);
        return;
    }
    if ime_diag_enabled() {
        log::info!(
            "JADIX_IME_DIAG commit rest={} text={:?} chars={}",
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
    std::env::var("JADIX_IME_DIAG")
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
    window: &dyn winit::window::Window,
    allowed: bool,
    latched: &mut bool,
) {
    if *latched == allowed {
        return;
    }
    *latched = allowed;
    // Same payload as the removed `Window::set_ime_allowed` shim.
    let request = if allowed {
        let caps = winit::window::ImeCapabilities::new()
            .with_hint_and_purpose()
            .with_cursor_area();
        let data = winit::window::ImeRequestData::default()
            .with_hint_and_purpose(
                winit::window::ImeHint::NONE,
                winit::window::ImePurpose::Normal,
            )
            .with_cursor_area(
                winit::dpi::LogicalPosition::new(0.0, 0.0).into(),
                winit::dpi::LogicalSize::new(0.0, 0.0).into(),
            );
        winit::window::ImeRequest::Enable(winit::window::ImeEnableRequest::new(caps, data).unwrap())
    } else {
        winit::window::ImeRequest::Disable
    };
    let _ = window.request_ime_update(request);
    if allowed {
        // Subclass must exist before the first composition: Shift toggles
        // 中/英 (日/英, 한/영) with no preedit on screen.
        #[cfg(target_os = "windows")]
        crate::platform::windows::arm_ime_composition_watch(window);
    } else {
        crate::platform::ime_disallow(window);
    }
}

/// Bind/release Windows IME HWND drive to the current text-edit focus id.
/// `None` / empty = no field owns the candidate window.
pub(crate) fn sync_ime_focus_owner(window: &dyn winit::window::Window, focus_id: Option<&str>) {
    let next = focus_id.filter(|s| !s.is_empty());
    crate::platform::sync_ime_focus_owner(window, next, |prev| {
        finish_composition_preserve_text(prev);
    });
    // Field→field keeps `set_ime_allowed(true)`, so winit may never send `Ime::Disabled`
    // to clear the end-drain started by `finish_composition_preserve_text`. A stuck drain
    // swallowed KeyEvents after switching DevTools CSSOM textarea ↔ app Input/Textarea.
    if next.is_some_and(is_text_edit_focus) {
        clear_ime_end_drain();
    }
}

/// OS window deactivated: force-end composition (browser-like cancel). Keeps already
/// committed `value`; discards in-flight preedit (incl. IME syllable `'` marks).
/// Does **not** clear focus id.
pub(crate) fn on_os_focus_lost(window: &dyn winit::window::Window) {
    let focused =
        crate::core::reactivity::use_signal("__jadix_button_focus_id", String::new).peek();
    if is_text_edit_focus(&focused) {
        finish_composition_preserve_text(&focused);
    }
    crate::platform::ime_release_ui(window, false);
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

/// `visual` 是否整盒落在 `clip` 内（逻辑 px）。半裁切不算。
pub(crate) fn visual_rect_fully_inside(visual: [f32; 4], clip: [f32; 4]) -> bool {
    const EPS: f32 = 0.5;
    let [x, y, w, h] = visual;
    let [cx, cy, cw, ch] = clip;
    w > 0.0
        && h > 0.0
        && cw > 0.0
        && ch > 0.0
        && x >= cx - EPS
        && y >= cy - EPS
        && x + w <= cx + cw + EPS
        && y + h <= cy + ch + EPS
}

/// Idle `ImmSetCompositionWindow` / winit `set_ime_cursor_area` is safe only when the
/// painted text leaf is fully inside its overflow clip (and the window).
///
/// Gallery forms card「2) Input」: the first wrap field's leaf stays in `text_nodes`
/// (live clip-cull exemption) after it is only a chrome sliver. Pushing Imm for that
/// half-clipped box hangs Windows `WaitUntil` — clicks/keys sit in the OS queue until
/// a wheel or a click that cancels IME. Card 2b's leaf stays fully inside the
/// ScrollArea clip, so the same scroll does not hang.
pub(crate) fn idle_ime_may_push_imm(
    visual: [f32; 4],
    clip: Option<[f32; 4]>,
    win_w: f32,
    win_h: f32,
) -> bool {
    if visual_fully_outside_window(visual, win_w, win_h) {
        return false;
    }
    if visual[1] < -0.5 || visual[1] + visual[3] > win_h + 0.5 {
        return false;
    }
    match clip {
        None => true,
        Some(c) => visual_rect_fully_inside(visual, c),
    }
}

/// Last collected caret belongs to `focus_id`.
///
/// `false` when the DL still has a caret on another field (paint-only `focus_id.set`
/// has not been presented). The previous field's blink `WaitUntil` must not swallow
/// the chrome frame — and IME must not treat that sliver caret as the live anchor.
pub(crate) fn painted_caret_matches_focus(
    text_nodes: &[crate::core::tree::TextLayoutInfo],
    focus_id: &str,
) -> bool {
    let Some(rest) = text_edit_rest(focus_id) else {
        return true;
    };
    let tid = crate::utils::text_field::input_text_template_id(rest);
    let mut other_caret = false;
    for n in text_nodes {
        if n.style.input_caret.is_none() {
            continue;
        }
        if n.template_id.as_deref() == Some(tid.as_str()) {
            return true;
        }
        other_caret = true;
    }
    !other_caret
}

/// Painted **caret** leaf is fully inside its collect clip.
///
/// `false` if the focused field was not collected with a caret — including the
/// placeholder leaf that stays in `text_nodes` after focus moved but before the
/// next paint-sync. Imm on that chrome/placeholder rect (or on a sliver) hangs
/// Windows `WaitUntil`: IME candidates follow `focus_id`, while the last
/// presented frame keeps the previous field's ring/caret (gallery forms: number
/// chrome frozen, Search composing).
pub(crate) fn painted_ime_field_in_overflow_clip(
    text_nodes: &[crate::core::tree::TextLayoutInfo],
    focus_id: &str,
    win_w: f32,
    win_h: f32,
) -> bool {
    let Some(n) = painted_text_edit_node(text_nodes, focus_id) else {
        return false;
    };
    if n.style.input_caret.is_none() {
        return false;
    }
    let scale = if n.paint_scale.is_finite() && n.paint_scale > 0.0 {
        n.paint_scale
    } else {
        1.0
    };
    let bounds = [n.x, n.y, n.width * scale, n.height * scale];
    let mut visual = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    for corner in [
        [bounds[0], bounds[1]],
        [bounds[0] + bounds[2], bounds[1]],
        [bounds[0], bounds[1] + bounds[3]],
        [bounds[0] + bounds[2], bounds[1] + bounds[3]],
    ] {
        let point = paint_space_ime_origin(corner, &n.paint_transform, n.paint_origin);
        visual[0] = visual[0].min(point[0]);
        visual[1] = visual[1].min(point[1]);
        visual[2] = visual[2].max(point[0]);
        visual[3] = visual[3].max(point[1]);
    }
    visual[2] -= visual[0];
    visual[3] -= visual[1];
    idle_ime_may_push_imm(visual, n.clip_bounds, win_w, win_h)
}

/// Strip IME syllable-separator apostrophes from a preedit string before force-commit.
pub(crate) fn preedit_without_syllable_separators(preedit: &str) -> String {
    preedit.chars().filter(|c| *c != '\'').collect()
}

/// CJK IME Shift (中/英 · 日/英 · 한/영) while composing: fold preedit into `value`
/// and clear the underline. Windows often dismisses the candidate window on that
/// toggle without a winit `Commit`/`Preedit("")`/`Disabled`, leaving a zombie
/// composition underline on the last syllable.
pub(crate) fn on_ime_alphanumeric_toggle(focus_id: &str) {
    let Some(rest) = text_edit_rest(focus_id) else {
        return;
    };
    let pk = crate::utils::text_field::input_preedit_signal_key(rest);
    let pre = crate::core::reactivity::use_signal(&pk, String::new).peek();
    if pre.is_empty() {
        return;
    }
    finish_composition_preserve_text(focus_id);
}

/// Drain Windows lone-Shift compose-finish (see `take_pending_ime_shift_compose_finish`).
pub(crate) fn drain_pending_ime_shift_compose_finish(focus_id: &str) {
    #[cfg(target_os = "windows")]
    {
        if crate::platform::windows::take_pending_ime_shift_compose_finish() {
            on_ime_alphanumeric_toggle(focus_id);
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = focus_id;
    }
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
        let mut v = val_sig.peek();
        let cs = crate::utils::text_field::load_text_cursors(rest, &v);
        if cs.len() > 1 {
            let old_cs = cs.clone();
            let old_latest = crate::utils::text_field::latest_caret_signal(rest).peek();
            let cs = crate::utils::text_multi_cursor::insert_at_cursors(&mut v, cs, &insert);
            #[cfg(feature = "text-undo")]
            crate::utils::text_edit_history::will_user_edit_field(
                rest,
                crate::utils::text_edit_history::EditKind::Replace,
            );
            crate::utils::text_field::commit_value_sig(rest, val_sig, v);
            crate::utils::text_field::store_text_cursors(rest, &cs);
            if let Some(l) = old_latest {
                crate::utils::text_field::latest_caret_signal(rest).set(Some(
                    crate::utils::text_multi_cursor::follow_latest(&old_cs, &cs, l),
                ));
            }
            #[cfg(feature = "text-undo")]
            crate::utils::text_edit_history::note_field_after_edit(rest);
        } else {
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
            #[cfg(feature = "text-undo")]
            crate::utils::text_edit_history::will_user_edit_field(
                rest,
                crate::utils::text_edit_history::EditKind::Replace,
            );
            crate::utils::text_field::commit_value_sig(rest, val_sig, new);
            crate::core::reactivity::use_signal(&ck, || None::<usize>).set(Some(new_caret));
            #[cfg(feature = "text-undo")]
            crate::utils::text_edit_history::note_field_after_edit(rest);
        }
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
    let mk = crate::utils::text_field::input_preedit_marks_signal_key(rest);
    crate::core::reactivity::use_signal(&pk, String::new).set(String::new());
    crate::core::reactivity::use_signal(&sk, || None::<(usize, usize)>).set(None);
    crate::core::reactivity::use_signal(&ak, || None::<usize>).set(None);
    crate::core::reactivity::use_signal(&mk, Vec::<(usize, usize, u8)>::new).set(Vec::new());
    crate::core::reactivity::use_signal(crate::utils::text_edit::field::IME_ACTIVE_SIGNAL, || {
        false
    })
    .set(false);
    on_ime_composition_ended();
}

/// Cancel in-flight OS composition (candidate / preedit). Safe from UI click handlers.
/// Enters end-drain so CPS_CANCEL echoes cannot mutate fields until `Ime::Disabled` /
/// `Ime::Enabled`, or until the next text-edit focus owner clears the drain (field→field
/// switch keeps IME allowed so Disabled may never arrive).
pub(crate) fn cancel_os_ime_composition() {
    begin_ime_end_drain();
    crate::platform::ime_cancel_composition();
}

/// Preedit cleared / commit finished — drop HWND candidate ownership so the next
/// composition can use Imm again.
pub(crate) fn on_ime_composition_ended() {
    crate::platform::ime_end_composition_ui();
}

/// 交给 OS：组字锚点几何（逻辑坐标）+ Windows Imm / `WM_IME_REQUEST`。
///
/// `area_*`：**组字起点**（composition origin），不是插入符末端。
/// `composing`：当前有 preedit / IME active。
///
/// Windows policy:
/// - **键入**：只维护 `IMR`（X=组字起点）；Imm 仅组字开始一次；**不**动候选 HWND（横向交给 TIP）。
/// - **滚动**：现代中文 TIP 常忽略组字中 Imm/IMR 的 Y 更新 → 仅此时对已闩锁候选 HWND
///   做 **ΔY** `SetWindowPos`（不改 X、不改尺寸、不每帧绝对钉死）。
/// - **空闲 / 组字**：仅当本帧 `text_nodes` 里该焦点已带 caret 且整盒在 overflow clip 内
///   才 Imm。半裁切、以及焦点已切走但仍是 placeholder 的叶，都不推 Imm
///   （`ImmSetCompositionWindow` 会挂死 `WaitUntil`：IME 跟 `focus_id`，画面停在上一字段）。
pub(crate) fn apply_ime_cursor_area(
    window: &dyn winit::window::Window,
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
    let py = (area_y * scale).round() as i32;
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
                    "JADIX_IME_DIAG move visual_y={:.1} phys_y={} (dpy={}) composing={composing}",
                    area_y,
                    py,
                    if prev == i32::MIN { 0 } else { py - prev }
                );
                LAST_LOG.set(Some(now));
            }
            LAST_Y.set(py);
        }
    }
    crate::platform::apply_ime_cursor_area(
        window,
        area_x,
        area_y,
        area_w,
        area_h,
        ui_scale,
        scroll_acc,
        composing,
        field_in_overflow_clip,
    );
}

/// Windows 韩文 IME 的 `WM_IME_COMPOSITION` 经常不带 `GCS_CURSORPOS`。
/// winit 仍调用 `ImmGetCompositionString(GCS_CURSORPOS)`，缺标志时返回 0，
/// 再编码成预编辑光标 `(0, 0)`。按这个光标绘制时，带下划线的组字会落在插入符右侧。
///
/// `caret_unspecified`：本条组字消息没有光标位。此时光标应在组字末尾（`None`），
/// 而不是字节 0。IME 真的把光标放在组字开头时会带上 `GCS_CURSORPOS`，原样保留。
pub(crate) fn map_preedit_cursor(
    cursor: Option<(usize, usize)>,
    caret_unspecified: bool,
) -> Option<(usize, usize)> {
    if caret_unspecified {
        None
    } else {
        cursor
    }
}

/// Windows `GCS_COMPATTR` values. Japanese IME uses these for clause underlines.
const ATTR_INPUT: u8 = 0x00;
const ATTR_TARGET_CONVERTED: u8 = 0x01;
const ATTR_CONVERTED: u8 = 0x02;
const ATTR_TARGET_NOTCONVERTED: u8 = 0x03;
const ATTR_INPUT_ERROR: u8 = 0x04;
const ATTR_FIXEDCONVERTED: u8 = 0x05;

/// Thin solid: converted clauses that are not the target.
pub const PREEDIT_UL_THIN: u8 = 0;
/// Thick solid: the clause just submitted for conversion (target).
pub const PREEDIT_UL_THICK: u8 = 1;
/// Wavy: the character still being typed.
pub const PREEDIT_UL_WAVY: u8 = 2;

fn is_japanese_kana(c: char) -> bool {
    matches!(
        c,
        '\u{3040}'..='\u{30FF}' | '\u{31F0}'..='\u{31FF}' | '\u{FF66}'..='\u{FF9D}'
    )
}

fn preedit_attr_style(attr: u8) -> u8 {
    match attr {
        ATTR_TARGET_CONVERTED | ATTR_TARGET_NOTCONVERTED => PREEDIT_UL_THICK,
        ATTR_CONVERTED | ATTR_FIXEDCONVERTED => PREEDIT_UL_THIN,
        ATTR_INPUT | ATTR_INPUT_ERROR => PREEDIT_UL_WAVY,
        _ => PREEDIT_UL_WAVY,
    }
}

/// Clause underlines for a composition string.
///
/// Chinese / Korean stay a single solid line (empty result): their string is
/// all `ATTR_INPUT` and not kana. Japanese kana, and any string that already
/// has converted clauses, splits into thick / thin / wavy runs.
pub(crate) fn preedit_underline_spans(text: &str, attrs: &[u8]) -> Vec<(usize, usize, u8)> {
    if text.is_empty() || attrs.is_empty() {
        return Vec::new();
    }
    let kana = text.chars().any(is_japanese_kana);
    let clause = attrs.iter().any(|&a| {
        matches!(
            a,
            ATTR_TARGET_CONVERTED | ATTR_TARGET_NOTCONVERTED | ATTR_CONVERTED | ATTR_FIXEDCONVERTED
        )
    });
    if !kana && !clause {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut byte = 0usize;
    let mut attr_idx = 0usize;
    let mut run_start = 0usize;
    let mut run_style: Option<u8> = None;
    for ch in text.chars() {
        let attr = attrs.get(attr_idx).copied().unwrap_or(ATTR_INPUT);
        let style = preedit_attr_style(attr);
        if run_style != Some(style) {
            if let Some(st) = run_style {
                if byte > run_start {
                    out.push((run_start, byte, st));
                }
            }
            run_start = byte;
            run_style = Some(style);
        }
        byte += ch.len_utf8();
        attr_idx += ch.len_utf16();
    }
    if let Some(st) = run_style {
        if byte > run_start {
            out.push((run_start, byte, st));
        }
    }
    out
}

/// Caret + underline runs for one `Ime::Preedit`.
///
/// `cursor_chars`: `None` when this event has no Windows composition snapshot
/// (use winit's cursor). `Some(None)` when `GCS_CURSORPOS` is absent (caret at
/// the end). `Some(Some(n))` is the real character index — not the target
/// clause winit puts in the cursor range.
pub(crate) fn resolve_preedit_chrome(
    text: &str,
    winit_cursor: Option<(usize, usize)>,
    caret_unspecified: bool,
    cursor_chars: Option<Option<usize>>,
    attrs: &[u8],
) -> (Option<(usize, usize)>, Vec<(usize, usize, u8)>) {
    let caret = if let Some(cur) = cursor_chars {
        match cur {
            Some(n) => {
                let b = text
                    .chars()
                    .take(n)
                    .map(|c| c.len_utf8())
                    .sum::<usize>()
                    .min(text.len());
                Some((b, b))
            }
            // Windows GCS_CURSORPOS absent means end-of-composition. Normalize
            // it here so None elsewhere retains winit's "hide caret" meaning.
            None => Some((text.len(), text.len())),
        }
    } else if caret_unspecified {
        Some((text.len(), text.len()))
    } else {
        map_preedit_cursor(winit_cursor, caret_unspecified)
    };
    let marks = if cursor_chars.is_some() {
        preedit_underline_spans(text, attrs)
    } else {
        Vec::new()
    };
    (caret, marks)
}

pub(crate) fn preedit_caret_unspecified() -> bool {
    #[cfg(target_os = "windows")]
    {
        crate::platform::windows::ime_preedit_caret_unspecified()
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
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
///
/// `line_full`：caret 所在硬行全文，用于 CSS `text-align` 水平偏移（与绘制/命中一致）。
pub(crate) fn estimate_caret_x_in_field(
    font_system: Option<&mut glyphon::FontSystem>,
    display_prefix: &str,
    line_full: &str,
    text_style: &crate::style::TextStyle,
    field_visual: [f32; 4],
    ui_scale: f32,
    pad_left_logical: f32,
) -> f32 {
    let scale = ui_scale.max(0.001);
    let content_w = (field_visual[2] - pad_left_logical).max(1.0);
    let Some(fs) = font_system else {
        return pad_left_logical;
    };
    let scaled_size = text_style.size_resolved() * scale;
    let line_height = if let Some(lh) = text_style.line_height_px {
        lh * scale
    } else {
        let leading = text_style.line_height_scale.unwrap_or_else(|| {
            crate::style::tailwind_default_leading_scale(text_style.size_resolved())
        });
        scaled_size * leading
    };
    let family = if let Some(ref f) = text_style.font_family {
        crate::utils::font::resolve_family(fs.db(), f)
    } else {
        glyphon::Family::SansSerif
    };
    let weight = crate::utils::font::normalize_weight_for_family(
        fs.db(),
        family,
        text_style.weight_resolved(),
    );
    let measure_one = |fs: &mut glyphon::FontSystem, text: &str| -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        crate::renderer::text_hit::measure_prefix_width(
            fs,
            text,
            scaled_size,
            line_height,
            weight,
            family,
            content_w * scale,
        ) / scale
    };
    let line_w = measure_one(fs, line_full);
    let prefix_w = measure_one(fs, display_prefix);
    let align_dx = text_style.align_inline_offset(line_w, content_w);
    pad_left_logical + (align_dx + prefix_w - text_style.scroll_x.max(0.0)).clamp(0.0, content_w)
}

#[cfg(test)]
#[path = "tests/ime_support.rs"]
mod tests;
