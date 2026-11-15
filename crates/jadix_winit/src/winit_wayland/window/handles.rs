use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::winit_core::error::RequestError;
use crate::winit_core::event::WindowEvent;
use crate::winit_core::monitor::MonitorHandle as CoreMonitorHandle;
use crate::winit_core::window::{UserAttentionType, WindowId};
use sctk::compositor::{CompositorState, Region};
use sctk::reexports::client::QueueHandle;
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use sctk::reexports::protocols::xdg::activation::v1::client::xdg_activation_v1::XdgActivationV1;
use tracing::warn;

use super::super::event_loop::sink::EventSink;
use super::super::output::MonitorHandle;
use super::super::state::WinitState;
use super::super::types::xdg_activation::XdgActivationTokenData;

#[derive(Debug)]
pub struct Handles {
    /// Handle to the main queue to perform requests.
    pub(crate) queue_handle: QueueHandle<WinitState>,

    /// Window requests to the event loop.
    pub(crate) window_requests: Arc<WindowRequests>,

    /// Observed monitors.
    pub(crate) monitors: Arc<Mutex<Vec<MonitorHandle>>>,

    /// Source to wake-up the event-loop for window requests.
    pub(crate) event_loop_awakener: calloop::ping::Ping,

    /// The event sink to deliver synthetic events.
    pub(crate) window_events_sink: Arc<Mutex<EventSink>>,

    /// Xdg activation to request user attention.
    pub(crate) xdg_activation: Option<XdgActivationV1>,

    /// The state of the requested attention from the `xdg_activation`.
    pub(crate) attention_requested: Arc<AtomicBool>,

    /// Compositor to handle WlRegion stuff.
    pub(crate) compositor: Arc<CompositorState>,
}

impl Handles {
    pub(crate) fn request_redraw(&self) {
        // NOTE: try to not wake up the loop when the event was already scheduled and not yet
        // processed by the loop, because if at this point the value was `true` it could only
        // mean that the loop still haven't dispatched the value to the client and will do
        // eventually, resetting it to `false`.
        if self
            .window_requests
            .redraw_requested
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            self.event_loop_awakener.ping();
        }
    }

    pub(crate) fn push_window_event(&self, event: WindowEvent, id: WindowId) {
        self.window_events_sink
            .lock()
            .unwrap()
            .push_window_event(event, id);
        self.event_loop_awakener.ping();
    }

    pub(crate) fn available_monitors(&self) -> Box<dyn Iterator<Item = CoreMonitorHandle>> {
        Box::new(
            self.monitors
                .lock()
                .unwrap()
                .clone()
                .into_iter()
                .map(|inner| CoreMonitorHandle(Arc::new(inner))),
        )
    }

    pub(crate) fn request_user_attention(
        &self,
        surface: &WlSurface,
        request_type: Option<UserAttentionType>,
    ) {
        let xdg_activation = match self.xdg_activation.as_ref() {
            Some(xdg_activation) => xdg_activation,
            None => {
                warn!("`request_user_attention` isn't supported");
                return;
            }
        };

        // Urgency is only removed by the compositor and there's no need to raise urgency when it
        // was already raised.
        if request_type.is_none() || self.attention_requested.load(Ordering::Relaxed) {
            return;
        }

        self.attention_requested.store(true, Ordering::Relaxed);
        let data = XdgActivationTokenData::Attention((
            surface.clone(),
            Arc::downgrade(&self.attention_requested),
        ));
        let xdg_activation_token = xdg_activation.get_activation_token(&self.queue_handle, data);
        xdg_activation_token.set_surface(surface);
        xdg_activation_token.commit();
    }

    pub(crate) fn set_input_region(
        &self,
        surface: &WlSurface,
        rectangles: Option<&[crate::winit_core::window::InputRegionRect]>,
        scale: f64,
    ) -> Result<(), RequestError> {
        let Some(rectangles) = rectangles else {
            surface.set_input_region(None);
            return Ok(());
        };
        let region = Region::new(&*self.compositor).map_err(|error| os_error!(error))?;
        for rect in rectangles
            .iter()
            .filter(|rect| rect.size.width > 0 && rect.size.height > 0)
        {
            let left = (f64::from(rect.position.x) / scale).floor();
            let top = (f64::from(rect.position.y) / scale).floor();
            let right = ((f64::from(rect.position.x) + f64::from(rect.size.width)) / scale).ceil();
            let bottom =
                ((f64::from(rect.position.y) + f64::from(rect.size.height)) / scale).ceil();
            if ![left, top, right, bottom].iter().all(|value| {
                value.is_finite() && *value >= f64::from(i32::MIN) && *value <= f64::from(i32::MAX)
            }) || right - left > f64::from(i32::MAX)
                || bottom - top > f64::from(i32::MAX)
            {
                return Err(crate::winit_core::error::NotSupportedError::new(
                    "input rectangle exceeds Wayland coordinate range",
                )
                .into());
            }
            region.add(
                left as i32,
                top as i32,
                (right - left) as i32,
                (bottom - top) as i32,
            );
        }
        surface.set_input_region(Some(region.wl_region()));
        Ok(())
    }

    pub(crate) fn set_cursor_hittest(
        &self,
        surface: &WlSurface,
        hittest: bool,
    ) -> Result<(), RequestError> {
        if hittest {
            surface.set_input_region(None);
            Ok(())
        } else {
            let region = Region::new(&*self.compositor).map_err(|err| os_error!(err))?;
            region.add(0, 0, 0, 0);
            surface.set_input_region(Some(region.wl_region()));
            Ok(())
        }
    }
}

/// The request from the window to the event loop.
#[derive(Debug)]
pub struct WindowRequests {
    /// The window was closed.
    pub closed: AtomicBool,

    /// Redraw Requested.
    pub redraw_requested: AtomicBool,
}

impl WindowRequests {
    pub fn take_closed(&self) -> bool {
        self.closed.swap(false, Ordering::Relaxed)
    }

    pub fn take_redraw_requested(&self) -> bool {
        self.redraw_requested.swap(false, Ordering::Relaxed)
    }
}
