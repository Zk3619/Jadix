use super::*;
use crate::layout::MeasureMode;
use gtk::pango;
use jadix_core::presentation::style::*;

fn rgba([r, g, b, a]: [u8; 4]) -> String {
    format!("rgba({r},{g},{b},{})", f64::from(a) / 255.)
}
fn quoted(value: &str) -> String {
    let mut result = String::from("\"");
    for c in value.chars() {
        if c == '"' || c == '\\' {
            result.push('\\');
        }
        if !c.is_control() {
            result.push(c);
        }
    }
    result.push('"');
    result
}

pub(super) fn apply(item: &Item, descriptor: &NodeDescriptor) {
    let style = &descriptor.style;
    let widget = &item.widget;
    let css = stylesheet(style);
    widget.set_visible(!style.view.as_ref().is_some_and(|v| v.hidden));
    widget.set_overflow(if style.view.as_ref().is_some_and(|v| v.clip) {
        gtk::Overflow::Hidden
    } else {
        gtk::Overflow::Visible
    });
    item.css.load_from_string(&css);
    widget.set_direction(match style.direction {
        Some(crate::environment::LayoutDirection::RightToLeft) => gtk::TextDirection::Rtl,
        Some(_) => gtk::TextDirection::Ltr,
        None => gtk::TextDirection::None,
    });
    if let Some(label) = widget.downcast_ref::<gtk::Label>() {
        label.set_wrap(style.text_overflow == TextOverflow::Wrap);
        label.set_wrap_mode(pango::WrapMode::WordChar);
        label.set_ellipsize(ellipsize(style.text_overflow));
        label.set_lines(style.max_lines.map_or(-1, i32::from));
        label.set_selectable(
            style
                .text
                .as_ref()
                .and_then(|s| s.selectable)
                .unwrap_or(false),
        );
        label.set_attributes(Some(&attributes(style)));
        if let Some(text) = &style.text {
            label.set_xalign(match text.align {
                TextAlignment::Center => 0.5,
                TextAlignment::Right | TextAlignment::End => 1.,
                _ => 0.,
            });
            label.set_yalign(
                text.vertical_align_fraction
                    .unwrap_or(match text.vertical_align {
                        TextVerticalAlignment::Center => 0.5,
                        TextVerticalAlignment::Bottom => 1.,
                        _ => 0.,
                    }),
            );
            label.set_justify(match text.align {
                TextAlignment::Center => gtk::Justification::Center,
                TextAlignment::Right | TextAlignment::End => gtk::Justification::Right,
                TextAlignment::Justify => gtk::Justification::Fill,
                _ => gtk::Justification::Left,
            });
        } else {
            label.set_xalign(0.);
            label.set_yalign(0.);
        }
    }
    accessibility::apply(item, descriptor);
}

pub(super) fn stylesheet(style: &NodeStyle) -> String {
    let mut background = style
        .view
        .as_ref()
        .and_then(|v| v.background)
        .unwrap_or(style.background);
    if style
        .view
        .as_ref()
        .is_some_and(|v| !v.backgrounds.is_empty())
    {
        background = [0; 4];
    }
    let mut css = format!(
        "* {{ color:{}; background-color:{}; font-family:{}; font-size:{}px; font-weight:{};",
        rgba(style.foreground),
        rgba(background),
        quoted(&style.font_family),
        style.font_size,
        style.font_weight
    );
    if let Some(text) = &style.text {
        if text.italic {
            css.push_str("font-style:italic;");
        }
        css.push_str(&format!("letter-spacing:{}px;", text.letter_spacing));
        if let Some(color) = text.caret_color {
            css.push_str(&format!("caret-color:{};", rgba(color)));
        }
    }
    if let Some(view) = &style.view {
        css.push_str(&format!("border-radius:{}px {}px {}px {}px; border-style:solid; border-width:{}px {}px {}px {}px; border-color:{} {} {} {}; outline:{}px solid {}; outline-offset:{}px;",view.corners[0],view.corners[1],view.corners[2],view.corners[3],view.border_widths[0],view.border_widths[1],view.border_widths[2],view.border_widths[3],rgba(view.border_colors[0]),rgba(view.border_colors[1]),rgba(view.border_colors[2]),rgba(view.border_colors[3]),view.outline_width,rgba(view.outline_color),view.outline_offset));
        let stroke = |value| match value {
            StrokeStyle::Solid => "solid",
            StrokeStyle::Dashed => "dashed",
            StrokeStyle::Dotted => "dotted",
        };
        css.push_str(&format!(
            "border-style:{};outline-style:{};",
            stroke(view.border_style),
            stroke(view.outline_style)
        ));
        if !view.shadows.is_empty() {
            css.push_str("box-shadow:");
            css.push_str(
                &view
                    .shadows
                    .iter()
                    .map(|s| {
                        format!(
                            "{}{}px {}px {}px {}px {}",
                            if s.inset { "inset " } else { "" },
                            s.offset[0],
                            s.offset[1],
                            s.blur,
                            s.spread,
                            rgba(s.color)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(","),
            );
            css.push(';');
        }
    }
    css.push('}');
    if let Some(accent) = style.view.as_ref().and_then(|v| v.accent) {
        css.push_str(&format!(
            "@define-color accent_color {}; @define-color accent_bg_color {};",
            rgba(accent),
            rgba(accent)
        ));
    }
    css
}

fn ellipsize(overflow: TextOverflow) -> pango::EllipsizeMode {
    match overflow {
        TextOverflow::Ellipsis => pango::EllipsizeMode::End,
        TextOverflow::EllipsisStart => pango::EllipsizeMode::Start,
        TextOverflow::EllipsisMiddle => pango::EllipsizeMode::Middle,
        _ => pango::EllipsizeMode::None,
    }
}
fn attributes(style: &NodeStyle) -> pango::AttrList {
    let attrs = pango::AttrList::new();
    if let Some(text) = &style.text {
        attrs.insert(pango::AttrInt::new_letter_spacing(
            (text.letter_spacing * pango::SCALE as f32) as i32,
        ));
        if text.underline {
            attrs.insert(pango::AttrInt::new_underline(pango::Underline::Single));
        }
        if text.strikethrough {
            attrs.insert(pango::AttrInt::new_strikethrough(true));
        }
        if let Some(height) = text.line_height {
            attrs.insert(pango::AttrInt::new_line_height_absolute(
                (height * pango::SCALE as f32) as i32,
            ));
        } else if let Some(multiple) = text.line_height_multiple {
            attrs.insert(pango::AttrFloat::new_line_height(f64::from(multiple)));
        }
    }
    attrs
}

pub(super) fn measure(
    widget: &gtk::Fixed,
    request: MeasureRequest,
    descriptor: &NodeDescriptor,
) -> Result<MeasureResult, LayoutError> {
    let style = &descriptor.style;
    let (text, chrome, min_height) = match &descriptor.content {
        NodeContent::Text(text) => (text.as_str(), 0., 0.),
        NodeContent::Button { label, .. } => (label.as_str(), 32., 34.),
        NodeContent::Checkbox { label, .. } => (label.as_str(), 28., 24.),
        NodeContent::Switch { label, .. } => (label.as_str(), 48., 26.),
        NodeContent::Input {
            value,
            placeholder,
            password,
            ..
        } => {
            let value = if value.is_empty() {
                placeholder.clone()
            } else if *password {
                "•".repeat(value.chars().count())
            } else {
                value.clone()
            };
            return text_measure(widget, request, descriptor, &value, 24., 34.);
        }
        NodeContent::Textarea {
            value, placeholder, ..
        } => (
            if value.is_empty() {
                placeholder.as_str()
            } else {
                value.as_str()
            },
            16.,
            72.,
        ),
        NodeContent::Select {
            options,
            value,
            placeholder,
            ..
        } => (
            options
                .iter()
                .find(|o| &o.value == value)
                .map_or(placeholder.as_str(), |o| o.label.as_str()),
            40.,
            34.,
        ),
        NodeContent::Image { .. } => {
            let [width, height] = descriptor.content.image_dimensions().unwrap_or([32, 32]);
            return Ok(MeasureResult {
                key: request.key,
                width: f64::from(width).min(request.key.max_width),
                height: f64::from(height).min(request.key.max_height),
                baseline: None,
            });
        }
        _ => ("", 0., 0.),
    };
    let text = style
        .text
        .as_ref()
        .map_or_else(|| text.to_owned(), |s| s.display_text(text));
    text_measure(widget, request, descriptor, &text, chrome, min_height)
}
fn text_measure(
    widget: &gtk::Fixed,
    request: MeasureRequest,
    descriptor: &NodeDescriptor,
    text: &str,
    chrome: f64,
    min_height: f64,
) -> Result<MeasureResult, LayoutError> {
    let style = &descriptor.style;
    let layout = widget.create_pango_layout(Some(text));
    let mut font = pango::FontDescription::new();
    font.set_family(&style.font_family);
    font.set_absolute_size(f64::from(style.font_size) * f64::from(pango::SCALE));
    font.set_weight(match style.font_weight {
        0..=299 => pango::Weight::Light,
        300..=499 => pango::Weight::Normal,
        500..=599 => pango::Weight::Medium,
        600..=699 => pango::Weight::Semibold,
        700..=799 => pango::Weight::Bold,
        _ => pango::Weight::Heavy,
    });
    if style.text.as_ref().is_some_and(|s| s.italic) {
        font.set_style(pango::Style::Italic);
    }
    layout.set_font_description(Some(&font));
    layout.set_attributes(Some(&attributes(style)));
    layout.set_wrap(pango::WrapMode::WordChar);
    if request.width_mode != MeasureMode::MaxContent && style.text_overflow == TextOverflow::Wrap {
        let width = if request.width_mode == MeasureMode::MinContent {
            1.
        } else {
            (request.key.max_width - chrome).max(0.)
        };
        layout.set_width((width * f64::from(pango::SCALE)).min(i32::MAX as f64) as i32);
    }
    layout.set_ellipsize(ellipsize(style.text_overflow));
    if let Some(lines) = style.max_lines {
        layout.set_height(-i32::from(lines.max(1)));
    }
    let (width, height) = layout.pixel_size();
    let height = f64::from(height)
        .max(min_height)
        .min(request.key.max_height);
    Ok(MeasureResult {
        key: request.key,
        width: (f64::from(width) + chrome).min(request.key.max_width),
        height,
        baseline: Some((f64::from(layout.baseline()) / f64::from(pango::SCALE)).min(height)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn css_strings_cannot_escape_the_font_family() {
        assert_eq!(quoted("a\"; }\\\n"), "\"a\\\"; }\\\\\"");
    }
}
