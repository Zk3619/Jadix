//! Runtime integration for startup providers such as `jadix_splash`.
use crate::core::tree::Element;
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll},
};
use winit::window::Window;

/// A generation of a drawable primary surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceStamp {
    pub scene: u64,
    pub generation: u64,
}

/// An early presentation that can redraw before the GPU renderer exists.
pub trait StartupPresentation {
    fn redraw(&mut self) -> Result<(), String>;
}

/// Startup policy implemented outside the runtime; native window ownership stays here.
pub trait StartupHost {
    fn epoch(&self) -> u64;
    fn content_ready(&self) -> bool;
    fn content_presented(&self) -> bool;
    fn system_presentation(&self) -> bool;
    fn window_options(&self) -> Option<super::WindowOptions>;
    fn window_failed(&mut self);
    fn start(&mut self);
    fn attach(&mut self);
    fn detach(&mut self);
    fn retry(&mut self, epoch: u64) -> bool;
    fn cancel(&self);
    fn renderer_failed(&self, message: String);
    fn acknowledge_update(&self);
    fn view(&mut self) -> Element;
    fn frame(&self, root: &Element) -> Option<(SurfaceStamp, bool)>;
    fn presented(&mut self, stamp: SurfaceStamp, content: bool) -> bool;
    fn present_system(
        &self,
        window: Arc<dyn Window>,
        icon: Option<&super::AppIcon>,
    ) -> Result<Option<Box<dyn StartupPresentation>>, String>;
    fn presentation_failed(&self, window: Option<&dyn Window>, message: &str);
    fn handoff(&self, content: bool);
}

/// A startup provider owned by the UI thread.
pub type StartupHandle = Rc<RefCell<dyn StartupHost>>;

/// Coalesced background-task wakeup. False means the event loop is not installed.
pub fn notify_changed() -> bool {
    super::ui_bridge::send_user_event(super::JadixUserEvent::StartupChanged).is_ok()
}

/// Request an epoch-checked retry on the UI thread.
pub fn request_retry(epoch: u64) {
    let _ = super::ui_bridge::send_user_event(super::JadixUserEvent::StartupRetry { epoch });
}

#[doc(hidden)]
pub fn build_root(
    startup: Option<&std::cell::RefCell<dyn StartupHost>>,
    content: impl FnOnce() -> Element,
) -> Element {
    let root = match startup {
        Some(session) if !session.borrow().content_ready() => {
            if session.borrow().window_options().is_some() {
                let mut root = Element::new().w("100%").h("100%");
                root.set_attr("data-startup-view", "true");
                root
            } else {
                session.borrow_mut().view()
            }
        }
        _ => content(),
    };
    let startup_view = root.get_attr("data-startup-view").is_some();
    let root = if startup.is_some() {
        Element::new().w("100%").h("100%").child(root)
    } else {
        root
    };
    let mut root = super::prepare_primary_ui(root);
    if let Some(session) = startup {
        root.set_attr("data-startup-epoch", session.borrow().epoch().to_string());
    }
    if startup_view {
        root.set_attr("data-startup-view", "true");
    }
    root
}

pub(crate) struct YieldBootstrap(bool);

impl Future for YieldBootstrap {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            Poll::Ready(())
        } else {
            self.0 = true;
            context.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

pub(crate) fn yield_bootstrap() -> YieldBootstrap {
    YieldBootstrap(false)
}
