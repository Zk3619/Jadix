//! 树装配：单行 hstack chrome / 多行 scroll + 可选 resize grip。

use crate::components::input::{
    effective_invalid, input_adornment_button, input_caret_signal_key, input_clear_template_id,
    input_ime_anchor_signal_key, input_overwrite_signal_key, input_password_reveal_signal_key,
    input_preedit_sel_signal_key, input_preedit_signal_key, input_reveal_template_id,
    input_selection_signal_key, input_text_template_id, input_type_signal_key,
    input_value_signal_key, lucide_named, map_byte_index_to_masked, map_byte_range_to_masked,
    mask_password, resolve_placeholder, set_adornment_visible, InputType,
};
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::{parse_color, parse_text_color, Element, TextEditPaintSync};
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box, BoxBuilder};
use crate::style::InputCaretDraw;
use crate::utils::text_field::{bind_controlled_value, focus_id_for_text_edit};
use crate::utils::text_node::{build_text_node, TextNodeConfig};
pub(crate) struct TextEditBuildParams {
    pub field_id: String,
    pub placeholder: String,
    pub disabled: bool,
    pub invalid: bool,
    pub input_type: InputType,
    pub clearable: bool,
    pub password_toggle: bool,
    pub text_size_px: f32,
    pub font_weight: u16,
    pub line_height_px: Option<f32>,
    pub leading_scale: Option<f32>,
    pub value: Option<Signal<String>>,
    /// Single-line: preconfigured BoxBuilder (hstack chrome).
    /// Multi-line: ignore; build relative flex_col box inside.
    pub single_line_container: Option<BoxBuilder>,
    pub multiline: bool,
    pub rows: u32,
    pub resizable: bool,
    pub resize_max_h: f32,
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
        text_size_px,
        font_weight,
        line_height_px,
        leading_scale,
        value,
        single_line_container,
        multiline,
        rows,
        resizable,
        resize_max_h,
    } = params;

    if let Some(sig) = value {
        bind_controlled_value(&field_id, sig);
    }

    let focus_key = "__rui_button_focus_id";
    let focus_visible_key = "__rui_focus_visible";
    let focused = use_signal(focus_key, String::new);
    let _ = use_signal(focus_visible_key, || false);

    let fid = focus_id_for_text_edit(&field_id, multiline);
    let is_focused = focused.peek() == fid && !disabled;

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
    let real_display = if show_placeholder {
        placeholder.clone()
    } else if preedit.is_empty() {
        committed.clone()
    } else {
        let mut a = ime_anchor.unwrap_or(committed.len()).min(committed.len());
        a = committed.floor_char_boundary(a);
        format!("{}{}{}", &committed[..a], preedit, &committed[a..])
    };

    let caret_in_real = if is_focused {
        if show_placeholder {
            0
        } else if !preedit.is_empty() {
            let mut a = ime_anchor.unwrap_or(committed.len()).min(committed.len());
            a = committed.floor_char_boundary(a);
            let end = preedit_sel
                .map(|(_, e)| e)
                .unwrap_or_else(|| preedit.len())
                .min(preedit.len());
            a + preedit.floor_char_boundary(end)
        } else {
            let len = real_display.len();
            let ix = text_sel_sig
                .peek()
                .and_then(|(a, b)| {
                    if a.min(b) < a.max(b) {
                        Some(a.max(b))
                    } else {
                        None
                    }
                })
                .or_else(|| caret_sig.peek())
                .unwrap_or(len);
            real_display.floor_char_boundary(ix.min(len))
        }
    } else {
        0
    };

    let reveal = if input_type.is_password() && password_toggle {
        use_signal(&input_password_reveal_signal_key(&field_id), || false).peek()
    } else {
        false
    };
    let mask = input_type.is_password() && !reveal && !show_placeholder;
    let display_text = if mask {
        mask_password(&real_display)
    } else {
        real_display.clone()
    };
    let caret_byte = if mask {
        map_byte_index_to_masked(&real_display, caret_in_real, &display_text)
    } else {
        caret_in_real
    };

    let mut text_color = if show_placeholder {
        parse_text_color("muted-foreground")
    } else {
        parse_text_color("foreground")
    };
    if disabled {
        text_color[3] = (text_color[3] as f32 * 0.5) as u8;
    }

    let mut border_c = if invalid {
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
    let default_h =
        crate::utils::text_field::textarea_default_height(rows, line_box, pad_y);
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
        single_line_container.expect("single-line text edit requires single_line_container")
    };
    container = container.bg(bg).border(border_c, 1.0);
    if !is_focused && !disabled {
        container = container.shadow([0.0, 0.0, 0.0, 0.06], 1.0);
    }

    let mut text_el = build_text_node(
        display_text.clone(),
        TextNodeConfig {
            size_px: text_size_px,
            weight: font_weight,
            color: text_color,
            underline: false,
            underline_offset_px: None,
            center: false,
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
    if multiline {
        // Content-driven leaf (field-sizing-content). Do NOT use viewport/chrome height —
        // that coupled glyph soft_h to textarea resize. Ink only at content bottom.
        let content_h = crate::utils::text_field::text_edit_content_height_px(
            &display_text,
            text_size_px,
            font_weight,
            line_box,
            10_000.0,
        );
        text_el = text_el.h(content_h).min_h(content_h);
    } else {
        // Single-line: pin one line box so Input `items_center` centers the row.
        // Extra bottom ink lives in glyphon soft_h, not leaf height (else glyphs sit high).
        text_el = text_el.h(line_box);
    }

    if is_focused {
        let mut pc = parse_color("primary");
        pc[3] *= 0.35;
        let selection_bg = [
            (pc[0] * 255.0) as u8,
            (pc[1] * 255.0) as u8,
            (pc[2] * 255.0) as u8,
            (pc[3] * 255.0) as u8,
        ];
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
                let ix =
                    real_display.floor_char_boundary(caret_in_real.min(real_display.len()));
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
        });
    }

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
            .px("0.75rem")
            .py("0.5rem")
            .child(text_el);
        region = crate::core::scroll_viewport::activate_scroll_viewport(
            region,
            rest,
            "hidden",
            "scroll",
            "smooth",
            "area",
        );
        region.template_id = Some(format!("__rui_textarea:{field_id}:region"));
        region
    } else {
        hstack()
            .flex_grow(1.0)
            .min_w("0")
            .items_center()
            .child(text_el)
            .into()
    };

    let show_password_toggle = !multiline && password_toggle && input_type.is_password();
    let has_adornments = !multiline && (clearable || show_password_toggle);
    if has_adornments {
        container = container.gap("0.25rem");
    }

    let resizable = multiline && resizable && !disabled;

    let mut el: Element = container.into();
    if is_focused {
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
            .bottom(14.0) // leave room for resize grip
            .right(2.0)
            .into();
        el = el.child(vbar);
    }

    if resizable {
        let resize_fid = crate::utils::text_field::textarea_resize_focus_id(&field_id);
        let grip_label = build_text_node(
            "◢",
            TextNodeConfig {
                size_px: 10.0,
                weight: 400,
                color: parse_text_color("muted-foreground"),
                ..Default::default()
            },
        );
        let mut grip: Element = r#box()
            .absolute(true)
            .right(0.0)
            .bottom(0.0)
            .w(14.0)
            .h(14.0)
            .z_index(2)
            .items_center()
            .justify_center()
            .child(grip_label)
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
            lucide_named("x"),
            disabled_clear,
            move || {
                if disabled_clear {
                    return;
                }
                crate::utils::text_field::commit_value(&id_clear, String::new());
                use_signal(&input_preedit_signal_key(&id_clear), String::new).set(String::new());
                use_signal(&input_caret_signal_key(&id_clear), || None::<usize>).set(Some(0));
                use_signal(&input_selection_signal_key(&id_clear), || None::<(usize, usize)>)
                    .set(None);
                use_signal(&input_ime_anchor_signal_key(&id_clear), || None::<usize>).set(None);
                use_signal(focus_key_clear, String::new).set(fid_clear.clone());
                use_signal("__rui_focus_visible", || false).set(true);
            },
        );
        let show_clear = is_focused && (!committed.is_empty() || !preedit.is_empty());
        let _ = set_adornment_visible(&mut clear_btn, show_clear && !disabled);
        el = el.child(clear_btn);
    }

    if show_password_toggle {
        let fid_rev = fid.clone();
        let focus_key_rev = focus_key;
        let id_rev = field_id.clone();
        let disabled_rev = disabled;
        let eye_glyph = if reveal {
            lucide_named("eye-off")
        } else {
            lucide_named("eye")
        };
        let mut reveal_btn = input_adornment_button(
            input_reveal_template_id(&field_id),
            eye_glyph,
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

    el = if disabled {
        el.cursor_not_allowed()
    } else {
        el.cursor_text()
    };

    let type_key = input_type_signal_key(&field_id);
    use_signal(&type_key, || input_type.as_str().to_string())
        .set(input_type.as_str().to_string());

    let sync = TextEditPaintSync {
        base_id: field_id.clone(),
        placeholder,
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
    };

    let id_for_click = fid.clone();
    let mut out: Element = interactive(el)
        .focus_id(fid)
        .focusable(!disabled)
        .on_click(move || {
            if !disabled {
                use_signal(focus_key, String::new).set(id_for_click.clone());
                use_signal(focus_visible_key, || false).set(true);
            }
        })
        .into();
    out.template_id = Some(field_id);
    out.input_paint_sync = Some(sync);
    if multiline {
        out.attrs
            .insert("accesskit_role".to_string(), "textinput".to_string());
    }
    out
}

