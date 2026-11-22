//! GTK selection, clipboard and IME over the shared bounded VT screen.
use super::*;
use jadix_core::hosted::HostedContent;
use jadix_core::terminal::{CellStyle, TerminalScreen, TerminalSpec};

pub(super) struct Terminal {
    view: gtk::TextView,
    scroll: gtk::ScrolledWindow,
    screen: RefCell<TerminalScreen>,
    revision: Cell<Option<u64>>,
    application_cursor: Cell<bool>,
    bracketed_paste: Cell<bool>,
    events: Rc<Events>,
    control: Rc<Control>,
}
impl Terminal {
    pub fn new(
        columns: u16,
        rows: u16,
        scrollback: usize,
        events: Rc<Events>,
        control: Rc<Control>,
    ) -> Result<Rc<Self>, String> {
        let view = gtk::TextView::new();
        view.set_monospace(true);
        view.set_editable(false);
        view.set_wrap_mode(gtk::WrapMode::None);
        let scroll = gtk::ScrolledWindow::builder().child(&view).build();
        let this = Rc::new(Self {
            view,
            scroll,
            screen: RefCell::new(
                TerminalScreen::new(TerminalSpec {
                    columns,
                    rows,
                    scrollback,
                })
                .map_err(str::to_owned)?,
            ),
            revision: Cell::new(None),
            application_cursor: Cell::new(false),
            bracketed_paste: Cell::new(false),
            events,
            control,
        });
        let ime = gtk::IMMulticontext::new();
        ime.set_client_widget(Some(&this.view));
        let weak = Rc::downgrade(&this);
        ime.connect_commit(move |_, text| {
            if let Some(this) = weak.upgrade() {
                this.input(text.to_owned());
            }
        });
        let focus = gtk::EventControllerFocus::new();
        let context = ime.clone();
        focus.connect_enter(move |_| context.focus_in());
        let context = ime.clone();
        focus.connect_leave(move |_| {
            context.reset();
            context.focus_out();
        });
        this.view.add_controller(focus);
        let keys = gtk::EventControllerKey::new();
        keys.set_im_context(Some(&ime));
        let weak = Rc::downgrade(&this);
        keys.connect_key_pressed(move |_, key, _, flags| {
            let Some(this) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let mods = keyboard::modifiers(flags);
            if mods.control
                && mods.shift
                && key
                    .to_unicode()
                    .is_some_and(|c| c.eq_ignore_ascii_case(&'c'))
            {
                this.view.buffer().copy_clipboard(&this.view.clipboard());
                return glib::Propagation::Stop;
            }
            if mods.control
                && mods.shift
                && key
                    .to_unicode()
                    .is_some_and(|c| c.eq_ignore_ascii_case(&'v'))
            {
                let weak = Rc::downgrade(&this);
                this.view
                    .clipboard()
                    .read_text_async(gio::Cancellable::NONE, move |result| {
                        if let (Some(this), Ok(Some(text))) = (weak.upgrade(), result) {
                            if text.len() <= 64 * 1024 - 12 {
                                this.input(if this.bracketed_paste.get() {
                                    format!("\x1b[200~{text}\x1b[201~")
                                } else {
                                    text.to_string()
                                });
                            }
                        }
                    });
                return glib::Propagation::Stop;
            }
            let name = match key {
                gdk::Key::Up => "up",
                gdk::Key::Down => "down",
                gdk::Key::Left => "left",
                gdk::Key::Right => "right",
                gdk::Key::Return | gdk::Key::KP_Enter => "return",
                gdk::Key::Tab | gdk::Key::ISO_Left_Tab => "tab",
                gdk::Key::BackSpace => "backspace",
                gdk::Key::Escape => "escape",
                gdk::Key::Delete => "delete",
                gdk::Key::Insert => "insert",
                gdk::Key::Home => "home",
                gdk::Key::End => "end",
                gdk::Key::Page_Up => "page_up",
                gdk::Key::Page_Down => "page_down",
                _ => "",
            };
            let function = key
                .name()
                .map(|v| v.to_ascii_lowercase())
                .unwrap_or_default();
            if let Some(bytes) = jadix_core::terminal::special_key(
                if name.is_empty() { &function } else { name },
                mods.shift,
                mods.alt,
                mods.control,
                this.application_cursor.get(),
            ) {
                this.input(bytes);
                return glib::Propagation::Stop;
            }
            if let Some(character) = key.to_unicode().filter(|_| mods.control || mods.alt) {
                let bytes = if mods.control && character.is_ascii() {
                    ((character.to_ascii_uppercase() as u8) & 0x1f) as char
                } else {
                    character
                };
                this.input(format!("{}{bytes}", if mods.alt { "\x1b" } else { "" }));
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        this.view.add_controller(keys);
        Ok(this)
    }
    pub fn widget(&self) -> gtk::ScrolledWindow {
        self.scroll.clone()
    }
    fn input(&self, bytes: String) {
        hosted::emit(
            &self.events,
            &self.control,
            "input",
            serde_json::json!({"bytes":bytes}),
        );
    }
    pub fn update(&self, spec: &HostedContent) {
        let HostedContent::Terminal {
            columns,
            rows,
            scrollback,
            input,
            input_revision,
        } = spec
        else {
            return;
        };
        if self.revision.get() == Some(*input_revision)
            && hosted::specification(&self.control).as_ref() == Some(spec)
        {
            return;
        }
        let mut screen = self.screen.borrow_mut();
        if let Err(error) = screen.configure(TerminalSpec {
            columns: *columns,
            rows: *rows,
            scrollback: *scrollback,
        }) {
            hosted::emit(&self.events, &self.control, "error", error.into());
            return;
        }
        if self.revision.get() != Some(*input_revision) {
            if let Some(input) = input {
                for chunk in input.as_bytes().chunks(64 * 1024) {
                    let _ = screen.feed(chunk);
                }
            }
            self.revision.set(Some(*input_revision));
        }
        let frame = screen.frame();
        drop(screen);
        self.application_cursor.set(frame.application_cursor);
        self.bracketed_paste.set(frame.bracketed_paste);
        let buffer = self.view.buffer();
        let table = buffer.tag_table();
        let mut old = Vec::new();
        table.foreach(|tag| old.push(tag.clone()));
        for tag in old {
            table.remove(&tag);
        }
        let text: String = frame.runs.iter().map(|r| r.text.as_str()).collect();
        buffer.set_text(&text);
        let mut tags: Vec<(CellStyle, gtk::TextTag)> = Vec::new();
        let mut offset = 0;
        for run in frame.runs {
            let end = offset + run.text.chars().count() as i32;
            if run.style != CellStyle::default()
                && (tags.len() < 1024 || tags.iter().any(|(style, _)| style == &run.style))
            {
                let tag = if let Some((_, tag)) = tags.iter().find(|(style, _)| style == &run.style)
                {
                    tag.clone()
                } else {
                    let tag = gtk::TextTag::new(None);
                    let style = &run.style;
                    let color = |n: u32| {
                        gdk::RGBA::new(
                            ((n >> 16) & 255) as f32 / 255.,
                            ((n >> 8) & 255) as f32 / 255.,
                            (n & 255) as f32 / 255.,
                            1.,
                        )
                    };
                    let (foreground, background) = if style.inverse {
                        (
                            style.background.or(Some(0x1d1f21)),
                            style.foreground.or(Some(0xc5c8c6)),
                        )
                    } else {
                        (style.foreground, style.background)
                    };
                    tag.set_foreground_rgba(foreground.map(color).as_ref());
                    tag.set_background_rgba(background.map(color).as_ref());
                    if style.bold {
                        tag.set_weight(700);
                    }
                    if style.italic {
                        tag.set_style(gtk::pango::Style::Italic);
                    }
                    if style.underline {
                        tag.set_underline(gtk::pango::Underline::Single);
                    }
                    table.add(&tag);
                    tags.push((style.clone(), tag.clone()));
                    tag
                };
                buffer.apply_tag(
                    &tag,
                    &buffer.iter_at_offset(offset),
                    &buffer.iter_at_offset(end),
                );
            }
            offset = end;
        }
        let offset =
            jadix_core::presentation::input::utf16_to_scalar(&text, frame.cursor_utf16) as i32;
        buffer.place_cursor(&buffer.iter_at_offset(offset));
        self.view.set_cursor_visible(frame.cursor_visible);
        let mut iter = buffer.iter_at_offset(offset);
        self.view.scroll_to_iter(&mut iter, 0., false, 0., 0.);
        for reply in frame.replies {
            // Parser replies are input even while publishing an output update.
            self.events.enqueue(
                &self.control,
                EventPayload::Hosted {
                    event: "input".into(),
                    data: serde_json::json!({"bytes":reply}),
                },
            );
        }
    }
}
