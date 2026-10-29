#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextAlignLast {
    #[default]
    Auto,
    Start,
    End,
    Left,
    Right,
    Center,
    Justify,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAlignLastDeclaration {
    Value(TextAlignLast),
    Inherit,
    Invalid,
}

impl TextAlignLast {
    pub fn parse_declaration(value: &str) -> Option<TextAlignLastDeclaration> {
        Some(match value.trim().to_ascii_lowercase().as_str() {
            "inherit" | "unset" => TextAlignLastDeclaration::Inherit,
            "initial" | "auto" => TextAlignLastDeclaration::Value(Self::Auto),
            "start" => TextAlignLastDeclaration::Value(Self::Start),
            "end" => TextAlignLastDeclaration::Value(Self::End),
            "left" => TextAlignLastDeclaration::Value(Self::Left),
            "right" => TextAlignLastDeclaration::Value(Self::Right),
            "center" => TextAlignLastDeclaration::Value(Self::Center),
            "justify" => TextAlignLastDeclaration::Value(Self::Justify),
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Start => "start",
            Self::End => "end",
            Self::Left => "left",
            Self::Right => "right",
            Self::Center => "center",
            Self::Justify => "justify",
        }
    }

    pub(crate) fn to_glyphon(
        self,
        align: super::TextAlign,
    ) -> Option<glyphon::cosmic_text::AlignLast> {
        use glyphon::cosmic_text::AlignLast;
        match self {
            Self::Auto if align == super::TextAlign::JustifyAll => Some(AlignLast::Justified),
            Self::Auto => None,
            Self::Start => Some(AlignLast::Start),
            Self::End => Some(AlignLast::End),
            Self::Left => Some(AlignLast::Left),
            Self::Right => Some(AlignLast::Right),
            Self::Center => Some(AlignLast::Center),
            Self::Justify => Some(AlignLast::Justified),
        }
    }
}

impl From<TextAlignLast> for TextAlignLastDeclaration {
    fn from(value: TextAlignLast) -> Self {
        Self::Value(value)
    }
}

impl From<&str> for TextAlignLastDeclaration {
    fn from(value: &str) -> Self {
        TextAlignLast::parse_declaration(value).unwrap_or(Self::Invalid)
    }
}

impl From<String> for TextAlignLastDeclaration {
    fn from(value: String) -> Self {
        value.as_str().into()
    }
}

impl From<&String> for TextAlignLastDeclaration {
    fn from(value: &String) -> Self {
        value.as_str().into()
    }
}
