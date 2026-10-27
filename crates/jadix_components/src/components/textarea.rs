//! # Textarea
//!
//! 多行文本输入的 **样式壳**：编辑能力在 [`crate::utils::text_field`]，
//! 共享 chrome 构建/绘制在 [`crate::utils::text_edit`]。
//! `focus_id` 为 `textarea:{id}`；Enter 插入换行（见 `input_controller`）。
//!
//! ## Wrap modes ([`crate::text::WrapMode`])
//!
//! Soft wrap **never** inserts `\n` into the value — only visual rows
//! ([`crate::text::VisualLineModel`]). Default is [`WrapMode::Word`].
//! - `wrap("word"|"soft")` — word-boundary soft wrap (default)
//! - `wrap("char")` — mid-word when needed; falls back to Word under BiDi/WJ
//! - `wrap("off"|"none"|"scroll")` — no soft wrap; horizontal scroll (not clip)
//!
//! 内容超出可视区时在内部滚动视口滚动；默认右下角可拖拽改高（斜线手柄）。
//! 超长文档（≥64KB / ≥2048 视觉行）自动启用高度估算虚拟化 MVP。
//! [`TextareaBuilder::resizable`] 为 `false` 时禁止改高并隐藏手柄。
//!
//! Caret / 选区色：[`TextareaBuilder::caret_color`] / [`TextareaBuilder::selection_background`] /
//! [`TextareaBuilder::selection_foreground`]，与 Input 相同（默认 token `caret` /
//! `selection-background` / `selection-foreground`）。

use std::sync::atomic::{AtomicU64, Ordering};

use crate::authoring::widgets::text_edit::{build_text_edit_element, TextEditBuildParams};
use crate::components::base::SlotChrome;
use crate::components::input::InputType;
use crate::core::reactivity::Signal;
use crate::core::tree::Element;
use crate::utils::text_field::TextareaWrapMode;

pub use crate::utils::text_field::{textarea_focus_id, SelectionOnBlur, TEXTAREA_FOCUS_PREFIX};

const DEFAULT_TEXT_SIZE_PX: f32 = 0.875 * 16.0;
const DEFAULT_FONT_WEIGHT: u16 = 400;
const DEFAULT_RESIZE_MAX_H: f32 = 480.0;

/// Signal key reuse: value/caret/selection share Input's key helpers by id.
pub fn textarea(id: &str) -> TextareaBuilder {
    TextareaBuilder {
        id: id.to_string(),
        placeholder: String::new(),
        rows: 4,
        disabled: false,
        invalid: bool::default(),
        value: None,
        resizable: true,
        resize_max_h: None,
        wrap_mode: TextareaWrapMode::Word,
        plain_chrome: false,
        caret_color: None,
        selection_background: None,
        selection_foreground: None,
        selection_on_blur: SelectionOnBlur::Clear,
        ring_width: None,
        ring_color: None,
        chrome: SlotChrome::default(),
        #[cfg(feature = "text-undo")]
        undo: crate::utils::text_edit_history::TextUndoConfig::textarea_default(),
    }
}

pub struct TextareaBuilder {
    id: String,
    placeholder: String,
    rows: u32,
    disabled: bool,
    invalid: bool,
    value: Option<Signal<String>>,
    resizable: bool,
    resize_max_h: Option<f32>,
    wrap_mode: TextareaWrapMode,
    plain_chrome: bool,
    caret_color: Option<[u8; 4]>,
    selection_background: Option<[u8; 4]>,
    selection_foreground: Option<[u8; 4]>,
    selection_on_blur: SelectionOnBlur,
    ring_width: Option<f32>,
    ring_color: Option<[f32; 4]>,
    chrome: SlotChrome,
    #[cfg(feature = "text-undo")]
    undo: crate::utils::text_edit_history::TextUndoConfig,
}

impl TextareaBuilder {
    crate::impl_slot_chrome_methods!();

    pub fn placeholder(mut self, s: impl Into<String>) -> Self {
        self.placeholder = s.into();
        self
    }

    pub fn rows(mut self, n: u32) -> Self {
        self.rows = n.max(1);
        self
    }

    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }

    pub fn invalid(mut self, v: bool) -> Self {
        self.invalid = v;
        self
    }

    /// 右下角拖拽斜线手柄改高；默认开启。`false` 时禁止改高并隐藏手柄。
    pub fn resizable(mut self, yes: bool) -> Self {
        self.resizable = yes;
        self
    }

    pub fn resize_max_h(mut self, px: f32) -> Self {
        self.resize_max_h = Some(px.max(32.0));
        self
    }

    /// 单行溢出：`word`/`soft`（默认软换行）| `char`（中词断开）| `off`/`none`/`scroll`（横向滚动）。
    pub fn wrap(mut self, mode: impl Into<TextareaWrapMode>) -> Self {
        self.wrap_mode = mode.into();
        self
    }

    /// Controlled value: see [`crate::utils::text_field::bind_controlled_value`].
    pub fn value_signal(mut self, s: Signal<String>) -> Self {
        self.value = Some(s);
        self
    }

    #[cfg(feature = "text-undo")]
    pub fn undo_enabled(mut self, enabled: bool) -> Self {
        self.undo.enabled = enabled;
        self
    }

    #[cfg(feature = "text-undo")]
    pub fn max_undo_entries(mut self, n: usize) -> Self {
        self.undo.max_entries = n;
        self
    }

    #[cfg(feature = "text-undo")]
    pub fn max_undo_memory(mut self, bytes: usize) -> Self {
        self.undo.max_bytes = bytes;
        self
    }

    #[cfg(feature = "text-undo")]
    pub fn typing_merge_timeout_ms(mut self, ms: u64) -> Self {
        self.undo.typing_merge = std::time::Duration::from_millis(ms);
        self
    }

    /// 去边框/阴影/焦点环，供 [`crate::components::input_adornment`] 外壳使用。
    pub fn plain_chrome(mut self, v: bool) -> Self {
        self.plain_chrome = v;
        self
    }

    /// CSS `caret-color`. Tokens / hex / `[u8; 4]`. Default theme token `caret`.
    pub fn caret_color(mut self, color: impl crate::style::IntoTextColor) -> Self {
        self.caret_color = Some(color.into_text_color());
        self
    }

    /// Selection highlight fill. Tokens / hex / `[u8; 4]` used as-is (including alpha).
    /// Default is theme token `selection-background`.
    pub fn selection_background(mut self, color: impl crate::style::IntoTextColor) -> Self {
        self.selection_background = Some(color.into_text_color());
        self
    }

    /// Selected glyph color (CSS `::selection { color }`). Tokens / hex / `[u8; 4]`
    /// used as-is. Default is theme token `selection-foreground` (inverse of
    /// `selection-background`).
    pub fn selection_foreground(mut self, color: impl crate::style::IntoTextColor) -> Self {
        self.selection_foreground = Some(color.into_text_color());
        self
    }

    /// `clear` (default) clears on blur; `retain` keeps a muted selection.
    pub fn selection_on_blur(mut self, policy: impl Into<SelectionOnBlur>) -> Self {
        self.selection_on_blur = policy.into();
        self
    }

    /// Focus-ring thickness in logical px. Overrides the theme default.
    pub fn ring_width(mut self, width: impl crate::core::tree::IntoF32) -> Self {
        self.ring_width = Some(width.into_f32().max(0.0));
        self
    }

    /// Focus-ring color. Tokens / hex / `[f32; 4]`. Overrides theme token `ring`.
    pub fn ring_color(mut self, color: impl crate::style::IntoPaintColor) -> Self {
        self.ring_color = Some(color.into_paint_color());
        self
    }
}

impl From<TextareaBuilder> for Element {
    fn from(b: TextareaBuilder) -> Element {
        let TextareaBuilder {
            id,
            placeholder,
            rows,
            disabled,
            invalid,
            value,
            resizable,
            resize_max_h,
            wrap_mode,
            plain_chrome,
            caret_color,
            selection_background,
            selection_foreground,
            selection_on_blur,
            ring_width,
            ring_color,
            chrome,
            #[cfg(feature = "text-undo")]
            undo,
        } = b;

        let field_id = if id.is_empty() {
            static RT_EMPTY_TEXTAREA_ID: AtomicU64 = AtomicU64::new(0);
            format!(
                "__jadix_textarea_rt_{}",
                RT_EMPTY_TEXTAREA_ID.fetch_add(1, Ordering::Relaxed)
            )
        } else {
            id
        };

        let resize_max_h = resize_max_h.unwrap_or(DEFAULT_RESIZE_MAX_H);

        let mut el = chrome.apply(build_text_edit_element(TextEditBuildParams {
            field_id,
            placeholder,
            disabled,
            invalid,
            input_type: InputType::Text,
            clearable: false,
            password_toggle: false,
            number_steppers: false,
            step: 1.0,
            text_size_px: DEFAULT_TEXT_SIZE_PX,
            font_weight: DEFAULT_FONT_WEIGHT,
            line_height_px: None,
            leading_scale: None,
            value,
            text_align: crate::style::TextAlign::Start,
            single_line_container: None,
            multiline: true,
            rows,
            resizable: resizable && !plain_chrome,
            resize_max_h,
            wrap_mode,
            plain_chrome,
            drop_undo_on_unmount: true,
            caret_color,
            selection_bg: selection_background,
            selection_foreground,
            selection_on_blur,
            undo: {
                #[cfg(feature = "text-undo")]
                {
                    undo
                }
                #[cfg(not(feature = "text-undo"))]
                {
                    crate::utils::text_edit_history::TextUndoConfig::textarea_default()
                }
            },
        }));
        if let Some(w) = ring_width {
            el.style.focus_ring_width = Some(w);
        }
        if let Some(c) = ring_color {
            el.style.focus_ring_color = Some(c);
        }
        el
    }
}

crate::__component_contract! {
TextareaProps [];
#[allow(non_snake_case)]
pub fn Textarea(
identity [] id: String,
default [] placeholder: String,
plain [] rows: Option<i32>,
default [] disabled: bool,
default [] invalid: bool,
plain [] resizable: Option<bool>,
plain [] resize_max_h: Option<f32>,
plain [] wrap: Option<String>,
plain [] selection_on_blur: Option<String>,
plain [] value: Option<Signal<String>>,
plain [] undo_enabled: Option<bool>,
plain [] max_undo_entries: Option<u32>,
plain [] max_undo_memory: Option<u32>,
plain [] typing_merge_timeout_ms: Option<u32>,
plain [] children: Option<Element>,
) -> Element {
    let _ = children;
    let mut b = textarea(&id)
        .placeholder(placeholder)
        .disabled(disabled)
        .invalid(invalid);
    if let Some(r) = rows {
        b = b.rows(r.max(1) as u32);
    }
    if let Some(r) = resizable {
        b = b.resizable(r);
    }
    if let Some(h) = resize_max_h {
        b = b.resize_max_h(h);
    }
    if let Some(w) = wrap {
        b = b.wrap(w);
    }
    if let Some(policy) = selection_on_blur {
        b = b.selection_on_blur(policy);
    }
    if let Some(sig) = value {
        b = b.value_signal(sig);
    }
    #[cfg(feature = "text-undo")]
    {
        if let Some(v) = undo_enabled {
            b = b.undo_enabled(v);
        }
        if let Some(n) = max_undo_entries {
            b = b.max_undo_entries(n as usize);
        }
        if let Some(n) = max_undo_memory {
            b = b.max_undo_memory(n as usize);
        }
        if let Some(ms) = typing_merge_timeout_ms {
            b = b.typing_merge_timeout_ms(ms as u64);
        }
    }
    #[cfg(not(feature = "text-undo"))]
    let _ = (
        undo_enabled,
        max_undo_entries,
        max_undo_memory,
        typing_merge_timeout_ms,
    );
    b.into()
}
}

#[cfg(all(test, feature = "product-tests"))]
mod tests {
    use super::*;
    use crate::components::input::{input_caret_signal_key, input_value_signal_key};
    use crate::core::reactivity::use_signal;
    use crate::utils::text_field::{textarea_resize_focus_id, TEXTAREA_RESIZE_FOCUS_PREFIX};

    fn find_focus(el: &Element, want: &str) -> bool {
        if el.focus_id.as_deref() == Some(want) {
            return true;
        }
        el.children.iter().any(|c| find_focus(c, want))
    }

    #[test]
    fn textarea_focus_prefix() {
        assert_eq!(textarea_focus_id("bio"), "textarea:bio");
        let el: Element = textarea("bio").rows(3).placeholder("Say hi").into();
        assert_eq!(el.focus_id.as_deref(), Some("textarea:bio"));
    }

    #[test]
    fn textarea_default_resize_grip() {
        let el: Element = textarea("bio").rows(3).into();
        let grip = textarea_resize_focus_id("bio");
        assert!(grip.starts_with(TEXTAREA_RESIZE_FOCUS_PREFIX));
        assert!(find_focus(&el, &grip), "expected resize grip focus_id");
    }

    #[test]
    fn textarea_resizable_false_omits_grip() {
        let el: Element = textarea("bio").resizable(false).into();
        assert!(!find_focus(&el, &textarea_resize_focus_id("bio")));
    }

    #[test]
    fn textarea_default_leaf_soft_wraps() {
        use crate::style::WhiteSpace;
        use crate::utils::text_field::input_text_template_id;

        fn find_tid<'a>(el: &'a Element, tid: &str) -> Option<&'a Element> {
            if el.template_id.as_deref() == Some(tid) {
                return Some(el);
            }
            el.children.iter().find_map(|c| find_tid(c, tid))
        }

        let id = "ta-soft";
        let el: Element = textarea(id).rows(3).into();
        let tid = input_text_template_id(id);
        let leaf = find_tid(&el, &tid).expect("text leaf");
        assert_eq!(leaf.text_style.white_space, WhiteSpace::Normal);
    }

    #[test]
    fn textarea_wrap_off_is_nowrap() {
        use crate::style::WhiteSpace;
        use crate::utils::text_field::input_text_template_id;

        fn find_tid<'a>(el: &'a Element, tid: &str) -> Option<&'a Element> {
            if el.template_id.as_deref() == Some(tid) {
                return Some(el);
            }
            el.children.iter().find_map(|c| find_tid(c, tid))
        }

        let id = "ta-nowrap";
        let el: Element = textarea(id).rows(3).wrap("off").into();
        let tid = input_text_template_id(id);
        let leaf = find_tid(&el, &tid).expect("text leaf");
        assert_eq!(leaf.text_style.white_space, WhiteSpace::Nowrap);
    }

    #[test]
    fn textarea_value_signal_keys() {
        let _ = use_signal(&input_value_signal_key("t1"), String::new);
        let _ = use_signal(&input_caret_signal_key("t1"), || None::<usize>);
        let el: Element = textarea("t1").into();
        assert_eq!(el.focus_id.as_deref(), Some("textarea:t1"));
    }

    #[test]
    fn builder_style_reaches_textarea() {
        let el: Element = textarea("ta-style").style(|b| b.text_size(20.0)).into();
        assert!(
            el.text_style.size.is_some_and(|s| (s - 20.0).abs() < 0.01)
                || el
                    .children
                    .iter()
                    .any(|c| c.text_style.size.is_some_and(|s| (s - 20.0).abs() < 0.01)),
            "textarea .style(|b| b.text_size(20)) must land on the field"
        );
    }
}
