use crate::core::reactivity::Signal;
use crate::core::scroll_viewport::{activate_scroll_viewport, scroll_overflow_allows_scrollbar};
use crate::core::scrollbar::{SCROLLBAR_DEFAULT_CROSS_PX, SCROLLBAR_EDGE_PAD};
use crate::core::tree::{Element, IntoCssDim, IntoF32, IntoNodeText};
use crate::primitives::hstack;
use crate::style::{parse_color, parse_text_color};
#[cfg(test)]
use crate::utils::build_icon_text_node;
use crate::utils::build_lucide_icon_text_node_or as inspector_icon;
use crate::utils::text_edit::field::InputType;
use crate::utils::text_edit::{build_text_edit_element, TextEditBuildParams};
use crate::utils::text_field::{SelectionOnBlur, TextareaWrapMode};
use winit::event::ElementState;
use winit::keyboard::{Key, NamedKey};

pub(crate) fn label(id: impl Into<String>) -> Element {
    crate::utils::text_node::apply_label_defaults(Element::new()).template_id(id)
}

pub(crate) struct ToolButton(Element, bool);

pub(crate) fn button(id: impl AsRef<str>) -> ToolButton {
    ToolButton(
        Element::new()
            .template_id(id.as_ref())
            .focus_id(id.as_ref())
            .accesskit_role("button")
            .focusable(true)
            .flex_row()
            .items_center()
            .justify("center")
            .gap(4.0)
            .text_center()
            .h(28.0)
            .px(8.0)
            .rounded(4.0)
            .flex_shrink(0.0)
            .whitespace_nowrap()
            .text_size(12.0)
            .cursor("pointer")
            .bg(parse_color("primary"))
            .text_color(parse_text_color("primary-foreground")),
        false,
    )
}

impl ToolButton {
    pub(crate) fn size(mut self, size: &str) -> Self {
        self.0 = if size == "icon-sm" {
            self.0.w(28.0).h(28.0).p(0.0)
        } else {
            self.0.h(28.0).px(8.0)
        };
        self
    }
    pub(crate) fn variant(mut self, variant: &str) -> Self {
        self.0 = match variant {
            "ghost" => self
                .0
                .bg([0.0; 4])
                .text_color(parse_text_color("foreground"))
                .class("hover:bg-info/20 hover:text-foreground"),
            "secondary" => self
                .0
                .bg(parse_color("info/20"))
                .text_color(parse_text_color("foreground"))
                .border(parse_color("info"), 1.0)
                .class("hover:bg-info/30 hover:text-foreground"),
            "outline" => self
                .0
                .bg(parse_color("background"))
                .text_color(parse_text_color("foreground"))
                .border(parse_color("border"), 1.0)
                .class("hover:bg-accent hover:text-foreground"),
            _ => self
                .0
                .bg(parse_color("primary"))
                .text_color(parse_text_color("primary-foreground"))
                .class("hover:bg-primary/80"),
        };
        self
    }
    pub(crate) fn w(mut self, value: impl IntoCssDim) -> Self {
        self.0 = self.0.w(value);
        self
    }
    pub(crate) fn min_w(mut self, value: impl IntoCssDim) -> Self {
        self.0 = self.0.min_w(value);
        self
    }
    pub(crate) fn flex_shrink(mut self, value: f32) -> Self {
        self.0 = self.0.flex_shrink(value);
        self
    }
    pub(crate) fn accesskit_role(mut self, value: impl Into<String>) -> Self {
        self.0 = self.0.accesskit_role(value);
        self
    }
    pub(crate) fn h(mut self, value: impl IntoCssDim) -> Self {
        self.0 = self.0.h(value);
        self
    }
    pub(crate) fn p(mut self, value: impl IntoCssDim) -> Self {
        self.0 = self.0.p(value);
        self
    }
    pub(crate) fn text_size(mut self, value: impl IntoF32) -> Self {
        self.0 = self.0.text_size(value);
        self
    }
    pub(crate) fn text(mut self, value: impl IntoNodeText) -> Self {
        self.0 = self.0.text(value);
        self
    }
    pub(crate) fn text_align(mut self, value: &str) -> Self {
        self.0 = self.0.text_align(value);
        self
    }
    pub(crate) fn justify(mut self, value: &str) -> Self {
        self.0 = self.0.justify(value);
        self
    }
    pub(crate) fn child(mut self, value: impl Into<Element>) -> Self {
        self.0 = self.0.child(value);
        self
    }
    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.1 = disabled;
        self
    }
    pub(crate) fn focus_ring(mut self, enabled: bool) -> Self {
        self.0 = self.0.focus_ring(enabled);
        self
    }
    pub(crate) fn on_click(mut self, click: impl FnMut() + 'static) -> Self {
        self.0 = self.0.on_click(click);
        self
    }
    pub(crate) fn tooltip(mut self, caption: &str) -> Self {
        use crate::app::portal_overlay::OverlayKind;
        use crate::utils::anchored_overlay::{anchored_overlay, FloatingStrategy, Placement};
        use crate::utils::popup::PopupPlacement;

        let trigger_id = self.0.focus_id.clone().expect("DevTools tooltip trigger");
        let id = format!("{trigger_id}:tooltip");
        let open = crate::core::reactivity::use_signal(&format!("{id}:open"), || false);
        let caption = super::locale::text(caption);
        self.0.set_attr("aria-label", &caption);
        self.0.set_attr("aria-describedby", &id);
        self.0 = self.0.on_hover(move |inside| {
            open.set(inside);
            crate::app::runtime::request_redraw();
        });
        let bubble_id = id.clone();
        let tip = anchored_overlay(id)
            .open(open)
            .retained()
            .kind(OverlayKind::Tooltip)
            .pinned(true)
            .width("auto")
            .placement(Placement::trigger(
                trigger_id,
                PopupPlacement::BottomStart,
                6.0,
                FloatingStrategy::Auto,
            ))
            .lock_page_scroll(false)
            .child(move || {
                Element::new()
                    .template_id(&bubble_id)
                    .w("auto")
                    .flex_col()
                    .class("items-start")
                    .accesskit_role("tooltip")
                    .text_size(12.0)
                    .text_color(parse_text_color("popover-foreground"))
                    .bg(parse_color("popover"))
                    .border(parse_color("border"), 1.0)
                    .rounded(4.0)
                    .px(8.0)
                    .py(5.0)
                    .whitespace_nowrap()
                    .pointer_events("none")
                    .child(
                        label(format!("{bubble_id}:caption"))
                            .text(caption.clone())
                            .text_size(12.0),
                    )
            });
        self.0 = self.0.child(tip.absolute().w(0.0).h(0.0));
        self
    }
}

impl From<ToolButton> for Element {
    fn from(button: ToolButton) -> Self {
        let mut element = button.0;
        if let Some(text) = element.text.take() {
            element = element.child(
                Element::new()
                    .text(text)
                    .whitespace_nowrap()
                    .flex_shrink(0.0),
            );
        }
        if element.class_set.is_none() {
            element = element.class("hover:bg-primary/80");
        }
        crate::primitives::interactive::apply_disabled(&mut element, button.1);
        if button.1 {
            element.on_click = None;
            element = element.opacity(0.5).cursor("not-allowed");
            return element;
        }
        let click = element.on_click.clone();
        element.on_key_down(move |event| {
            if event.state == ElementState::Pressed
                && !event.repeat
                && (matches!(event.logical_key, Key::Named(NamedKey::Enter))
                    || matches!(&event.logical_key, Key::Character(value) if value == " "))
            {
                if let Some(click) = &click {
                    click.borrow_mut()();
                }
                true
            } else {
                false
            }
        })
    }
}

pub(super) fn dismiss_tooltips(root: &Element) {
    if root
        .attrs
        .get("aria-describedby")
        .is_some_and(|id| id.ends_with(":tooltip"))
    {
        if let Some(hover) = &root.on_hover {
            (hover.borrow_mut())(false);
        }
    }
    for child in &root.children {
        dismiss_tooltips(child);
    }
}

pub(crate) struct TextField {
    params: TextEditBuildParams,
    style: Element,
}

fn text_field(id: &str, multiline: bool) -> TextField {
    TextField {
        params: TextEditBuildParams {
            field_id: id.into(),
            placeholder: String::new(),
            disabled: false,
            invalid: false,
            input_type: InputType::Text,
            clearable: false,
            password_toggle: false,
            number_steppers: false,
            step: 1.0,
            text_size_px: 14.0,
            font_weight: 400,
            line_height_px: None,
            leading_scale: None,
            value: None,
            text_align: crate::style::TextAlign::Start,
            single_line_container: Some(
                hstack()
                    .h(36.0)
                    .w("100%")
                    .min_w(0.0)
                    .rounded(4.0)
                    .px(8.0)
                    .py(4.0)
                    .items_center()
                    .overflow("hidden"),
            ),
            multiline,
            rows: 4,
            resizable: false,
            resize_max_h: 2000.0,
            wrap_mode: TextareaWrapMode::Word,
            plain_chrome: false,
            caret_color: None,
            selection_bg: None,
            selection_foreground: None,
            selection_on_blur: SelectionOnBlur::Clear,
            undo: if multiline {
                crate::utils::text_edit_history::TextUndoConfig::textarea_default()
            } else {
                crate::utils::text_edit_history::TextUndoConfig::input_default()
            },
            drop_undo_on_unmount: true,
        },
        style: Element::new(),
    }
}

pub(crate) fn input(id: &str) -> TextField {
    text_field(id, false)
}
pub(crate) fn textarea(id: &str) -> TextField {
    text_field(id, true)
}

impl TextField {
    pub(crate) fn placeholder(mut self, value: impl Into<String>) -> Self {
        self.params.placeholder = value.into();
        self
    }
    pub(crate) fn rows(mut self, value: u32) -> Self {
        self.params.rows = value;
        self
    }
    pub(crate) fn resizable(mut self, value: bool) -> Self {
        self.params.resizable = value;
        self
    }
    pub(crate) fn wrap(mut self, value: &str) -> Self {
        self.params.wrap_mode = TextareaWrapMode::parse(value);
        self
    }
    pub(crate) fn h(mut self, value: impl IntoCssDim) -> Self {
        self.style = self.style.h(value);
        self
    }
    pub(crate) fn w(mut self, value: impl IntoCssDim) -> Self {
        self.style = self.style.w(value);
        self
    }
    pub(crate) fn flex_grow(mut self, value: f32) -> Self {
        self.style = self.style.flex_grow(value);
        self
    }
    pub(crate) fn text_size(mut self, value: impl IntoF32) -> Self {
        self.params.text_size_px = value.into_f32();
        self
    }
}

impl From<TextField> for Element {
    fn from(field: TextField) -> Self {
        let mut element = build_text_edit_element(field.params);
        if field.style.style.flex_grow != 0.0 {
            element.style.flex_grow = field.style.style.flex_grow;
            element.style.size.width = taffy::style::Dimension::auto();
        }
        if !field.style.style.size.width.is_auto() {
            element.style.size.width = field.style.style.size.width;
        }
        if !field.style.style.size.height.is_auto() {
            element.style.size.height = field.style.style.size.height;
        }
        element
    }
}

fn scrollbar_track(target: &str, axis: &str, visibility: &str) -> Element {
    if visibility == "never" {
        return Element::new().w(0.0).h(0.0);
    }
    let cross = if crate::style::theme_preset() == crate::style::ThemePreset::Fluent {
        8.0
    } else {
        SCROLLBAR_DEFAULT_CROSS_PX
    };
    let mut track_bg = parse_color("scrollbar-track");
    let mut thumb_bg = parse_color("scrollbar-thumb");
    let mut track_border = parse_color("scrollbar-track-border");
    let mut thumb_border = parse_color("scrollbar-thumb-border");
    let track_border_width = if track_border[3] > 0.01 { 1.0 } else { 0.0 };
    let thumb_border_width = if thumb_border[3] > 0.01 { 1.0 } else { 0.0 };
    let track_fades = track_bg[3] > 0.01 || track_border[3] > 0.01;
    crate::core::scrollbar::apply_scrollbar_visible_alpha(
        if visibility == "always" { 1.0 } else { 0.0 },
        track_fades,
        thumb_border[3] > 0.01,
        &mut track_bg,
        &mut thumb_bg,
        &mut track_border,
        &mut thumb_border,
    );
    let mut track = Element::new()
        .flex_grow(0.0)
        .flex_shrink(0.0)
        .rounded(cross * 0.5)
        .bg(track_bg)
        .border(track_border, track_border_width)
        .absolute()
        .z_index(1000);
    let mut thumb = Element::new()
        .rounded((cross - 2.0).max(1.0) * 0.5)
        .bg(thumb_bg)
        .border(thumb_border, thumb_border_width)
        .cursor_kind(crate::core::tree::CursorKind::Pointer);
    crate::core::scrollbar::configure_scrollbar_parts(
        &mut track,
        &mut thumb,
        Some(target),
        axis,
        visibility,
        cross,
    );
    track.set_attr(
        crate::core::scrollbar::ATTR_SCROLLBAR_TRACK_FADES,
        if track_fades { "1" } else { "0" },
    );
    track.child(thumb)
}

pub(crate) struct ScrollArea {
    id: String,
    element: Element,
    overflow_x: String,
    overflow_y: String,
    visibility: String,
    border_top: Option<([f32; 4], f32)>,
}

pub(crate) fn scroll_area(id: &str) -> ScrollArea {
    ScrollArea {
        id: id.into(),
        element: Element::new()
            .w("100%")
            .h("100%")
            .min_w(0.0)
            .min_h(0.0)
            .flex_col(),
        overflow_x: "auto".into(),
        overflow_y: "auto".into(),
        visibility: "auto".into(),
        border_top: None,
    }
}

impl ScrollArea {
    pub(crate) fn w(mut self, value: impl IntoCssDim) -> Self {
        self.element = self.element.w(value);
        self
    }
    pub(crate) fn h(mut self, value: impl IntoCssDim) -> Self {
        self.element = self.element.h(value);
        self
    }
    pub(crate) fn min_h(mut self, value: impl IntoCssDim) -> Self {
        self.element = self.element.min_h(value);
        self
    }
    pub(crate) fn flex_grow(mut self, value: f32) -> Self {
        self.element = self.element.flex_grow(value);
        self
    }
    pub(crate) fn flex_shrink(mut self, value: f32) -> Self {
        self.element = self.element.flex_shrink(value);
        self
    }
    pub(crate) fn overflow_x(mut self, value: &str) -> Self {
        self.overflow_x = value.into();
        self
    }
    pub(crate) fn overflow_y(mut self, value: &str) -> Self {
        self.overflow_y = value.into();
        self
    }
    pub(crate) fn scrollbar_visibility(mut self, value: &str) -> Self {
        self.visibility = value.into();
        self
    }
    pub(crate) fn border_top(mut self, color: [f32; 4], width: f32) -> Self {
        self.border_top = Some((color, width));
        self
    }
    pub(crate) fn child(mut self, value: impl Into<Element>) -> Self {
        self.element = self.element.child(value);
        self
    }
}

impl From<ScrollArea> for Element {
    fn from(area: ScrollArea) -> Self {
        let mut outer = area.element.relative().template_id(&area.id);
        if let Some((color, width)) = area.border_top {
            outer = outer.border_top(color, width);
        }
        let body = std::mem::take(&mut outer.children);
        let viewport = activate_scroll_viewport(
            Element::new()
                .w("100%")
                .min_w(0.0)
                .min_h(0.0)
                .flex_grow(1.0)
                .flex_shrink(1.0)
                .flex_col()
                .children(body),
            &area.id,
            &area.overflow_x,
            &area.overflow_y,
            crate::core::scroll_viewport::DEFAULT_SCROLL_BEHAVIOR,
            "scrollbar",
        );
        let show_v = scroll_overflow_allows_scrollbar(&area.overflow_y);
        let show_h = scroll_overflow_allows_scrollbar(&area.overflow_x);
        outer = outer.child(viewport);
        if show_v {
            let mut track = scrollbar_track(&area.id, "y", &area.visibility)
                .right(SCROLLBAR_EDGE_PAD)
                .top(SCROLLBAR_EDGE_PAD)
                .bottom(SCROLLBAR_EDGE_PAD);
            track.style.size.height = taffy::style::Dimension::auto();
            track.style.min_size.height = taffy::style::Dimension::auto();
            outer = outer.child(track);
        }
        if show_h {
            let mut track = scrollbar_track(&area.id, "x", &area.visibility)
                .left(SCROLLBAR_EDGE_PAD)
                .right(SCROLLBAR_EDGE_PAD)
                .bottom(SCROLLBAR_EDGE_PAD);
            track.style.size.width = taffy::style::Dimension::auto();
            track.style.min_size.width = taffy::style::Dimension::auto();
            track.set_attr(
                crate::core::scrollbar::ATTR_H_LANE_INSET,
                SCROLLBAR_EDGE_PAD.to_string(),
            );
            track.set_attr(
                crate::core::scrollbar::ATTR_H_LANE_V_CROSS,
                crate::layout::dim_length_px(track.style.max_size.height)
                    .unwrap_or(SCROLLBAR_DEFAULT_CROSS_PX)
                    .to_string(),
            );
            outer = outer.child(track);
        }
        outer
    }
}

pub(crate) struct Select {
    id: String,
    value: Option<Signal<String>>,
    width: String,
    options: Vec<(String, String)>,
}

pub(crate) fn select(id: &str) -> Select {
    Select {
        id: id.into(),
        value: None,
        width: "140px".into(),
        options: Vec::new(),
    }
}

impl Select {
    pub(crate) fn value_signal(mut self, value: Signal<String>) -> Self {
        self.value = Some(value);
        self
    }
    pub(crate) fn trigger_width(mut self, width: &str) -> Self {
        self.width = width.into();
        self
    }
    pub(crate) fn options(mut self, options: Vec<(String, String)>) -> Self {
        self.options = options;
        self
    }
}

impl From<Select> for Element {
    fn from(select: Select) -> Self {
        use crate::app::portal_overlay::OverlayKind;
        use crate::utils::anchored_overlay::{
            anchored_overlay, FloatingMotionKind, FloatingStrategy, Placement,
        };
        use crate::utils::popup::PopupPlacement;

        let value = select.value.expect("DevTools select value");
        let open =
            crate::core::reactivity::use_signal(format!("{}:open", select.id).as_str(), || false);
        let selected = value.peek();
        let selected_label = select
            .options
            .iter()
            .find(|(key, _)| *key == selected)
            .map(|(_, label)| super::locale::text(label))
            .unwrap_or_else(|| selected.clone());
        let trigger_id = format!("{}:trigger", select.id);
        let menu_id = format!("{}:menu", select.id);
        let width = select.width;
        let menu_options = select.options;
        let menu_id_for_body = menu_id.clone();
        let value_for_menu = value;
        let menu = anchored_overlay(select.id.clone())
            .open(open)
            .retained()
            .placement(Placement::trigger(
                trigger_id.clone(),
                PopupPlacement::BottomStart,
                4.0,
                FloatingStrategy::Auto,
            ))
            .kind(OverlayKind::Menu)
            .motion(FloatingMotionKind::Expand)
            .match_trigger_width(true)
            .lock_page_scroll(false)
            .max_h("280px")
            .child(move || {
                let selected = value_for_menu.peek();
                let mut rows = Vec::with_capacity(menu_options.len());
                for (index, (key, caption)) in menu_options.iter().enumerate() {
                    let key = key.clone();
                    let value = value_for_menu;
                    let open = open;
                    let row: Element = button(format!("{}:option:{}", menu_id_for_body, index))
                        .variant(if selected == key {
                            "secondary"
                        } else {
                            "ghost"
                        })
                        .focus_ring(false)
                        .justify("start")
                        .text(super::locale::text(caption))
                        .text_align("left")
                        .p(8.0)
                        .on_click(move || {
                            value.set(key.clone());
                            open.set(false);
                            crate::app::runtime::request_redraw();
                        })
                        .w("100%")
                        .accesskit_role("option")
                        .into();
                    rows.push(row);
                }
                let options: Element = crate::primitives::vstack()
                    .w("100%")
                    .gap(2.0)
                    .children(rows)
                    .into();
                let viewport: Element = scroll_area(&format!("{menu_id_for_body}:scroll"))
                    .overflow_x("hidden")
                    .overflow_y("auto")
                    .scrollbar_visibility("auto")
                    .w("100%")
                    .h("100%")
                    .min_h(0.0)
                    .flex_grow(1.0)
                    .child(options)
                    .into();
                Element::from(
                    crate::primitives::vstack()
                        .w("100%")
                        .h("auto")
                        .min_h(0.0)
                        .overflow_x("hidden")
                        .overflow_y("hidden")
                        .p(4.0)
                        .child(viewport),
                )
                .max_h(280.0)
                .bg(parse_color("background"))
                .border(parse_color("border"), 1.0)
                .rounded(4.0)
                .accesskit_role("listbox")
            });
        let caption = label(format!("{}:value", select.id))
            .text(selected_label)
            .text_size(12.0)
            .min_w(0.0)
            .flex_grow(1.0)
            .text_align("center")
            .whitespace_nowrap()
            .overflow("hidden")
            .text_overflow_ellipsis();
        let arrow = inspector_icon("chevron-down", "⌄")
            .text_size(12.0)
            .flex_shrink(0.0)
            .pointer_events("none");
        let trigger: Element = button(&trigger_id)
            .variant("outline")
            .focus_ring(false)
            .child(caption)
            .child(
                label(format!("{}:arrow", select.id))
                    .child(arrow)
                    .pointer_events("none"),
            )
            .on_click(move || {
                open.set(!open.peek());
                crate::app::runtime::request_redraw();
            })
            .w(width)
            .min_w(0.0)
            .flex_shrink(1.0)
            .accesskit_role("combobox")
            .into();
        Element::new()
            .relative()
            .min_w(0.0)
            .flex_shrink(1.0)
            .child(trigger)
            .child(menu)
    }
}

#[cfg(test)]
#[path = "tests/controls.rs"]
mod tests;
