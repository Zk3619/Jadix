use super::*;
use jadix_core::presentation::input::{scalar_to_byte, scalar_to_utf16, utf16_to_scalar};
use jadix_core::presentation::style::ImageFit;

pub(super) fn create(
    id: NodeId,
    descriptor: &NodeDescriptor,
    events: Rc<Events>,
    window: &gtk::Window,
    web: Rc<web::Runtime>,
    components: &components::Components,
) -> Result<Item, String> {
    let control = Rc::new(Control {
        id,
        descriptor: RefCell::new(descriptor.clone()),
        revision: Cell::new(revision(&descriptor.content)),
        alive: Cell::new(true),
        intrinsic: Cell::new(None),
    });
    let mut children = None;
    let mut panel = None;
    let mut popover = None;
    let mut fallback_label = None;
    let mut hosted_state = None;
    let widget: gtk::Widget = match &descriptor.content {
        NodeContent::Hosted { specification } => {
            let (widget, state) = hosted::create(
                &jadix_core::hosted::HostedContent::from_json(specification)
                    .map_err(str::to_owned)?,
                events.clone(),
                control.clone(),
                web,
                components,
            )?;
            if let Some(hosted::State::Shader(shader)) = &state {
                children = Some(shader.children.clone());
            }
            hosted_state = state;
            widget
        }
        NodeContent::Container => {
            let fixed = gtk::Fixed::new();
            children = Some(fixed.clone());
            fixed.upcast()
        }
        NodeContent::Text(_) => gtk::Label::new(None).upcast(),
        NodeContent::Button { .. } => {
            let button = gtk::Button::new();
            let fixed = gtk::Fixed::new();
            let label = gtk::Label::new(None);
            fixed.put(&label, 0., 0.);
            fallback_label = Some(label);
            button.set_child(Some(&fixed));
            children = Some(fixed);
            let (events, control) = (events.clone(), control.clone());
            button.connect_clicked(move |_| events.push(&control, EventPayload::Activate));
            button.upcast()
        }
        NodeContent::Input { .. } => {
            let entry = gtk::Entry::new();
            let (queue, state) = (events.clone(), control.clone());
            entry.connect_changed(move |entry| {
                if queue.publishing.get() {
                    return;
                }
                let value = entry.text().to_string();
                let (a, b) = entry
                    .selection_bounds()
                    .unwrap_or((entry.position(), entry.position()));
                edit(&queue, &state, value, [a.min(b), a.max(b)]);
            });
            let (queue, state) = (events.clone(), control.clone());
            entry.connect_activate(move |_| {
                if !queue.publishing.get() {
                    if let Some(revision) = state.next_revision() {
                        queue.push(&state, EventPayload::Submit { revision });
                    }
                }
            });
            let (queue, state) = (events.clone(), control.clone());
            entry.connect_insert_text(move |entry, inserted, position| {
                if queue.publishing.get() {
                    return;
                }
                let text = entry.text();
                let offset = scalar_to_byte(&text, (*position).max(0) as usize);
                let allowed = jadix_core::presentation::input::limited_insertion(
                    &text,
                    offset..offset,
                    inserted,
                    state.descriptor.borrow().content.input_max_length(),
                )
                .unwrap_or("")
                .to_owned();
                if allowed != inserted {
                    entry.stop_signal_emission_by_name("insert-text");
                    if !allowed.is_empty() {
                        entry.insert_text(&allowed, position);
                    }
                }
            });
            entry.upcast()
        }
        NodeContent::Textarea { .. } => {
            let view = gtk::TextView::new();
            view.set_wrap_mode(gtk::WrapMode::WordChar);
            let buffer = view.buffer();
            buffer.set_enable_undo(true);
            let (queue, state) = (events.clone(), control.clone());
            buffer.connect_changed(move |buffer| {
                if queue.publishing.get() {
                    return;
                }
                let value = buffer
                    .text(&buffer.start_iter(), &buffer.end_iter(), true)
                    .to_string();
                let (a, b) = buffer
                    .selection_bounds()
                    .map(|(a, b)| (a.offset(), b.offset()))
                    .unwrap_or((buffer.cursor_position(), buffer.cursor_position()));
                edit(&queue, &state, value, [a.min(b), a.max(b)]);
            });
            let (queue, state) = (events.clone(), control.clone());
            buffer.connect_insert_text(move |buffer, position, inserted| {
                if queue.publishing.get() {
                    return;
                }
                let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
                let offset = scalar_to_byte(&text, position.offset().max(0) as usize);
                let allowed = jadix_core::presentation::input::limited_insertion(
                    &text,
                    offset..offset,
                    inserted,
                    state.descriptor.borrow().content.input_max_length(),
                )
                .unwrap_or("")
                .to_owned();
                if allowed != inserted {
                    buffer.stop_signal_emission_by_name("insert-text");
                    if !allowed.is_empty() {
                        buffer.insert(position, &allowed);
                    }
                }
            });
            view.upcast()
        }
        NodeContent::Checkbox { .. } => {
            let check = gtk::CheckButton::new();
            let (queue, state) = (events.clone(), control.clone());
            check.connect_toggled(move |check| toggle(&queue, &state, check.is_active()));
            check.upcast()
        }
        NodeContent::Switch { .. } => {
            let switch = gtk::Switch::new();
            let (queue, state) = (events.clone(), control.clone());
            switch.connect_active_notify(move |switch| toggle(&queue, &state, switch.is_active()));
            switch.upcast()
        }
        NodeContent::Select { .. } => {
            let select = gtk::DropDown::from_strings(&[]);
            let (queue, state) = (events.clone(), control.clone());
            select.connect_selected_notify(move |select| {
                if queue.publishing.get() {
                    return;
                }
                let descriptor = state.descriptor.borrow();
                if let NodeContent::Select { options, value, .. } = &descriptor.content {
                    if let Some(option) = options.get(select.selected() as usize) {
                        if !option.enabled {
                            queue.publishing.set(true);
                            select.set_selected(
                                options
                                    .iter()
                                    .position(|o| &o.value == value)
                                    .map_or(gtk::INVALID_LIST_POSITION, |i| i as u32),
                            );
                            queue.publishing.set(false);
                        } else if let Some(revision) = state.next_revision() {
                            queue.push(
                                &state,
                                EventPayload::Select {
                                    value: option.value.clone(),
                                    revision,
                                },
                            );
                        }
                    }
                }
            });
            select.upcast()
        }
        NodeContent::Scroll => {
            let fixed = gtk::Fixed::new();
            let scroll = gtk::ScrolledWindow::builder().child(&fixed).build();
            children = Some(fixed);
            for adjustment in [scroll.hadjustment(), scroll.vadjustment()] {
                let (queue, state, weak) = (events.clone(), control.clone(), scroll.downgrade());
                adjustment.connect_value_changed(move |_| {
                    if let Some(scroll) = weak.upgrade() {
                        scroll_event(&queue, &state, &scroll);
                    }
                });
            }
            for adjustment in [scroll.hadjustment(), scroll.vadjustment()] {
                let (queue, state, weak) = (events.clone(), control.clone(), scroll.downgrade());
                adjustment.connect_changed(move |_| {
                    if let Some(scroll) = weak.upgrade() {
                        scroll_event(&queue, &state, &scroll);
                    }
                });
            }
            scroll.upcast()
        }
        NodeContent::Dialog { .. } => {
            let fixed = gtk::Fixed::new();
            children = Some(fixed.clone());
            let dialog = gtk::Window::builder()
                .transient_for(window)
                .modal(true)
                .child(&fixed)
                .build();
            let (queue, state) = (events.clone(), control.clone());
            dialog.connect_close_request(move |_| {
                if !queue.publishing.get() {
                    if let Some(revision) = state.next_revision() {
                        queue.push(
                            &state,
                            EventPayload::Dismiss {
                                backdrop: false,
                                revision,
                            },
                        );
                    }
                }
                glib::Propagation::Stop
            });
            panel = Some(dialog);
            fixed.upcast()
        }
        NodeContent::Popover { .. } => {
            let fixed = gtk::Fixed::new();
            children = Some(fixed.clone());
            let surface = gtk::Popover::builder()
                .child(&fixed)
                .has_arrow(false)
                .build();
            let (queue, state) = (events.clone(), control.clone());
            surface.connect_closed(move |_| {
                if matches!(
                    state.descriptor.borrow().content,
                    NodeContent::Popover { open: true, .. }
                ) {
                    queue.push(&state, EventPayload::PopoverCancel { backdrop: true });
                }
            });
            popover = Some(surface);
            fixed.upcast()
        }
        NodeContent::Image { .. } => gtk::Picture::new().upcast(),
    };
    let focus = gtk::EventControllerFocus::new();
    let (queue, state) = (events.clone(), control.clone());
    focus.connect_enter(move |_| {
        if state.descriptor.borrow().can_focus() {
            queue.push(&state, EventPayload::Focus);
        }
    });
    let (queue, state) = (events.clone(), control.clone());
    focus.connect_leave(move |_| {
        if state.descriptor.borrow().can_focus() {
            queue.push(&state, EventPayload::Blur);
        }
    });
    widget.add_controller(focus);
    keyboard::bind(&widget, events.clone(), control.clone());
    interaction::bind(&widget, events.clone(), control.clone());
    let css = gtk::CssProvider::new();
    widget
        .style_context()
        .add_provider(&css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    if let Some(panel) = &panel {
        panel.set_child(gtk::Widget::NONE);
    }
    if let Some(popover) = &popover {
        popover.set_child(gtk::Widget::NONE);
    }
    let surface = visual::Surface::new(&widget, &css);
    if let Some(panel) = &panel {
        panel.set_child(Some(&surface));
    }
    if let Some(popover) = &popover {
        popover.set_child(Some(&surface));
    }
    Ok(Item {
        surface,
        widget,
        children,
        panel,
        popover,
        fallback_label,
        control,
        css,
        parent: None,
        image_source: RefCell::new(None),
        hosted: hosted_state,
    })
}

fn edit(queue: &Events, state: &Control, value: String, selection: [i32; 2]) {
    if let Some(revision) = state.next_revision() {
        let selection_utf16 = selection.map(|p| scalar_to_utf16(&value, p.max(0) as usize));
        queue.push(
            state,
            EventPayload::Edit {
                value,
                revision,
                selection_utf16,
                composing_utf16: None,
            },
        );
    }
}
fn toggle(queue: &Events, state: &Control, checked: bool) {
    if !queue.publishing.get() {
        if let Some(revision) = state.next_revision() {
            queue.push(state, EventPayload::Toggle { checked, revision });
        }
    }
}
fn scroll_event(queue: &Events, state: &Control, scroll: &gtk::ScrolledWindow) {
    queue.push(
        state,
        EventPayload::ScrollViewport {
            x: scroll.hadjustment().value(),
            y: scroll.vadjustment().value(),
            width: scroll.hadjustment().page_size(),
            height: scroll.vadjustment().page_size(),
        },
    );
}

pub(super) fn update(item: &Item, descriptor: &NodeDescriptor, has_children: bool) {
    let widget = &item.widget;
    widget.set_focusable(descriptor.can_focus());
    match &descriptor.content {
        NodeContent::Text(text) => {
            let label = widget.downcast_ref::<gtk::Label>().unwrap();
            label.set_text(
                &descriptor
                    .style
                    .text
                    .as_ref()
                    .map_or_else(|| text.clone(), |s| s.display_text(text)),
            );
        }
        NodeContent::Button { label, enabled } => {
            let button = widget.downcast_ref::<gtk::Button>().unwrap();
            button.set_sensitive(*enabled);
            // Child-bearing buttons use their authored children as the label.
            if let Some(text) = &item.fallback_label {
                text.set_text(label);
                text.set_visible(!has_children);
            }
        }
        NodeContent::Input {
            value,
            placeholder,
            enabled,
            password,
            ..
        } => {
            let entry = widget.downcast_ref::<gtk::Entry>().unwrap();
            entry.set_sensitive(*enabled);
            entry.set_visibility(!password);
            entry.set_placeholder_text(Some(placeholder));
            if entry.text().as_str() != value {
                let before = entry.text();
                let (a, b) = entry
                    .selection_bounds()
                    .unwrap_or((entry.position(), entry.position()));
                let selection = jadix_core::text_selection::remap_utf16(
                    &before,
                    value,
                    [
                        scalar_to_utf16(&before, a.min(b).max(0) as usize),
                        scalar_to_utf16(&before, a.max(b).max(0) as usize),
                    ],
                );
                entry.set_text(value);
                entry.select_region(
                    utf16_to_scalar(value, selection[0]) as i32,
                    utf16_to_scalar(value, selection[1]) as i32,
                );
            }
        }
        NodeContent::Textarea { value, enabled, .. } => {
            let view = widget.downcast_ref::<gtk::TextView>().unwrap();
            view.set_editable(*enabled);
            view.set_sensitive(*enabled);
            let buffer = view.buffer();
            let before = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
            if before.as_str() != value {
                let (a, b) = buffer
                    .selection_bounds()
                    .map(|(a, b)| (a.offset(), b.offset()))
                    .unwrap_or((buffer.cursor_position(), buffer.cursor_position()));
                let selection = jadix_core::text_selection::remap_utf16(
                    &before,
                    value,
                    [
                        scalar_to_utf16(&before, a.min(b).max(0) as usize),
                        scalar_to_utf16(&before, a.max(b).max(0) as usize),
                    ],
                );
                buffer.set_text(value);
                buffer.select_range(
                    &buffer.iter_at_offset(utf16_to_scalar(value, selection[0]) as i32),
                    &buffer.iter_at_offset(utf16_to_scalar(value, selection[1]) as i32),
                );
            }
        }
        NodeContent::Checkbox {
            label,
            checked,
            enabled,
            ..
        } => {
            let check = widget.downcast_ref::<gtk::CheckButton>().unwrap();
            check.set_label(Some(label));
            check.set_active(*checked);
            check.set_sensitive(*enabled);
        }
        NodeContent::Switch {
            checked, enabled, ..
        } => {
            let switch = widget.downcast_ref::<gtk::Switch>().unwrap();
            switch.set_active(*checked);
            switch.set_sensitive(*enabled);
        }
        NodeContent::Select {
            options,
            value,
            enabled,
            ..
        } => {
            let select = widget.downcast_ref::<gtk::DropDown>().unwrap();
            let old = item.control.descriptor.borrow();
            if !matches!(&old.content, NodeContent::Select { options: previous, .. } if previous == options)
                || select
                    .model()
                    .is_none_or(|m| m.n_items() != options.len() as u32)
            {
                select.set_model(Some(&gtk::StringList::new(
                    &options.iter().map(|o| o.label.as_str()).collect::<Vec<_>>(),
                )));
            }
            select.set_selected(
                options
                    .iter()
                    .position(|o| &o.value == value)
                    .map_or(gtk::INVALID_LIST_POSITION, |i| i as u32),
            );
            select.set_sensitive(*enabled);
        }
        NodeContent::Scroll => {
            let scroll = widget.downcast_ref::<gtk::ScrolledWindow>().unwrap();
            scroll.set_policy(
                if descriptor.style.scrollbars[0] {
                    gtk::PolicyType::Automatic
                } else {
                    gtk::PolicyType::External
                },
                if descriptor.style.scrollbars[1] {
                    gtk::PolicyType::Automatic
                } else {
                    gtk::PolicyType::External
                },
            );
            scroll.set_kinetic_scrolling(descriptor.style.scroll_enabled != Some(false));
            if descriptor.style.scroll_to != item.control.descriptor.borrow().style.scroll_to {
                if let Some([x, y]) = descriptor.style.scroll_to {
                    scroll.hadjustment().set_value(x);
                    scroll.vadjustment().set_value(y);
                }
            }
        }
        NodeContent::Popover { placement, gap, .. } => {
            if let Some(popover) = &item.popover {
                use PopoverPlacement::*;
                popover.set_position(match placement {
                    Top | TopStart | TopEnd => gtk::PositionType::Top,
                    Right | RightStart | RightEnd => gtk::PositionType::Right,
                    Left | LeftStart | LeftEnd => gtk::PositionType::Left,
                    _ => gtk::PositionType::Bottom,
                });
                popover.set_offset(0, i32::from(*gap));
            }
        }
        NodeContent::Image { alt, .. } => {
            let picture = widget.downcast_ref::<gtk::Picture>().unwrap();
            picture.set_alternative_text(Some(alt));
            picture.set_content_fit(match descriptor.style.image_fit.unwrap_or_default() {
                ImageFit::Cover => gtk::ContentFit::Cover,
                ImageFit::Fill => gtk::ContentFit::Fill,
                ImageFit::ScaleDown => gtk::ContentFit::ScaleDown,
                _ => gtk::ContentFit::Contain,
            });
        }
        _ => (),
    }
}

pub(super) fn texture(source: &str) -> Result<gdk::Texture, String> {
    use base64::Engine;
    if source.len() > 1024 * 1024 {
        return Err("image source exceeds limit".into());
    }
    let (_, encoded) = source
        .split_once(";base64,")
        .ok_or("image must be a base64 data URL")?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| e.to_string())?;
    if source.starts_with("data:image/svg+xml;") {
        let (pixels, _) =
            crate::images::svg::rasterize(&bytes, 1, 1, 3).ok_or("invalid or oversized SVG")?;
        return Ok(gdk::MemoryTexture::new(
            pixels.width() as i32,
            pixels.height() as i32,
            gdk::MemoryFormat::R8g8b8a8Premultiplied,
            &glib::Bytes::from_owned(pixels.data().to_vec()),
            pixels.width() as usize * 4,
        )
        .upcast());
    }
    let size = imagesize::blob_size(&bytes).map_err(|e| e.to_string())?;
    if size.width == 0
        || size.height == 0
        || size.width > 4096
        || size.height > 4096
        || size.width.saturating_mul(size.height) > 4 * 1024 * 1024
    {
        return Err("decoded image exceeds dimension limit".into());
    }
    gdk::Texture::from_bytes(&glib::Bytes::from_owned(bytes)).map_err(|e| e.to_string())
}
pub(super) fn image_result(
    events: &Events,
    control: &Control,
    result: Result<gdk::Texture, String>,
) {
    events.enqueue(
        control,
        EventPayload::ImageLoad(match result {
            Ok(texture) => ImageLoadEvent::Loaded {
                width: texture.width() as u32,
                height: texture.height() as u32,
            },
            Err(message) => ImageLoadEvent::Failed { message },
        }),
    );
    events.enqueue(control, EventPayload::ImageLoad(ImageLoadEvent::Ended));
}
