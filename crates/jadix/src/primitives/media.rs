//! Unstyled media primitives. Sources and playback belong to the external controller.
#![allow(non_snake_case)]

use crate::{
    app::{
        pending_images::{queue_image_from_src, texture_key_for_src},
        reactive::bind_prop,
    },
    components::base::{stamp_slot, SlotChrome},
    core::tree::{Element, IntoElementList, LayoutStatus},
    media::{MediaController, MediaSnapshot},
};
use jadix_macros::component;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MediaFit {
    #[default]
    Contain,
    Cover,
    Fill,
}

impl MediaFit {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Contain => "contain",
            Self::Cover => "cover",
            Self::Fill => "fill",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MediaAspectRatio {
    #[default]
    Native,
    Widescreen,
    Standard,
    Square,
    Portrait,
}

impl MediaAspectRatio {
    pub fn ratio(self) -> Option<f32> {
        match self {
            Self::Native => None,
            Self::Widescreen => Some(16.0 / 9.0),
            Self::Standard => Some(4.0 / 3.0),
            Self::Square => Some(1.0),
            Self::Portrait => Some(9.0 / 16.0),
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Native => "Native",
            Self::Widescreen => "16:9",
            Self::Standard => "4:3",
            Self::Square => "1:1",
            Self::Portrait => "9:16",
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::Native => Self::Widescreen,
            Self::Widescreen => Self::Standard,
            Self::Standard => Self::Square,
            Self::Square => Self::Portrait,
            Self::Portrait => Self::Native,
        }
    }
}

impl From<&str> for MediaAspectRatio {
    fn from(value: &str) -> Self {
        match value {
            "16:9" => Self::Widescreen,
            "4:3" => Self::Standard,
            "1:1" => Self::Square,
            "9:16" => Self::Portrait,
            _ => Self::Native,
        }
    }
}
impl From<String> for MediaAspectRatio {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MediaRotation {
    #[default]
    None,
    Clockwise90,
    Clockwise180,
    Clockwise270,
}

impl MediaRotation {
    pub fn degrees(self) -> u16 {
        match self {
            Self::None => 0,
            Self::Clockwise90 => 90,
            Self::Clockwise180 => 180,
            Self::Clockwise270 => 270,
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::None => Self::Clockwise90,
            Self::Clockwise90 => Self::Clockwise180,
            Self::Clockwise180 => Self::Clockwise270,
            Self::Clockwise270 => Self::None,
        }
    }
}

impl From<&str> for MediaRotation {
    fn from(value: &str) -> Self {
        match value {
            "90" => Self::Clockwise90,
            "180" => Self::Clockwise180,
            "270" => Self::Clockwise270,
            _ => Self::None,
        }
    }
}
impl From<String> for MediaRotation {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}

impl From<&str> for MediaFit {
    fn from(value: &str) -> Self {
        match value {
            "cover" => Self::Cover,
            "fill" => Self::Fill,
            _ => Self::Contain,
        }
    }
}

impl From<String> for MediaFit {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}

/// Resolve a video rectangle and source UV window using adapter-reported dimensions.
pub(crate) fn video_geometry(
    attrs: &std::collections::HashMap<String, String>,
    rect: [f32; 4],
) -> ([f32; 4], [f32; 4]) {
    let horizontal = attrs
        .get("jadix_video_mirror_x")
        .is_some_and(|value| value == "true");
    let vertical = attrs
        .get("jadix_video_mirror_y")
        .is_some_and(|value| value == "true");
    let full_uv = [
        if horizontal { 1.0 } else { 0.0 },
        if vertical { 1.0 } else { 0.0 },
        if horizontal { -1.0 } else { 1.0 },
        if vertical { -1.0 } else { 1.0 },
    ];
    let rotation = attrs
        .get("jadix_video_rotation")
        .map_or(MediaRotation::None, |value| {
            MediaRotation::from(value.as_str())
        });
    let rect = if matches!(
        rotation,
        MediaRotation::Clockwise90 | MediaRotation::Clockwise270
    ) {
        [
            rect[0] + (rect[2] - rect[3]) * 0.5,
            rect[1] + (rect[3] - rect[2]) * 0.5,
            rect[3],
            rect[2],
        ]
    } else {
        rect
    };
    let native_ratio = attrs
        .get("jadix_video_size")
        .and_then(|size| size.split_once('x'))
        .and_then(|(width, height)| width.parse::<u32>().ok().zip(height.parse::<u32>().ok()))
        .filter(|(width, height)| *width > 0 && *height > 0)
        .map(|(width, height)| width as f32 / height as f32);
    let ratio = attrs
        .get("jadix_video_content_ratio")
        .and_then(|value| MediaAspectRatio::from(value.as_str()).ratio())
        .or(native_ratio);
    let Some(source_ratio) = ratio else {
        return (rect, full_uv);
    };
    if rect[2] <= 0.0 || rect[3] <= 0.0 {
        return (rect, full_uv);
    }
    let target_ratio = rect[2] / rect[3];
    let fit = attrs
        .get("jadix_video_fit")
        .map(String::as_str)
        .unwrap_or("contain");
    if fit == "contain" {
        let (width, height) = if source_ratio > target_ratio {
            (rect[2], rect[2] / source_ratio)
        } else {
            (rect[3] * source_ratio, rect[3])
        };
        (
            [
                rect[0] + (rect[2] - width) * 0.5,
                rect[1] + (rect[3] - height) * 0.5,
                width,
                height,
            ],
            full_uv,
        )
    } else if fit == "cover" {
        let (width, height) = if source_ratio > target_ratio {
            (target_ratio / source_ratio, 1.0)
        } else {
            (1.0, source_ratio / target_ratio)
        };
        (
            rect,
            [
                (1.0 - width) * 0.5 + if horizontal { width } else { 0.0 },
                (1.0 - height) * 0.5 + if vertical { height } else { 0.0 },
                width * full_uv[2],
                height * full_uv[3],
            ],
        )
    } else {
        (rect, full_uv)
    }
}

fn media_element(
    id: String,
    controller: MediaController,
    video: bool,
    poster: String,
    fit: MediaFit,
) -> Element {
    let mut element = Element::new();
    element.template_id = Some(crate::utils::resolve_instance_id(
        id,
        if video { "video" } else { "audio" },
    ));
    if !video {
        element = element.w(0.0).h(0.0);
    }
    let poster_key = if video && !poster.is_empty() {
        let key = texture_key_for_src(&poster);
        queue_image_from_src(&key, &poster);
        Some(key)
    } else {
        None
    };
    let mut element = stamp_slot(element, if video { "video" } else { "audio" });
    if video {
        element.set_attr("jadix_video_fit", fit.as_str());
    }
    let signal = controller.snapshot();
    bind_prop(
        element,
        "media-primitive",
        move || signal.get().normalized(),
        move |element, snapshot: MediaSnapshot| {
            let mut changed = element.attrs.get("data-state").map(String::as_str)
                != Some(snapshot.state.as_str());
            element.set_attr("data-state", snapshot.state.as_str());
            element.set_attr("data-media-kind", if video { "video" } else { "audio" });
            if video {
                let size = snapshot
                    .video_size
                    .filter(|_| snapshot.video_texture.is_some())
                    .map(|size| format!("{}x{}", size.width, size.height));
                changed |= element.attrs.get("jadix_video_size") != size.as_ref();
                if let Some(size) = size {
                    element.set_attr("jadix_video_size", size);
                } else {
                    element.attrs.remove("jadix_video_size");
                }
                let texture = snapshot.video_texture.or_else(|| poster_key.clone());
                if element.image_texture != texture {
                    element.image_texture = texture;
                    return LayoutStatus::StyleChanged;
                }
            }
            if changed {
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        },
    )
}

/// Nonvisual audio endpoint. Rendering never opens, plays, or stops a source.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Audio(id: String, controller: MediaController) -> Element {
    media_element(id, controller, false, String::new(), MediaFit::Fill)
}

/// Video texture endpoint with optional poster. Size, fit, clipping and chrome are caller-owned.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Video(
    id: String,
    controller: MediaController,
    poster: String,
    fit: MediaFit,
    content_ratio: MediaAspectRatio,
    rotation: MediaRotation,
    mirror_horizontal: bool,
    mirror_vertical: bool,
) -> Element {
    let mut element = media_element(id, controller, true, poster, fit);
    element.set_attr("jadix_video_content_ratio", content_ratio.label());
    element.set_attr("jadix_video_rotation", rotation.degrees().to_string());
    element.set_attr("jadix_video_mirror_x", mirror_horizontal.to_string());
    element.set_attr("jadix_video_mirror_y", mirror_vertical.to_string());
    element
}

pub struct AudioBuilder {
    id: String,
    controller: MediaController,
    chrome: SlotChrome,
}

pub fn audio() -> AudioBuilder {
    AudioBuilder {
        id: String::new(),
        controller: MediaController::default(),
        chrome: SlotChrome::default(),
    }
}

impl AudioBuilder {
    crate::impl_slot_chrome_methods!();
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }
    pub fn controller(mut self, controller: MediaController) -> Self {
        self.controller = controller;
        self
    }
}

impl From<AudioBuilder> for Element {
    fn from(builder: AudioBuilder) -> Self {
        builder.chrome.apply(Audio(AudioProps {
            id: builder.id,
            controller: Some(builder.controller),
            ..Default::default()
        }))
    }
}

impl IntoElementList for AudioBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

pub struct VideoBuilder {
    id: String,
    controller: MediaController,
    poster: String,
    fit: MediaFit,
    content_ratio: MediaAspectRatio,
    rotation: MediaRotation,
    mirror_horizontal: bool,
    mirror_vertical: bool,
    chrome: SlotChrome,
}

pub fn video() -> VideoBuilder {
    VideoBuilder {
        id: String::new(),
        controller: MediaController::default(),
        poster: String::new(),
        fit: MediaFit::default(),
        content_ratio: MediaAspectRatio::default(),
        rotation: MediaRotation::default(),
        mirror_horizontal: false,
        mirror_vertical: false,
        chrome: SlotChrome::default(),
    }
}

impl VideoBuilder {
    crate::impl_slot_chrome_methods!();
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }
    pub fn controller(mut self, controller: MediaController) -> Self {
        self.controller = controller;
        self
    }
    pub fn poster(mut self, poster: impl Into<String>) -> Self {
        self.poster = poster.into();
        self
    }
    pub fn fit(mut self, fit: impl Into<MediaFit>) -> Self {
        self.fit = fit.into();
        self
    }
    pub fn content_ratio(mut self, ratio: impl Into<MediaAspectRatio>) -> Self {
        self.content_ratio = ratio.into();
        self
    }
    pub fn rotation(mut self, rotation: impl Into<MediaRotation>) -> Self {
        self.rotation = rotation.into();
        self
    }
    pub fn mirror_horizontal(mut self, mirror: bool) -> Self {
        self.mirror_horizontal = mirror;
        self
    }
    pub fn mirror_vertical(mut self, mirror: bool) -> Self {
        self.mirror_vertical = mirror;
        self
    }
}

impl From<VideoBuilder> for Element {
    fn from(builder: VideoBuilder) -> Self {
        builder.chrome.apply(Video(VideoProps {
            id: builder.id,
            controller: Some(builder.controller),
            poster: Some(builder.poster),
            fit: Some(builder.fit),
            content_ratio: Some(builder.content_ratio),
            rotation: Some(builder.rotation),
            mirror_horizontal: Some(builder.mirror_horizontal),
            mirror_vertical: Some(builder.mirror_vertical),
            ..Default::default()
        }))
    }
}

impl IntoElementList for VideoBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}
