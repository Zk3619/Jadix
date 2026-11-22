//! Text styling shared by authoring, wire protocols and platform hosts.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TextAppearance {
    pub italic: bool,
    /// Absolute logical points; takes precedence over `line_height_multiple`.
    pub line_height: Option<f32>,
    pub line_height_multiple: Option<f32>,
    pub letter_spacing: f32,
    pub align: TextAlignment,
    pub direction: TextDirection,
    pub vertical_align: TextVerticalAlignment,
    /// Fraction of spare block height; overrides vertical_align when supplied.
    pub vertical_align_fraction: Option<f32>,
    pub transform: TextTransform,
    pub variants: Vec<FontVariant>,
    pub underline: bool,
    pub overline: bool,
    pub strikethrough: bool,
    pub decoration_style: TextDecorationStyle,
    pub decoration_color: Option<[u8; 4]>,
    pub decoration_thickness: Option<f32>,
    pub underline_offset: Option<f32>,
    pub shadow: Option<super::ViewShadow>,
    pub placeholder_color: Option<[u8; 4]>,
    pub selection_color: Option<[u8; 4]>,
    pub caret_color: Option<[u8; 4]>,
    /// Read-only native text selection. Omission preserves the host default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selectable: Option<bool>,
}

macro_rules! text_enum {
    ($name:ident { $default:ident $(, $value:ident)* $(,)? }) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { #[default] $default, $($value),* }
    };
}
text_enum!(TextAlignment {
    Start,
    End,
    Left,
    Right,
    Center,
    Justify
});
text_enum!(TextDirection { Auto, Ltr, Rtl });
text_enum!(TextVerticalAlignment {
    Top,
    Center,
    Bottom
});
text_enum!(TextTransform {
    None,
    Uppercase,
    Lowercase,
    Capitalize
});
text_enum!(FontVariant {
    Normal,
    SmallCaps,
    OldstyleNumbers,
    LiningNumbers,
    TabularNumbers,
    ProportionalNumbers
});
text_enum!(TextDecorationStyle {
    Solid,
    Double,
    Dotted,
    Dashed,
    Wavy
});

impl TextAppearance {
    pub fn is_valid(&self) -> bool {
        self.line_height
            .into_iter()
            .chain(self.line_height_multiple)
            .all(|v| v.is_finite() && v > 0.0 && v <= 1_000_000.0)
            && self
                .vertical_align_fraction
                .is_none_or(|v| v.is_finite() && (0.0..=1.0).contains(&v))
            && self.letter_spacing.is_finite()
            && self.letter_spacing.abs() <= 1_000_000.0
            && self
                .decoration_thickness
                .is_none_or(|v| v.is_finite() && (0.0..=1024.0).contains(&v))
            && self
                .underline_offset
                .is_none_or(|v| v.is_finite() && v.abs() <= 1024.0)
            && self.shadow.as_ref().is_none_or(|shadow| {
                shadow
                    .offset
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= 1_000_000.0)
                    && shadow.blur.is_finite()
                    && (0.0..=1024.0).contains(&shadow.blur)
                    && shadow.spread == 0.0
                    && !shadow.inset
            })
    }

    /// Transform only display text. Editable values and source byte ranges stay intact.
    pub fn display_text(&self, source: &str) -> String {
        match self.transform {
            TextTransform::None => source.into(),
            TextTransform::Uppercase => source.to_uppercase(),
            TextTransform::Lowercase => source.to_lowercase(),
            TextTransform::Capitalize => {
                let mut start = true;
                let mut result = String::new();
                for c in source.chars() {
                    if start && c.is_alphanumeric() {
                        result.extend(c.to_uppercase());
                    } else {
                        result.push(c);
                    }
                    start = !c.is_alphanumeric() && c != '\'' && c != '’';
                }
                result
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_metrics_and_unsupported_shadow_shapes_are_rejected() {
        for value in [f32::NAN, f32::INFINITY, -1.0, 0.0] {
            assert!(!TextAppearance {
                line_height: Some(value),
                ..Default::default()
            }
            .is_valid());
        }
        for value in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
            assert!(!TextAppearance {
                vertical_align_fraction: Some(value),
                ..Default::default()
            }
            .is_valid());
        }
        assert!(!TextAppearance {
            letter_spacing: f32::NAN,
            ..Default::default()
        }
        .is_valid());
        assert!(!TextAppearance {
            shadow: Some(super::super::ViewShadow {
                inset: true,
                ..Default::default()
            }),
            ..Default::default()
        }
        .is_valid());
        assert!(TextAppearance {
            letter_spacing: -2.0,
            line_height_multiple: Some(1.5),
            ..Default::default()
        }
        .is_valid());
    }
    #[test]
    fn wire_roundtrip_preserves_combined_font_features_and_fractional_alignment() {
        let style = TextAppearance {
            variants: vec![
                FontVariant::SmallCaps,
                FontVariant::OldstyleNumbers,
                FontVariant::TabularNumbers,
            ],
            vertical_align_fraction: Some(0.75),
            ..Default::default()
        };
        let encoded = serde_json::to_value(&style).unwrap();
        assert_eq!(
            encoded["variants"],
            serde_json::json!(["small_caps", "oldstyle_numbers", "tabular_numbers"])
        );
        assert_eq!(
            serde_json::from_value::<TextAppearance>(encoded).unwrap(),
            style
        );
        assert!(style.is_valid());
    }
    #[test]
    fn display_transform_keeps_unicode_and_word_boundaries() {
        let style = TextAppearance {
            transform: TextTransform::Capitalize,
            ..Default::default()
        };
        assert_eq!(
            style.display_text("hello don't-stop 中文 😀straße"),
            "Hello Don't-Stop 中文 😀Straße"
        );
        assert_eq!(
            TextAppearance {
                transform: TextTransform::Uppercase,
                ..Default::default()
            }
            .display_text("straße"),
            "STRASSE"
        );
    }
}
