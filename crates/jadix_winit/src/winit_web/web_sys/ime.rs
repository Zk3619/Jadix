use std::cell::{Cell, RefCell};
use std::rc::Rc;

use wasm_bindgen::{closure::Closure, JsCast};
use web_sys::{
    CompositionEvent, Document, Event, HtmlCanvasElement, HtmlTextAreaElement, InputEvent,
    KeyboardEvent, KeyboardEventInit,
};

use crate::winit_core::event::Ime;
use crate::winit_core::window::{
    ImeCapabilities, ImePurpose, ImeRequest, ImeRequestData, ImeRequestError,
};

use super::event_handle::EventListenerHandle;

type Callback = Rc<RefCell<Option<Rc<dyn Fn(Ime)>>>>;

pub struct ImeInput {
    pub input: HtmlTextAreaElement,
    enabled: Rc<Cell<Option<ImeCapabilities>>>,
    pub composing: Rc<Cell<bool>>,
    callback: Callback,
    _listeners: Vec<EventListenerHandle<dyn FnMut(Event)>>,
}

fn emit(callback: &Callback, event: Ime) {
    let handler = callback.borrow().clone();
    if let Some(handler) = handler {
        handler(event);
    }
}

impl ImeInput {
    pub fn new(document: &Document) -> Result<Self, wasm_bindgen::JsValue> {
        let input: HtmlTextAreaElement = document.create_element("textarea")?.dyn_into()?;
        input.set_attribute("aria-label", "Jadix text input")?;
        input.set_attribute("data-jadix-ime", "")?;
        input.set_attribute("autocomplete", "off")?;
        input.set_attribute("autocorrect", "off")?;
        input.set_attribute("autocapitalize", "off")?;
        input.set_spellcheck(false);
        input.set_tab_index(-1);
        input.set_disabled(true);
        input.style().set_css_text("position:fixed;left:0;top:0;width:10000px;height:1px;transform:scaleX(0.0001);transform-origin:top left;opacity:0;pointer-events:none;padding:0;border:0;resize:none;overflow:hidden;font-size:16px;white-space:pre;overflow-wrap:normal;");
        document
            .body()
            .ok_or_else(|| wasm_bindgen::JsValue::from_str("missing document body"))?
            .append_child(&input)?;

        let enabled = Rc::new(Cell::new(None));
        let composing = Rc::new(Cell::new(false));
        let callback: Callback = Rc::default();
        let last_commit = Rc::new(RefCell::new(None::<String>));
        let mut listeners = Vec::new();
        for kind in [
            "compositionstart",
            "compositionupdate",
            "compositionend",
            "input",
            "beforeinput",
        ] {
            let input_clone = input.clone();
            let enabled = enabled.clone();
            let composing = composing.clone();
            let callback = callback.clone();
            let last_commit = last_commit.clone();
            let listener: Closure<dyn FnMut(Event)> = Closure::new(move |event: Event| {
                if enabled.get().is_none() {
                    return;
                }
                if let Some(event) = event.dyn_ref::<CompositionEvent>() {
                    let text = event.data().unwrap_or_default();
                    match kind {
                        "compositionstart" => {
                            last_commit.borrow_mut().take();
                            composing.set(true);
                            emit(&callback, Ime::Preedit(String::new(), Some((0, 0))));
                        }
                        "compositionupdate" => {
                            let cursor = text.len();
                            emit(&callback, Ime::Preedit(text, Some((cursor, cursor))));
                        }
                        "compositionend" => {
                            composing.set(false);
                            input_clone.set_value("");
                            *last_commit.borrow_mut() = Some(text.clone());
                            emit(&callback, Ime::Preedit(String::new(), None));
                            if !text.is_empty() {
                                emit(&callback, Ime::Commit(text));
                            }
                        }
                        _ => (),
                    }
                } else if let Some(event) = event.dyn_ref::<InputEvent>() {
                    if composing.get() || event.is_composing() {
                        return;
                    }
                    if kind == "beforeinput" {
                        let command = match event.input_type().as_str() {
                            "deleteContentBackward" => Some("Backspace"),
                            "deleteContentForward" => Some("Delete"),
                            "insertLineBreak" | "insertParagraph" => Some("Enter"),
                            _ => None,
                        };
                        if let Some(key) = command {
                            event.prevent_default();
                            let options = KeyboardEventInit::new();
                            options.set_key(key);
                            options.set_code(key);
                            for kind in ["keydown", "keyup"] {
                                if let Ok(keyboard) =
                                    KeyboardEvent::new_with_keyboard_event_init_dict(kind, &options)
                                {
                                    let _ = input_clone.dispatch_event(&keyboard);
                                }
                            }
                        }
                    } else {
                        let text = event.data().unwrap_or_else(|| input_clone.value());
                        input_clone.set_value("");
                        let echoed = last_commit.borrow_mut().take().as_deref()
                            == Some(text.as_str())
                            && matches!(
                                event.input_type().as_str(),
                                "insertCompositionText" | "insertFromComposition"
                            );
                        if !text.is_empty() && !echoed {
                            emit(&callback, Ime::Commit(text));
                        }
                    }
                }
            });
            listeners.push(EventListenerHandle::new(input.clone(), kind, listener));
        }
        Ok(Self {
            input,
            enabled,
            composing,
            callback,
            _listeners: listeners,
        })
    }

    pub fn on_event(&self, handler: impl Fn(Ime) + 'static) {
        *self.callback.borrow_mut() = Some(Rc::new(handler));
    }

    pub fn capabilities(&self) -> Option<ImeCapabilities> {
        self.enabled.get()
    }

    pub fn detach(&self) {
        self.callback.borrow_mut().take();
        self.enabled.set(None);
        self.input.set_disabled(true);
        self.input.remove();
    }

    pub fn request(
        &self,
        request: ImeRequest,
        canvas: &HtmlCanvasElement,
    ) -> Result<(), ImeRequestError> {
        match request {
            ImeRequest::Enable(request) => {
                if self.enabled.get().is_some() {
                    return Err(ImeRequestError::AlreadyEnabled);
                }
                let (capabilities, data) = request.into_raw();
                if capabilities.surrounding_text() {
                    return Err(ImeRequestError::NotSupported);
                }
                self.enabled.set(Some(capabilities));
                self.input.set_disabled(false);
                emit(&self.callback, Ime::Enabled);
                self.update(data, canvas);
                let _ = self.input.focus();
            }
            ImeRequest::Update(data) => {
                if self.enabled.get().is_none() {
                    return Err(ImeRequestError::NotEnabled);
                }
                self.update(data, canvas);
            }
            ImeRequest::Disable => {
                if self.enabled.take().is_some() {
                    self.composing.set(false);
                    self.input.set_value("");
                    if self
                        .input
                        .owner_document()
                        .and_then(|document| document.active_element())
                        .is_some_and(|active| {
                            active == self.input.clone().unchecked_into::<web_sys::Element>()
                        })
                    {
                        let _ = canvas.focus();
                    }
                    self.input.set_disabled(true);
                    emit(&self.callback, Ime::Disabled);
                }
            }
        }
        Ok(())
    }

    fn update(&self, data: ImeRequestData, canvas: &HtmlCanvasElement) {
        let capabilities = self.enabled.get().unwrap_or_default();
        if capabilities.hint_and_purpose() {
            if let Some((_, purpose)) = data.hint_and_purpose {
                let mode = match purpose {
                    ImePurpose::Number => "decimal",
                    ImePurpose::Pin => "numeric",
                    ImePurpose::Phone => "tel",
                    ImePurpose::Email => "email",
                    ImePurpose::Url => "url",
                    _ => "text",
                };
                let _ = self.input.set_attribute("inputmode", mode);
            }
        }
        if capabilities.cursor_area() {
            if let Some((position, size)) = data.cursor_area {
                let scale = canvas
                    .owner_document()
                    .and_then(|document| document.default_view())
                    .map_or(1.0, |window| window.device_pixel_ratio());
                let position = position.to_logical::<f64>(scale);
                let size = size.to_logical::<f64>(scale);
                let bounds = canvas.get_bounding_client_rect();
                let _ = self
                    .input
                    .style()
                    .set_property("left", &format!("{}px", bounds.left() + position.x));
                let _ = self
                    .input
                    .style()
                    .set_property("top", &format!("{}px", bounds.top() + position.y));
                let height = format!("{}px", size.height.max(1.0));
                let _ = self.input.style().set_property("height", &height);
                let _ = self.input.style().set_property("line-height", &height);
            }
        }
        if canvas
            .owner_document()
            .and_then(|document| document.active_element())
            .is_some_and(|active| active == canvas.clone().unchecked_into::<web_sys::Element>())
        {
            let _ = self.input.focus();
        }
    }
}

impl Drop for ImeInput {
    fn drop(&mut self) {
        self.input.remove();
    }
}
