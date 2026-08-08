//! # Text field（能力层）
//!
//! Input / Textarea 共用的无外观契约：信号键、focus 前缀、受控绑定。
//! **禁止**写入圆角、配色、阴影等产品样式（见 `docs/COMPONENT_LIBRARY.md` §2）。
//!
//! `input_controller` 写 store 后应调用 [`commit_value`]，以便受控 Signal 在
//! **非 paint** 路径上通知订阅者；paint sync 仅允许 [`pull_controlled_into_store`]
//!（`set_silent`），避免 `AppState` RefCell 重入。

use std::cell::RefCell;
use std::collections::HashMap;

use crate::core::reactivity::{use_signal, Signal};

/// 单行文本焦点前缀（`input:{id}`）。
pub const INPUT_FOCUS_PREFIX: &str = "input:";
/// 多行文本焦点前缀（`textarea:{id}`）；controller 据此允许 Enter 换行。
pub const TEXTAREA_FOCUS_PREFIX: &str = "textarea:";

pub fn input_focus_id(id: &str) -> String {
    format!("{INPUT_FOCUS_PREFIX}{id}")
}

pub fn textarea_focus_id(id: &str) -> String {
    format!("{TEXTAREA_FOCUS_PREFIX}{id}")
}

/// 多行内部滚动视口 rest（`scroll:{rest}`）。
pub fn textarea_scroll_rest(id: &str) -> String {
    format!("{id}__ta_scroll")
}

/// 多行高度信号（resize / 固定可视区）。
pub fn textarea_height_signal_key(id: &str) -> String {
    format!("__rui_textarea_h_{id}")
}

/// 右下角 resize grip 的 `focus_id` 前缀。
pub const TEXTAREA_RESIZE_FOCUS_PREFIX: &str = "textarea-resize:";

pub fn textarea_resize_focus_id(id: &str) -> String {
    format!("{TEXTAREA_RESIZE_FOCUS_PREFIX}{id}")
}

/// 默认可视高度：`rows * line_box + vertical padding`。
/// `line_height_px` 须与文本测量/绘制行盒一致（通常为 tw 默认 leading × 字号）。
pub fn textarea_default_height(rows: u32, line_height_px: f32, pad_y_px: f32) -> f32 {
    rows.max(1) as f32 * line_height_px.max(1.0) + pad_y_px * 2.0
}

/// 解析 Input/Textarea 行盒高度（逻辑 px）。
///
/// 未显式指定时用 **leading-normal（1.5）**，与 Gallery 中正常的 Label 对照一致，
/// 并避免 `text-sm` 默认 leading 在 glyphon/CJK 下仍偏紧导致「行底裁切 / 行距挤叠」。
/// ShadCN Textarea 虽跟 `text-sm` token，但浏览器 ink 可溢出 line-box；RUI 必须把余量做进行盒。
pub fn text_edit_line_box_px(
    size_px: f32,
    line_height_px: Option<f32>,
    leading_scale: Option<f32>,
) -> f32 {
    if let Some(lh) = line_height_px {
        lh.max(1.0)
    } else if let Some(scale) = leading_scale {
        (size_px * scale).max(1.0)
    } else {
        (size_px * 1.5).max(1.0)
    }
}

/// 未显式 leading 时 TextEdit 使用的行高倍率（= CSS `leading-normal`）。
pub fn text_edit_default_leading_scale() -> f32 {
    1.5
}

/// 末行下方 ink 余量（见 `docs/font.md`）：只加在内容块底部与 glyphon buffer，
/// **不要**再做 `y += top_ink` 绘制偏移——否则叶高/裁切与字形原点脱节，表现为半截字。
pub fn text_edit_ink_pad_px(font_size_px: f32) -> f32 {
    (font_size_px * 0.25).max(4.0)
}

/// 编辑叶内容块高度：`layout(text).height + bottom_ink`。
/// 多行对齐 ShadCN `field-sizing-content`：叶高由内容驱动。
pub fn text_edit_content_height_px(
    text: &str,
    size_px: f32,
    weight: u16,
    line_height_px: f32,
    avail_width_px: f32,
) -> f32 {
    let lh = line_height_px.max(1.0);
    let prepared = crate::text::prepare(text, size_px, weight, lh);
    let metrics = crate::text::layout(&prepared, avail_width_px.max(1.0));
    metrics.height.max(lh) + text_edit_ink_pad_px(size_px)
}

/// 光标高度：略高于行盒（+1px），行内居中；避免再抬到 lh+4 / 1.2×lh 显得顶满。
pub fn text_edit_caret_height_px(line_height_px: f32, font_size_px: f32) -> f32 {
    let lh = line_height_px.max(1.0);
    (lh + 1.0).max(font_size_px + 1.0).max(1.0)
}

/// Hard-`\n`：全局字节下标 → `(line_i, local_byte)`；落在 `\n` 之后为下一行行首。
pub fn caret_line_and_local(text: &str, byte_ix: usize) -> (usize, usize) {
    let ix = text.floor_char_boundary(byte_ix.min(text.len()));
    let mut line_i = 0usize;
    let mut line_start = 0usize;
    let mut i = 0usize;
    while i < text.len() {
        if i >= ix {
            return (line_i, ix.saturating_sub(line_start));
        }
        let ch = text[i..].chars().next().unwrap();
        let next = i + ch.len_utf8();
        if ch == '\n' {
            if next == ix {
                return (line_i + 1, 0);
            }
            line_i += 1;
            line_start = next;
        }
        i = next;
    }
    (line_i, ix.saturating_sub(line_start))
}

/// 若 caret 行盒不在视口内，返回应写入的新 `scroll_y`；已可见则 `None`。
///
/// - `viewport_h` 过小（未就绪）时：若仍有明显 `scroll_y`，强制清零（避免信号未就绪时
///   陈旧 sy 把多行裁成顶上一薄条，看起来像「字形被压扁」）。
/// - `max_scroll_y`（通常 `max(0, content_h - viewport_h)`）用于钳制，避免把内容顶出裁切区。
pub fn ensure_caret_visible_scroll_y(
    text: &str,
    caret_byte: usize,
    line_height: f32,
    viewport_h: f32,
    scroll_y: f32,
    max_scroll_y: f32,
) -> Option<f32> {
    let lh = line_height.max(1.0);
    let max_sy = max_scroll_y.max(0.0);
    if !viewport_h.is_finite() || viewport_h < lh * 0.5 {
        // Viewport metrics not ready — still kill stale offsets that clip glyphs.
        return if scroll_y > 0.5 { Some(0.0) } else { None };
    }
    if max_sy < 0.5 {
        // Content fits: any stale sy must clear.
        return if scroll_y > 0.5 { Some(0.0) } else { None };
    }
    let (line_i, _) = caret_line_and_local(text, caret_byte);
    let caret_top = line_i as f32 * lh;
    let caret_bot = caret_top + lh;
    let mut new_sy = scroll_y.clamp(0.0, max_sy);
    if caret_top < new_sy {
        new_sy = caret_top;
    }
    if caret_bot > new_sy + viewport_h {
        new_sy = (caret_bot - viewport_h).max(0.0);
    }
    new_sy = new_sy.clamp(0.0, max_sy);
    if (new_sy - scroll_y).abs() > 0.5 {
        Some(new_sy)
    } else {
        None
    }
}

pub fn focus_id_for_text_edit(id: &str, multiline: bool) -> String {
    if multiline {
        textarea_focus_id(id)
    } else {
        input_focus_id(id)
    }
}

/// `input:{rest}` / `textarea:{rest}` → `rest`。
pub fn text_edit_rest(focus_id: &str) -> Option<&str> {
    focus_id
        .strip_prefix(INPUT_FOCUS_PREFIX)
        .or_else(|| focus_id.strip_prefix(TEXTAREA_FOCUS_PREFIX))
}

pub fn input_value_signal_key(id: &str) -> String {
    format!("__rui_input_value_{id}")
}
pub fn input_preedit_signal_key(id: &str) -> String {
    format!("__rui_input_preedit_{id}")
}
pub fn input_preedit_sel_signal_key(id: &str) -> String {
    format!("__rui_input_preedit_sel_{id}")
}
pub fn input_caret_signal_key(id: &str) -> String {
    format!("__rui_input_caret_{id}")
}
pub fn input_selection_signal_key(id: &str) -> String {
    format!("__rui_input_selection_{id}")
}
pub fn input_overwrite_signal_key(id: &str) -> String {
    format!("__rui_input_overwrite_{id}")
}
pub fn input_ime_anchor_signal_key(id: &str) -> String {
    format!("__rui_input_ime_anchor_{id}")
}
pub fn input_type_signal_key(id: &str) -> String {
    format!("__rui_input_type_{id}")
}
pub fn input_password_reveal_signal_key(id: &str) -> String {
    format!("__rui_input_pw_reveal_{id}")
}

pub(crate) fn input_text_template_id(id: &str) -> String {
    format!("__rui_input:{id}:text")
}
pub(crate) fn input_clear_template_id(id: &str) -> String {
    format!("__rui_input:{id}:clear")
}
pub(crate) fn input_reveal_template_id(id: &str) -> String {
    format!("__rui_input:{id}:reveal")
}

thread_local! {
    static CONTROLLED_VALUES: RefCell<HashMap<String, Signal<String>>> =
        RefCell::new(HashMap::new());
}

fn controlled_of(field_id: &str) -> Option<Signal<String>> {
    CONTROLLED_VALUES.with(|m| m.borrow().get(field_id).copied())
}

/// 挂载时以外部 Signal 为准灌入 store，并登记对拍。
pub fn bind_controlled_value(field_id: &str, external: Signal<String>) {
    let store = use_signal(&input_value_signal_key(field_id), String::new);
    let ev = external.peek();
    if store.peek() != ev {
        store.set(ev);
    }
    CONTROLLED_VALUES.with(|m| {
        m.borrow_mut().insert(field_id.to_string(), external);
    });
}

fn mirror_store_to_controlled(field_id: &str, value: &str) {
    let Some(ext) = controlled_of(field_id) else {
        return;
    };
    if ext.peek() != value {
        // Safe outside paint / AppState borrow (e.g. input_controller key path).
        ext.set(value.to_string());
    }
}

/// 写入值 store 并同步受控 Signal（供 `input_controller` / 程序化赋值）。
pub fn commit_value(field_id: &str, value: String) {
    let store = use_signal(&input_value_signal_key(field_id), String::new);
    if store.peek() != value {
        store.set(value.clone());
    } else {
        // Value unchanged in store; still push to controlled if it lagged.
    }
    mirror_store_to_controlled(field_id, &value);
}

/// 已持有 store [`Signal`] 时写入并同步受控端。
pub fn commit_value_sig(field_id: &str, store: Signal<String>, value: String) {
    if store.peek() != value {
        store.set(value.clone());
    }
    mirror_store_to_controlled(field_id, &value);
}

/// Paint 路径专用：失焦时把外部值 **silent** 拉进 store；焦点内不写外部（由 [`commit_value`] 负责）。
///
/// 返回 store 被外部改写后的新 committed（供当帧绘制）。
pub fn pull_controlled_into_store(field_id: &str, store_value: &str, is_focused: bool) -> Option<String> {
    if is_focused {
        return None;
    }
    let ext = controlled_of(field_id)?;
    let ev = ext.peek();
    if ev == store_value {
        return None;
    }
    let store = use_signal(&input_value_signal_key(field_id), String::new);
    store.set_silent(ev.clone());
    Some(ev)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::create_root;

    #[test]
    fn focus_prefixes() {
        assert_eq!(input_focus_id("a"), "input:a");
        assert_eq!(textarea_focus_id("a"), "textarea:a");
        assert_eq!(text_edit_rest("textarea:bio"), Some("bio"));
        assert_eq!(focus_id_for_text_edit("x", true), "textarea:x");
    }

    #[test]
    fn commit_mirrors_controlled() {
        let (_owner, ()) = create_root(|| {
            let ext = use_signal("tf_ext", || "seed".to_string());
            bind_controlled_value("tf1", ext);
            assert_eq!(
                use_signal(&input_value_signal_key("tf1"), String::new).peek(),
                "seed"
            );
            commit_value("tf1", "typed".into());
            assert_eq!(ext.peek(), "typed");
        });
    }

    #[test]
    fn caret_after_newline_maps_to_next_line_start() {
        assert_eq!(caret_line_and_local("a\n", 2), (1, 0));
        assert_eq!(caret_line_and_local("a\nb", 2), (1, 0));
        assert_eq!(caret_line_and_local("a\nb", 3), (1, 1));
        assert_eq!(caret_line_and_local("a", 1), (0, 1));
    }

    #[test]
    fn ensure_caret_visible_scrolls_when_below_viewport() {
        // line 4 at y=80..100, viewport 50 tall scrolled to 0 → need sy=50
        let text = "a\nb\nc\nd\ne";
        let caret = text.len();
        let new_sy = ensure_caret_visible_scroll_y(text, caret, 20.0, 50.0, 0.0, 50.0);
        assert_eq!(new_sy, Some(50.0));
        assert_eq!(
            ensure_caret_visible_scroll_y(text, caret, 20.0, 50.0, 50.0, 50.0),
            None
        );
    }

    #[test]
    fn ensure_caret_visible_skips_when_viewport_unready() {
        let text = "a\nb";
        // Unready viewport still clears a stale scroll offset (gallery squash).
        assert_eq!(
            ensure_caret_visible_scroll_y(text, text.len(), 20.0, 0.0, 40.0, 0.0),
            Some(0.0)
        );
        assert_eq!(
            ensure_caret_visible_scroll_y(text, text.len(), 20.0, 5.0, 0.0, 0.0),
            None
        );
    }

    #[test]
    fn ensure_caret_visible_clears_stale_sy_when_content_fits() {
        let text = "a\nb";
        assert_eq!(
            ensure_caret_visible_scroll_y(text, text.len(), 20.0, 100.0, 40.0, 0.0),
            Some(0.0)
        );
    }

    #[test]
    fn content_height_grows_with_hard_newlines_and_ink() {
        let size = 14.0;
        let lh = text_edit_line_box_px(size, None, None);
        let h1 = text_edit_content_height_px("啊", size, 400, lh, 400.0);
        let h2 = text_edit_content_height_px("啊\n啊", size, 400, lh, 400.0);
        assert!(h1 >= lh && h1 <= lh + text_edit_ink_pad_px(size) + 1.0);
        assert!(h2 > h1 + lh * 0.9, "h1={h1} h2={h2} lh={lh}");
    }
}
