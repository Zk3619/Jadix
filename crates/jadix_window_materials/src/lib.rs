pub mod glass_params;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowBackdrop {
    Auto,
    None,
    Mica,
    #[default]
    Acrylic,
    MicaAlt,
    Blur,
    Aero,
}
impl From<&str> for WindowBackdrop {
    fn from(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "mica" => Self::Mica,
            "mica-alt" | "mica_alt" | "micaalt" => Self::MicaAlt,
            "none" => Self::None,
            "auto" => Self::Auto,
            "blur" => Self::Blur,
            "aero" => Self::Aero,
            _ => Self::Acrylic,
        }
    }
}
impl From<String> for WindowBackdrop {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}
