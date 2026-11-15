use crate::environment::{
    ColorScheme, EnvironmentSnapshot, LayoutDirection, Lifecycle, LocaleSettings, OperatingSystem,
    PlatformInfo,
};

#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OhosConfiguration {
    pub language: Option<String>,
    pub color_mode: Option<i32>,
    pub direction: Option<i32>,
    pub font_size_scale: Option<f32>,
    pub high_contrast: Option<bool>,
}

impl OhosConfiguration {
    pub fn snapshot(&self, lifecycle: Lifecycle) -> std::io::Result<EnvironmentSnapshot> {
        let mut locale = LocaleSettings::default();
        if let Some(language) = &self.language {
            locale.locale = language.replace('_', "-");
            locale.languages = vec![locale.locale.clone()];
        }
        let snapshot = EnvironmentSnapshot {
            platform: Some(PlatformInfo {
                name: "ohos".into(),
                os: Some(OperatingSystem::Ohos),
            }),
            locale,
            color_scheme: match self.color_mode {
                Some(0) => Some(ColorScheme::Dark),
                Some(1) => Some(ColorScheme::Light),
                _ => None,
            },
            layout_direction: match self.direction {
                Some(0) => Some(LayoutDirection::LeftToRight),
                Some(1) => Some(LayoutDirection::RightToLeft),
                _ => None,
            },
            high_contrast: self.high_contrast,
            font_scale: self.font_size_scale.unwrap_or(1.0),
            lifecycle,
            ..Default::default()
        };
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub fn from_json(bytes: &[u8], lifecycle: Lifecycle) -> std::io::Result<EnvironmentSnapshot> {
        if bytes.len() > EnvironmentSnapshot::MAX_JSON_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "OHOS configuration exceeds byte limit",
            ));
        }
        let configuration: Self = serde_json::from_slice(bytes)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidInput, error))?;
        configuration.snapshot(lifecycle)
    }
}
