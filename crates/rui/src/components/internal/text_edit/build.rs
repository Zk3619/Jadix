//! 树装配：单行 hstack chrome / 多行 scroll + 可选 resize 斜线手柄。

use super::strings::{compose_text_edit_strings, finish_selection_draw, TextEditStringsInput};
use crate::components::input::{
    effective_invalid, input_adornment_button, input_caret_signal_key, input_clear_template_id,
    input_ime_anchor_signal_key, input_number_dec_template_id, input_number_inc_template_id,
    input_number_step_button, input_number_steppers_column, input_overwrite_signal_key,
    input_password_reveal_signal_key, input_preedit_marks_signal_key, input_preedit_sel_signal_key,
    input_preedit_signal_key, input_reveal_template_id, input_selection_signal_key,
    input_text_template_id, input_type_signal_key, input_value_signal_key,
    map_byte_range_to_masked, resolve_placeholder, set_adornment_visible, InputType,
};
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::{parse_color, parse_text_color, Element, TextEditPaintSync};
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box, BoxBuilder};
use crate::style::InputCaretDraw;
use crate::tokens;
use crate::utils::text_field::{
    attach_text_edit_field_lifecycle, bind_controlled_value, extra_carets_signal,
    focus_id_for_text_edit,
};
use crate::utils::text_node::{build_text_node, TextNodeConfig};
pub(crate) struct TextEditBuildParams {
    pub field_id: String,
    pub placeholder: String,
    pub disabled: bool,
    pub invalid: bool,
    pub input_type: InputType,
    pub clearable: bool,
    pub password_toggle: bool,
    /// Show step triggers when `type=number`.
    pub number_steppers: bool,
    /// `type=number` Step amount.
    pub step: f64,
    pub text_size_px: f32,
    pub font_weight: u16,
    pub line_height_px: Option<f32>,
    pub leading_scale: Option<f32>,
    pub value: Option<Signal<String>>,
    /// CSS `text-align` (default `start`).
    pub text_align: crate::style::TextAlign,
    /// Single-line: preconfigured BoxBuilder (hstack chrome).
    /// Multi-line: ignore; build relative flex_col box inside.
    pub single_line_container: Option<BoxBuilder>,
    pub multiline: bool,
    pub rows: u32,
    pub resizable: bool,
    pub resize_max_h: f32,
    /// Multiline only: soft-wrap (default) or horizontal scroll.
    pub wrap_mode: crate::utils::text_field::TextareaWrapMode,
    /// Command / InputAdornment 内嵌控件：无边框/阴影/焦点环（外壳提供 chrome）。
    pub plain_chrome: bool,
    /// CSS `caret-color`. `None` → [`InputCaretDraw::default_caret_color`].
    pub caret_color: Option<[u8; 4]>,
    /// Selection highlight. `None` → [`InputCaretDraw::default_selection_bg`].
    pub selection_bg: Option<[u8; 4]>,
    /// Selected glyph color. `None` → [`InputCaretDraw::default_selection_foreground`].
    pub selection_foreground: Option<[u8; 4]>,
    pub selection_on_blur: crate::utils::text_field::SelectionOnBlur,
    pub undo: crate::utils::text_edit_history::TextUndoConfig,
    /// L1 Input/Textarea: drop undo on unmount. L3 CodeEditor: `false` (session lives in
    /// [`crate::utils::document_registry::DocumentRegistry`] until `unregister`).
    pub drop_undo_on_unmount: bool,
}
pub(crate) fn build_text_edit_element(params: TextEditBuildParams) -> Element {
    let TextEditBuildParams {
        field_id,
        placeholder: explicit_placeholder,
        disabled,
        invalid: explicit_invalid,
        input_type,
        clearable,
        password_toggle,
        number_steppers,
        step,
        text_size_px,
        font_weight,
        line_height_px,
        leading_scale,
        value,
        text_align,
        single_line_container,
        multiline,
        rows,
        resizable,
        resize_max_h,
        wrap_mode,
        plain_chrome,
        caret_color: caret_color_override,
        selection_bg: selection_bg_override,
        selection_foreground: selection_foreground_override,
        selection_on_blur,
        undo,
        drop_undo_on_unmount,
    } = params;

    if input_type.is_password() {
        crate::utils::text_edit_history::install_field_config(
            &field_id,
            crate::utils::text_edit_history::TextUndoConfig::password_default(),
        );
    } else {
        crate::utils::text_edit_history::install_field_config(&field_id, undo);
    }

    if let Some(sig) = value {
        bind_controlled_value(&field_id, sig);
    }

    let focus_key = "__rui_button_focus_id";
    let focus_visible_key = "__rui_focus_visible";
    let focused = use_signal(focus_key, String::new);
    let _ = use_signal(focus_visible_key, || false);

    let fid = focus_id_for_text_edit(&field_id, multiline);
    let is_focused = focused.peek() == fid && !disabled;

    // Clear on focus change even when the old field is offscreen. Silent writes
    // avoid a root rebuild during event dispatch; lifecycle releases the effect.
    crate::utils::text_field::install_selection_on_blur(
        &field_id,
        fid.clone(),
        selection_on_blur,
        disabled,
    );

    let value_sig = use_signal(&input_value_signal_key(&field_id), String::new);
    let preedit_sig = use_signal(&input_preedit_signal_key(&field_id), String::new);
    let preedit_sel_sig = use_signal(&input_preedit_sel_signal_key(&field_id), || {
        Option::<(usize, usize)>::None
    });
    let caret_sig = use_signal(&input_caret_signal_key(&field_id), || None::<usize>);
    let text_sel_sig = use_signal(&input_selection_signal_key(&field_id), || {
        None::<(usize, usize)>
    });
    let overwrite_sig = use_signal(&input_overwrite_signal_key(&field_id), || false);
    let ime_anchor_sig = use_signal(&input_ime_anchor_signal_key(&field_id), || None::<usize>);

    let committed = value_sig.peek();
    let preedit = preedit_sig.peek();
    let preedit_sel = preedit_sel_sig.peek();
    let ime_anchor = ime_anchor_sig.peek();

    let placeholder = resolve_placeholder(&explicit_placeholder, input_type);
    let invalid = effective_invalid(explicit_invalid, input_type, &committed);

    let show_placeholder = committed.is_empty() && preedit.is_empty();

    let reveal = if input_type.is_password() && password_toggle {
        use_signal(&input_password_reveal_signal_key(&field_id), || false).peek()
    } else {
        false
    };
    let mask = input_type.is_password() && !reveal && !show_placeholder;

    let caret_override = if is_focused && !show_placeholder && preedit.is_empty() {
        let len = committed.len();
        caret_sig.peek().map(|ix| ix.min(len))
    } else {
        None
    };

    let extras = extra_carets_signal(&field_id).peek();
    let preedit_marks = use_signal(
        &input_preedit_marks_signal_key(&field_id),
        Vec::<(usize, usize, u8)>::new,
    )
    .peek();
    let strings = compose_text_edit_strings(TextEditStringsInput {
        committed: &committed,
        preedit: &preedit,
        preedit_sel,
        preedit_marks: &preedit_marks,
        ime_anchor,
        extras: &extras,
        committed_caret: caret_sig.peek(),
        placeholder: &placeholder,
        show_placeholder,
        mask,
        is_focused,
        caret_override,
    });
    let display_text = strings.display_text;
    let layout_measure_text = strings.layout_measure_text;
    let caret_byte = strings.caret_byte;
    let real_display = strings.real_display;
    let caret_in_real = strings.caret_in_real;
    let preedit_underline = strings.preedit_underline;
    let extra_carets_draw = strings.extra_carets;
    let extra_selections_draw = strings.extra_selections;
    let extra_preedit_underlines = strings.extra_preedit_underlines;
    let preedit_ul = strings.preedit_ul;

    let mut text_color = if show_placeholder {
        parse_text_color("muted-foreground")
    } else {
        parse_text_color("foreground")
    };
    if disabled {
        text_color[3] = (text_color[3] as f32 * 0.5) as u8;
    }
    // Placeholder glyphs are muted; the I-beam stays on token `caret` unless overridden.
    let caret_color = caret_color_override.unwrap_or_else(InputCaretDraw::default_caret_color);

    let mut border_c = if plain_chrome {
        [0.0, 0.0, 0.0, 0.0]
    } else if invalid {
        parse_color("destructive")
    } else if is_focused {
        crate::style::focus_ring_border_color()
    } else {
        parse_color("input")
    };
    let mut bg = parse_color("transparent");
    if disabled {
        border_c[3] *= 0.5;
        bg[3] *= 0.5;
    }

    let pad_y = 0.5 * 16.0;
    let line_box = crate::utils::text_field::text_edit_line_box_px(
        text_size_px,
        line_height_px,
        leading_scale,
    );
    let default_h = crate::utils::text_field::textarea_default_height(rows, line_box, pad_y);
    let resize_min_h = default_h;
    let resize_max_h = resize_max_h.max(resize_min_h);
    let scroll_rest = if multiline {
        Some(crate::utils::text_field::textarea_scroll_rest(&field_id))
    } else {
        None
    };

    let mut container = if multiline {
        let h_key = crate::utils::text_field::textarea_height_signal_key(&field_id);
        let height = use_signal(&h_key, || default_h)
            .peek()
            .clamp(resize_min_h, resize_max_h);
        r#box()
            .w("100%")
            .min_w("0")
            .h(height)
            .relative(true)
            .flex_col()
            .flex_shrink(0.0)
            .rounded("0.375rem")
    } else {
        single_line_container
            .expect("single-line text edit requires single_line_container")
            .overflow("hidden")
    };
    if plain_chrome {
        container = container.bg(bg).border(border_c, 0.0);
    } else {
        container = container.bg(bg).border(border_c, 1.0);
        if !is_focused && !disabled {
            container = container.shadow(tokens::shadow_rgba(tokens::Elevation::Xs), 1.0);
        }
    }

    let mut text_el = build_text_node(
        display_text.clone(),
        TextNodeConfig {
            size_px: text_size_px,
            weight: font_weight,
            color: text_color,
            underline: false,
            underline_offset_px: None,
            text_align,
            line_height_px: Some(line_box),
            leading_scale: None,
            font_family: None,
        },
    );
    // Absolute line box + leading attr so cascade cannot inject leading-none.
    text_el = text_el.text_line_height(line_box);
    text_el.attrs.insert("leading".into(), "1".into());
    text_el.text_shape_suffix = None;
    text_el.template_id = Some(input_text_template_id(&field_id));
    text_el = text_el.w("100%").flex_shrink(0.0);
    if !multiline {
        // Value: Hidden (caret pan). Placeholder: ellipsis at inline-end (static hint).
        let overflow = if show_placeholder {
            crate::style::TextOverflow::editable_placeholder()
        } else {
            crate::style::TextOverflow::editable_single_line()
        };
        text_el = text_el
            .text_overflow(overflow)
            .min_w(0.0)
            .max_w("100%")
            .flex_shrink(1.0);
    }
    if multiline {
        let soft = wrap_mode.soft_wrap();
        if soft {
            // Soft-wrap long hard lines (HTML wrap=soft). Preserve hard `\n`.
            text_el = text_el.white_space("normal");
        } else {
            // Horizontal scroll (wrap=off): nowrap + scrollport overflow-x auto.
            text_el = text_el.white_space("nowrap");
        }
        text_el = text_el.text_wrap(wrap_mode);
        crate::core::reactivity::use_signal(
            &crate::utils::text_field::textarea_wrap_mode_signal_key(&field_id),
            || crate::text::WrapMode::Word,
        )
        .set_silent(wrap_mode);
        // Content-driven leaf (field-sizing-content). Do NOT use viewport/chrome height —
        // that coupled glyph soft_h to textarea resize. Ink only at content bottom.
        let avail_w = if soft {
            // Build-time width unknown; paint sync remeasures with live leaf width.
            // Use a modest placeholder so first layout is not unbounded-tall.
            320.0
        } else {
            10_000.0
        };
        let content_h = crate::utils::text_field::text_edit_content_height_px_wrap(
            &layout_measure_text,
            text_size_px,
            font_weight,
            line_box,
            avail_w,
            wrap_mode,
        );
        text_el = text_el.h(content_h).min_h(content_h);
        if !soft {
            // Grow with the longest hard line so overflow-x can scroll (never clip).
            let prepared = crate::text::prepare(&display_text, text_size_px, font_weight, line_box);
            let ink_w = crate::text::intrinsic_width(&prepared).max(1.0);
            let content_w = ink_w + crate::utils::text_field::TEXT_EDIT_CARET_W;
            text_el = text_el.w(content_w).min_w("100%");
        }
    } else {
        // Single-line: pin one line box so Input `items_center` centers the row.
        // Extra bottom ink lives in glyphon soft_h, not leaf height (else glyphs sit high).
        text_el = text_el.h(line_box);
    }

    if is_focused || selection_on_blur == crate::utils::text_field::SelectionOnBlur::Retain {
        let selection_bg =
            selection_bg_override.unwrap_or_else(InputCaretDraw::default_selection_bg);
        let selection_foreground = selection_foreground_override
            .unwrap_or_else(InputCaretDraw::default_selection_foreground);
        let sel_draw = if preedit.is_empty() {
            text_sel_sig.peek().and_then(|(a, b)| {
                if a.min(b) < a.max(b) {
                    let lo = a.min(b);
                    let hi = a.max(b);
                    Some(if mask {
                        map_byte_range_to_masked(&real_display, lo, hi, &display_text)
                    } else {
                        (lo, hi)
                    })
                } else {
                    None
                }
            })
        } else {
            None
        };
        let overwrite_highlight =
            if overwrite_sig.peek() && preedit.is_empty() && sel_draw.is_none() {
                let ix = real_display.floor_char_boundary(caret_in_real.min(real_display.len()));
                if ix < real_display.len() {
                    let end = ix + real_display[ix..].chars().next().unwrap().len_utf8();
                    Some(if mask {
                        map_byte_range_to_masked(&real_display, ix, end, &display_text)
                    } else {
                        (ix, end)
                    })
                } else {
                    None
                }
            } else {
                None
            };
        text_el.text_style.input_caret = Some(InputCaretDraw {
            caret_byte_index: caret_byte,
            selection: sel_draw,
            selection_bg,
            overwrite_highlight,
            preedit_underline,
            extra_carets: extra_carets_draw,
            extra_selections: extra_selections_draw,
            extra_preedit_underlines,
            preedit_ul,
            drop_caret: false,
            caret_color: Some(caret_color),
            selection_foreground: Some(selection_foreground),
        });
    }
    text_el.text_style.input_caret = finish_selection_draw(
        text_el.text_style.input_caret.take(),
        is_focused,
        selection_on_blur,
        text_color,
    );

    let text_region: Element = if multiline {
        // Absolute fill of chrome so the clip/scrollport always matches the field box.
        // Flex `h(100%)`+`min_h(0)` could leave a short clip while chrome bg stayed
        // tall — gallery: only line 1 visible until drag grew the clip window.
        // overflow-y auto + scroll viewport: content taller than chrome scrolls; paint
        // sync clamps stale sy when content shrinks so glyphs don't flatten.
        let rest = scroll_rest
            .as_deref()
            .expect("multiline text edit always has scroll_rest");
        let mut region = Element::new()
            .absolute()
            .inset(0.0)
            .w("100%")
            .h("100%")
            .flex_col()
            .flex_items("start")
            .py("0.5rem")
            .child(text_el);
        if wrap_mode.soft_wrap() {
            region = region.px("0.75rem");
        } else {
            // Keep inline padding stationary while the text pans inside it.
            region = region.left("0.75rem").right("0.75rem").w("auto");
        }
        let ox = if wrap_mode.soft_wrap() {
            // Soft-wrap: no horizontal overflow of ink; hidden (not clip) on x is fine.
            "hidden"
        } else {
            // wrap=off: scroll long lines — never clip.
            "auto"
        };
        region = crate::core::scroll_viewport::activate_scroll_viewport(
            region, rest, ox, "scroll", "smooth", "area",
        );
        region.template_id = Some(format!("__rui_textarea:{field_id}:region"));
        region
    } else {
        hstack()
            .flex_grow(1.0)
            .min_w(crate::utils::text_field::TEXT_EDIT_CARET_W)
            .items_center()
            .overflow_x("hidden")
            .overflow_y("visible")
            .child(text_el)
            .into()
    };

    let show_password_toggle = !multiline && password_toggle && input_type.is_password();
    let show_number_steppers = !multiline && number_steppers && input_type.is_number();
    let has_adornments = !multiline && (clearable || show_password_toggle || show_number_steppers);
    if has_adornments {
        container = container.gap("0.25rem");
    }

    let resizable = multiline && resizable && !disabled;

    let mut el: Element = container.into();
    if is_focused && !plain_chrome {
        let ring_color = if invalid {
            crate::style::focus_ring_destructive_color()
        } else {
            crate::style::focus_ring_color()
        };
        crate::style::apply_focus_ring_shadow_color(&mut el, true, false, ring_color);
    }
    el = el.child(text_region);

    if multiline {
        // Overlay Y scrollbar (ScrollArea-style). Visibility auto: show on hover/scroll
        // when content overflows the chrome.
        let rest = scroll_rest
            .as_deref()
            .expect("multiline text edit always has scroll_rest");
        use crate::components::scrollbar::{scrollbar, ScrollbarOrientation};
        use crate::core::scroll_viewport::scroll_scrollbar_inset_y_signal_key;
        use_signal(&scroll_scrollbar_inset_y_signal_key(rest), || {
            "inside".to_string()
        })
        .set("inside".to_string());
        let vbar: Element = scrollbar(&format!("__rui_v_scrollbar_{rest}"))
            .target(rest.to_string())
            .orientation(ScrollbarOrientation::Vertical)
            .visibility("auto")
            .thickness(8.0)
            .overlay(true)
            .z_index(1000)
            .top(4.0)
            .bottom(2.0) // minimal inset; resize grip overlays the corner (z below track)
            .right(2.0)
            .into();
        el = el.child(vbar);
        if !wrap_mode.soft_wrap() {
            use crate::core::scroll_viewport::scroll_scrollbar_inset_x_signal_key;
            use_signal(&scroll_scrollbar_inset_x_signal_key(rest), || {
                "inside".to_string()
            })
            .set("inside".to_string());
            // Edge pad only — do not bake the vertical thickness into `right`.
            // Runtime adds the L-join lane when Y actually occupies the corner.
            let mut hbar: Element = scrollbar(&format!("__rui_h_scrollbar_{rest}"))
                .target(rest.to_string())
                .orientation(ScrollbarOrientation::Horizontal)
                .visibility("auto")
                .thickness(8.0)
                .overlay(true)
                .z_index(1000)
                .left(4.0)
                .right(crate::components::scrollbar::SCROLLBAR_EDGE_PAD)
                .bottom(2.0)
                .into();
            hbar.set_attr(
                crate::components::scrollbar::ATTR_H_LANE_INSET,
                crate::components::scrollbar::SCROLLBAR_EDGE_PAD.to_string(),
            );
            hbar.set_attr(crate::components::scrollbar::ATTR_H_LANE_V_CROSS, "8");
            el = el.child(hbar);
        }
    }

    if resizable {
        let resize_fid = crate::utils::text_field::textarea_resize_focus_id(&field_id);
        // ShadCN / solid-ui / Browser native style: two 45° slashes (/), one long and one short.
        // When the wheelbase corner point is `d`, the line length = d√2, the center distance is d/2 to the right/bottom; the endpoints fall on the bottom and right sides.
        // chrome 圆角 0.375rem≈6px：d 必须 > 圆角，否则端点落在圆角外侧被裁成角尖短划。
        // `rotate` 只作用于本节点绘制实例，故每条线各自旋转。
        let mut stroke = parse_color("muted-foreground");
        stroke[3] *= 0.65;
        const GRIP: f32 = 14.0;
        const D_LONG: f32 = 11.0;
        const D_SHORT: f32 = 7.0;
        let grip_line = |d: f32| -> Element {
            let len = d * std::f32::consts::SQRT_2;
            Element::from(
                r#box()
                    .absolute(true)
                    .left(GRIP - d * 0.5 - len * 0.5)
                    .top(GRIP - d * 0.5 - 0.5)
                    .w(len)
                    .h(1.0)
                    .bg(stroke)
                    .rounded(0.5)
                    .pointer_events("none"),
            )
            .rotate(-45.0)
        };
        let mut grip: Element = r#box()
            .absolute(true)
            .right(0.0)
            .bottom(0.0)
            .w(GRIP)
            .h(GRIP)
            .z_index(2)
            .child(grip_line(D_LONG))
            .child(grip_line(D_SHORT))
            .into();
        grip = grip
            .focus_id(resize_fid)
            .cursor_kind(crate::core::tree::CursorKind::NwseResize);
        grip.template_id = Some(format!("__rui_textarea:{field_id}:resize"));
        grip.attrs
            .insert("__rui_ta_resize_min".into(), format!("{resize_min_h}"));
        grip.attrs
            .insert("__rui_ta_resize_max".into(), format!("{resize_max_h}"));
        el = el.child(grip);
    }

    if !multiline && clearable {
        let fid_clear = fid.clone();
        let focus_key_clear = focus_key;
        let id_clear = field_id.clone();
        let disabled_clear = disabled;
        let mut clear_btn = input_adornment_button(
            input_clear_template_id(&field_id),
            "x",
            disabled_clear,
            move || {
                if disabled_clear {
                    return;
                }
                #[cfg(feature = "text-undo")]
                crate::utils::text_edit_history::will_user_edit_field(
                    &id_clear,
                    crate::utils::text_edit_history::EditKind::Replace,
                );
                crate::utils::text_field::commit_value(&id_clear, String::new());
                use_signal(&input_preedit_signal_key(&id_clear), String::new).set(String::new());
                use_signal(&input_preedit_sel_signal_key(&id_clear), || {
                    None::<(usize, usize)>
                })
                .set(None);
                use_signal(&input_caret_signal_key(&id_clear), || None::<usize>).set(Some(0));
                use_signal(&input_selection_signal_key(&id_clear), || {
                    None::<(usize, usize)>
                })
                .set(None);
                crate::utils::text_field::clear_extra_carets(&id_clear);
                use_signal(&input_ime_anchor_signal_key(&id_clear), || None::<usize>).set(None);
                #[cfg(feature = "text-undo")]
                crate::utils::text_edit_history::note_field_after_edit(&id_clear);
                // Drop system candidate / composition — clearing value alone leaves TIP UI up.
                crate::app::ime_support::cancel_os_ime_composition();
                use_signal(focus_key_clear, String::new).set(fid_clear.clone());
                use_signal("__rui_focus_visible", || false).set(true);
            },
        );
        let show_clear = (!committed.is_empty() || !preedit.is_empty()) && !disabled;
        let _ = set_adornment_visible(&mut clear_btn, show_clear);
        el = el.child(clear_btn);
    }

    if show_password_toggle {
        let fid_rev = fid.clone();
        let focus_key_rev = focus_key;
        let id_rev = field_id.clone();
        let disabled_rev = disabled;
        let eye_name = if reveal { "eye-off" } else { "eye" };
        let mut reveal_btn = input_adornment_button(
            input_reveal_template_id(&field_id),
            eye_name,
            disabled_rev,
            move || {
                if disabled_rev {
                    return;
                }
                let sig = use_signal(&input_password_reveal_signal_key(&id_rev), || false);
                sig.set(!sig.peek());
                use_signal(focus_key_rev, String::new).set(fid_rev.clone());
                use_signal("__rui_focus_visible", || false).set(true);
            },
        );
        let _ = set_adornment_visible(&mut reveal_btn, is_focused && !disabled);
        el = el.child(reveal_btn);
    }

    if show_number_steppers {
        // solid-ui NumberField：右侧上下 size-4 增减触发器；支持长按连发。
        let step = if step.is_finite() && step > 0.0 {
            step
        } else {
            1.0
        };
        let inc = input_number_step_button(
            input_number_inc_template_id(&field_id),
            "chevron-up",
            disabled,
            field_id.clone(),
            fid.clone(),
            step,
        );
        let dec = input_number_step_button(
            input_number_dec_template_id(&field_id),
            "chevron-down",
            disabled,
            field_id.clone(),
            fid.clone(),
            -step,
        );
        let steppers = input_number_steppers_column(inc, dec);
        el = el.child(steppers);
    }

    el = if disabled {
        el.cursor_not_allowed()
    } else {
        el.cursor_text()
    };

    let type_key = input_type_signal_key(&field_id);
    use_signal(&type_key, || input_type.as_str().to_string()).set(input_type.as_str().to_string());

    let sync = TextEditPaintSync {
        base_id: field_id.clone(),
        placeholder: placeholder.clone(),
        invalid: explicit_invalid,
        disabled,
        text_size_px,
        font_weight,
        line_height_px,
        leading_scale,
        input_type: input_type.as_str().to_string(),
        clearable: !multiline && clearable,
        password_toggle: show_password_toggle,
        multiline,
        scroll_rest,
        resizable,
        resize_min_h,
        resize_max_h,
        plain_chrome,
        text_align,
        wrap_mode,
        caret_color: caret_color_override,
        selection_bg: selection_bg_override,
        selection_foreground: selection_foreground_override,
        selection_on_blur,
    };

    let id_for_click = fid.clone();
    let mut out: Element = interactive(el)
        .focus_id(fid)
        .focusable(!disabled)
        // Group shell owns the ring (`InputAdornment` / Command). `focusable`
        // also latches `focus_ring`; plain_chrome must opt out.
        .focus_ring(!disabled && !plain_chrome)
        .on_click(move || {
            if !disabled {
                use_signal(focus_key, String::new).set(id_for_click.clone());
                use_signal(focus_visible_key, || false).set(true);
            }
        })
        .into();
    out.template_id = Some(field_id.clone());
    out.input_paint_sync = Some(sync);
    out.attrs
        .insert("accesskit_role".to_string(), "textinput".to_string());
    if !placeholder.is_empty() {
        out.attrs.insert("placeholder".to_string(), placeholder);
    }
    if plain_chrome {
        out.set_attr("data-slot", "input-adornment-control");
        if invalid {
            out.set_attr("data-invalid", "true");
        }
    }
    attach_text_edit_field_lifecycle(out, &field_id, drop_undo_on_unmount)
}
