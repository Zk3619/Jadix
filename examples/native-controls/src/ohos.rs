use std::cell::RefCell;

use jadix_native::ohos::OhosHost;
use napi_derive_ohos::napi;
use napi_ohos::{
    bindgen_prelude::{Function, Unknown},
    Env, Error, JsValue, Result,
};

#[cfg(feature = "field-workflow")]
use crate::field::FieldApp as NativeApp;
#[cfg(not(feature = "field-workflow"))]
use crate::NativeApp;

thread_local! {
    static APPLICATION: RefCell<Option<NativeApp<OhosHost>>> = const { RefCell::new(None) };
}

fn with_app<Output>(
    identity: &str,
    run: impl FnOnce(&mut NativeApp<OhosHost>) -> std::result::Result<Output, String>,
) -> Result<Output> {
    APPLICATION.with(|slot| {
        let mut slot = slot
            .try_borrow_mut()
            .map_err(|_| Error::from_reason("reentrant OHOS native call"))?;
        let app = slot
            .as_mut()
            .ok_or_else(|| Error::from_reason("OHOS native root is not mounted"))?;
        if app.root.model().id().token() != identity {
            return Err(Error::from_reason("stale OHOS native root"));
        }
        run(app).map_err(Error::from_reason)
    })
}

#[napi]
pub fn native_mount(
    environment: Env,
    content: Unknown,
    width: f64,
    height: f64,
    density: f64,
    notify: Function<(), ()>,
) -> Result<String> {
    APPLICATION.with(|slot| {
        let mut slot = slot
            .try_borrow_mut()
            .map_err(|_| Error::from_reason("reentrant OHOS native mount"))?;
        if slot.is_some() {
            return Err(Error::from_reason(
                "dispose the previous OHOS native root first",
            ));
        }
        let viewport = [width as f32, height as f32];
        let host = unsafe {
            OhosHost::from_napi(
                environment.raw().cast(),
                content.raw().cast(),
                viewport,
                density,
            )
        }
        .map_err(Error::from_reason)?;
        let mut app =
            NativeApp::new(host, viewport, 0, String::new()).map_err(Error::from_reason)?;
        let notify = notify.create_ref()?;
        app.root.host_mut().set_notifier(move || {
            environment
                .run_in_scope(|| notify.borrow_back(&environment)?.call(()))
                .map_err(|error| error.to_string())
        });
        let identity = app.root.model().id().token();
        *slot = Some(app);
        Ok(identity)
    })
}

#[napi]
pub fn native_poll(identity: String) -> Result<String> {
    with_app(&identity, |app| {
        app.poll_events()?;
        Ok(app.state())
    })
}

#[napi]
pub fn native_inspect(identity: String, include_text: bool) -> Result<String> {
    with_app(&identity, |app| {
        let mut snapshot = app.root.inspection(include_text);
        snapshot["native_host"] = app.root.host().inspection()?;
        Ok(snapshot.to_string())
    })
}

#[napi]
pub fn native_state(identity: String) -> Result<String> {
    with_app(&identity, |app| Ok(app.state()))
}

#[napi]
pub fn native_restore(identity: String, source: String) -> Result<()> {
    with_app(&identity, |app| app.restore(&source))
}

#[napi]
pub fn native_resize(
    identity: String,
    width: f64,
    height: f64,
    density: f64,
    environment: u32,
) -> Result<()> {
    with_app(&identity, |app| {
        let viewport = [width as f32, height as f32];
        app.root.host_mut().resize(viewport, density)?;
        app.resize(viewport, environment.into())
    })
}

#[napi]
pub fn native_unmount(identity: String) -> Result<()> {
    APPLICATION.with(|slot| {
        let mut slot = slot
            .try_borrow_mut()
            .map_err(|_| Error::from_reason("reentrant OHOS native unmount"))?;
        if let Some(app) = slot.as_ref() {
            if app.root.model().id().token() != identity {
                return Err(Error::from_reason("stale OHOS native unmount"));
            }
        }
        slot.take();
        Ok(())
    })
}
