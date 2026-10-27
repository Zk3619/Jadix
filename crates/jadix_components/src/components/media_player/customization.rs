use super::{
    media_error_message, media_rate_button, media_seek_button, media_seek_slider, media_status,
    media_time_display, media_viewport, media_volume_slider, use_media_player, MediaControls,
    MediaControlsProps, MediaDescription, MediaDescriptionProps, MediaKind, MediaLoopButton,
    MediaLoopButtonProps, MediaMuteButton, MediaMuteButtonProps, MediaPlayButton,
    MediaPlayButtonProps, MediaSeekSlider, MediaSeekSliderProps, MediaTitle, MediaTitleProps,
};
use crate::{
    app::reactive::bind_prop,
    components::{
        base::{stamp_slot, SlotChrome, SlotMap},
        button::button,
    },
    core::{
        reactivity::Signal,
        tree::{Element, IntoElementList, LayoutStatus, PointerEvents},
    },
    media::MediaSnapshot,
    primitives::{apply_disabled, audio, hstack, vstack},
    utils::take_packed,
};
use std::{rc::Rc, time::Duration};

/// Stable regions of the default player. Custom children replace the entire layout instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaPlayerSlot {
    Header,
    Viewport,
    Overlay,
    Timeline,
    ControlsStart,
    ControlsEnd,
    Presentation,
    Settings,
    Sidebar,
    Footer,
}

impl MediaPlayerSlot {
    fn name(self) -> &'static str {
        match self {
            Self::Header => "media-header",
            Self::Viewport => "media-surface",
            Self::Overlay => "media-overlays",
            Self::Timeline => "media-timeline",
            Self::ControlsStart => "media-controls-start",
            Self::ControlsEnd => "media-controls-end",
            Self::Presentation => "media-presentation-controls",
            Self::Settings => "media-settings",
            Self::Sidebar => "media-sidebar",
            Self::Footer => "media-footer",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Placement {
    Before,
    Replace,
    After,
}

#[derive(Clone)]
struct SlotContent {
    slot: MediaPlayerSlot,
    placement: Placement,
    render: Rc<dyn Fn() -> Element>,
}

/// Lazy UI composition, not a backend plugin registry. Factories run inside the player owner
/// and context, never at configuration time. Clone this value to reuse a layout across players.
#[derive(Clone, Default)]
pub struct MediaPlayerSlots {
    content: Vec<SlotContent>,
    chrome: SlotMap,
}

impl MediaPlayerSlots {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn style(
        mut self,
        slot: MediaPlayerSlot,
        style: impl FnOnce(SlotChrome) -> SlotChrome,
    ) -> Self {
        self.chrome = self.chrome.slot(slot.name(), style);
        self
    }

    /// Replace only this region. The last replacement wins without running earlier factories.
    pub fn replace(
        mut self,
        slot: MediaPlayerSlot,
        render: impl Fn() -> Element + 'static,
    ) -> Self {
        self.content
            .retain(|entry| entry.slot != slot || entry.placement != Placement::Replace);
        self.push(slot, Placement::Replace, render);
        self
    }

    pub fn before(mut self, slot: MediaPlayerSlot, render: impl Fn() -> Element + 'static) -> Self {
        self.push(slot, Placement::Before, render);
        self
    }

    pub fn after(mut self, slot: MediaPlayerSlot, render: impl Fn() -> Element + 'static) -> Self {
        self.push(slot, Placement::After, render);
        self
    }

    /// Remove the default content without constructing its controls or subscriptions.
    pub fn hide(self, slot: MediaPlayerSlot) -> Self {
        self.replace(slot, empty_content)
    }

    fn push(
        &mut self,
        slot: MediaPlayerSlot,
        placement: Placement,
        render: impl Fn() -> Element + 'static,
    ) {
        self.content.push(SlotContent {
            slot,
            placement,
            render: Rc::new(render),
        });
    }

    fn render(&self, slot: MediaPlayerSlot, default: impl FnOnce() -> Element) -> Option<Element> {
        let entries: Vec<_> = self
            .content
            .iter()
            .filter(|entry| entry.slot == slot)
            .collect();
        let mut children = Vec::new();
        for entry in entries
            .iter()
            .filter(|entry| entry.placement == Placement::Before)
        {
            children.extend(take_packed((entry.render)()));
        }
        let replacement = entries
            .iter()
            .find(|entry| entry.placement == Placement::Replace);
        children.extend(take_packed(
            replacement.map_or_else(default, |entry| (entry.render)()),
        ));
        for entry in entries
            .iter()
            .filter(|entry| entry.placement == Placement::After)
        {
            children.extend(take_packed((entry.render)()));
        }
        if children.is_empty() {
            return None;
        }
        let row = matches!(
            slot,
            MediaPlayerSlot::ControlsStart
                | MediaPlayerSlot::ControlsEnd
                | MediaPlayerSlot::Presentation
        );
        let root = if row { hstack() } else { vstack() };
        let mut element = self.chrome.paint(
            slot.name(),
            root.class(match slot {
                MediaPlayerSlot::Sidebar => "w-64 min-w-0 gap-2",
                MediaPlayerSlot::ControlsEnd => "min-w-0 items-center gap-2 flex-wrap ml-auto",
                _ if row => "min-w-0 items-center gap-2 flex-wrap",
                _ => "w-full min-w-0 gap-2",
            })
            .children(children)
            .into(),
        );
        Some(if slot == MediaPlayerSlot::Sidebar {
            if element.template_id.is_none() {
                element.template_id = Some(crate::utils::resolve_instance_id("", "media-sidebar"));
            }
            let state = use_media_player().presentation.state();
            let display = element.style.display;
            bind_prop(
                element,
                "media-sidebar-layout",
                move || state.get().hide_surroundings(),
                move |element, hidden| {
                    let next = if hidden {
                        taffy::style::Display::None
                    } else {
                        display
                    };
                    if element.style.display == next {
                        return LayoutStatus::Clean;
                    }
                    element.style.display = next;
                    crate::authoring::widgets::classes::patch_class_set_base_layout(element);
                    LayoutStatus::LayoutChanged
                },
            )
        } else if slot == MediaPlayerSlot::Viewport {
            presentation_flex(element)
        } else {
            element
        })
    }
}

fn packed(elements: Vec<Element>) -> Element {
    crate::components::base::packed_children(elements).unwrap_or_else(empty_content)
}

fn empty_content() -> Element {
    let mut fragment = Element::new();
    fragment.set_attr("jadix_fragment", "1");
    fragment
}

fn presentation_flex(mut element: Element) -> Element {
    if element.template_id.is_none() {
        element.template_id = Some(crate::utils::resolve_instance_id("", "media-layout"));
    }
    let state = use_media_player().presentation.state();
    let grow = element.style.flex_grow;
    let basis = element.style.flex_basis;
    let wrap = element.style.flex_wrap;
    bind_prop(
        element,
        "media-expanded-layout",
        move || state.get().expanded(),
        move |element, expanded| {
            let next_grow = if expanded { 1.0 } else { grow };
            let next_basis = if expanded {
                taffy::style::Dimension::length(0.0)
            } else {
                basis
            };
            let next_wrap = if expanded {
                taffy::style::FlexWrap::NoWrap
            } else {
                wrap
            };
            if element.style.flex_grow == next_grow
                && element.style.flex_basis == next_basis
                && element.style.flex_wrap == next_wrap
                && element.style.min_size.height == taffy::style::Dimension::length(0.0)
            {
                return LayoutStatus::Clean;
            }
            element.style.flex_grow = next_grow;
            element.style.flex_basis = next_basis;
            element.style.flex_wrap = next_wrap;
            element.style.min_size.height = taffy::style::Dimension::length(0.0);
            crate::authoring::widgets::classes::patch_class_set_base_layout(element);
            LayoutStatus::LayoutChanged
        },
    )
}

pub(super) fn default_body(kind: MediaKind, poster: String, slots: MediaPlayerSlots) -> Element {
    let mut main = Vec::new();
    if kind == MediaKind::Video {
        let surface = slots.render(MediaPlayerSlot::Viewport, || media_viewport(poster));
        let overlay = slots
            .render(MediaPlayerSlot::Overlay, empty_content)
            .map(|element| {
                element
                    .absolute()
                    .inset(0.0)
                    .pointer_events(PointerEvents::BoxNone)
            });
        if surface.is_some() || overlay.is_some() {
            main.push(presentation_flex(stamp_slot(
                vstack()
                    .class("relative w-full min-w-0 overflow-hidden rounded-md")
                    .children(surface.into_iter().chain(overlay).collect::<Vec<_>>())
                    .into(),
                "media-stage",
            )));
        }
    } else {
        main.push(audio().controller(use_media_player().controller).into());
    }
    main.extend(slots.render(MediaPlayerSlot::Header, || {
        packed(vec![
            MediaTitle(MediaTitleProps::default()),
            MediaDescription(MediaDescriptionProps::default()),
        ])
    }));
    main.extend(slots.render(MediaPlayerSlot::Timeline, media_seek_slider));
    let mut controls = Vec::new();
    controls.extend(slots.render(MediaPlayerSlot::ControlsStart, || {
        packed(vec![
            MediaPlayButton(MediaPlayButtonProps::default()),
            media_seek_button(-10.0),
            media_seek_button(10.0),
            media_time_display(),
        ])
    }));
    controls.extend(slots.render(MediaPlayerSlot::ControlsEnd, || {
        packed(vec![
            MediaMuteButton(MediaMuteButtonProps::default()),
            media_volume_slider(),
            media_rate_button(1.0),
            media_rate_button(1.5),
            MediaLoopButton(MediaLoopButtonProps::default()),
        ])
    }));
    if !controls.is_empty() {
        main.push(MediaControls(MediaControlsProps {
            children: Some(packed(controls)),
            ..Default::default()
        }));
    }
    if kind == MediaKind::Video {
        main.extend(slots.render(MediaPlayerSlot::Presentation, || {
            packed(vec![
                super::media_wide_button().into(),
                super::media_window_fullscreen_button().into(),
                super::media_fullscreen_button().into(),
                super::media_picture_in_picture_button().into(),
                super::media_mirror_button(false),
                super::media_mirror_button(true),
                super::media_rotate_button().into(),
                super::media_aspect_ratio_button(false),
                super::media_aspect_ratio_button(true),
                super::media_fit_button().into(),
                super::media_reset_picture_button().into(),
            ])
        }));
    }
    main.extend(slots.render(MediaPlayerSlot::Settings, empty_content));
    main.extend(slots.render(MediaPlayerSlot::Footer, || {
        packed(vec![media_status(), media_error_message()])
    }));
    let body: Element = vstack().class("min-w-0 flex-1 gap-3").children(main).into();
    if let Some(sidebar) = slots.render(MediaPlayerSlot::Sidebar, empty_content) {
        presentation_flex(
            hstack()
                .class("w-full min-w-0 items-stretch gap-4 flex-wrap")
                .child(presentation_flex(body.flex_basis("18rem").max_w("100%")))
                .child(sidebar.max_w("100%"))
                .into(),
        )
    } else {
        presentation_flex(body)
    }
}

crate::__component_contract! {
MediaLayerProps [];
/// Decorative layers pass through all pointer events. Interactive layers allow their children
/// to receive input without making the whole video surface an invisible hit target.
#[allow(non_snake_case)]
pub fn MediaLayer(
default [] interactive: bool,
plain [] children: Option<Element>,
) -> Element {
    let _context = use_media_player();
    stamp_slot(
        vstack()
            .class("absolute inset-0 min-w-0")
            .pointer_events(if interactive {
                PointerEvents::BoxNone
            } else {
                PointerEvents::None
            })
            .children(children.map(take_packed).unwrap_or_default())
            .into(),
        "media-layer",
    )
}
}

pub struct MediaLayerBuilder {
    interactive: bool,
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn media_layer() -> MediaLayerBuilder {
    MediaLayerBuilder {
        interactive: false,
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl MediaLayerBuilder {
    crate::impl_slot_chrome_methods!();
    pub fn interactive(mut self, interactive: bool) -> Self {
        self.interactive = interactive;
        self
    }
    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }
}

impl From<MediaLayerBuilder> for Element {
    fn from(builder: MediaLayerBuilder) -> Self {
        builder.chrome.apply(MediaLayer(MediaLayerProps {
            interactive: Some(builder.interactive),
            children: crate::components::base::packed_children(builder.children),
            ..Default::default()
        }))
    }
}

impl IntoElementList for MediaLayerBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

/// Application-confirmed choice, shared by subtitle, quality, translation or episode controls.
/// The owning extension updates it only after confirming the requested choice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaChoiceState {
    pub load_session: u64,
    pub selected: Option<String>,
    pub enabled: bool,
}

impl MediaChoiceState {
    pub fn new(load_session: u64) -> Self {
        Self {
            load_session,
            selected: None,
            enabled: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaChoiceRequest {
    pub load_session: u64,
    pub value: String,
}

crate::__component_contract! {
MediaChoiceProps [];
/// A controlled, keyboard-accessible choice button; it never switches streams or mutates state.
#[allow(non_snake_case)]
pub fn MediaChoice(
default [] value: String,
required [] state: Signal<MediaChoiceState>,
default [] disabled: bool,
plain [] on_select: Option<Rc<dyn Fn(MediaChoiceRequest)>>,
plain [] children: Option<Element>,
) -> Element {
    let context = use_media_player();
    let snapshot = context.controller.snapshot();
    let inherited_disabled =
        context.disabled || disabled || value.trim().is_empty() || on_select.is_none();
    let click_value = value.clone();
    let controller = context.controller;
    let mut element: Element = button("")
        .variant("outline")
        .size("sm")
        .class("data-[state=on]:bg-accent data-[state=on]:text-accent-foreground")
        .children(
            children
                .map(take_packed)
                .unwrap_or_else(|| vec![Element::new().text(value.clone())]),
        )
        .on_click(move || {
            let Some(current) = state.try_peek() else {
                return;
            };
            if !inherited_disabled
                && current.enabled
                && snapshot.is_alive()
                && current.load_session == controller.current().load_session
            {
                if let Some(handler) = &on_select {
                    handler(MediaChoiceRequest {
                        load_session: current.load_session,
                        value: click_value.clone(),
                    });
                }
            }
        })
        .into();
    element = stamp_slot(element, "media-choice");
    element.set_attr("data-value", value.clone());
    bind_prop(
        element,
        "media-choice-state",
        move || (state.get(), snapshot.get().load_session),
        move |element, (current, session)| {
            let active = current.load_session == session;
            let available = !inherited_disabled && current.enabled && active;
            let impact = apply_disabled(element, !available);
            element.set_focusable(available);
            let pressed = active && current.selected.as_deref() == Some(value.as_str());
            element.set_attr("aria-pressed", if pressed { "true" } else { "false" });
            element.set_attr("data-state", if pressed { "on" } else { "off" });
            impact.merge(LayoutStatus::StyleChanged)
        },
    )
}
}

pub struct MediaChoiceBuilder {
    value: String,
    state: Signal<MediaChoiceState>,
    disabled: bool,
    on_select: Option<Rc<dyn Fn(MediaChoiceRequest)>>,
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn media_choice(
    value: impl Into<String>,
    state: Signal<MediaChoiceState>,
) -> MediaChoiceBuilder {
    MediaChoiceBuilder {
        value: value.into(),
        state,
        disabled: false,
        on_select: None,
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl MediaChoiceBuilder {
    crate::impl_slot_chrome_methods!();
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn on_select(mut self, handler: impl Fn(MediaChoiceRequest) + 'static) -> Self {
        self.on_select = Some(Rc::new(handler));
        self
    }
    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }
}

impl From<MediaChoiceBuilder> for Element {
    fn from(builder: MediaChoiceBuilder) -> Self {
        builder.chrome.apply(MediaChoice(MediaChoiceProps {
            value: Some(builder.value),
            state: Some(builder.state),
            disabled: Some(builder.disabled),
            on_select: builder.on_select,
            children: crate::components::base::packed_children(builder.children),
            ..Default::default()
        }))
    }
}

impl IntoElementList for MediaChoiceBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

/// Hover intent for an external storyboard or chapter renderer, never a playback position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MediaTimelinePreview {
    pub load_session: u64,
    pub fraction: f32,
    pub position: Duration,
}

pub(super) fn preview_at(
    snapshot: &MediaSnapshot,
    pointer_x: f32,
    bounds: [f32; 4],
) -> Option<MediaTimelinePreview> {
    let range = snapshot.seek_window()?;
    if !pointer_x.is_finite()
        || !bounds[0].is_finite()
        || !bounds[2].is_finite()
        || bounds[2] <= 0.0
    {
        return None;
    }
    let fraction = ((pointer_x - bounds[0]) / bounds[2]).clamp(0.0, 1.0);
    let offset = Duration::try_from_secs_f64(range.duration().as_secs_f64() * fraction as f64)
        .unwrap_or(range.duration());
    Some(MediaTimelinePreview {
        load_session: snapshot.load_session,
        fraction,
        position: range.start().saturating_add(offset).min(range.end()),
    })
}

pub fn media_seek_slider_with_preview(preview: Signal<Option<MediaTimelinePreview>>) -> Element {
    MediaSeekSlider(MediaSeekSliderProps {
        preview: Some(preview),
        ..Default::default()
    })
}
