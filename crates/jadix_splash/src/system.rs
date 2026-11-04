use std::sync::Arc;
use winit::window::Window;

pub(crate) fn failed(window: Option<&dyn Window>, message: &str) {
    if let Some(window) = window {
        window.set_title(&format!("Startup failed (Enter to retry): {message}"));
    }
    #[cfg(target_arch = "wasm32")]
    if let Some(document) = web_sys::window().and_then(|window| window.document()) {
        if let Some(status) = document.get_element_by_id("status") {
            status.set_text_content(Some(message));
            let _ = status.remove_attribute("hidden");
        }
        if let Some(startup) = document.get_element_by_id("jadix-startup") {
            let _ = startup.set_attribute("hidden", "");
        }
        if let Some(body) = document.body() {
            let _ = body.set_attribute("data-state", "error");
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    log::error!("startup cannot draw: {message}");
}

pub(crate) fn handoff(content: bool) {
    #[cfg(target_arch = "wasm32")]
    if let Some(document) = web_sys::window().and_then(|window| window.document()) {
        if let Some(status) = document.get_element_by_id("jadix-startup") {
            let _ = status.set_attribute("hidden", "");
        }
        if let Some(status) = document.get_element_by_id("status") {
            let _ = status.set_attribute("hidden", "");
        }
        if let Some(body) = document.body() {
            let _ = body.set_attribute(
                "data-startup",
                if content { "content" } else { "component" },
            );
            let _ = body.set_attribute("data-state", "started");
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = content;
}

pub(crate) struct Presentation {
    #[cfg(all(
        feature = "system",
        any(
            target_os = "windows",
            target_os = "macos",
            all(target_os = "linux", not(target_env = "ohos"))
        )
    ))]
    surface: softbuffer::Surface<Arc<dyn Window>, Arc<dyn Window>>,
    #[cfg(all(
        feature = "system",
        any(
            target_os = "windows",
            target_os = "macos",
            all(target_os = "linux", not(target_env = "ohos"))
        )
    ))]
    icon: Option<jadix::app::AppIcon>,
}

impl Presentation {
    pub(crate) fn redraw(&mut self) -> Result<(), String> {
        #[cfg(all(
            feature = "system",
            any(
                target_os = "windows",
                target_os = "macos",
                all(target_os = "linux", not(target_env = "ohos"))
            )
        ))]
        {
            let window = self.surface.window().clone();
            let size = window.surface_size();
            let (Some(width), Some(height)) = (
                std::num::NonZeroU32::new(size.width),
                std::num::NonZeroU32::new(size.height),
            ) else {
                return Ok(());
            };
            self.surface
                .resize(width, height)
                .map_err(|error| error.to_string())?;
            let mut buffer = self
                .surface
                .buffer_mut()
                .map_err(|error| error.to_string())?;
            let background = jadix::core::tree::parse_color("background");
            let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u32;
            let color = (channel(background[0]) << 16)
                | (channel(background[1]) << 8)
                | channel(background[2]);
            buffer.fill(color);
            let icon_size = ((64.0 * window.scale_factor()).round() as u32).max(1);
            let left = size.width.saturating_sub(icon_size) / 2;
            let top = size.height.saturating_sub(icon_size) / 2;
            let stripe = (3.0 * window.scale_factor()).round().max(1.0) as u32;
            for vertical in 0..icon_size.min(size.height) {
                for horizontal in 0..icon_size.min(size.width) {
                    let offset = ((top + vertical) * size.width + left + horizontal) as usize;
                    if let Some(pixel) = buffer.get_mut(offset) {
                        if let Some(icon) = &self.icon {
                            let source_x = horizontal * icon.width() / icon_size;
                            let source_y = vertical * icon.height() / icon_size;
                            let source = ((source_y * icon.width() + source_x) * 4) as usize;
                            let rgba = &icon.rgba()[source..source + 4];
                            let alpha = rgba[3] as u32;
                            let blend = |foreground: u8, shift: u32| {
                                ((foreground as u32 * alpha
                                    + ((color >> shift) & 255) * (255 - alpha))
                                    / 255)
                                    << shift
                            };
                            *pixel = blend(rgba[0], 16) | blend(rgba[1], 8) | blend(rgba[2], 0);
                        } else if horizontal < stripe
                            || vertical < stripe
                            || horizontal + stripe >= icon_size
                            || vertical + stripe >= icon_size
                        {
                            *pixel = 0x004f_8cff;
                        }
                    }
                }
            }
            window.pre_present_notify();
            buffer.present().map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

pub(crate) fn present(
    window: Arc<dyn Window>,
    icon: Option<&jadix::app::AppIcon>,
) -> Result<Option<Presentation>, String> {
    #[cfg(all(
        feature = "system",
        any(
            target_os = "windows",
            target_os = "macos",
            all(target_os = "linux", not(target_env = "ohos"))
        )
    ))]
    {
        let context =
            softbuffer::Context::new(window.clone()).map_err(|error| error.to_string())?;
        let surface = softbuffer::Surface::new(&context, window.clone())
            .map_err(|error| error.to_string())?;
        let mut presentation = Presentation {
            surface,
            icon: icon.cloned(),
        };
        presentation.redraw()?;
        window.set_visible(true);
        log::info!("startup system=cpu-buffer");
        return Ok(Some(presentation));
    }
    #[cfg(not(all(
        feature = "system",
        any(
            target_os = "windows",
            target_os = "macos",
            all(target_os = "linux", not(target_env = "ohos"))
        )
    )))]
    {
        let _ = (window, icon);
        if cfg!(any(
            target_os = "android",
            target_os = "ios",
            target_env = "ohos"
        )) {
            log::info!("startup system=host-launch-resources");
            Ok(None)
        } else if cfg!(target_arch = "wasm32") {
            log::info!("startup system=html-host");
            Ok(None)
        } else {
            Err("Enable the jadix_splash/system feature for native CPU presentation".into())
        }
    }
}

impl jadix::app::startup::StartupPresentation for Presentation {
    fn redraw(&mut self) -> Result<(), String> {
        Presentation::redraw(self)
    }
}
