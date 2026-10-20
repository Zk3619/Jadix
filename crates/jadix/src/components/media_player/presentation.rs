use super::{use_media_player, MediaKind};
use crate::{
    app::reactive::bind_prop,
    components::{base::stamp_slot, button::button},
    core::{
        reactivity::Signal,
        tree::{Element, LayoutStatus},
    },
    primitives::apply_disabled,
    utils::text_node::build_lucide_icon_text_node,
};
use jadix_macros::component;
use std::{cell::Cell, rc::Rc};

#[derive(Clone, Copy)]
enum PresentationAction {
    Wide,
    WindowFullscreen,
    Fullscreen,
    PictureInPicture,
    Mirror(bool),
    Rotate,
    Ratio(bool),
    Fit,
    Reset,
}

impl PresentationAction {
    fn describe(self, state: MediaPresentationState) -> (&'static str, String, bool) {
        match self {
            Self::Wide => (
                "rectangle-horizontal",
                crate::tr_default("media.wide", "Wide mode"),
                state.layout == MediaLayoutMode::Wide,
            ),
            Self::WindowFullscreen => (
                "expand",
                crate::tr_default("media.window_fullscreen", "Window fullscreen"),
                state.layout == MediaLayoutMode::WindowFullscreen,
            ),
            Self::Fullscreen => (
                "maximize",
                crate::tr_default("media.fullscreen", "System fullscreen"),
                state.host.fullscreen,
            ),
            Self::PictureInPicture => (
                "picture-in-picture-2",
                crate::tr_default("media.pip", "Picture in picture"),
                state.host.picture_in_picture,
            ),
            Self::Mirror(vertical) => (
                if vertical {
                    "flip-vertical-2"
                } else {
                    "flip-horizontal-2"
                },
                crate::tr_default(
                    if vertical {
                        "media.flip_vertical"
                    } else {
                        "media.flip_horizontal"
                    },
                    if vertical {
                        "Mirror vertically"
                    } else {
                        "Mirror horizontally"
                    },
                ),
                if vertical {
                    state.mirror_vertical
                } else {
                    state.mirror_horizontal
                },
            ),
            Self::Rotate => (
                "rotate-cw",
                format!(
                    "{}: {}",
                    crate::tr_default("media.rotate", "Rotate clockwise"),
                    state.rotation.degrees()
                ),
                state.rotation != MediaRotation::default(),
            ),
            Self::Ratio(content) => (
                "ratio",
                format!(
                    "{}: {}",
                    crate::tr_default(
                        if content {
                            "media.content_ratio"
                        } else {
                            "media.viewport_ratio"
                        },
                        if content {
                            "Picture aspect ratio"
                        } else {
                            "Viewport aspect ratio"
                        }
                    ),
                    if content {
                        state.content_ratio
                    } else {
                        state.viewport_ratio
                    }
                    .label()
                ),
                false,
            ),
            Self::Fit => (
                "scan",
                format!(
                    "{}: {}",
                    crate::tr_default("media.fit", "Picture fit"),
                    state.fit.as_str()
                ),
                false,
            ),
            Self::Reset => (
                "undo-2",
                crate::tr_default("media.reset_picture", "Reset picture"),
                false,
            ),
        }
    }

    fn host_action(self, state: MediaPresentationState) -> Option<MediaHostAction> {
        match self {
            Self::Fullscreen => Some(MediaHostAction::Fullscreen(!state.host.fullscreen)),
            Self::PictureInPicture => Some(MediaHostAction::PictureInPicture(
                !state.host.picture_in_picture,
            )),
            _ => None,
        }
    }

    fn available(self, controller: &MediaPresentationController) -> bool {
        let state = controller.current();
        self.host_action(state)
            .is_none_or(|action| controller.can_request(action))
            && !(state.host.fullscreen && matches!(self, Self::Wide | Self::WindowFullscreen))
    }

    fn run(self, controller: &MediaPresentationController) {
        let state = controller.current();
        if let Some(action) = self.host_action(state) {
            let _ = controller.request(action);
            return;
        }
        match self {
            Self::Wide | Self::WindowFullscreen => {
                let target = if matches!(self, Self::Wide) {
                    MediaLayoutMode::Wide
                } else {
                    MediaLayoutMode::WindowFullscreen
                };
                if state.layout == MediaLayoutMode::WindowFullscreen && state.layout == target {
                    controller.exit_expanded();
                } else {
                    controller.set_layout(if state.layout == target {
                        MediaLayoutMode::Inline
                    } else {
                        target
                    });
                }
            }
            Self::Mirror(vertical) => controller.set_mirror(
                if vertical {
                    state.mirror_horizontal
                } else {
                    !state.mirror_horizontal
                },
                if vertical {
                    !state.mirror_vertical
                } else {
                    state.mirror_vertical
                },
            ),
            Self::Rotate => controller.set_rotation(state.rotation.next()),
            Self::Ratio(true) => controller.set_content_ratio(state.content_ratio.next()),
            Self::Ratio(false) => controller.set_viewport_ratio(state.viewport_ratio.next()),
            Self::Fit => controller.set_fit(match state.fit {
                MediaFit::Contain => MediaFit::Cover,
                MediaFit::Cover => MediaFit::Fill,
                MediaFit::Fill => MediaFit::Contain,
            }),
            Self::Reset => controller.reset_picture(),
            _ => {}
        }
    }
}

fn presentation_button(
    action: PresentationAction,
    slot: &'static str,
    children: Option<Element>,
) -> Element {
    let context = use_media_player();
    let controller = context.presentation;
    let (icon, label, _) = action.describe(controller.current());
    let click_controller = controller.clone();
    let unavailable = context.disabled || context.kind != MediaKind::Video;
    let element: Element = button("")
        .variant("ghost")
        .size("icon-sm")
        .child(children.unwrap_or_else(|| build_lucide_icon_text_node(icon).text_size(16.0)))
        .on_click(move || {
            if !unavailable && action.available(&click_controller) {
                action.run(&click_controller);
            }
        })
        .into();
    let state = controller.state();
    let caps = controller.capabilities();
    let pending = controller.pending();
    let element = stamp_slot(element, slot).aria_label(label);
    bind_prop(
        element,
        slot,
        move || (state.get(), caps.get(), pending.get()),
        move |element, (state, _, pending)| {
            let disabled = unavailable || !action.available(&controller);
            let impact = apply_disabled(element, disabled);
            element.set_focusable(!disabled);
            let (_, label, pressed) = action.describe(state);
            element.set_attr("accesskit_name", &label);
            element.set_attr("title", label);
            element.set_attr("aria-pressed", pressed.to_string());
            element.set_attr(
                "aria-busy",
                (pending.is_some() && action.host_action(state).is_some()).to_string(),
            );
            impact.merge(LayoutStatus::StyleChanged)
        },
    )
}

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn MediaWideButton(children: Option<Element>) -> Element {
    presentation_button(PresentationAction::Wide, "media-wide", children)
}
crate::impl_simple_slot_builder!(
    MediaWideButtonBuilder,
    media_wide_button,
    MediaWideButton,
    MediaWideButtonProps
);

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn MediaWindowFullscreenButton(children: Option<Element>) -> Element {
    presentation_button(
        PresentationAction::WindowFullscreen,
        "media-window-fullscreen",
        children,
    )
}
crate::impl_simple_slot_builder!(
    MediaWindowFullscreenButtonBuilder,
    media_window_fullscreen_button,
    MediaWindowFullscreenButton,
    MediaWindowFullscreenButtonProps
);

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn MediaFullscreenButton(children: Option<Element>) -> Element {
    presentation_button(PresentationAction::Fullscreen, "media-fullscreen", children)
}
crate::impl_simple_slot_builder!(
    MediaFullscreenButtonBuilder,
    media_fullscreen_button,
    MediaFullscreenButton,
    MediaFullscreenButtonProps
);

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn MediaPictureInPictureButton(children: Option<Element>) -> Element {
    presentation_button(PresentationAction::PictureInPicture, "media-pip", children)
}
crate::impl_simple_slot_builder!(
    MediaPictureInPictureButtonBuilder,
    media_picture_in_picture_button,
    MediaPictureInPictureButton,
    MediaPictureInPictureButtonProps
);

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn MediaRotateButton(children: Option<Element>) -> Element {
    presentation_button(PresentationAction::Rotate, "media-rotate", children)
}
crate::impl_simple_slot_builder!(
    MediaRotateButtonBuilder,
    media_rotate_button,
    MediaRotateButton,
    MediaRotateButtonProps
);

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn MediaFitButton(children: Option<Element>) -> Element {
    presentation_button(PresentationAction::Fit, "media-fit", children)
}
crate::impl_simple_slot_builder!(
    MediaFitButtonBuilder,
    media_fit_button,
    MediaFitButton,
    MediaFitButtonProps
);

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn MediaResetPictureButton(children: Option<Element>) -> Element {
    presentation_button(PresentationAction::Reset, "media-reset-picture", children)
}
crate::impl_simple_slot_builder!(
    MediaResetPictureButtonBuilder,
    media_reset_picture_button,
    MediaResetPictureButton,
    MediaResetPictureButtonProps
);

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn MediaMirrorButton(vertical: bool, children: Option<Element>) -> Element {
    presentation_button(
        PresentationAction::Mirror(vertical),
        "media-mirror",
        children,
    )
}

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn MediaAspectRatioButton(content: bool, children: Option<Element>) -> Element {
    presentation_button(
        PresentationAction::Ratio(content),
        "media-aspect-ratio",
        children,
    )
}

pub fn media_mirror_button(vertical: bool) -> Element {
    MediaMirrorButton(MediaMirrorButtonProps {
        vertical: Some(vertical),
        ..Default::default()
    })
}

pub fn media_aspect_ratio_button(content: bool) -> Element {
    MediaAspectRatioButton(MediaAspectRatioButtonProps {
        content: Some(content),
        ..Default::default()
    })
}

mod viewport;
pub(super) use viewport::register_presentation;
pub(crate) use viewport::sync_media_presentation_subtree;

#[cfg(test)]
mod tests;
use crate::primitives::media::{MediaAspectRatio, MediaFit, MediaRotation};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MediaLayoutMode {
    #[default]
    Inline,
    Wide,
    WindowFullscreen,
}

impl MediaLayoutMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Inline => "inline",
            Self::Wide => "wide",
            Self::WindowFullscreen => "window-fullscreen",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MediaPresentationCapabilities {
    pub fullscreen: bool,
    pub picture_in_picture: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MediaHostState {
    pub fullscreen: bool,
    pub picture_in_picture: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaPresentationState {
    pub layout: MediaLayoutMode,
    pub host: MediaHostState,
    pub viewport_ratio: MediaAspectRatio,
    pub content_ratio: MediaAspectRatio,
    pub fit: MediaFit,
    pub mirror_horizontal: bool,
    pub mirror_vertical: bool,
    pub rotation: MediaRotation,
}

impl Default for MediaPresentationState {
    fn default() -> Self {
        Self {
            layout: MediaLayoutMode::Inline,
            host: MediaHostState::default(),
            viewport_ratio: MediaAspectRatio::Widescreen,
            content_ratio: MediaAspectRatio::Native,
            fit: MediaFit::Contain,
            mirror_horizontal: false,
            mirror_vertical: false,
            rotation: MediaRotation::default(),
        }
    }
}

impl MediaPresentationState {
    pub fn expanded(self) -> bool {
        self.host.fullscreen || self.layout == MediaLayoutMode::WindowFullscreen
    }

    /// Applications use this to hide adjacent columns and release their width limits.
    pub fn hide_surroundings(self) -> bool {
        self.expanded() || self.layout == MediaLayoutMode::Wide
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaHostAction {
    Fullscreen(bool),
    PictureInPicture(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaPresentationRequest {
    pub id: u64,
    pub action: MediaHostAction,
}

/// UI-owned presentation state with acknowledged asynchronous host requests.
/// Create outside transient player views. Host callbacks enqueue work on the owning window.
#[derive(Clone)]
pub struct MediaPresentationController {
    state: Signal<MediaPresentationState>,
    capabilities: Signal<MediaPresentationCapabilities>,
    pending: Signal<Option<MediaPresentationRequest>>,
    error: Signal<Option<crate::media::MediaError>>,
    next_request: Rc<Cell<u64>>,
    return_layout: Rc<Cell<MediaLayoutMode>>,
    host: Rc<dyn Fn(MediaPresentationRequest) -> Result<(), crate::media::MediaError>>,
}

impl Default for MediaPresentationController {
    fn default() -> Self {
        Self::new(MediaPresentationCapabilities::default(), |_| {
            Err(crate::media::MediaError::unsupported("presentation host"))
        })
    }
}

impl MediaPresentationController {
    pub fn new(
        capabilities: MediaPresentationCapabilities,
        host: impl Fn(MediaPresentationRequest) -> Result<(), crate::media::MediaError> + 'static,
    ) -> Self {
        Self {
            state: Signal::new(MediaPresentationState::default()),
            capabilities: Signal::new(capabilities),
            pending: Signal::new(None),
            error: Signal::new(None),
            next_request: Rc::new(Cell::new(0)),
            return_layout: Rc::new(Cell::new(MediaLayoutMode::Inline)),
            host: Rc::new(host),
        }
    }

    pub fn state(&self) -> Signal<MediaPresentationState> {
        self.state
    }
    pub fn current(&self) -> MediaPresentationState {
        self.state.try_peek().unwrap_or_default()
    }
    pub fn capabilities(&self) -> Signal<MediaPresentationCapabilities> {
        self.capabilities
    }
    pub fn pending(&self) -> Signal<Option<MediaPresentationRequest>> {
        self.pending
    }
    pub fn request_error(&self) -> Signal<Option<crate::media::MediaError>> {
        self.error
    }

    pub fn set_capabilities(&self, capabilities: MediaPresentationCapabilities) {
        self.capabilities.try_set(capabilities);
        crate::app::runtime::request_redraw();
    }

    fn update(&self, update: impl FnOnce(&mut MediaPresentationState)) {
        if let Some(mut state) = self.state.try_peek() {
            let previous = state;
            update(&mut state);
            if state != previous {
                self.state.try_set(state);
                crate::app::runtime::request_redraw();
            }
        }
    }

    pub fn set_layout(&self, layout: MediaLayoutMode) {
        let current = self.current().layout;
        if layout == MediaLayoutMode::WindowFullscreen && current != layout {
            self.return_layout.set(current);
        }
        self.update(|state| state.layout = layout);
    }

    pub fn exit_expanded(&self) {
        if self.current().host.fullscreen {
            let _ = self.request(MediaHostAction::Fullscreen(false));
        } else if self.current().layout == MediaLayoutMode::WindowFullscreen {
            self.set_layout(self.return_layout.get());
        }
    }
    pub fn set_viewport_ratio(&self, ratio: MediaAspectRatio) {
        self.update(|state| state.viewport_ratio = ratio);
    }
    pub fn set_content_ratio(&self, ratio: MediaAspectRatio) {
        self.update(|state| state.content_ratio = ratio);
    }
    pub fn set_fit(&self, fit: MediaFit) {
        self.update(|state| state.fit = fit);
    }
    pub fn set_rotation(&self, rotation: MediaRotation) {
        self.update(|state| state.rotation = rotation);
    }
    pub fn set_mirror(&self, horizontal: bool, vertical: bool) {
        self.update(|state| {
            state.mirror_horizontal = horizontal;
            state.mirror_vertical = vertical;
        });
    }

    pub fn reset_picture(&self) {
        self.update(|state| {
            let defaults = MediaPresentationState::default();
            state.viewport_ratio = defaults.viewport_ratio;
            state.content_ratio = defaults.content_ratio;
            state.fit = defaults.fit;
            state.rotation = defaults.rotation;
            state.mirror_horizontal = false;
            state.mirror_vertical = false;
        });
    }

    pub fn can_request(&self, action: MediaHostAction) -> bool {
        if !self.state.is_alive() || self.pending.try_peek().flatten().is_some() {
            return false;
        }
        let state = self.current().host;
        let caps = self.capabilities.try_peek().unwrap_or_default();
        match action {
            MediaHostAction::Fullscreen(enter) => {
                !enter || (caps.fullscreen && !state.picture_in_picture)
            }
            MediaHostAction::PictureInPicture(enter) => {
                !enter || (caps.picture_in_picture && !state.fullscreen)
            }
        }
    }

    /// Enqueue only. Confirmed host state changes exclusively through publish/observe_host.
    pub fn request(&self, action: MediaHostAction) -> Result<u64, crate::media::MediaError> {
        let result = (|| {
            if !self.state.is_alive() {
                return Err(crate::media::MediaError::new(
                    crate::media::MediaErrorKind::Disconnected,
                    "presentation owner was disposed",
                ));
            }
            if !self.can_request(action) {
                return Err(crate::media::MediaError::unsupported(
                    "presentation request unavailable",
                ));
            }
            let id = self.next_request.get().checked_add(1).ok_or_else(|| {
                crate::media::MediaError::invalid("presentation request id exhausted")
            })?;
            self.next_request.set(id);
            let request = MediaPresentationRequest { id, action };
            self.pending.try_set(Some(request));
            self.error.try_set(None);
            if let Err(error) = (self.host)(request) {
                if self.pending.try_peek().flatten() == Some(request) {
                    self.pending.try_set(None);
                }
                return Err(error);
            }
            Ok(id)
        })();
        if let Err(error) = &result {
            self.error.try_set(Some(error.clone()));
        }
        crate::app::runtime::request_redraw();
        result
    }

    /// Acknowledge exactly the pending request; late replies cannot overwrite newer observations.
    pub fn publish(
        &self,
        id: u64,
        result: Result<MediaHostState, crate::media::MediaError>,
    ) -> bool {
        let Some(request) = self
            .pending
            .try_peek()
            .flatten()
            .filter(|request| request.id == id)
        else {
            return false;
        };
        let result = result.and_then(|state| {
            let matches = match request.action {
                MediaHostAction::Fullscreen(value) => state.fullscreen == value,
                MediaHostAction::PictureInPicture(value) => state.picture_in_picture == value,
            };
            if !matches || (state.fullscreen && state.picture_in_picture) {
                Err(crate::media::MediaError::invalid(
                    "host did not confirm requested presentation",
                ))
            } else {
                Ok(state)
            }
        });
        self.pending.try_set(None);
        match result {
            Ok(host) => {
                self.update(|state| state.host = host);
                self.error.try_set(None);
            }
            Err(error) => {
                self.error.try_set(Some(error));
            }
        }
        crate::app::runtime::request_redraw();
        true
    }

    /// OS-initiated exits or window closure supersede in-flight requests.
    pub fn observe_host(&self, host: MediaHostState) -> bool {
        if !self.state.is_alive() || (host.fullscreen && host.picture_in_picture) {
            return false;
        }
        self.pending.try_set(None);
        self.error.try_set(None);
        self.update(|state| state.host = host);
        true
    }
}
