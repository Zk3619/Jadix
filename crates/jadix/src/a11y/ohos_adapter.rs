//! Per-surface ArkUI provider; callbacks only enqueue work on Jadix's UI loop.
use super::{
    ActivationHandler, AdapterWindowEvent, ChannelActionHandler, Event, TreeUpdate, Window,
};
use jadix_accesskit_ohos::{Adapter, Provider};
use openharmony_ability::arkui::component::attribute::ArkUIAttributeBasic;
use openharmony_ability::OpenHarmonyApp;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use winit::event::WindowEvent;
use winit::platform::ohos::WindowExtOpenHarmony;

struct Activation {
    window_id: winit::window::WindowId,
    active: Arc<AtomicBool>,
}

impl ActivationHandler for Activation {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        let _ =
            crate::app::ui_bridge::send_user_event(crate::app::JadixUserEvent::AccessKit(Event {
                window_id: self.window_id,
                window_event: AdapterWindowEvent::InitialTreeRequested,
                active: Some(self.active.clone()),
            }));
        None
    }
}

pub struct PlatformAdapter {
    adapter: Option<Adapter<'static>>,
    app: OpenHarmonyApp,
    window_id: winit::window::WindowId,
    component: usize,
    active: Arc<AtomicBool>,
    focused: bool,
    scale: f64,
}

impl PlatformAdapter {
    pub fn new(
        window: &dyn Window,
        _activation: impl 'static + ActivationHandler,
        _action: ChannelActionHandler,
    ) -> Self {
        let mut this = Self {
            adapter: None,
            app: window.openharmony_app().clone(),
            window_id: window.id(),
            component: 0,
            active: Arc::new(AtomicBool::new(true)),
            focused: window.has_focus(),
            scale: window.scale_factor(),
        };
        this.attach();
        this
    }

    fn attach(&mut self) {
        if self.adapter.is_some() {
            return;
        }
        static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);
        let window_id = self.window_id;
        self.active = Arc::new(AtomicBool::new(true));
        let event_app = self.app.clone();
        let result = self
            .app
            .with_xcomponent_for(window_id.into_raw() as i64, |component| {
                let native = component.native_xcomponent();
                let provider = native.accessibility_provider()?;
                // SAFETY: registration is disabled at destroy_surfaces/Destroyed before
                // another UI turn. Native calls below are additionally scoped to the
                // matching retained Ability XComponent; replacement pointers are rejected.
                let provider = unsafe { Provider::from_raw(provider.as_raw()) }?;
                let instance = NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed);
                let component_id = component.raw().raw_handle() as usize;
                let provider_id = provider.as_raw() as usize;
                let adapter = Adapter::new_with_instance_and_event_sender(
                    provider,
                    &format!("jadix-{}-{instance}", window_id.into_raw()),
                    Activation {
                        window_id,
                        active: self.active.clone(),
                    },
                    ChannelActionHandler {
                        window_id,
                        active: Some(self.active.clone()),
                    },
                    move |event| {
                        // Teardown takes the Ability write lock. Retain its read
                        // lock through each send, including sends on AT threads.
                        event_app
                            .with_xcomponent_for(window_id.into_raw() as i64, |component| {
                                if component.raw().raw_handle() as usize != component_id {
                                    return Err(jadix_accesskit_ohos::AccessibilityError::Failed);
                                }
                                // SAFETY: current owner matches and is retained by the
                                // Ability read lock; no ArkUI node lookup on the AT thread.
                                unsafe { Provider::from_raw(provider_id as *mut _) }?
                                    .send_event(event, None);
                                Ok(())
                            })
                            .unwrap_or(Err(jadix_accesskit_ohos::AccessibilityError::Failed))
                    },
                )?;
                Ok::<_, jadix_accesskit_ohos::AccessibilityError>((adapter, component_id))
            });
        match result {
            Some(Ok((adapter, component))) => {
                if let Err(error) = adapter.set_host_focus_state(self.focused) {
                    log::warn!("OHOS accessibility initial focus: {error}");
                }
                self.adapter = Some(adapter);
                self.component = component;
            }
            Some(Err(error)) => {
                self.active.store(false, Ordering::Release);
                log::warn!("OHOS accessibility registration: {error}");
            }
            None => {}
        }
    }

    pub fn process_event(&mut self, _window: &dyn Window, event: &WindowEvent) {
        if matches!(event, WindowEvent::Destroyed) {
            self.active.store(false, Ordering::Release);
            self.adapter = None;
            return;
        }
        self.scale = _window.scale_factor();
        if let WindowEvent::Focused(focused) = event {
            self.focused = *focused;
            if let Some(adapter) = &self.adapter {
                if let Err(error) = adapter.set_host_focus_state(*focused) {
                    log::warn!("OHOS accessibility focus: {error}");
                }
            }
        }
    }

    pub fn update_if_active(&mut self, update_factory: impl FnOnce() -> TreeUpdate) {
        self.attach();
        let Some(adapter) = &self.adapter else { return };
        let matched = self
            .app
            .with_xcomponent_for(self.window_id.into_raw() as i64, |component| {
                component.raw().raw_handle() as usize == self.component
            });
        if matched == Some(true) {
            if let Err(error) = adapter
                .update_if_active(|| super::physical_tree_update(update_factory(), self.scale))
            {
                log::warn!("OHOS accessibility update: {error}");
            }
        } else {
            self.active.store(false, Ordering::Release);
            self.adapter = None;
        }
    }
}

impl Drop for PlatformAdapter {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
        self.adapter = None;
    }
}
