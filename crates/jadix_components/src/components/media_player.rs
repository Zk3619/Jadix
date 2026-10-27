//! Composable, controlled media chrome. Playback is implemented by external adapters.
#![allow(unused_braces)]

use crate::{
    app::reactive::{bind_prop, bind_text},
    components::{
        base::{stamp_slot, SlotChrome},
        button::button,
        slider::slider,
    },
    core::{
        reactivity::{
            component_root, create_context, create_effect, provide_context, use_context, Context,
            Signal,
        },
        tree::{Element, IntoElementList, LayoutStatus},
    },
    media::{MediaCommand, MediaController, MediaSnapshot, PlaybackState},
    primitives::{apply_disabled, hstack, video, vstack},
    utils::{resolve_instance_id, take_packed, text_node::build_lucide_icon_text_node},
};
use std::{cell::Cell, rc::Rc, time::Duration};

mod customization;
pub use customization::*;
mod presentation;
pub use presentation::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MediaKind {
    #[default]
    Audio,
    Video,
}

impl From<&str> for MediaKind {
    fn from(value: &str) -> Self {
        if value == "video" {
            Self::Video
        } else {
            Self::Audio
        }
    }
}

impl From<String> for MediaKind {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}

#[derive(Clone)]
pub struct MediaPlayerCtx {
    pub id: String,
    pub controller: MediaController,
    pub disabled: bool,
    pub kind: MediaKind,
    pub presentation: MediaPresentationController,
}

fn media_context() -> Context<MediaPlayerCtx> {
    thread_local! { static CONTEXT: Context<MediaPlayerCtx> = create_context(); }
    CONTEXT.with(|context| *context)
}

pub fn use_media_player() -> MediaPlayerCtx {
    use_context(media_context())
        .expect("media parts must be inside media_player_scope or <MediaPlayer>")
}

/// Shared scope for RSX, builders, and application-defined compositions.
pub fn media_player_scope(
    id: impl Into<String>,
    controller: MediaController,
    kind: MediaKind,
    disabled: bool,
    body: impl FnOnce() -> Element,
) -> Element {
    media_player_presentation_scope(
        id,
        controller,
        MediaPresentationController::default(),
        kind,
        disabled,
        body,
    )
}

pub fn media_player_presentation_scope(
    id: impl Into<String>,
    controller: MediaController,
    presentation: MediaPresentationController,
    kind: MediaKind,
    disabled: bool,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = resolve_instance_id(id, "media_player");
    component_root(|| {
        provide_context(
            media_context(),
            MediaPlayerCtx {
                id: id.clone(),
                controller: controller.clone(),
                disabled,
                kind,
                presentation: presentation.clone(),
            },
        );
        let children = take_packed(body());
        let mut root: Element = vstack().class("w-full min-w-0 gap-3 rounded-xl border border-border bg-card p-4 text-card-foreground").children(children).into();
        root.template_id = Some(id.clone());
        root = stamp_slot(root, "media-player").accesskit_role("group");
        root.set_attr(
            "data-media-kind",
            if kind == MediaKind::Video {
                "video"
            } else {
                "audio"
            },
        );
        if kind == MediaKind::Video {
            root.set_attr("data-media-presentation-host", &id);
            presentation::register_presentation(&id, presentation.clone());
            presentation::attach_presentation(&mut root, id.clone());
        }
        let presentation_state = presentation.state();
        root = bind_prop(
            root,
            "media-presentation-state",
            move || presentation_state.get(),
            |element, state| {
                element.set_attr("data-layout", state.layout.as_str());
                element.set_attr("data-fullscreen", state.host.fullscreen.to_string());
                element.set_attr(
                    "data-picture-in-picture",
                    state.host.picture_in_picture.to_string(),
                );
                LayoutStatus::StyleChanged
            },
        );
        let signal = controller.snapshot();
        bind_prop(
            root,
            "media-player-state",
            move || signal.get().state,
            |element, state| {
                let previous = element.attrs.get("data-state").map(String::as_str);
                if previous == Some(state.as_str()) {
                    return LayoutStatus::Clean;
                }
                element.set_attr("data-state", state.as_str());
                LayoutStatus::StyleChanged
            },
        )
    })
}

crate::__component_contract! {
MediaPlayerProps [];
/// Controlled player root. RSX children are evaluated inside the shared scope.
#[allow(non_snake_case)]
pub fn MediaPlayer(
identity [] id: String,
default [] controller: MediaController,
default [] presentation: MediaPresentationController,
default [] kind: MediaKind,
default [] disabled: bool,
default [] poster: String,
default [] slots: MediaPlayerSlots,
plain [] body: Option<Box<dyn FnOnce() -> Element>>,
plain [] children: Option<Element>,
) -> Element {
    media_player_presentation_scope(id, controller, presentation, kind, disabled, || {
        body.map(|body| body())
            .or(children)
            .unwrap_or_else(|| default_body(kind, poster, slots))
    })
}
}

crate::__component_contract! {
AudioPlayerProps [];
#[allow(non_snake_case)]
pub fn AudioPlayer(
identity [] id: String,
default [] controller: MediaController,
default [] presentation: MediaPresentationController,
default [] disabled: bool,
default [] slots: MediaPlayerSlots,
plain [] body: Option<Box<dyn FnOnce() -> Element>>,
plain [] children: Option<Element>,
) -> Element {
    media_player_presentation_scope(
        id,
        controller,
        presentation,
        MediaKind::Audio,
        disabled,
        || {
            body.map(|body| body())
                .or(children)
                .unwrap_or_else(|| default_body(MediaKind::Audio, String::new(), slots))
        },
    )
}
}

crate::__component_contract! {
VideoPlayerProps [];
#[allow(non_snake_case)]
pub fn VideoPlayer(
identity [] id: String,
default [] controller: MediaController,
default [] presentation: MediaPresentationController,
default [] disabled: bool,
default [] poster: String,
default [] slots: MediaPlayerSlots,
plain [] body: Option<Box<dyn FnOnce() -> Element>>,
plain [] children: Option<Element>,
) -> Element {
    media_player_presentation_scope(
        id,
        controller,
        presentation,
        MediaKind::Video,
        disabled,
        || {
            body.map(|body| body())
                .or(children)
                .unwrap_or_else(|| default_body(MediaKind::Video, poster, slots))
        },
    )
}
}

pub struct MediaPlayerBuilder {
    id: String,
    controller: MediaController,
    presentation: MediaPresentationController,
    kind: MediaKind,
    disabled: bool,
    poster: String,
    body: Option<Box<dyn FnOnce() -> Element>>,
    slots: MediaPlayerSlots,
    chrome: SlotChrome,
}

pub fn media_player() -> MediaPlayerBuilder {
    MediaPlayerBuilder {
        id: String::new(),
        controller: MediaController::default(),
        presentation: MediaPresentationController::default(),
        kind: MediaKind::Audio,
        disabled: false,
        poster: String::new(),
        body: None,
        slots: MediaPlayerSlots::default(),
        chrome: SlotChrome::default(),
    }
}

pub fn audio_player() -> MediaPlayerBuilder {
    media_player()
}
pub fn video_player() -> MediaPlayerBuilder {
    media_player().kind(MediaKind::Video)
}

impl MediaPlayerBuilder {
    crate::impl_slot_chrome_methods!();
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }
    pub fn controller(mut self, controller: MediaController) -> Self {
        self.controller = controller;
        self
    }
    pub fn kind(mut self, kind: impl Into<MediaKind>) -> Self {
        self.kind = kind.into();
        self
    }
    pub fn presentation(mut self, presentation: MediaPresentationController) -> Self {
        self.presentation = presentation;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn poster(mut self, poster: impl Into<String>) -> Self {
        self.poster = poster.into();
        self
    }
    pub fn child(mut self, body: impl FnOnce() -> Element + 'static) -> Self {
        self.body = Some(Box::new(body));
        self
    }
    pub fn slots(mut self, slots: MediaPlayerSlots) -> Self {
        self.slots = slots;
        self
    }
}

impl From<MediaPlayerBuilder> for Element {
    fn from(builder: MediaPlayerBuilder) -> Self {
        builder.chrome.apply(MediaPlayer(MediaPlayerProps {
            id: builder.id,
            controller: Some(builder.controller),
            presentation: Some(builder.presentation),
            kind: Some(builder.kind),
            disabled: Some(builder.disabled),
            poster: Some(builder.poster),
            body: builder.body,
            slots: Some(builder.slots),
            ..Default::default()
        }))
    }
}

impl IntoElementList for MediaPlayerBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

crate::__component_contract! {
MediaViewportProps [];
#[allow(non_snake_case)]
pub fn MediaViewport(
default [] poster: String,
plain [] children: Option<Element>,
) -> Element {
    let context = use_media_player();
    let snapshot = context.controller.snapshot();
    let presentation = context.presentation.state();
    let picture: Element = video()
        .controller(context.controller)
        .poster(poster)
        .class("absolute inset-0 w-full h-full")
        .into();
    let picture = bind_prop(
        picture,
        "media-picture",
        move || presentation.get(),
        |element, state| {
            element.set_attr("jadix_video_fit", state.fit.as_str());
            element.set_attr("jadix_video_content_ratio", state.content_ratio.label());
            element.set_attr("jadix_video_rotation", state.rotation.degrees().to_string());
            element.set_attr("jadix_video_mirror_x", state.mirror_horizontal.to_string());
            element.set_attr("jadix_video_mirror_y", state.mirror_vertical.to_string());
            LayoutStatus::StyleChanged
        },
    );
    let mut surface: Element = vstack()
        .class("relative w-full min-h-0 overflow-hidden rounded-md bg-muted")
        .child(picture)
        .into();
    surface = stamp_slot(surface, "media-viewport")
        .accesskit_role("image")
        .aria_label(crate::tr_default("media.video", "Video"));
    surface.template_id = Some(resolve_instance_id("", "media-viewport"));
    if let Some(children) = children {
        surface.children.extend(take_packed(children));
    }
    bind_prop(
        surface,
        "media-viewport-layout",
        move || (presentation.get(), snapshot.get().video_size),
        |element, (state, size)| {
            let mut ratio = state.viewport_ratio.ratio().unwrap_or_else(|| {
                size.filter(|size| size.width > 0 && size.height > 0)
                    .map_or(16.0 / 9.0, |size| size.width as f32 / size.height as f32)
            });
            if state.viewport_ratio == crate::primitives::media::MediaAspectRatio::Native
                && matches!(
                    state.rotation,
                    crate::primitives::media::MediaRotation::Clockwise90
                        | crate::primitives::media::MediaRotation::Clockwise270
                )
            {
                ratio = 1.0 / ratio;
            }
            let ratio = if state.expanded() { None } else { Some(ratio) };
            let grow = if state.expanded() { 1.0 } else { 0.0 };
            if element.style.aspect_ratio == ratio && element.style.flex_grow == grow {
                return LayoutStatus::Clean;
            }
            element.style.aspect_ratio = ratio;
            element.style.flex_grow = grow;
            crate::authoring::widgets::classes::patch_class_set_base_layout(element);
            LayoutStatus::LayoutChanged
        },
    )
}
}

crate::__component_contract! {
MediaControlsProps [];
#[allow(non_snake_case)]
pub fn MediaControls(
plain [] children: Option<Element>,
) -> Element {
    stamp_slot(
        hstack()
            .class("w-full min-w-0 items-center gap-2 flex-wrap")
            .children(children.map(take_packed).unwrap_or_default())
            .into(),
        "media-controls",
    )
    .accesskit_role("group")
    .aria_label(crate::tr_default("media.controls", "Playback controls"))
}
}

fn bound_text(
    slot: &str,
    class: &str,
    select: impl Fn(MediaSnapshot) -> String + 'static,
) -> Element {
    let context = use_media_player();
    let snapshot = context.controller.snapshot();
    let element = crate::app::reactive::bind_text(resolve_instance_id("", slot), move || {
        select(snapshot.get().normalized())
    });
    stamp_slot(element.class(class), slot)
}

crate::__component_contract! {
MediaTitleProps [];
#[allow(non_snake_case)]
pub fn MediaTitle(
plain [] children: Option<Element>,
) -> Element {
    if let Some(children) = children {
        return stamp_slot(children.class("text-sm font-medium"), "media-title");
    }
    bound_text("media-title", "text-sm font-medium", |snapshot| {
        snapshot.metadata.title
    })
}
}

crate::__component_contract! {
MediaDescriptionProps [];
#[allow(non_snake_case)]
pub fn MediaDescription(
plain [] children: Option<Element>,
) -> Element {
    if let Some(children) = children {
        return stamp_slot(
            children.class("text-xs text-muted-foreground"),
            "media-description",
        );
    }
    bound_text(
        "media-description",
        "text-xs text-muted-foreground",
        |snapshot| {
            [snapshot.metadata.artist, snapshot.metadata.album]
                .into_iter()
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join(" · ")
        },
    )
}
}

pub fn format_media_time(time: Duration) -> String {
    let total = time.as_secs();
    if total >= 3600 {
        format!("{}:{:02}:{:02}", total / 3600, total / 60 % 60, total % 60)
    } else {
        format!("{}:{:02}", total / 60, total % 60)
    }
}

crate::__component_contract! {
MediaTimeDisplayProps [];
#[allow(non_snake_case)]
pub fn MediaTimeDisplay(
plain [] children: Option<crate::core::tree::Element>,
) -> Element {
    bound_text("media-time", "text-xs text-muted-foreground", |snapshot| {
        let duration = snapshot
            .duration
            .map(format_media_time)
            .unwrap_or_else(|| "--:--".into());
        format!("{} / {duration}", format_media_time(snapshot.position))
    })
}
}

crate::__component_contract! {
MediaStatusProps [];
#[allow(non_snake_case)]
pub fn MediaStatus(
plain [] children: Option<crate::core::tree::Element>,
) -> Element {
    let kind = use_media_player().kind;
    let mut element = bound_text(
        "media-status",
        "text-xs text-muted-foreground",
        move |snapshot| {
            if kind == MediaKind::Video
                && snapshot.capabilities.audio
                && !snapshot.capabilities.video
            {
                return crate::tr_default(
                    "media.video_unavailable",
                    "This adapter does not support video",
                );
            }
            let (key, fallback) = match snapshot.state {
                PlaybackState::Idle => ("media.idle", "No media loaded"),
                PlaybackState::Loading => ("media.loading", "Loading…"),
                PlaybackState::Buffering => ("media.buffering", "Buffering…"),
                PlaybackState::Ended => ("media.ended", "Playback ended"),
                _ => return String::new(),
            };
            crate::tr_default(key, fallback)
        },
    );
    element.set_attr("aria-live", "polite");
    element
}
}

crate::__component_contract! {
MediaErrorMessageProps [];
#[allow(non_snake_case)]
pub fn MediaErrorMessage(
plain [] children: Option<crate::core::tree::Element>,
) -> Element {
    let context = use_media_player();
    let snapshot = context.controller.snapshot();
    let request_error = context.controller.request_error();
    let presentation_error = context.presentation.request_error();
    let mut element = bind_text(resolve_instance_id("", "media-error"), move || {
        snapshot
            .get()
            .error
            .or_else(|| request_error.get())
            .or_else(|| presentation_error.get())
            .map(|error| error.message)
            .unwrap_or_default()
    });
    element = stamp_slot(element.class("text-xs text-destructive"), "media-error");
    element.set_attr("aria-live", "assertive");
    element
}
}

#[derive(Clone, Copy)]
enum Action {
    Play,
    Mute,
    Stop,
    Seek(f64),
    Rate(f32),
    Loop,
}

impl Action {
    fn command(self, snapshot: &MediaSnapshot) -> Option<MediaCommand> {
        Some(match self {
            Self::Play => {
                if snapshot.wants_playback() {
                    MediaCommand::Pause
                } else {
                    MediaCommand::Play
                }
            }
            Self::Mute => MediaCommand::SetMuted(!snapshot.muted),
            Self::Stop => MediaCommand::Stop,
            Self::Seek(seconds) => {
                let window = snapshot.seek_window()?;
                let seconds = (snapshot.position.as_secs_f64() + seconds)
                    .clamp(window.start().as_secs_f64(), window.end().as_secs_f64());
                MediaCommand::Seek(window.clamp(Duration::try_from_secs_f64(seconds).ok()?))
            }
            Self::Rate(rate) => MediaCommand::SetPlaybackRate(rate),
            Self::Loop => MediaCommand::SetLoop(!snapshot.looping),
        })
    }

    fn presentation(self, snapshot: &MediaSnapshot) -> (&'static str, String) {
        let (icon, key, fallback) = match self {
            Self::Play if snapshot.wants_playback() => ("pause", "media.pause", "Pause"),
            Self::Play => ("play", "media.play", "Play"),
            Self::Mute if snapshot.muted => ("volume-x", "media.unmute", "Unmute"),
            Self::Mute => ("volume-2", "media.mute", "Mute"),
            Self::Stop => ("square", "media.stop", "Stop"),
            Self::Seek(seconds) if seconds < 0.0 => {
                ("rewind", "media.seek_backward", "Seek backward")
            }
            Self::Seek(_) => ("fast-forward", "media.seek_forward", "Seek forward"),
            Self::Rate(_) => ("gauge", "media.rate", "Playback speed"),
            Self::Loop => ("repeat", "media.loop", "Loop"),
        };
        let mut label = crate::tr_default(key, fallback);
        if let Self::Rate(rate) = self {
            label = format!("{label}: {rate}×");
        }
        if let Self::Seek(seconds) = self {
            label = format!("{label}: {} s", seconds.abs());
        }
        (icon, label)
    }
}

fn action_button(slot: &str, action: Action, children: Option<Element>) -> Element {
    let context = use_media_player();
    let snapshot = context.controller.snapshot();
    let initial = context.controller.current();
    let (icon_name, label) = action.presentation(&initial);
    let custom_children = children.is_some();
    let icon = children.unwrap_or_else(|| {
        hstack()
            .child(build_lucide_icon_text_node(icon_name).text_size(16.0))
            .into()
    });
    let controller = context.controller.clone();
    let disabled = context.disabled;
    let mut element: Element = button("")
        .variant("ghost")
        .size("icon-sm")
        .child(icon)
        .on_click(move || {
            if disabled {
                return;
            }
            if let Some(command) = action.command(&controller.current()) {
                let _ = controller.request(command);
            }
        })
        .into();
    element = stamp_slot(element, slot).aria_label(label);
    bind_prop(
        element,
        format!("media-action:{slot}"),
        move || snapshot.get().normalized(),
        move |element, snapshot| {
            let supported = action
                .command(&snapshot)
                .is_some_and(|command| command.validate(&snapshot).is_ok());
            let mut impact = apply_disabled(element, disabled || !supported);
            let (icon_name, label) = action.presentation(&snapshot);
            if element.attrs.get("accesskit_name") != Some(&label) {
                element.set_attr("accesskit_name", label);
                impact = impact.merge(LayoutStatus::StyleChanged);
            }
            if matches!(action, Action::Mute | Action::Loop | Action::Rate(_)) {
                let pressed = match action {
                    Action::Mute => snapshot.muted,
                    Action::Loop => snapshot.looping,
                    Action::Rate(rate) => (snapshot.playback_rate - rate).abs() < f32::EPSILON,
                    _ => false,
                };
                let pressed = if pressed { "true" } else { "false" };
                if element.attrs.get("aria-pressed").map(String::as_str) != Some(pressed) {
                    element.set_attr("aria-pressed", pressed);
                    impact = impact.merge(LayoutStatus::StyleChanged);
                }
            }
            if !custom_children {
                if let Some(icon) = element
                    .children
                    .first_mut()
                    .and_then(|wrapper| wrapper.children.first_mut())
                {
                    if icon
                        .attrs
                        .get(crate::utils::ATTR_ICON_NAME)
                        .map(String::as_str)
                        != Some(icon_name)
                    {
                        *icon = build_lucide_icon_text_node(icon_name).text_size(16.0);
                        impact = impact.merge(LayoutStatus::StyleChanged);
                    }
                }
            }
            impact
        },
    )
}

crate::__component_contract! {
MediaPlayButtonProps [];
#[allow(non_snake_case)]
pub fn MediaPlayButton(
plain [] children: Option<Element>,
) -> Element {
    action_button("media-play", Action::Play, children)
}
}
crate::__component_contract! {
MediaMuteButtonProps [];
#[allow(non_snake_case)]
pub fn MediaMuteButton(
plain [] children: Option<Element>,
) -> Element {
    action_button("media-mute", Action::Mute, children)
}
}
crate::__component_contract! {
MediaStopButtonProps [];
#[allow(non_snake_case)]
pub fn MediaStopButton(
plain [] children: Option<Element>,
) -> Element {
    action_button("media-stop", Action::Stop, children)
}
}
crate::__component_contract! {
MediaLoopButtonProps [];
#[allow(non_snake_case)]
pub fn MediaLoopButton(
plain [] children: Option<Element>,
) -> Element {
    action_button("media-loop", Action::Loop, children)
}
}
crate::__component_contract! {
MediaSeekButtonProps [];
#[allow(non_snake_case)]
pub fn MediaSeekButton(
plain [] seconds: Option<f64>,
plain [] children: Option<Element>,
) -> Element {
    action_button(
        "media-seek",
        Action::Seek(
            seconds
                .filter(|seconds| seconds.is_finite())
                .unwrap_or(10.0),
        ),
        children,
    )
}
}
crate::__component_contract! {
MediaRateButtonProps [];
#[allow(non_snake_case)]
pub fn MediaRateButton(
plain [] rate: Option<f32>,
plain [] children: Option<Element>,
) -> Element {
    let rate = rate
        .filter(|rate| rate.is_finite() && *rate > 0.0)
        .unwrap_or(1.5);
    let children = children.or_else(|| {
        Some(
            Element::new()
                .text(format!("{rate}×"))
                .class("text-xs text-foreground"),
        )
    });
    action_button("media-rate", Action::Rate(rate), children)
}
}

fn media_slider(volume: bool) -> Element {
    let context = use_media_player();
    let snapshot_signal = context.controller.snapshot();
    let initial = context.controller.current();
    let value_of = move |snapshot: &MediaSnapshot| -> f32 {
        if volume {
            snapshot.volume
        } else {
            snapshot
                .seek_window()
                .map(|range| {
                    (range
                        .clamp(snapshot.position)
                        .saturating_sub(range.start())
                        .as_secs_f64()
                        / range.duration().as_secs_f64()) as f32
                })
                .unwrap_or(0.0)
        }
    };
    let value = Signal::new(value_of(&initial));
    let accepted = Rc::new(Cell::new(value.peek()));
    let observed = accepted.clone();
    create_effect(move || {
        let snapshot = snapshot_signal.get().normalized();
        let next = value_of(&snapshot);
        observed.set(next);
        value.set_silent(next);
    });
    let controller = context.controller.clone();
    let disabled = context.disabled;
    create_effect(move || {
        let next = value.get();
        if accepted.replace(next) == next || disabled {
            return;
        }
        let snapshot = controller.current();
        let command = if volume {
            Some(MediaCommand::SetVolume(next))
        } else {
            snapshot.seek_window().and_then(|range| {
                let seconds =
                    range.start().as_secs_f64() + range.duration().as_secs_f64() * f64::from(next);
                Duration::try_from_secs_f64(seconds)
                    .ok()
                    .map(|position| MediaCommand::Seek(range.clamp(position)))
            })
        };
        if let Some(command) = command {
            if controller.request(command).is_err() {
                let confirmed = value_of(&controller.current());
                accepted.set(confirmed);
                value.set_silent(confirmed);
            }
        }
    });
    let slot = if volume {
        "media-volume"
    } else {
        "media-seek-slider"
    };
    let mut element: Element = slider(resolve_instance_id("", slot))
        .value_signal(value)
        .min(0.0)
        .max(1.0)
        .step(0.001)
        .into();
    let pointer_controller = context.controller.clone();
    let pointer_down = element.on_mousedown.clone();
    element = element.on_mousedown(move |event| {
        let snapshot = pointer_controller.current();
        let available = !disabled
            && if volume {
                snapshot.capabilities.volume
            } else {
                snapshot.seek_window().is_some()
            };
        if available {
            if let Some(handler) = &pointer_down {
                handler.borrow_mut()(event);
            }
        }
    });
    let key_controller = context.controller.clone();
    let key_down = element.on_key_down.clone();
    element = element.on_key_down(move |event| {
        let snapshot = key_controller.current();
        let available = !disabled
            && if volume {
                snapshot.capabilities.volume
            } else {
                snapshot.seek_window().is_some()
            };
        available
            && key_down
                .as_ref()
                .is_some_and(|handler| handler.borrow_mut()(event))
    });
    element = stamp_slot(element, slot)
        .class(if volume {
            "w-24 min-w-0"
        } else {
            "w-full min-w-0"
        })
        .aria_label(crate::tr_default(
            if volume { "media.volume" } else { "media.seek" },
            if volume { "Volume" } else { "Seek" },
        ));
    bind_prop(
        element,
        slot,
        move || snapshot_signal.get().normalized(),
        move |element, snapshot| {
            let available = !disabled
                && if volume {
                    snapshot.capabilities.volume
                } else {
                    snapshot.seek_window().is_some()
                };
            let impact = apply_disabled(element, !available);
            element.set_focusable(available);
            element.set_attr("aria-valuemin", "0");
            element.set_attr("aria-valuemax", if volume { "100" } else { "1" });
            element.set_attr(
                "aria-valuenow",
                if volume {
                    format!("{:.0}", snapshot.volume * 100.0)
                } else {
                    value_of(&snapshot).to_string()
                },
            );
            element.set_attr(
                "aria-valuetext",
                if volume {
                    format!("{:.0}%", snapshot.volume * 100.0)
                } else {
                    format_media_time(snapshot.position)
                },
            );
            impact.merge(LayoutStatus::StyleChanged)
        },
    )
}

crate::__component_contract! {
MediaSeekSliderProps [];
#[allow(non_snake_case)]
pub fn MediaSeekSlider(
plain [] preview: Option<Signal<Option<MediaTimelinePreview>>>,
plain [] children: Option<crate::core::tree::Element>,
) -> Element {
    let element = media_slider(false);
    let Some(preview) = preview else {
        return element;
    };
    let context = use_media_player();
    let snapshot = context.controller.snapshot();
    let disabled = context.disabled;
    let previous_window = Cell::new(snapshot.peek().seek_window());
    create_effect(move || {
        let current = snapshot.get();
        let window = current.seek_window();
        let window_changed = previous_window.replace(window) != window;
        let previous = preview.try_peek().flatten();
        if previous.is_some_and(|value| {
            value.load_session != current.load_session
                || disabled
                || window_changed
                || window.is_none_or(|range| range.clamp(value.position) != value.position)
        }) {
            preview.try_set(None);
        }
    });
    let on_move = element.on_mousemove.clone();
    let on_out = element.on_mouseout.clone();
    element
        .on_mousemove(move |event| {
            if let Some(handler) = &on_move {
                handler.borrow_mut()(event);
            }
            let value = if disabled {
                None
            } else {
                snapshot.try_peek().and_then(|snapshot| {
                    customization::preview_at(&snapshot, event.x, event.current_bounds)
                })
            };
            if preview.try_peek().flatten() != value {
                preview.try_set(value);
            }
        })
        .on_mouseout(move |event| {
            if let Some(handler) = &on_out {
                handler.borrow_mut()(event);
            }
            if preview.try_peek().flatten().is_some() {
                preview.try_set(None);
            }
        })
}
}
crate::__component_contract! {
MediaVolumeSliderProps [];
#[allow(non_snake_case)]
pub fn MediaVolumeSlider(
plain [] children: Option<crate::core::tree::Element>,
) -> Element {
    media_slider(true)
}
}

pub fn media_time_display() -> Element {
    MediaTimeDisplay(MediaTimeDisplayProps::default())
}
pub fn media_status() -> Element {
    MediaStatus(MediaStatusProps::default())
}
pub fn media_error_message() -> Element {
    MediaErrorMessage(MediaErrorMessageProps::default())
}
pub fn media_seek_slider() -> Element {
    MediaSeekSlider(MediaSeekSliderProps::default())
}
pub fn media_volume_slider() -> Element {
    MediaVolumeSlider(MediaVolumeSliderProps::default())
}
pub fn media_seek_button(seconds: f64) -> Element {
    MediaSeekButton(MediaSeekButtonProps {
        seconds: Some(seconds),
        ..Default::default()
    })
}
pub fn media_rate_button(rate: f32) -> Element {
    MediaRateButton(MediaRateButtonProps {
        rate: Some(rate),
        ..Default::default()
    })
}
pub fn media_viewport(poster: impl Into<String>) -> Element {
    MediaViewport(MediaViewportProps {
        poster: Some(poster.into()),
        ..Default::default()
    })
}

crate::impl_simple_slot_builder!(
    MediaControlsBuilder,
    media_controls,
    MediaControls,
    MediaControlsProps
);
crate::impl_simple_slot_builder!(MediaTitleBuilder, media_title, MediaTitle, MediaTitleProps);
crate::impl_simple_slot_builder!(
    MediaDescriptionBuilder,
    media_description,
    MediaDescription,
    MediaDescriptionProps
);
crate::impl_simple_slot_builder!(
    MediaPlayButtonBuilder,
    media_play_button,
    MediaPlayButton,
    MediaPlayButtonProps
);
crate::impl_simple_slot_builder!(
    MediaMuteButtonBuilder,
    media_mute_button,
    MediaMuteButton,
    MediaMuteButtonProps
);
crate::impl_simple_slot_builder!(
    MediaStopButtonBuilder,
    media_stop_button,
    MediaStopButton,
    MediaStopButtonProps
);
crate::impl_simple_slot_builder!(
    MediaLoopButtonBuilder,
    media_loop_button,
    MediaLoopButton,
    MediaLoopButtonProps
);

#[cfg(all(test, feature = "product-tests"))]
#[path = "media_player_tests.rs"]
mod tests;
