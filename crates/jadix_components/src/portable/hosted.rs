//! Native surfaces use the same validated specifications as the script host.
use super::{Base, Element};
pub use jadix::portable::hosted::{CanvasCommand, CivilDate, DateControlMode, HostedContent, HostedMenuItem, HostedSegment, HostedTab, MediaFit, ShaderComposition, ShaderFallback, ShaderLanguage, ShaderUniform, TimeOfDay};

pub type HostedHandler = Box<dyn FnMut(&str, &serde_json::Value)>;

pub struct HostedBuilder {
    id: String,
    content: HostedContent,
    on_event: Option<HostedHandler>,
    children: Vec<Element>,
}

pub fn hosted(id: impl Into<String>, content: HostedContent) -> HostedBuilder {
    HostedBuilder { id: id.into(), content, on_event: None, children: Vec::new() }
}

pub fn web_view(id: impl Into<String>, url: impl Into<String>) -> HostedBuilder {
    hosted(id, HostedContent::Web { url: Some(url.into()), html: None, base_url: None, origins: Vec::new() })
}
pub fn embed(id: impl Into<String>, kind: impl Into<String>, properties: serde_json::Value) -> HostedBuilder {
    hosted(id, HostedContent::Custom { kind: kind.into(), properties })
}
pub fn web_html(id: impl Into<String>, html: impl Into<String>, base_url: Option<String>) -> HostedBuilder {
    hosted(id, HostedContent::Web { url: None, html: Some(html.into()), base_url, origins: Vec::new() })
}
pub fn video(id: impl Into<String>, source: impl Into<String>) -> HostedBuilder {
    hosted(id, HostedContent::Media { source: source.into(), audio: false, paused: true,
        looping: false, controls: true, muted: false, progress_interval_ms: 250,
        volume: 1., rate: 1., seek: None, seek_revision: 0, fit: MediaFit::Contain })
}
pub fn audio(id: impl Into<String>, source: impl Into<String>) -> HostedBuilder {
    let mut builder = video(id, source);
    if let HostedContent::Media { audio, .. } = &mut builder.content { *audio = true; }
    builder
}
pub fn canvas(id: impl Into<String>, commands: Vec<CanvasCommand>) -> HostedBuilder {
    hosted(id, HostedContent::Canvas { commands })
}
pub fn terminal(id: impl Into<String>, columns: u16, rows: u16, scrollback: usize) -> HostedBuilder {
    hosted(id, HostedContent::Terminal { columns, rows, scrollback, input: None, input_revision: 0 })
}
pub fn rich_editor(id: impl Into<String>, html: impl Into<String>, editable: bool) -> HostedBuilder {
    hosted(id, HostedContent::RichEditor { html: html.into(), editable, selection: None, revision: 0 })
}
pub fn shader(id: impl Into<String>, source: impl Into<String>) -> HostedBuilder {
    hosted(id, HostedContent::Shader { source: source.into(), function: "mainImage".into(), language: ShaderLanguage::Glsl, uniforms: Vec::new(), frames_per_second: 60, paused: false, animated: true, time_seconds: None, composition: Default::default(), progress: None, max_sample_offset: 0., parameters: Default::default(), textures: Default::default() })
}
/// Portable WGSL fragment; `shade(uv)` receives normalized top-left coordinates.
pub fn wgsl_shader(id: impl Into<String>, source: impl Into<String>) -> HostedBuilder {
    let mut builder = shader(id, source);
    if let HostedContent::Shader { language, function, .. } = &mut builder.content {
        *language = ShaderLanguage::Wgsl; *function = "shade".into();
    }
    builder
}
pub fn shader_effect(id: impl Into<String>, source: impl Into<String>) -> HostedBuilder {
    wgsl_shader(id, source).shader_composition(ShaderComposition::Effect)
}
pub fn shader_transition(id: impl Into<String>, source: impl Into<String>, value: impl Into<String>) -> HostedBuilder {
    wgsl_shader(id, source).shader_composition(ShaderComposition::Transition { value: value.into(), duration_ms: 300, fallback: ShaderFallback::Fade })
}
pub fn menu_button(id: impl Into<String>, label: impl Into<String>, items: Vec<HostedMenuItem>) -> HostedBuilder {
    hosted(id, HostedContent::MenuButton { label: label.into(), enabled: true, items })
}
pub fn tab_bar(id: impl Into<String>, items: Vec<HostedTab>) -> HostedBuilder {
    hosted(id, HostedContent::TabBar { items, selected: None, enabled: true, hidden: false })
}
pub fn segmented_control(id: impl Into<String>, items: Vec<HostedSegment>) -> HostedBuilder {
    hosted(id, HostedContent::Segments { items, selected: None, enabled: true })
}
pub fn calendar(id: impl Into<String>, value: Option<CivilDate>) -> HostedBuilder {
    hosted(id, HostedContent::Date { value, minimum: None, maximum: None,
        mode: DateControlMode::Calendar, locale: None, enabled: true })
}
pub fn date_picker(id: impl Into<String>, value: Option<CivilDate>) -> HostedBuilder {
    hosted(id, HostedContent::Date { value, minimum: None, maximum: None,
        mode: DateControlMode::Field, locale: None, enabled: true })
}
pub fn time_field(id: impl Into<String>, value: TimeOfDay) -> HostedBuilder {
    hosted(id, HostedContent::Time { value, seconds: true, enabled: true })
}
pub fn color_picker(id: impl Into<String>, rgba: [u8; 4]) -> HostedBuilder {
    hosted(id, HostedContent::Color { rgba, alpha: true, enabled: true })
}
impl HostedBuilder {
    pub fn shader_composition(mut self, value: ShaderComposition) -> Self {
        if let HostedContent::Shader { composition, .. } = &mut self.content { *composition = value; }
        self
    }
    pub fn shader_progress(mut self, value: f32) -> Self {
        if let HostedContent::Shader { progress, .. } = &mut self.content { *progress = Some(value); }
        self
    }
    pub fn shader_max_sample_offset(mut self, value: f32) -> Self {
        if let HostedContent::Shader { max_sample_offset, .. } = &mut self.content { *max_sample_offset = value; }
        self
    }
    pub fn shader_parameter(mut self, name: impl Into<String>, value: ShaderUniform) -> Self {
        if let HostedContent::Shader { parameters, .. } = &mut self.content { parameters.insert(name.into(), value); }
        self
    }
    pub fn shader_texture(mut self, name: impl Into<String>, source: impl Into<String>) -> Self {
        if let HostedContent::Shader { textures, .. } = &mut self.content { textures.insert(name.into(), source.into()); }
        self
    }
    pub fn shader_paused(mut self, value: bool) -> Self {
        if let HostedContent::Shader { paused, .. } = &mut self.content { *paused = value; }
        self
    }
    pub fn shader_animated(mut self, value: bool) -> Self {
        if let HostedContent::Shader { animated, .. } = &mut self.content { *animated = value; }
        self
    }
    pub fn child(mut self, child: impl Into<Element>) -> Self { self.children.push(child.into()); self }
    pub fn selected(mut self, id: impl Into<String>) -> Self {
        match &mut self.content {
            HostedContent::Segments { selected, .. } | HostedContent::TabBar { selected, .. } => *selected = Some(id.into()),
            _ => {}
        }
        self
    }
    pub fn hidden(mut self, value: bool) -> Self {
        if let HostedContent::TabBar { hidden, .. } = &mut self.content { *hidden = value; }
        self
    }

    pub fn enabled(mut self, value: bool) -> Self {
        match &mut self.content {
            HostedContent::Date { enabled, .. } | HostedContent::Time { enabled, .. }
            | HostedContent::Color { enabled, .. } | HostedContent::Segments { enabled, .. }
            | HostedContent::MenuButton { enabled, .. } | HostedContent::TabBar { enabled, .. } => *enabled = value,
            _ => {}
        }
        self
    }
    pub fn date_range(mut self, min: Option<CivilDate>, max: Option<CivilDate>) -> Self {
        if let HostedContent::Date { minimum, maximum, .. } = &mut self.content { *minimum = min; *maximum = max; }
        self
    }
    pub fn locale(mut self, value: impl Into<String>) -> Self {
        if let HostedContent::Date { locale, .. } = &mut self.content { *locale = Some(value.into()); }
        self
    }
    pub fn seconds(mut self, value: bool) -> Self {
        if let HostedContent::Time { seconds, value: time, .. } = &mut self.content {
            *seconds = value; if !value { time.second = 0; }
        }
        self
    }
    pub fn alpha(mut self, value: bool) -> Self {
        if let HostedContent::Color { alpha, rgba, .. } = &mut self.content {
            *alpha = value; if !value { rgba[3] = 255; }
        }
        self
    }
    pub fn content(mut self, content: HostedContent) -> Self { self.content = content; self }
    pub fn on_event(mut self, callback: impl FnMut(&str, &serde_json::Value) + 'static) -> Self {
        self.on_event = Some(Box::new(callback)); self
    }
    /// Reject invalid specifications before constructing or publishing a platform view.
    pub fn build(self) -> Result<Element, &'static str> {
        if !self.children.is_empty() && !self.content.accepts_children() { return Err("hosted content does not accept children"); }
        let mut element = Element::new().id(&self.id).hosted(self.content)?;
        for child in self.children { element = element.child(child); }
        if let Some(callback) = self.on_event { element = element.on_hosted(callback); }
        Ok(element)
    }
    /// Recompute only the surface's specification when the presentation is refreshed.
    pub fn bind(self, content: impl Fn() -> HostedContent + 'static) -> Result<Element, &'static str> {
        let mut element = self.build()?;
        element.descriptor_binding = Some(std::rc::Rc::new(move |descriptor| {
            // Invalid reactive data remains visible to shared presentation validation,
            // which rejects the candidate atomically and keeps the previous view.
            descriptor.content = jadix::presentation::NodeContent::Hosted {
                specification: content().to_json().unwrap_or_default(),
            };
        }));
        Ok(element)
    }
}

#[derive(Default)]
pub struct HostedProps {
    pub id: String,
    pub content: Option<HostedContent>,
    pub children: Vec<Element>,
    pub on_event: Option<HostedHandler>,
    pub base: Base,
}
/// RSX counterpart of the fallible builder. Invalid properties are programmer errors.
#[allow(non_snake_case)]
pub fn Hosted(props: HostedProps) -> Element {
    let mut builder = hosted(props.id, props.content.expect("Hosted requires content"));
    builder.on_event = props.on_event; builder.children = props.children;
    let element = builder.build().expect("invalid Hosted content");
    match props.base { Some(base) => base(element), None => element }
}

#[derive(Default)]
pub struct DatePickerProps {
    pub id: String, pub value: Option<CivilDate>, pub minimum: Option<CivilDate>, pub maximum: Option<CivilDate>,
    pub locale: Option<String>, pub disabled: Option<bool>, pub on_event: Option<HostedHandler>, pub base: Base,
}
pub type CalendarProps = DatePickerProps;
fn date_component(props: DatePickerProps, mode: DateControlMode) -> Element {
    Hosted(HostedProps { id: props.id, content: Some(HostedContent::Date {
        value: props.value, minimum: props.minimum, maximum: props.maximum, mode, locale: props.locale,
        enabled: !props.disabled.unwrap_or(false),
    }), on_event: props.on_event, base: props.base, children: Vec::new() })
}
#[allow(non_snake_case)]
pub fn Calendar(props: CalendarProps) -> Element { date_component(props, DateControlMode::Calendar) }
#[allow(non_snake_case)]
pub fn DatePicker(props: DatePickerProps) -> Element { date_component(props, DateControlMode::Field) }
#[derive(Default)]
pub struct TimeFieldProps {
    pub id: String, pub value: Option<TimeOfDay>, pub seconds: Option<bool>, pub disabled: Option<bool>,
    pub on_event: Option<HostedHandler>, pub base: Base,
}
#[allow(non_snake_case)]
pub fn TimeField(props: TimeFieldProps) -> Element {
    let seconds = props.seconds.unwrap_or(true);
    let mut value = props.value.unwrap_or_default(); if !seconds { value.second = 0; }
    Hosted(HostedProps { id: props.id, content: Some(HostedContent::Time {
        value, seconds, enabled: !props.disabled.unwrap_or(false),
    }), on_event: props.on_event, base: props.base, children: Vec::new() })
}
#[derive(Default)]
pub struct ColorPickerProps {
    pub id: String, pub value: Option<[u8; 4]>, pub alpha: Option<bool>, pub disabled: Option<bool>,
    pub on_event: Option<HostedHandler>, pub base: Base,
}
#[allow(non_snake_case)]
pub fn ColorPicker(props: ColorPickerProps) -> Element {
    let alpha = props.alpha.unwrap_or(true);
    let mut rgba = props.value.unwrap_or([0, 0, 0, 255]); if !alpha { rgba[3] = 255; }
    Hosted(HostedProps { id: props.id, content: Some(HostedContent::Color {
        rgba, alpha, enabled: !props.disabled.unwrap_or(false),
    }), on_event: props.on_event, base: props.base, children: Vec::new() })
}

#[derive(Default)]
pub struct MenuButtonProps {
    pub id: String, pub label: String, pub items: Vec<HostedMenuItem>, pub disabled: Option<bool>,
    pub children: Vec<Element>, pub on_event: Option<HostedHandler>, pub base: Base,
}
#[allow(non_snake_case)]
pub fn MenuButton(props: MenuButtonProps) -> Element {
    Hosted(HostedProps { id: props.id, content: Some(HostedContent::MenuButton {
        label: props.label, items: props.items, enabled: !props.disabled.unwrap_or(false),
    }), children: props.children, on_event: props.on_event, base: props.base })
}
#[derive(Default)]
pub struct TabBarProps {
    pub id: String, pub items: Vec<HostedTab>, pub selected: Option<String>, pub disabled: Option<bool>,
    pub hidden: Option<bool>, pub on_event: Option<HostedHandler>, pub base: Base,
}
#[allow(non_snake_case)]
pub fn TabBar(props: TabBarProps) -> Element {
    Hosted(HostedProps { id: props.id, content: Some(HostedContent::TabBar {
        items: props.items, selected: props.selected, enabled: !props.disabled.unwrap_or(false), hidden: props.hidden.unwrap_or(false),
    }), children: Vec::new(), on_event: props.on_event, base: props.base })
}
