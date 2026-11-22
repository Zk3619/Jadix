//! GTK-owned embedded leaves. Shared HostedContent remains the event authority.
use super::*;
use jadix_core::hosted::{CanvasCommand, CivilDate, HostedContent as Content, TimeOfDay};

pub(super) enum State {
    Terminal(Rc<terminal::Terminal>),
    Web(Rc<web::Web>),
    Media(Rc<media::Media>),
    RichEditor(Rc<rich_editor::Editor>),
    Shader(Rc<shader::Shader>),
    Custom(RefCell<Box<dyn NativeComponent>>),
}

pub(super) fn specification(control: &Control) -> Option<Content> {
    let descriptor = control.descriptor.borrow();
    match &descriptor.content {
        NodeContent::Hosted { specification } => Content::from_json(specification).ok(),
        _ => None,
    }
}
pub(super) fn emit(events: &Events, control: &Control, event: &str, data: serde_json::Value) {
    if specification(control).is_some_and(|spec| spec.accepts_event(event, &data)) {
        let payload = EventPayload::Hosted {
            event: event.into(),
            data,
        };
        if event == "error" {
            events.enqueue(control, payload);
        } else {
            events.push(control, payload);
        }
    }
}
pub(super) fn create(
    spec: &Content,
    events: Rc<Events>,
    control: Rc<Control>,
    runtime: Rc<web::Runtime>,
    components: &components::Components,
) -> Result<(gtk::Widget, Option<State>), String> {
    if matches!(spec, Content::Shader { .. }) {
        let shader = shader::Shader::new(spec, events, control, runtime)?;
        return Ok((shader.widget.clone().upcast(), Some(State::Shader(shader))));
    }
    if let Content::Custom { kind, properties } = spec {
        let mut component = components.create(kind, &events, &control)?;
        component.update(properties)?;
        let widget = component.widget();
        if widget.parent().is_some() {
            return Err("native component widget already has a parent".into());
        }
        return Ok((widget, Some(State::Custom(RefCell::new(component)))));
    }
    if matches!(spec, Content::RichEditor { .. }) {
        let editor = rich_editor::Editor::new(events, control, runtime);
        return Ok((
            editor.widget.clone().upcast(),
            Some(State::RichEditor(editor)),
        ));
    }
    if matches!(spec, Content::Media { .. }) {
        let media = media::Media::new(events, control, runtime)?;
        return Ok((media.widget.clone().upcast(), Some(State::Media(media))));
    }
    if matches!(spec, Content::Web { .. }) {
        let web = web::Web::new(spec, runtime, events, control)?;
        return Ok((web.view.clone().upcast(), Some(State::Web(web))));
    }
    if let Content::Terminal {
        columns,
        rows,
        scrollback,
        ..
    } = spec
    {
        let terminal = terminal::Terminal::new(*columns, *rows, *scrollback, events, control)?;
        return Ok((terminal.widget().upcast(), Some(State::Terminal(terminal))));
    }
    create_control(spec, events, control).map(|widget| (widget, None))
}
fn create_control(
    spec: &Content,
    events: Rc<Events>,
    control: Rc<Control>,
) -> Result<gtk::Widget, String> {
    Ok(match spec {
        Content::Canvas { .. } => {
            let area = gtk::DrawingArea::new();
            area.set_draw_func(move |area, cr, _, _| {
                let Some(Content::Canvas { commands }) = specification(&control) else {
                    return;
                };
                let draw = || -> Result<(), gtk::cairo::Error> {
                    for command in commands {
                        let color = match &command {
                            CanvasCommand::Fill { color, .. }
                            | CanvasCommand::Stroke { color, .. }
                            | CanvasCommand::Text { color, .. } => *color,
                        };
                        cr.set_source_rgba(
                            color[0] as f64 / 255.,
                            color[1] as f64 / 255.,
                            color[2] as f64 / 255.,
                            color[3] as f64 / 255.,
                        );
                        match command {
                            CanvasCommand::Fill {
                                rect: [x, y, w, h], ..
                            } => {
                                cr.rectangle(x, y, w, h);
                                cr.fill()?;
                            }
                            CanvasCommand::Stroke {
                                points,
                                width,
                                closed,
                                ..
                            } => {
                                cr.new_path();
                                cr.set_line_width(width);
                                if let Some(first) = points.first() {
                                    cr.move_to(first[0], first[1]);
                                }
                                for [x, y] in points.iter().skip(1) {
                                    cr.line_to(*x, *y);
                                }
                                if closed {
                                    cr.close_path();
                                }
                                cr.stroke()?;
                            }
                            CanvasCommand::Text {
                                text,
                                position: [x, y],
                                size,
                                ..
                            } => {
                                let layout = area.create_pango_layout(None);
                                let mut font =
                                    gtk::pango::FontDescription::from_string("sans-serif");
                                font.set_absolute_size(size * gtk::pango::SCALE as f64);
                                layout.set_font_description(Some(&font));
                                layout.set_text(&text);
                                cr.move_to(x, y);
                                pangocairo::functions::show_layout(cr, &layout);
                            }
                        }
                    }
                    Ok(())
                };
                if let Err(error) = draw() {
                    emit(&events, &control, "error", error.to_string().into());
                }
            });
            area.upcast()
        }
        Content::MenuButton { .. } => gtk::MenuButton::new().upcast(),
        Content::Segments { .. } | Content::TabBar { .. } => {
            gtk::Box::new(gtk::Orientation::Horizontal, 0).upcast()
        }
        Content::Date { .. } => {
            let calendar = gtk::Calendar::new();
            calendar.connect_day_selected(move |calendar| {
                let date = calendar.date();
                emit(&events,&control,"change",serde_json::json!({"year":date.year(),"month":date.month(),"day":date.day_of_month()}));
            });
            calendar.upcast()
        }
        Content::Time { .. } => {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
            let fields: Vec<_> = [23., 59., 59.]
                .map(|max| gtk::SpinButton::with_range(0., max, 1.))
                .into();
            for field in &fields {
                field.set_wrap(true);
                field.set_numeric(true);
                field.set_width_chars(2);
                row.append(field);
            }
            for field in &fields {
                let weak: Vec<_> = fields.iter().map(ObjectExt::downgrade).collect();
                let (events, control) = (events.clone(), control.clone());
                field.connect_value_changed(move |_| {
                    let values: Vec<_> = weak.iter().filter_map(|w| w.upgrade()).map(|v|v.value_as_int()).collect();
                    if values.len()==3 { emit(&events,&control,"change",serde_json::json!({"hour":values[0],"minute":values[1],"second":values[2]})); }
                });
            }
            row.upcast()
        }
        Content::Color { .. } => {
            let button = gtk::ColorDialogButton::new(Some(gtk::ColorDialog::new()));
            button.connect_rgba_notify(move |button| {
                let color = button.rgba();
                let bytes = [color.red(), color.green(), color.blue(), color.alpha()]
                    .map(|v| (v.clamp(0., 1.) * 255.).round() as u8);
                emit(&events, &control, "change", serde_json::json!(bytes));
            });
            button.upcast()
        }
        _ => return Err("this hosted surface needs a registered Linux implementation".into()),
    })
}

pub(super) fn update(item: &Item, spec: &Content, events: Rc<Events>) {
    match spec {
        Content::Shader { .. } => {
            if let Some(State::Shader(shader)) = &item.hosted {
                shader.update(spec);
            }
        }
        Content::Custom { properties, .. } => {
            if let Some(State::Custom(component)) = &item.hosted {
                if specification(&item.control).as_ref() != Some(spec) {
                    if let Err(error) = component.borrow_mut().update(properties) {
                        emit(&events, &item.control, "error", error.into());
                    }
                }
            }
        }
        Content::RichEditor { .. } => {
            if let Some(State::RichEditor(editor)) = &item.hosted {
                editor.update(spec);
            }
        }
        Content::Media { .. } => {
            if let Some(State::Media(media)) = &item.hosted {
                media.update(spec);
            }
        }
        Content::Web { .. } => {
            if let Some(State::Web(web)) = &item.hosted {
                web.update(spec);
            }
        }
        Content::Terminal { .. } => {
            if let Some(State::Terminal(terminal)) = &item.hosted {
                terminal.update(spec);
            }
        }
        Content::Canvas { .. } => item.widget.queue_draw(),
        Content::MenuButton {
            label,
            enabled,
            items,
        } => {
            let button = item.widget.downcast_ref::<gtk::MenuButton>().unwrap();
            button.set_label(label);
            button.set_sensitive(*enabled);
            if specification(&item.control).as_ref() == Some(spec) && button.popover().is_some() {
                return;
            }
            let popover = gtk::Popover::new();
            let list = menu(items, &popover, events, item.control.clone());
            popover.set_child(Some(&list));
            button.set_popover(Some(&popover));
        }
        Content::Segments {
            items,
            selected,
            enabled,
        } => {
            if specification(&item.control).as_ref() == Some(spec)
                && item.widget.first_child().is_some()
            {
                return;
            }
            choices(
                item,
                items.iter().map(|i| {
                    (
                        &i.id,
                        &i.label,
                        i.enabled,
                        i.accessibility_label.as_deref(),
                        None,
                    )
                }),
                selected.as_deref(),
                *enabled,
                events,
            );
        }
        Content::TabBar {
            items,
            selected,
            enabled,
            hidden,
        } => {
            item.widget.set_visible(!hidden);
            if specification(&item.control).as_ref() == Some(spec)
                && item.widget.first_child().is_some()
            {
                return;
            }
            choices(
                item,
                items.iter().map(|i| {
                    (
                        &i.id,
                        &i.label,
                        i.enabled,
                        i.accessibility_label.as_deref(),
                        i.badge.as_deref(),
                    )
                }),
                selected.as_deref(),
                *enabled,
                events,
            );
        }
        Content::Date { value, enabled, .. } => {
            let calendar = item.widget.downcast_ref::<gtk::Calendar>().unwrap();
            calendar.set_sensitive(*enabled);
            if let Some(CivilDate { year, month, day }) = value {
                if let Ok(date) =
                    glib::DateTime::from_local(*year as i32, *month as i32, *day as i32, 12, 0, 0.)
                {
                    calendar.select_day(&date);
                }
            }
        }
        Content::Time {
            value:
                TimeOfDay {
                    hour,
                    minute,
                    second,
                },
            seconds,
            enabled,
        } => {
            item.widget.set_sensitive(*enabled);
            let mut field = item.widget.first_child();
            for (index, value) in [hour, minute, second].into_iter().enumerate() {
                if let Some(widget) = field {
                    let spin = widget.downcast::<gtk::SpinButton>().unwrap();
                    spin.set_value(*value as f64);
                    spin.set_visible(index < 2 || *seconds);
                    field = spin.next_sibling();
                }
            }
        }
        Content::Color {
            rgba,
            alpha,
            enabled,
        } => {
            let button = item
                .widget
                .downcast_ref::<gtk::ColorDialogButton>()
                .unwrap();
            button.set_sensitive(*enabled);
            if let Some(dialog) = button.dialog() {
                dialog.set_with_alpha(*alpha);
            }
            button.set_rgba(&gdk::RGBA::new(
                rgba[0] as f32 / 255.,
                rgba[1] as f32 / 255.,
                rgba[2] as f32 / 255.,
                rgba[3] as f32 / 255.,
            ));
        }
    }
}
fn choices<'a>(
    item: &Item,
    items: impl Iterator<
        Item = (
            &'a String,
            &'a String,
            bool,
            Option<&'a str>,
            Option<&'a str>,
        ),
    >,
    selected: Option<&str>,
    enabled: bool,
    events: Rc<Events>,
) {
    let row = item.widget.downcast_ref::<gtk::Box>().unwrap();
    row.set_sensitive(enabled);
    let entries: Vec<_> = items.collect();
    let mut existing = Vec::new();
    let mut child = row.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        existing.push(widget);
    }
    let same_ids = match specification(&item.control) {
        Some(Content::Segments { items, .. }) => items
            .iter()
            .map(|i| i.id.as_str())
            .eq(entries.iter().map(|i| i.0.as_str())),
        Some(Content::TabBar { items, .. }) => items
            .iter()
            .map(|i| i.id.as_str())
            .eq(entries.iter().map(|i| i.0.as_str())),
        _ => false,
    } && existing.len() == entries.len();
    if !same_ids {
        for child in &existing {
            row.remove(child);
        }
    }
    let mut group: Option<gtk::ToggleButton> = None;
    for (index, (id, label, enabled, a11y, badge)) in entries.into_iter().enumerate() {
        let title = badge.map_or_else(
            || label.clone(),
            |badge| format!("{label} {}", if badge.is_empty() { "•" } else { badge }),
        );
        let button = if same_ids {
            existing[index]
                .clone()
                .downcast::<gtk::ToggleButton>()
                .unwrap()
        } else {
            gtk::ToggleButton::new()
        };
        button.set_label(&title);
        button.set_sensitive(enabled);
        button.set_hexpand(true);
        if let Some(group) = &group {
            button.set_group(Some(group));
        } else {
            group = Some(button.clone());
        }
        if let Some(label) = a11y {
            button.update_property(&[gtk::accessible::Property::Label(label)]);
        } else {
            button.reset_property(gtk::AccessibleProperty::Label);
        }
        button.set_active(selected == Some(id));
        if !same_ids {
            let (events, control, id) = (events.clone(), item.control.clone(), id.clone());
            button.connect_clicked(move |button| {
                if button.is_active() {
                    emit(&events, &control, "select", id.clone().into());
                }
            });
            row.append(&button);
        }
    }
}
fn menu(
    items: &[jadix_core::hosted::HostedMenuItem],
    popover: &gtk::Popover,
    events: Rc<Events>,
    control: Rc<Control>,
) -> gtk::Box {
    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    for item in items {
        if !item.children.is_empty() {
            let nested = menu(&item.children, popover, events.clone(), control.clone());
            nested.set_sensitive(item.enabled);
            if item.inline {
                list.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
                list.append(&nested);
            } else {
                let expander = gtk::Expander::new(Some(&item.label));
                expander.set_child(Some(&nested));
                list.append(&expander);
            }
        } else {
            let button = gtk::Button::with_label(&format!(
                "{}{}{}",
                if item.checked { "✓ " } else { "" },
                item.label,
                item.accelerator
                    .as_ref()
                    .map_or(String::new(), |v| format!("\t{v}"))
            ));
            button.set_sensitive(item.enabled);
            if item.destructive {
                button.add_css_class("destructive-action");
            }
            if let Some(label) = &item.accessibility_label {
                button.update_property(&[gtk::accessible::Property::Label(label)]);
            }
            let (events, control, id, popover) = (
                events.clone(),
                control.clone(),
                item.id.clone(),
                popover.downgrade(),
            );
            button.connect_clicked(move |_| {
                emit(&events, &control, "select", id.clone().into());
                if let Some(popover) = popover.upgrade() {
                    popover.popdown();
                }
            });
            list.append(&button);
        }
    }
    list
}
