//! Native document text editing without form-field chrome.

use std::rc::Rc;

use crate::components::base::SlotChrome;
use crate::components::input::InputType;
use crate::core::reactivity::Signal;
use crate::core::tree::Element;
use crate::utils::text_edit::{build_text_edit_element, TextEditBuildParams};
use crate::utils::text_edit_history::TextUndoConfig;
use crate::utils::text_field::{SelectionOnBlur, TextareaWrapMode};
use jadix_macros::component;
use winit::event::KeyEvent;
use winit::keyboard::ModifiersState;

type EditorKeyHandler = crate::authoring::text::KeyHandler;

pub fn text_editor(id: impl Into<String>) -> TextEditorBuilder {
    TextEditorBuilder {
        id: id.into(),
        value: None,
        rows: 12,
        font_size: 14.0,
        wrap: TextareaWrapMode::Word,
        disabled: false,
        undo: true,
        key_handler: None,
        auto_height: None,
        line_numbers: false,
        language: None,
        chrome: SlotChrome::default(),
    }
}

pub struct TextEditorBuilder {
    id: String,
    value: Option<Signal<String>>,
    rows: u32,
    font_size: f32,
    wrap: TextareaWrapMode,
    disabled: bool,
    undo: bool,
    key_handler: Option<EditorKeyHandler>,
    auto_height: Option<f32>,
    line_numbers: bool,
    language: Option<String>,
    chrome: SlotChrome,
}

impl TextEditorBuilder {
    crate::impl_slot_chrome_methods!();
    pub fn line_numbers(mut self, enabled: bool) -> Self {
        self.line_numbers = enabled;
        self
    }
    pub fn language(mut self, language: impl Into<String>) -> Self {
        self.language = Some(language.into());
        self
    }
    pub fn value_signal(mut self, value: Signal<String>) -> Self {
        self.value = Some(value);
        self
    }
    pub fn rows(mut self, rows: u32) -> Self {
        self.rows = rows.max(1);
        self
    }
    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = size.clamp(8.0, 96.0);
        self
    }
    pub fn wrap(mut self, wrap: impl Into<TextareaWrapMode>) -> Self {
        self.wrap = wrap.into();
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn undo_enabled(mut self, enabled: bool) -> Self {
        self.undo = enabled;
        self
    }
    pub fn auto_height(mut self, maximum: f32) -> Self {
        self.auto_height = Some(maximum.clamp(40.0, 4096.0));
        self
    }
    pub fn key_handler(
        mut self,
        handler: impl Fn(&KeyEvent, ModifiersState) -> bool + 'static,
    ) -> Self {
        self.key_handler = Some(Rc::new(handler));
        self
    }
}

impl From<TextEditorBuilder> for Element {
    fn from(builder: TextEditorBuilder) -> Self {
        if let Some(maximum) = builder.auto_height {
            let text = builder.value.map(|value| value.peek()).unwrap_or_else(|| {
                crate::core::reactivity::use_signal(
                    &crate::utils::text_field::input_value_signal_key(&builder.id),
                    String::new,
                )
                .peek()
            });
            let width = crate::core::reactivity::use_signal(
                &crate::utils::text_field::textarea_wrap_width_signal_key(&builder.id),
                || 640.0_f32,
            )
            .peek()
            .max(80.0);
            let height = auto_height(&text, builder.font_size, builder.wrap, width, maximum);
            crate::core::reactivity::use_signal(
                &crate::utils::text_field::textarea_height_signal_key(&builder.id),
                || height,
            )
            .set_silent(height);
            let scroll = crate::utils::text_field::textarea_scroll_rest(&builder.id);
            crate::core::reactivity::use_signal(
                &crate::core::scroll_viewport::scroll_viewport_h_signal_key(&scroll),
                || height,
            )
            .set_silent(height);
        }
        let mut undo = TextUndoConfig::textarea_default();
        undo.enabled = builder.undo;
        let mut element = build_text_edit_element(TextEditBuildParams {
            field_id: builder.id.clone(),
            placeholder: String::new(),
            disabled: builder.disabled,
            invalid: false,
            input_type: InputType::Text,
            clearable: false,
            password_toggle: false,
            number_steppers: false,
            step: 1.0,
            text_size_px: builder.font_size,
            font_weight: 400,
            line_height_px: Some(builder.font_size * 1.5),
            leading_scale: None,
            value: builder.value,
            text_align: crate::style::TextAlign::Start,
            single_line_container: None,
            multiline: true,
            rows: builder.rows,
            resizable: false,
            resize_max_h: 4096.0,
            wrap_mode: builder.wrap,
            plain_chrome: true,
            caret_color: None,
            selection_bg: None,
            selection_foreground: None,
            selection_on_blur: SelectionOnBlur::Retain,
            undo,
            drop_undo_on_unmount: true,
        });
        element.set_attr("data-slot", "text-editor");
        if let Some(maximum) = builder.auto_height {
            let id = builder.id.clone();
            let size = builder.font_size;
            let wrap = builder.wrap;
            element = element.reactive_sync(move |element, _layout| {
                let text = crate::core::reactivity::use_signal(
                    &crate::utils::text_field::input_value_signal_key(&id),
                    String::new,
                )
                .peek();
                let width = (element.layout_bounds[2] - 16.0).max(80.0);
                let height = auto_height(&text, size, wrap, width, maximum);
                crate::core::reactivity::use_signal(
                    &crate::utils::text_field::textarea_height_signal_key(&id),
                    || height,
                )
                .set_silent(height);
                let previous =
                    crate::layout::dim_length_px(element.style.size.height).unwrap_or(-1.0);
                if (previous - height).abs() > 0.5 {
                    *element = std::mem::take(element).h(height);
                    crate::core::tree::LayoutStatus::LayoutChanged
                } else {
                    crate::core::tree::LayoutStatus::Clean
                }
            });
        }
        element.text_edit_key_handler = builder.key_handler;
        let element = crate::components::editor_chrome::decorate_editor(
            element,
            &builder.id,
            builder.line_numbers,
            builder.language,
            1,
        );
        builder.chrome.apply(element)
    }
}

fn auto_height(text: &str, size: f32, wrap: TextareaWrapMode, width: f32, maximum: f32) -> f32 {
    let wrap = match wrap {
        TextareaWrapMode::None => crate::text::WrapMode::None,
        TextareaWrapMode::Char => crate::text::WrapMode::Char,
        _ => crate::text::WrapMode::Word,
    };
    let measured = crate::text::visual_lines::textarea_visual_content_height(
        text,
        size,
        400,
        size * 1.5,
        width,
        wrap,
        4.0,
    );
    (measured.content_height + 16.0).clamp(size * 1.5 + 16.0, maximum)
}

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn TextEditor(
    id: String,
    value: Option<Signal<String>>,
    rows: Option<i32>,
    wrap: Option<String>,
) -> Element {
    let mut editor = text_editor(id);
    if let Some(value) = value {
        editor = editor.value_signal(value);
    }
    if let Some(rows) = rows {
        editor = editor.rows(rows.max(1) as u32);
    }
    if let Some(wrap) = wrap {
        editor = editor.wrap(TextareaWrapMode::parse(&wrap));
    }
    editor.into()
}
