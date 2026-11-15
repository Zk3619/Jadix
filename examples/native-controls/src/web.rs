use super::PlatformApp as NativeApp;
use jadix_native::dom::{DomHost, EventSink};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{prelude::*, JsCast};

thread_local! { static APP: RefCell<Option<NativeApp<DomHost>>> = const { RefCell::new(None) }; }
thread_local! { static LAST_ERROR: RefCell<Option<String>> = const { RefCell::new(None) }; }

fn js(error: String) -> JsValue {
    JsValue::from_str(&error)
}

#[wasm_bindgen]
pub fn mount(id: &str) -> Result<(), JsValue> {
    unmount();
    LAST_ERROR.with(|error| error.borrow_mut().take());
    let document = web_sys::window()
        .ok_or_else(|| js("missing window".into()))?
        .document()
        .ok_or_else(|| js("missing document".into()))?;
    let container: web_sys::HtmlElement = document
        .get_element_by_id(id)
        .ok_or_else(|| js("missing mount element".into()))?
        .dyn_into()
        .map_err(|_| js("mount must be an HTML element".into()))?;
    let sink: EventSink = Rc::new(RefCell::new(|json: String| {
        let result = APP.with(|app| {
            let mut app = app
                .try_borrow_mut()
                .map_err(|_| "native root is busy".to_string())?;
            app.as_mut()
                .ok_or_else(|| "unmounted native root".to_string())?
                .event(&json)
        });
        if let Err(error) = &result {
            LAST_ERROR.with(|current| *current.borrow_mut() = Some(error.clone()));
        }
        result
    }));
    let bounds = container.get_bounding_client_rect();
    let host = DomHost::new(container, sink).map_err(js)?;
    let app = NativeApp::new(
        host,
        [bounds.width() as f32, bounds.height() as f32],
        0,
        String::new(),
    )
    .map_err(js)?;
    APP.with(|current| *current.borrow_mut() = Some(app));
    Ok(())
}

#[wasm_bindgen]
pub fn resize(width: f32, height: f32, environment: u32) -> Result<(), JsValue> {
    APP.with(|app| {
        app.borrow_mut()
            .as_mut()
            .ok_or_else(|| js("unmounted native root".into()))?
            .resize([width, height], u64::from(environment))
            .map_err(js)
    })
}

#[wasm_bindgen]
pub fn state() -> Option<String> {
    APP.with(|app| app.borrow().as_ref().map(NativeApp::state))
}

#[wasm_bindgen]
pub fn take_error() -> Option<String> {
    LAST_ERROR.with(|error| error.borrow_mut().take())
}

#[cfg(any(feature = "notes", feature = "field-workflow"))]
#[wasm_bindgen]
pub fn restore(json: &str) -> Result<(), JsValue> {
    APP.with(|app| {
        app.borrow_mut()
            .as_mut()
            .ok_or_else(|| js("unmounted notes root".into()))?
            .restore(json)
            .map_err(js)
    })
}

#[cfg(any(feature = "notes", feature = "field-workflow"))]
#[wasm_bindgen]
pub fn storage_result(error: Option<String>) -> Result<(), JsValue> {
    APP.with(|app| {
        app.borrow_mut()
            .as_mut()
            .ok_or_else(|| js("unmounted notes root".into()))?
            .storage_result(error)
            .map_err(js)
    })
}

#[wasm_bindgen]
pub fn unmount() {
    APP.with(|app| {
        app.borrow_mut().take();
    });
}
