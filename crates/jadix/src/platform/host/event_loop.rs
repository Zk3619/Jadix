//! winit `EventLoop` construction + run. Platform extras stay here, not in `app/`.
//!
//! | Target | Builder | Run |
//! |--------|---------|-----|
//! | desktop | `EventLoop::builder` | `run_app` (blocks) |
//! | iOS | same builder | `run_app` (UIKit entry does not return) |
//! | Android | `+ EventLoopBuilderExtAndroid` | `run_app` (blocks) |
//! | HarmonyOS | `+ EventLoopBuilderExtOpenHarmony` | `run_app` (registers, returns) |
//! | wasm32 | same builder | `register_app` (returns) |

use winit::application::ApplicationHandler;

use super::{external_host_takes_over, HostError};

use winit::event_loop::EventLoop;

#[cfg(target_os = "android")]
use winit::platform::android::activity::AndroidApp;

#[cfg(target_os = "android")]
static ANDROID_APP: std::sync::OnceLock<AndroidApp> = std::sync::OnceLock::new();

/// Store the `AndroidApp` passed to `android_main`. Required before [`build_event_loop`].
#[cfg(target_os = "android")]
pub fn install_android_app(app: AndroidApp) {
    if ANDROID_APP.set(app).is_err() {
        log::warn!("install_android_app: AndroidApp already installed; ignoring the second call");
    }
}

#[cfg(target_os = "android")]
pub(crate) fn android_app() -> AndroidApp {
    ANDROID_APP
        .get()
        .expect("call jadix::platform::install_android_app from android_main before run")
        .clone()
}

#[cfg(target_env = "ohos")]
static OHOS_APP: std::sync::OnceLock<openharmony_ability::OpenHarmonyApp> =
    std::sync::OnceLock::new();

/// Store the Ability handle passed to `#[ability]`. Required before [`build_event_loop`].
#[cfg(target_env = "ohos")]
pub fn install_ohos_app(app: openharmony_ability::OpenHarmonyApp) {
    let r = app.content_rect();
    crate::platform::os::harmonyos::install_ohos_content_rect(
        crate::platform::os::harmonyos::OhosContentRect {
            left: r.left,
            top: r.top,
            width: r.width,
            height: r.height,
        },
    );
    if OHOS_APP.set(app).is_err() {
        log::warn!("install_ohos_app: OpenHarmonyApp already installed; ignoring the second call");
    }
}

#[cfg(target_env = "ohos")]
fn ohos_app() -> openharmony_ability::OpenHarmonyApp {
    OHOS_APP
        .get()
        .expect("call jadix::platform::install_ohos_app from #[ability] before run")
        .clone()
}

/// Refuse winit when an external host claimed the loop.
pub fn ensure_winit_host() -> Result<(), HostError> {
    if external_host_takes_over() {
        return Err(HostError::ExternalTookOver);
    }
    #[cfg(target_env = "ohos")]
    {
        if OHOS_APP.get().is_none() {
            return Err(HostError::HarmonyOsNeedsBackend);
        }
    }
    Ok(())
}

/// Build the process event loop. Android / HarmonyOS must install their host app first.
pub fn build_event_loop() -> EventLoop {
    ensure_winit_host().unwrap_or_else(|e| panic!("{e}"));

    let mut builder = EventLoop::builder();
    #[cfg(target_env = "ohos")]
    {
        use winit::platform::ohos::EventLoopBuilderExtOpenHarmony;
        builder.with_openharmony_app(ohos_app());
    }
    #[cfg(all(windows, feature = "native-menu"))]
    {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        // SAFETY: winit passes a valid MSG on the event-loop thread.
        builder.with_msg_hook(|msg| unsafe { crate::app::native_menu::translate_accelerator(msg) });
    }
    #[cfg(target_os = "android")]
    {
        use winit::platform::android::EventLoopBuilderExtAndroid;
        builder.with_android_app(android_app());
    }
    builder.build().expect("event loop")
}

/// Desktop/Android block, iOS does not return, OHOS registers callbacks and returns.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_winit_app<A>(event_loop: EventLoop, app: A)
where
    A: ApplicationHandler + 'static,
{
    let _ = event_loop.run_app(app);
}

/// wasm32: schedule the handler on the browser event loop and return immediately.
#[cfg(target_arch = "wasm32")]
pub fn spawn_winit_app<A>(event_loop: EventLoop, app: A)
where
    A: ApplicationHandler + 'static,
{
    use winit::event_loop::register::EventLoopExtRegister;
    event_loop.register_app(app);
}
