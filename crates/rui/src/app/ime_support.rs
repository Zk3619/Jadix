//! WP-1.1：IME 光标区几何与诊断快照（可单测、可 `RUI_IME_DIAG=1` 观察）。

use std::cell::RefCell;

/// 文本编辑焦点：`input:{rest}` / `textarea:{rest}` → `rest`。
pub(crate) fn text_edit_rest(focus_id: &str) -> Option<&str> {
    focus_id
        .strip_prefix(crate::components::INPUT_FOCUS_PREFIX)
        .or_else(|| focus_id.strip_prefix(crate::components::TEXTAREA_FOCUS_PREFIX))
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
}

thread_local! {
    static LAST_IME_CURSOR: RefCell<Option<ImeCursorSnapshot>> = const { RefCell::new(None) };
    static LAST_IME_COMMIT: RefCell<Option<(String, String)>> = const { RefCell::new(None) };
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
            "RUI_IME_DIAG cursor fid={} phys=({},{} {}x{}) scale={:.3} visual=({:.1},{:.1}) caret_x={:.1}",
            snap.focus_id,
            snap.physical_x,
            snap.physical_y,
            snap.physical_w,
            snap.physical_h,
            snap.ui_scale,
            snap.visual_x,
            snap.visual_y,
            snap.caret_x_in_field
        );
    }
    LAST_IME_CURSOR.with(|c| *c.borrow_mut() = Some(snap));
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
            let weight = crate::utils::font::normalize_weight_for_family(
                fs.db(),
                family,
                text_style.weight,
            );
            let prefix_w_phys = crate::renderer::text_hit::measure_prefix_width(
                fs,
                display_prefix,
                scaled_size,
                line_height,
                weight,
                family,
                content_w * scale,
            );
            return pad_left_logical + (prefix_w_phys / scale).min(content_w);
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
        };
        record_ime_cursor(snap.clone());
        assert_eq!(take_last_ime_cursor_snapshot(), Some(snap));
        assert!(take_last_ime_cursor_snapshot().is_none());
    }
}
