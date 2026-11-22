//! Native GTK editing with inert shared HTML identities attached to text ranges.
use super::*;
use jadix_core::hosted::HostedContent;
use jadix_document::native_html::{prepare_native_html, NativeHtmlPlan};
use jadix_net::service::{FetchEvent, FetchHandle, FetchRequest};
struct Attachment {
    anchor: gtk::TextChildAnchor,
    picture: gtk::Picture,
    index: usize,
}
struct Remote {
    index: usize,
    handle: FetchHandle,
    bytes: Vec<u8>,
    valid: bool,
}

pub(super) struct Editor {
    pub widget: gtk::ScrolledWindow,
    view: gtk::TextView,
    plan: RefCell<Option<NativeHtmlPlan>>,
    html: RefCell<String>,
    updating: Cell<bool>,
    revision: Cell<u64>,
    events: Rc<Events>,
    control: Rc<Control>,
    runtime: Rc<web::Runtime>,
    generation: Cell<u64>,
    attachments: RefCell<Vec<Attachment>>,
    waiting: RefCell<VecDeque<usize>>,
    remote: RefCell<Vec<Remote>>,
    reads: RefCell<Vec<gio::Cancellable>>,
    image_bytes: Cell<usize>,
    separators: RefCell<Vec<(gtk::TextMark, gtk::TextMark, String)>>,
}
impl Editor {
    pub fn new(events: Rc<Events>, control: Rc<Control>, runtime: Rc<web::Runtime>) -> Rc<Self> {
        let view = gtk::TextView::new();
        view.set_wrap_mode(gtk::WrapMode::WordChar);
        view.buffer().set_enable_undo(true);
        let widget = gtk::ScrolledWindow::builder().child(&view).build();
        let this = Rc::new(Self {
            widget,
            view,
            plan: Default::default(),
            html: Default::default(),
            updating: Cell::new(false),
            revision: Cell::new(0),
            events,
            control,
            runtime,
            generation: Cell::new(0),
            attachments: Default::default(),
            waiting: Default::default(),
            remote: Default::default(),
            reads: Default::default(),
            image_bytes: Cell::new(0),
            separators: Default::default(),
        });
        let weak = Rc::downgrade(&this);
        this.view.buffer().connect_changed(move |_| {
            let Some(this) = weak.upgrade() else { return };
            this.changed();
        });
        let weak = Rc::downgrade(&this);
        this.view.buffer().connect_mark_set(move |buffer, _, mark| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() || !matches!(mark.name().as_deref(), Some("insert" | "selection_bound")) { return; }
            let (start,end) = buffer.selection_bounds().unwrap_or_else(|| { let iter=buffer.iter_at_mark(&buffer.get_insert()); (iter,iter) });
            let text=buffer.slice(&buffer.start_iter(),&buffer.end_iter(),true);
            let range=[start.offset(),end.offset()].map(|offset|jadix_core::presentation::input::scalar_to_utf16(&text,offset.max(0) as usize));
            this.emit("selection", serde_json::json!({"type":"selection","range":range,"revision":this.revision.get()}));
        });
        let focus = gtk::EventControllerFocus::new();
        for focused in [true, false] {
            let weak = Rc::downgrade(&this);
            let callback = move |_: &gtk::EventControllerFocus| {
                if let Some(this) = weak.upgrade() {
                    this.emit("focus",serde_json::json!({"type":"focus","focused":focused,"revision":this.revision.get()}));
                }
            };
            if focused {
                focus.connect_enter(callback);
            } else {
                focus.connect_leave(callback);
            }
        }
        this.view.add_controller(focus);
        // Reject oversized typing/paste before changing the native undo history.
        let weak = Rc::downgrade(&this);
        this.view
            .buffer()
            .connect_insert_text(move |buffer, _, text| {
                let Some(this) = weak.upgrade() else { return };
                if !this.updating.get()
                    && this.export().map_or(true, |html| {
                        html.len().saturating_add(escaped(text).len()) > 1_048_576
                    })
                {
                    buffer.stop_signal_emission_by_name("insert-text");
                    this.emit("error", "rich editor HTML budget exceeded".into());
                }
            });
        let keys = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(&this);
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            let Some(this) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if modifiers.contains(gdk::ModifierType::CONTROL_MASK)
                && !modifiers
                    .intersects(gdk::ModifierType::ALT_MASK | gdk::ModifierType::SUPER_MASK)
            {
                let name = match key.to_unicode().map(|c| c.to_ascii_lowercase()) {
                    Some('b') => Some("b"),
                    Some('i') => Some("i"),
                    Some('u') => Some("u"),
                    _ => None,
                };
                if name.is_some_and(|name| this.format_selection(name)) {
                    return glib::Propagation::Stop;
                }
            }
            glib::Propagation::Proceed
        });
        this.view.add_controller(keys);
        this
    }
    fn changed(&self) {
        if self.updating.get() {
            return;
        }
        self.normalize_separators();
        match self.export() {
            Ok(html) => {
                *self.html.borrow_mut() = html.clone();
                self.emit(
                    "change",
                    serde_json::json!({"type":"change","html":html,"revision":self.revision.get()}),
                );
            }
            Err(error) => self.emit("error", error.into()),
        }
    }
    fn format_selection(&self, name: &str) -> bool {
        if !self.view.is_editable() {
            return false;
        }
        let buffer = self.view.buffer();
        let Some((start, end)) = buffer.selection_bounds() else {
            return false;
        };
        let mut slot = self.plan.borrow_mut();
        let Some(plan) = slot.as_mut() else {
            return false;
        };
        let table = buffer.tag_table();
        let tags: Vec<_> = plan
            .elements
            .iter()
            .enumerate()
            .filter(|(_, element)| {
                element.name == name
                    || (name == "b" && element.name == "strong")
                    || (name == "i" && element.name == "em")
            })
            .filter_map(|(index, _)| table.lookup(&format!("jadix-html-{index}")))
            .collect();
        let mut iter = start;
        let mut all = !tags.is_empty();
        while iter < end && all {
            all = tags.iter().any(|tag| iter.has_tag(tag));
            iter.forward_char();
        }
        if all {
            for tag in tags {
                buffer.remove_tag(&tag, &start, &end);
            }
        } else {
            if plan.elements.len() >= 32_768 {
                return false;
            }
            let tag = gtk::TextTag::new(Some(&format!("jadix-html-{}", plan.elements.len())));
            match name {
                "b" => tag.set_weight(700),
                "i" => tag.set_style(gtk::pango::Style::Italic),
                _ => tag.set_underline(gtk::pango::Underline::Single),
            }
            plan.elements
                .push(jadix_document::native_html::NativeHtmlElement {
                    name: name.into(),
                    attributes: Vec::new(),
                    open: format!("<{name}>"),
                    close: format!("</{name}>"),
                    block: false,
                    parent: None,
                });
            table.add(&tag);
            buffer.apply_tag(&tag, &start, &end);
        }
        drop(slot);
        self.changed();
        true
    }
    fn emit(&self, event: &str, data: serde_json::Value) {
        hosted::emit(&self.events, &self.control, event, data);
    }
    pub fn update(self: &Rc<Self>, spec: &HostedContent) {
        let HostedContent::RichEditor {
            html,
            editable,
            selection,
            revision,
        } = spec
        else {
            return;
        };
        self.updating.set(true);
        self.revision.set(*revision);
        self.view.set_editable(*editable);
        if self.plan.borrow().is_none() || *self.html.borrow() != *html {
            match prepare_native_html(html)
                .and_then(|plan| plan.text_runs().map(|runs| (plan, runs)))
            {
                Ok((plan, runs)) => {
                    self.cancel_images();
                    self.generation.set(self.generation.get().wrapping_add(1));
                    let buffer = self.view.buffer();
                    buffer.begin_irreversible_action();
                    buffer.set_text("");
                    let table = buffer.tag_table();
                    for tag in table_tags(&table) {
                        table.remove(&tag);
                    }
                    let tags: Vec<_> = plan
                        .elements
                        .iter()
                        .enumerate()
                        .map(|(index, element)| {
                            let tag = gtk::TextTag::new(Some(&format!("jadix-html-{index}")));
                            match element.name.as_str() {
                                "b" | "strong" | "th" => tag.set_weight(700),
                                "i" | "em" => tag.set_style(gtk::pango::Style::Italic),
                                "u" => tag.set_underline(gtk::pango::Underline::Single),
                                "s" | "del" | "strike" => tag.set_strikethrough(true),
                                "pre" | "code" => tag.set_family(Some("monospace")),
                                "h1" => {
                                    tag.set_weight(700);
                                    tag.set_scale(2.);
                                }
                                "h2" => {
                                    tag.set_weight(700);
                                    tag.set_scale(1.5);
                                }
                                "h3" | "h4" | "h5" | "h6" => tag.set_weight(700),
                                _ => (),
                            }
                            for (name, value) in &element.attributes {
                                match name.as_str() {
                                    "dir" => tag.set_direction(if value == "rtl" {
                                        gtk::TextDirection::Rtl
                                    } else {
                                        gtk::TextDirection::Ltr
                                    }),
                                    "style" => apply_style(&tag, value),
                                    _ => (),
                                }
                            }
                            table.add(&tag);
                            tag
                        })
                        .collect();
                    for run in runs {
                        let mut end = buffer.end_iter();
                        let offset = end.offset();
                        // An object replacement character preserves the attachment's selection coordinate.
                        let text = if run.image.is_some() {
                            "\u{fffc}"
                        } else {
                            &run.text
                        };
                        if let Some(index) = run.image {
                            let anchor = buffer.create_child_anchor(&mut end);
                            let picture = gtk::Picture::new();
                            if let Some(image) = plan.images.get(index) {
                                picture.set_alternative_text(Some(&image.alt));
                            }
                            self.view.add_child_at_anchor(&picture, &anchor);
                            self.attachments.borrow_mut().push(Attachment {
                                anchor,
                                picture,
                                index,
                            });
                        } else {
                            buffer.insert(&mut end, text);
                        }
                        let start = buffer.iter_at_offset(offset);
                        for index in run.elements {
                            if let Some(tag) = tags.get(index) {
                                buffer.apply_tag(tag, &start, &end);
                            }
                        }
                        if run.structural {
                            let tag = table.lookup("jadix-structural").unwrap_or_else(|| {
                                let tag = gtk::TextTag::new(Some("jadix-structural"));
                                table.add(&tag);
                                tag
                            });
                            buffer.apply_tag(&tag, &start, &end);
                            self.separators.borrow_mut().push((
                                buffer.create_mark(None, &start, false),
                                buffer.create_mark(None, &end, true),
                                text.into(),
                            ));
                        }
                        if let Some(index) = run.image {
                            let name = format!("jadix-image-{index}");
                            let tag = table.lookup(&name).unwrap_or_else(|| {
                                let tag = gtk::TextTag::new(Some(&name));
                                table.add(&tag);
                                tag
                            });
                            buffer.apply_tag(&tag, &start, &end);
                        }
                    }
                    buffer.end_irreversible_action();
                    self.waiting.borrow_mut().extend(0..plan.images.len());
                    *self.plan.borrow_mut() = Some(plan);
                    *self.html.borrow_mut() = html.clone();
                    self.events.wake.wake();
                }
                Err(error) => self.emit("error", error.into()),
            }
        }
        if let Some([start, end]) = selection {
            let buffer = self.view.buffer();
            let text = buffer.slice(&buffer.start_iter(), &buffer.end_iter(), true);
            let a = buffer.iter_at_offset(jadix_core::presentation::input::utf16_to_scalar(
                &text, *start,
            ) as i32);
            let b = buffer.iter_at_offset(jadix_core::presentation::input::utf16_to_scalar(
                &text, *end,
            ) as i32);
            buffer.select_range(&b, &a);
        }
        self.updating.set(false);
    }
    fn normalize_separators(&self) {
        let buffer = self.view.buffer();
        let Some(tag) = buffer.tag_table().lookup("jadix-structural") else {
            return;
        };
        buffer.remove_tag(&tag, &buffer.start_iter(), &buffer.end_iter());
        self.separators.borrow_mut().retain(|(start, end, text)| {
            let a = buffer.iter_at_mark(start);
            let b = buffer.iter_at_mark(end);
            if a < b && buffer.text(&a, &b, true).as_str() == text {
                buffer.apply_tag(&tag, &a, &b);
                true
            } else {
                buffer.delete_mark(start);
                buffer.delete_mark(end);
                false
            }
        });
    }
    fn export(&self) -> Result<String, &'static str> {
        let plan = self.plan.borrow();
        let Some(plan) = plan.as_ref() else {
            return Ok(String::new());
        };
        let buffer = self.view.buffer();
        let mut iter = buffer.start_iter();
        let end = buffer.end_iter();
        let mut html = String::new();
        let mut open: Vec<usize> = Vec::new();
        while iter < end {
            let tags = iter.tags();
            let mut current: Vec<_> = tags
                .iter()
                .filter_map(|tag| {
                    tag.name().and_then(|name| {
                        name.strip_prefix("jadix-html-")
                            .and_then(|v| v.parse::<usize>().ok())
                    })
                })
                .filter(|index| *index < plan.elements.len())
                .collect();
            current.sort_unstable();
            let common = open
                .iter()
                .zip(&current)
                .take_while(|(a, b)| a == b)
                .count();
            for index in open[common..].iter().rev() {
                html.push_str(&plan.elements[*index].close);
            }
            for index in &current[common..] {
                html.push_str(&plan.elements[*index].open);
            }
            open = current;
            let image = tags.iter().find_map(|tag| {
                tag.name().and_then(|name| {
                    name.strip_prefix("jadix-image-")
                        .and_then(|v| v.parse::<usize>().ok())
                })
            });
            let mut next = iter;
            next.forward_to_tag_toggle(None::<&gtk::TextTag>);
            if next <= iter || next > end {
                next = end;
            }
            if iter.child_anchor().is_some() {
                next = iter;
                next.forward_char();
                if let Some(image) = image.and_then(|index| plan.images.get(index)) {
                    html.push_str(&image.html);
                }
            } else {
                let text = buffer.text(&iter, &next, true);
                if tags
                    .iter()
                    .any(|tag| tag.name().as_deref() == Some("jadix-structural"))
                {
                    html.push_str(&escaped(&text.replace(['\n', '\t'], "")));
                } else {
                    html.push_str(&escaped(&text).replace('\n', "<br>"));
                }
            }
            if html.len() > 1_048_576 {
                return Err("rich editor HTML budget exceeded");
            }
            iter = next;
        }
        for index in open.iter().rev() {
            html.push_str(&plan.elements[*index].close);
        }
        if html.len() > 1_048_576 {
            Err("rich editor HTML budget exceeded")
        } else {
            Ok(html)
        }
    }
    fn cancel_images(&self) {
        let buffer = self.view.buffer();
        for (a, b, _) in self.separators.borrow_mut().drain(..) {
            buffer.delete_mark(&a);
            buffer.delete_mark(&b);
        }
        self.waiting.borrow_mut().clear();
        self.remote.borrow_mut().clear();
        for cancel in self.reads.borrow_mut().drain(..) {
            cancel.cancel();
        }
        for attachment in self.attachments.borrow_mut().drain(..) {
            self.view.remove(&attachment.picture);
        }
        self.image_bytes.set(0);
    }
    fn image(&self, index: usize, bytes: Result<Vec<u8>, String>) {
        use base64::Engine;
        let result = bytes.and_then(|bytes| {
            let mime = if bytes.starts_with(b"<svg")
                || bytes.windows(4).take(1024).any(|w| w == b"<svg")
            {
                "svg+xml"
            } else {
                "png"
            };
            controls::texture(&format!(
                "data:image/{mime};base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            ))
        });
        match result {
            Ok(texture) => {
                let size = texture.width() as usize * texture.height() as usize * 4;
                let budget = self
                    .plan
                    .borrow()
                    .as_ref()
                    .map_or(0, |plan| plan.image_budget_bytes);
                if self.image_bytes.get().saturating_add(size) > budget {
                    self.emit("error", "rich editor image budget exceeded".into());
                    return;
                }
                let attachments = self.attachments.borrow();
                let Some(attachment) = attachments
                    .iter()
                    .find(|a| a.index == index && !a.anchor.is_deleted())
                else {
                    return;
                };
                self.image_bytes.set(self.image_bytes.get() + size);
                attachment.picture.set_paintable(Some(&texture));
                if let Some(image) = self
                    .plan
                    .borrow()
                    .as_ref()
                    .and_then(|plan| plan.images.get(index))
                {
                    let width = image
                        .width
                        .unwrap_or(texture.width() as f64)
                        .clamp(1., 4096.);
                    let height = image
                        .height
                        .unwrap_or(texture.height() as f64)
                        .clamp(1., 4096.);
                    attachment
                        .picture
                        .set_size_request(width as i32, height as i32);
                }
            }
            Err(error) => self.emit("error", error.into()),
        }
    }
    pub fn pump(self: &Rc<Self>) {
        let mut completed = Vec::new();
        self.remote.borrow_mut().retain_mut(|job| {
            if !self
                .attachments
                .borrow()
                .iter()
                .any(|a| a.index == job.index && !a.anchor.is_deleted())
            {
                return false;
            }
            for event in job.handle.events() {
                match event {
                    FetchEvent::Headers { status, .. } => job.valid = status == 200,
                    FetchEvent::Chunk { bytes } => job.bytes.extend(bytes),
                    FetchEvent::Complete => {
                        completed.push((
                            job.index,
                            if job.valid {
                                Ok(std::mem::take(&mut job.bytes))
                            } else {
                                Err("image HTTP status rejected".into())
                            },
                        ));
                        return false;
                    }
                    FetchEvent::Failed { message } => {
                        completed.push((job.index, Err(message)));
                        return false;
                    }
                    FetchEvent::Cancelled => return false,
                }
            }
            true
        });
        for (index, bytes) in completed {
            self.image(index, bytes);
        }
        for _ in 0..4 {
            if self.remote.borrow().len() + self.reads.borrow().len() >= 4 {
                break;
            }
            let Some(index) = self.waiting.borrow_mut().pop_front() else {
                break;
            };
            let source = self
                .plan
                .borrow()
                .as_ref()
                .and_then(|plan| plan.images.get(index))
                .map(|image| image.source.clone());
            let Some(source) = source else { continue };
            if source.starts_with("data:image/") {
                use base64::Engine;
                self.image(
                    index,
                    source
                        .split_once(";base64,")
                        .ok_or("unsupported image encoding")
                        .and_then(|(_, data)| {
                            base64::engine::general_purpose::STANDARD
                                .decode(data)
                                .map_err(|_| "invalid image encoding")
                        })
                        .map_err(str::to_owned),
                );
            } else if source.starts_with("https://") {
                match self.runtime.fetch.send(FetchRequest {
                    url: source,
                    method: "GET".into(),
                    headers: vec![],
                    body: vec![],
                    timeout_ms: 10_000,
                    max_response_bytes: 768 * 1024,
                }) {
                    Ok(handle) => self.remote.borrow_mut().push(Remote {
                        index,
                        handle,
                        bytes: vec![],
                        valid: false,
                    }),
                    Err(error) => self.image(index, Err(error.to_string())),
                }
            } else {
                let root = self.runtime.asset_root();
                let path = if let Some(relative) = jadix_core::web_resource::bundle_path(&source) {
                    root.join(relative)
                        .canonicalize()
                        .ok()
                        .filter(|p| p.starts_with(&root))
                } else if let Some(relative) = source.strip_prefix("asset:/") {
                    root.join(relative.trim_start_matches('/'))
                        .canonicalize()
                        .ok()
                        .filter(|p| p.starts_with(&root))
                } else if source.starts_with("file:///") {
                    gio::File::for_uri(&source)
                        .path()
                        .and_then(|p| p.canonicalize().ok())
                } else {
                    None
                };
                let Some(path) = path
                    .filter(|p| p.is_file() && p.metadata().is_ok_and(|m| m.len() <= 768 * 1024))
                else {
                    self.image(index, Err("invalid or oversized image source".into()));
                    continue;
                };
                let cancel = gio::Cancellable::new();
                self.reads.borrow_mut().push(cancel.clone());
                let weak = Rc::downgrade(self);
                let generation = self.generation.get();
                gio::File::for_path(path).read_async(
                    glib::Priority::DEFAULT,
                    Some(&cancel.clone()),
                    move |result| {
                        if let Some(this) = weak
                            .upgrade()
                            .filter(|this| this.generation.get() == generation)
                        {
                            match result {
                                Ok(stream) => read_image(
                                    Rc::downgrade(&this),
                                    generation,
                                    index,
                                    stream,
                                    cancel,
                                    vec![],
                                ),
                                Err(error) => this.finish_read(
                                    generation,
                                    index,
                                    &cancel,
                                    Err(error.to_string()),
                                ),
                            }
                        }
                    },
                );
            }
        }
        if !self.waiting.borrow().is_empty()
            && self.remote.borrow().is_empty()
            && self.reads.borrow().is_empty()
        {
            self.events.wake.wake();
        }
    }
    fn finish_read(
        &self,
        generation: u64,
        index: usize,
        cancel: &gio::Cancellable,
        bytes: Result<Vec<u8>, String>,
    ) {
        if self.generation.get() != generation {
            return;
        }
        self.reads.borrow_mut().retain(|value| value != cancel);
        self.image(index, bytes);
        self.events.wake.wake();
    }
}
impl Drop for Editor {
    fn drop(&mut self) {
        self.cancel_images();
    }
}
fn read_image(
    weak: std::rc::Weak<Editor>,
    generation: u64,
    index: usize,
    stream: gio::FileInputStream,
    cancel: gio::Cancellable,
    mut bytes: Vec<u8>,
) {
    stream.clone().read_bytes_async(
        64 * 1024,
        glib::Priority::DEFAULT,
        Some(&cancel.clone()),
        move |result| {
            let Some(this) = weak
                .upgrade()
                .filter(|this| this.generation.get() == generation)
            else {
                return;
            };
            match result {
                Ok(chunk) if chunk.is_empty() => {
                    this.finish_read(generation, index, &cancel, Ok(bytes))
                }
                Ok(chunk) if bytes.len() + chunk.len() <= 768 * 1024 => {
                    bytes.extend_from_slice(&chunk);
                    read_image(
                        Rc::downgrade(&this),
                        generation,
                        index,
                        stream,
                        cancel,
                        bytes,
                    );
                }
                Ok(_) => this.finish_read(
                    generation,
                    index,
                    &cancel,
                    Err("image source exceeds budget".into()),
                ),
                Err(error) => this.finish_read(generation, index, &cancel, Err(error.to_string())),
            }
        },
    );
}
fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn table_tags(table: &gtk::TextTagTable) -> Vec<gtk::TextTag> {
    let tags = RefCell::new(Vec::new());
    table.foreach(|tag| tags.borrow_mut().push(tag.clone()));
    tags.into_inner()
}
fn apply_style(tag: &gtk::TextTag, style: &str) {
    for declaration in style.split(';') {
        let Some((name, value)) = declaration.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match name.trim() {
            "color" => {
                if let Ok(color) = gdk::RGBA::parse(value) {
                    tag.set_foreground_rgba(Some(&color));
                }
            }
            "background-color" => {
                if let Ok(color) = gdk::RGBA::parse(value) {
                    tag.set_background_rgba(Some(&color));
                }
            }
            "font-family" => tag.set_family(Some(value)),
            "font-weight" => tag.set_weight(if matches!(value, "bold" | "bolder") {
                700
            } else {
                value.parse().unwrap_or(400)
            }),
            "font-style" => tag.set_style(if matches!(value, "italic" | "oblique") {
                gtk::pango::Style::Italic
            } else {
                gtk::pango::Style::Normal
            }),
            "font-size" => {
                if let Some(size) = value
                    .strip_suffix("px")
                    .and_then(|s| s.parse::<f64>().ok())
                    .filter(|v| v.is_finite() && *v > 0. && *v <= 4096.)
                {
                    tag.set_size_points(size * 0.75);
                }
            }
            "text-decoration" => {
                tag.set_underline(if value.contains("underline") {
                    gtk::pango::Underline::Single
                } else {
                    gtk::pango::Underline::None
                });
                tag.set_strikethrough(value.contains("line-through"));
            }
            "text-align" => tag.set_justification(match value {
                "center" => gtk::Justification::Center,
                "right" | "end" => gtk::Justification::Right,
                "justify" => gtk::Justification::Fill,
                _ => gtk::Justification::Left,
            }),
            _ => (),
        }
    }
}
