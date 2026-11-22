//! Validated, backend-independent contracts for embedded native surfaces.
use serde::{Deserialize, Serialize};
mod controls;
mod media;
mod menu;
pub use menu::{MenuAction, MenuEntry, MenuNavigation};
mod shader;
pub use shader::{ShaderComposition, ShaderFallback, ShaderLanguage, ShaderUniform};
#[cfg(feature = "shader")]
pub use shader::compiler::{compile_shader_gles, compile_shader_wgsl, pack_shader_block, CompiledShader, ShaderBlock, ShaderFrame};
pub use controls::{CivilDate, DateControlMode, TimeOfDay};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum HostedContent {
    Web { url: Option<String>, html: Option<String>, base_url: Option<String>, origins: Vec<String> },
    Media { source: String, audio: bool, paused: bool, looping: bool, controls: bool,
        #[serde(default)] muted: bool,
        #[serde(default = "default_progress_interval")] progress_interval_ms: u32,
        volume: f64, rate: f64, seek: Option<f64>, seek_revision: u64, fit: MediaFit },
    Canvas { commands: Vec<CanvasCommand> },
    Shader { source: String, function: String, #[serde(default)] language: ShaderLanguage, uniforms: Vec<f32>, frames_per_second: u16,
        #[serde(default)] paused: bool,
        #[serde(default = "shader_animated_default")] animated: bool,
        #[serde(default)] time_seconds: Option<f64>,
        #[serde(default)] composition: ShaderComposition,
        #[serde(default)] progress: Option<f32>,
        #[serde(default)] max_sample_offset: f32,
        #[serde(default)] parameters: std::collections::BTreeMap<String, ShaderUniform>,
        #[serde(default)] textures: std::collections::BTreeMap<String, String> },
    Terminal { columns: u16, rows: u16, scrollback: usize,
        #[serde(default)] input: Option<String>, #[serde(default)] input_revision: u64 },
    RichEditor { html: String, #[serde(default)] editable: bool,
        #[serde(default)] selection: Option<[usize; 2]>, #[serde(default)] revision: u64 },
    MenuButton { label: String, enabled: bool, items: Vec<HostedMenuItem> },
    TabBar { items: Vec<HostedTab>, selected: Option<String>, enabled: bool, hidden: bool },
    Segments { items: Vec<HostedSegment>, selected: Option<String>, enabled: bool },
    Date { value: Option<CivilDate>, minimum: Option<CivilDate>, maximum: Option<CivilDate>,
        mode: DateControlMode, locale: Option<String>, enabled: bool },
    Time { value: TimeOfDay, seconds: bool, enabled: bool },
    Color { rgba: [u8; 4], alpha: bool, enabled: bool },
    Custom { kind: String, properties: serde_json::Value },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaFit { Contain, Cover, Fill }
fn shader_animated_default() -> bool { true }
fn default_progress_interval() -> u32 { 250 }
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum CanvasCommand {
    Fill { rect: [f64; 4], color: [u8; 4] },
    Stroke { points: Vec<[f64; 2]>, color: [u8; 4], width: f64, closed: bool },
    Text { text: String, position: [f64; 2], size: f64, color: [u8; 4] },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostedMenuItem {
    pub id: String, pub label: String, pub enabled: bool, pub checked: bool,
    #[serde(default)] pub children: Vec<HostedMenuItem>,
    #[serde(default)] pub symbol: Option<String>,
    #[serde(default)] pub accelerator: Option<String>,
    #[serde(default)] pub accessibility_label: Option<String>,
    #[serde(default)] pub destructive: bool,
    #[serde(default)] pub inline: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostedSegment {
    pub id: String, pub label: String, pub enabled: bool,
    #[serde(default)] pub symbol: Option<String>,
    #[serde(default)] pub accessibility_label: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostedTab {
    pub id: String, pub label: String, pub enabled: bool,
    #[serde(default)] pub symbol: Option<String>,
    #[serde(default)] pub accessibility_label: Option<String>,
    /// None removes the badge; an empty string displays the system badge dot.
    #[serde(default)] pub badge: Option<String>,
}
impl HostedContent {
    pub fn accepts_children(&self) -> bool { matches!(self, Self::MenuButton { .. } | Self::Shader { .. }) }

    pub fn accepts_event(&self, event: &str, data: &serde_json::Value) -> bool {
        let budget = match self {
            Self::Terminal { .. } => 512 * 1024,
            Self::RichEditor { .. } => 8 * 1024 * 1024,
            _ => 16 * 1024,
        };
        if serde_json::to_vec(data).map_or(true, |v| v.len() > budget) { return false; }
        if matches!(self, Self::Terminal { .. } | Self::RichEditor { .. }) && event == "error" {
            return data.as_str().is_some_and(|value| value.len() <= 4096);
        }
        match self {
            Self::Terminal { .. } => typed_event::<crate::terminal::TerminalEvent>(event, data)
                .is_some_and(|value| value.validate().is_ok()),
            Self::RichEditor { editable, revision, .. } => typed_event::<crate::rich_editor::RichEditorEvent>(event, data)
                .is_some_and(|value| value.validate().is_ok() && match value {
                    crate::rich_editor::RichEditorEvent::Change { revision: actual, .. } => *editable && actual == *revision,
                    crate::rich_editor::RichEditorEvent::Selection { revision: actual, .. }
                    | crate::rich_editor::RichEditorEvent::Focus { revision: actual, .. } => actual == *revision,
                }),
            Self::Web { .. } => match event {
                "loading" => data.is_boolean(),
                "navigation" => data.as_str().is_some_and(|v| v.len() <= 4096),
                "message" => true,
                "error" => data.as_str().is_some_and(|v| v.len() <= 4096),
                _ => false,
            },
            Self::Media { .. } => match event {
                "time" | "duration" => data.as_f64().is_some_and(|v| v.is_finite() && v >= 0.),
                "playing" | "ended" | "loadStart" | "buffer" => data.is_boolean(),
                "load" | "progress" => data.as_object().is_some_and(|v| {
                    !v.is_empty() && v.len() <= 3 && v.iter().all(|(key, value)|
                        matches!(key.as_str(), "position" | "duration" | "buffered")
                            && value.as_f64().is_some_and(|n| n.is_finite() && n >= 0.))
                }),
                "error" => data.as_str().is_some_and(|v| v.len() <= 4096),
                _ => false,
            },
            Self::Canvas { .. } | Self::Shader { .. } => event == "error" && data.is_string(),
            Self::Custom { .. } => !event.is_empty() && event.len() <= 32
                && event.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-')),
            Self::Date { minimum, maximum, enabled, .. } => *enabled && event == "change"
                && serde_json::from_value::<CivilDate>(data.clone()).ok().is_some_and(|date|
                    date.validate().is_ok() && minimum.is_none_or(|min| date >= min)
                        && maximum.is_none_or(|max| date <= max)),
            Self::Time { seconds, enabled, .. } => *enabled && event == "change"
                && serde_json::from_value::<TimeOfDay>(data.clone()).ok().is_some_and(|time|
                    time.validate().is_ok() && (*seconds || time.second == 0)),
            Self::Color { alpha, enabled, .. } => *enabled && event == "change"
                && serde_json::from_value::<[u8; 4]>(data.clone()).ok().is_some_and(|rgba| *alpha || rgba[3] == 255),
            Self::TabBar { items, enabled, hidden, .. } => *enabled && !*hidden && event == "select"
                && data.as_str().is_some_and(|id| items.iter().any(|item| item.enabled && item.id == id)),
            Self::Segments { items, enabled, .. } => *enabled && event == "select"
                && data.as_str().is_some_and(|id| items.iter().any(|item| item.enabled && item.id == id)),
            Self::MenuButton { items, enabled, .. } => {
                fn has(items: &[HostedMenuItem], id: &str) -> bool {
                    items.iter().any(|item| item.enabled && ((item.id == id && !item.inline && item.children.is_empty()) || has(&item.children, id)))
                }
                *enabled && event == "select" && data.as_str().is_some_and(|id| has(items, id))
            }
        }
    }
    pub const MAX_BYTES: usize = 1024 * 1024;
    /// Navigation authority shared by native web adapters. Application providers
    /// decide their own authorities; bundle URLs must pass the shared path check.
    pub fn allows_web_navigation(&self, value: &str, application_resource: bool) -> bool {
        let Self::Web { html, origins, .. } = self else { return false; };
        if value.len() > 4096 { return false; }
        if html.is_some() && value == "about:blank" { return true; }
        if crate::web_resource::bundle_path(value).is_some() { return true; }
        let Ok(url) = url::Url::parse(value) else { return false; };
        if url.scheme() == "app" { return url.host_str() != Some("bundle") && application_resource; }
        if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() { return false; }
        origins.iter().filter_map(|origin| url::Url::parse(origin).ok()).any(|origin| origin.origin() == url.origin())
    }
    pub fn from_json(json: &str) -> Result<Self, &'static str> {
        if json.len() > Self::MAX_BYTES { return Err("hosted content exceeds budget"); }
        let value: Self = serde_json::from_str(json).map_err(|_| "invalid hosted content")?;
        value.validate()?; Ok(value)
    }
    pub fn to_json(&self) -> Result<String, &'static str> {
        self.validate()?;
        let json = serde_json::to_string(self).map_err(|_| "invalid hosted content")?;
        if json.len() > Self::MAX_BYTES { return Err("hosted content exceeds budget"); } Ok(json)
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        fn text(value: &str) -> bool { value.len() <= 4096 && !value.contains('\0') }
        fn source(value: &str) -> bool {
            text(value) && (crate::presentation::NodeContent::is_local_image_source(value)
                || crate::presentation::NodeContent::is_remote_image_source(value)
                || value.starts_with("app://") && value.len() > 6 && !value.contains(['\\', '#', '?'])
                    && value[6..].split('/').all(|p| !matches!(p, "." | "..") && !p.is_empty()))
        }
        fn finite(values: &[f64]) -> bool { values.iter().all(|v| v.is_finite() && v.abs() <= 1e7) }
        let valid = match self {
            Self::Terminal { columns, rows, scrollback, input, .. } =>
                crate::terminal::TerminalSpec { columns: *columns, rows: *rows, scrollback: *scrollback }.validate().is_ok()
                    && input.as_ref().is_none_or(|value| value.len() <= 64 * 1024),
            Self::RichEditor { html, editable, selection, revision } => crate::rich_editor::RichEditorSpec {
                html: html.clone(), editable: *editable, selection: *selection, revision: *revision,
            }.validate().is_ok(),
            Self::Custom { kind, properties } => !kind.is_empty() && kind.len() <= 128
                && kind.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
                && properties.is_object() && bounded_properties(properties, 0, &mut 0),
            Self::Date { value, minimum, maximum, locale, .. } => {
                [value, minimum, maximum].into_iter().all(|date| date.is_none_or(|date| date.validate().is_ok()))
                    && minimum.zip(*maximum).is_none_or(|(min, max)| min <= max)
                    && value.is_none_or(|date| minimum.is_none_or(|min| date >= min) && maximum.is_none_or(|max| date <= max))
                    && locale.as_ref().is_none_or(|locale| !locale.is_empty() && locale.len() <= 128
                        && locale.split('-').all(|part| !part.is_empty() && part.len() <= 8 && part.bytes().all(|c| c.is_ascii_alphanumeric())))
            }
            Self::Time { value, seconds, .. } => value.validate().is_ok() && (*seconds || value.second == 0),
            Self::Color { rgba, alpha, .. } => *alpha || rgba[3] == 255,
            Self::Web { url, html, base_url, origins } => {
                (url.is_some() != html.is_some()) && url.as_deref().is_none_or(source)
                    && html.as_ref().is_none_or(|v| v.len() <= 768 * 1024 && !v.contains('\0'))
                    && base_url.as_deref().is_none_or(source) && origins.len() <= 32
                    && origins.iter().all(|value| url::Url::parse(value).ok().is_some_and(|u|
                        u.scheme() == "https" && u.host_str().is_some() && u.username().is_empty()
                        && u.password().is_none() && u.path() == "/" && u.query().is_none() && u.fragment().is_none()))
            }
            Self::Media { source: value, volume, rate, seek, progress_interval_ms, .. } => source(value)
                && (16..=60_000).contains(progress_interval_ms)
                && volume.is_finite() && (0.0..=1.0).contains(volume)
                && rate.is_finite() && (0.25..=4.0).contains(rate)
                && seek.is_none_or(|v| v.is_finite() && (0.0..=1e9).contains(&v)),
            Self::Canvas { commands } => commands.len() <= 4096 && commands.iter().all(|command| match command {
                CanvasCommand::Fill { rect, .. } => finite(rect) && rect[2] >= 0.0 && rect[3] >= 0.0,
                CanvasCommand::Stroke { points, width, .. } => points.len() <= 4096 && points.iter().all(|p| finite(p)) && width.is_finite() && (0.0..=4096.0).contains(width),
                CanvasCommand::Text { text: value, position, size, .. } => text(value) && finite(position) && size.is_finite() && (1.0..=1024.0).contains(size),
            }),
            Self::Shader { source: code, function, uniforms, frames_per_second, parameters, textures, time_seconds, composition, progress, max_sample_offset, .. } =>
                !code.is_empty() && code.len() <= 256 * 1024 && !code.contains('\0')
                && shader::identifier(function) && uniforms.len() <= 256
                && uniforms.iter().all(|v| v.is_finite()) && (1..=120).contains(frames_per_second)
                && time_seconds.is_none_or(|v| v.is_finite() && (0.0..=1e9).contains(&v))
                && composition.validate() && progress.is_none_or(|v| v.is_finite() && (0.0..=1.0).contains(&v))
                && max_sample_offset.is_finite() && (0.0..=4096.0).contains(max_sample_offset)
                && parameters.len() <= 64 && textures.len() + composition.inputs().len() <= 8
                && parameters.iter().all(|(name, value)| shader::parameter_name(name) && !composition.inputs().contains(&name.as_str()) && value.is_valid())
                && textures.iter().all(|(name, value)| shader::parameter_name(name) && !composition.inputs().contains(&name.as_str())
                    && !parameters.contains_key(name) && (source(value)
                        || crate::presentation::NodeContent::image_source_dimensions(value).is_some()
                        || crate::presentation::NodeContent::is_inline_raster_source(value)
                        || crate::presentation::NodeContent::is_inline_svg_source(value))),
            Self::MenuButton { label, items, .. } => {
                fn menu(items: &[HostedMenuItem], depth: usize, count: &mut usize, ids: &mut std::collections::BTreeSet<String>) -> bool {
                    depth <= 8 && items.iter().all(|item| { *count += 1;
                        *count <= 256 && !item.id.is_empty() && text(&item.id) && text(&item.label)
                            && item.symbol.as_ref().is_none_or(|v| !v.is_empty() && v.len() <= 128 && text(v))
                            && item.accessibility_label.as_ref().is_none_or(|v| text(v))
                            && item.accelerator.as_ref().is_none_or(|v| v.parse::<crate::shortcut::KeyChord>().is_ok())
                            && (!item.inline || !item.children.is_empty())
                            && ids.insert(item.id.clone()) && menu(&item.children, depth + 1, count, ids)
                    })
                }
                text(label) && menu(items, 0, &mut 0, &mut Default::default())
            }
            Self::TabBar { items, selected, .. } => {
                let mut ids = std::collections::BTreeSet::new();
                !items.is_empty() && items.len() <= 64 && items.iter().all(|item| !item.id.is_empty()
                    && text(&item.id) && text(&item.label)
                    && item.symbol.as_ref().is_none_or(|v| !v.is_empty() && v.len() <= 128 && text(v))
                    && item.accessibility_label.as_ref().is_none_or(|v| text(v))
                    && item.badge.as_ref().is_none_or(|v| v.len() <= 128 && text(v)) && ids.insert(item.id.as_str()))
                    && selected.as_ref().is_none_or(|id| ids.contains(id.as_str()))
            }
            Self::Segments { items, selected, .. } => {
                let mut ids = std::collections::BTreeSet::new();
                !items.is_empty() && items.len() <= 64 && items.iter().all(|item| !item.id.is_empty()
                    && text(&item.id) && text(&item.label)
                    && item.symbol.as_ref().is_none_or(|v| !v.is_empty() && v.len() <= 128 && text(v))
                    && item.accessibility_label.as_ref().is_none_or(|v| text(v)) && ids.insert(item.id.as_str()))
                    && selected.as_ref().is_none_or(|id| ids.contains(id.as_str()))
            }
        };
        if valid { Ok(()) } else { Err("invalid hosted content") }
    }
}

fn typed_event<T: serde::de::DeserializeOwned>(event: &str, data: &serde_json::Value) -> Option<T> {
    let mut fields = data.as_object()?.clone();
    if fields.contains_key("type") { return None; }
    fields.insert("type".into(), event.into());
    serde_json::from_value(serde_json::Value::Object(fields)).ok()
}

fn bounded_properties(value: &serde_json::Value, depth: usize, nodes: &mut usize) -> bool {
    *nodes += 1;
    if depth > 64 || *nodes > 16384 { return false; }
    match value {
        serde_json::Value::String(value) => value.len() <= 256 * 1024 && !value.contains('\0'),
        serde_json::Value::Array(values) => values.iter().all(|value| bounded_properties(value, depth + 1, nodes)),
        serde_json::Value::Object(values) => values.iter().all(|(key, value)| key.len() <= 4096 && !key.contains('\0') && bounded_properties(value, depth + 1, nodes)),
        _ => true,
    }
}
