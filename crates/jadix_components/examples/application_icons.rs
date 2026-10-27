//! cargo run -p jadix --example application_icons --features system-tray
use jadix_components::app::{current_window, tray_icon, AppIcon, TrayIcon};
use jadix_components::prelude::*;
use std::cell::RefCell;

thread_local! {
    static TRAY: RefCell<Option<TrayIcon>> = const { RefCell::new(None) };
}

fn color_icon(color: [u8; 4]) -> AppIcon {
    AppIcon::from_rgba(vec![color; 32 * 32].into_iter().flatten().collect(), 32, 32)
        .expect("valid RGBA icon")
}

fn update_icon(icon: AppIcon) {
    let window = current_window().expect("UI window");
    if let Err(error) = window.set_icon(Some(icon.clone())) {
        eprintln!("window icon: {error}");
    }
    TRAY.with(|slot| {
        if let Some(tray) = slot.borrow().as_ref() {
            if let Err(error) = tray.set_icon(icon) {
                eprintln!("tray icon: {error}");
            } else {
                eprintln!("native tray icon updated");
            }
        }
    });
}

fn scene() -> Element {
    let initialized = use_signal("application-icons-initialized", || false);
    if !initialized.peek() {
        initialized.set(true);
        match tray_icon()
            .icon(color_icon([37, 99, 235, 255]))
            .tooltip("Jadix icon demo")
            .try_build()
        {
            Ok(tray) => {
                eprintln!("native tray created: {}", tray.is_native());
                TRAY.with(|slot| *slot.borrow_mut() = Some(tray));
            }
            Err(error) => eprintln!("tray: {error}"),
        }
    }
    ui! {
        <VStack w="100%" h="100%" p="1.5rem" gap="1rem" bg="background">
            <Label text_size="1.25rem" font_weight="600">"Window and tray icons"</Label>
            <Label>"The caption and tray share one decoded AppIcon."</Label>
            <Button on_click={|| update_icon(color_icon([249, 115, 22, 255]))}>"Orange icon"</Button>
            <Button on_click={|| update_icon(color_icon([37, 99, 235, 255]))}>"Reset blue icon"</Button>
            <Button variant="outline" on_click={|| {
                if let Some(path) = jadix_components::utils::file_dialog::pick_file(".png,.ico,.webp,.jpg") {
                    match AppIcon::from_path(path) {
                        Ok(icon) => update_icon(icon),
                        Err(error) => eprintln!("icon: {error}"),
                    }
                }
            }}>"Choose image..."</Button>
        </VStack>
    }
}

fn main() {
    app()
        .title("Jadix application icons")
        .size(520.0, 340.0)
        .icon(color_icon([37, 99, 235, 255]))
        .mount(scene);
}
