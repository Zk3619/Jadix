#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    #[cfg(target_arch = "wasm32")]
    {
        struct BrowserLog;
        impl log::Log for BrowserLog {
            fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
                metadata.level() <= log::max_level()
            }
            fn log(&self, record: &log::Record<'_>) {
                if self.enabled(record.metadata()) {
                    let message = format!("{}: {}", record.level(), record.args()).into();
                    if record.level() == log::Level::Error {
                        web_sys::console::error_1(&message);
                    } else {
                        web_sys::console::log_1(&message);
                    }
                }
            }
            fn flush(&self) {}
        }
        let _ = log::set_logger(&BrowserLog);
        let verbose = web_sys::window()
            .and_then(|window| window.location().search().ok())
            .is_some_and(|query| query.contains("log=debug"));
        log::set_max_level(if verbose {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Info
        });
        std::panic::set_hook(Box::new(|info| {
            web_sys::console::error_1(&info.to_string().into());
        }));
    }
    jadix::utils::font::register_font_data(
        include_bytes!("../../../crates/jadix_cosmic_text/fonts/InterVariable.ttf").to_vec(),
    )
    .expect("bundled Inter font");
    jadix::utils::font::register_font_data(include_bytes!("../assets/gallery-sans.otf").to_vec())
        .expect("bundled Gallery CJK font");
    for bytes in [
        include_bytes!("../../../crates/jadix_cosmic_text/fonts/FiraMono-Medium.ttf").as_slice(),
        include_bytes!("../../../crates/jadix_cosmic_text/fonts/NotoSansArabic.ttf").as_slice(),
        include_bytes!("../../../crates/jadix_cosmic_text/fonts/NotoSansHebrew.ttf").as_slice(),
        include_bytes!("../assets/NotoSansThai.ttf").as_slice(),
        include_bytes!("../assets/NotoSansDevanagari.ttf").as_slice(),
        include_bytes!("../../../crates/jadix/assets/NotoSansMath-Regular.ttf").as_slice(),
        include_bytes!("../../../crates/jadix/assets/NotoSansSymbols2-Regular.ttf").as_slice(),
        include_bytes!("../../../crates/jadix/assets/NotoColorEmoji.ttf").as_slice(),
    ] {
        jadix::utils::font::register_font_data(bytes.to_vec()).expect("bundled fallback font");
    }
    gallery::run_gallery_web();
}
