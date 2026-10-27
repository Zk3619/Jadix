//! cargo run -p jadix_components --example desktop_shell --features macros,jadix/native-menu
use jadix_components::app::{
    close_window, current_window,
    native_menu::{native_menu_supported, set_window_menu, NativeMenuItem as Item},
    open_secondary_window_with, request_exit, shared_slot, windows,
};
use jadix_components::prelude::*;

fn open_window() {
    if let Err(error) = open_secondary_window_with(
        WindowOptions::new()
            .title("Desktop shell · secondary")
            .size(480.0, 320.0)
            .min_size(320.0, 240.0)
            .frameless()
            .resize_border(5.0)
            .max_size(1200.0, 900.0),
        window_ui,
    ) {
        eprintln!("{error}");
    }
}

fn window_ui() -> Element {
    let window = current_window().expect("window exists before building its UI");
    let count = shared_slot("desktop-shell-count", || 0u32);
    let installed = use_signal(&format!("desktop-shell-menu-{:?}", window.id()), || false);
    if native_menu_supported() && !installed.peek() {
        let menu = vec![Item::submenu(
            "File",
            vec![
                Item::command("new", "New Window", open_window).accelerator("CmdOrCtrl+N"),
                Item::command("increment", "Increment", move || count.update(|n| *n += 1))
                    .accelerator("CmdOrCtrl+I"),
                Item::Separator,
                Item::command("close", "Close Window", close_window).accelerator("CmdOrCtrl+W"),
                Item::command("quit", "Quit", request_exit).accelerator("CmdOrCtrl+Q"),
            ],
        )];
        match set_window_menu(&window, menu) {
            Ok(()) => installed.set(true),
            Err(error) => eprintln!("{error}"),
        }
    }
    let content = ui! {
        <VStack gap="1rem" padding="1.5rem" flex_grow="1">
            <Label>{format!("Shared count: {}", count.get())}</Label>
            <Button on_click={move || count.update(|n| *n += 1)}>"Increment"</Button>
            <Button on_click={open_window}>"New window"</Button>
            <Button on_click={move || {
                if let Some(primary) = windows().first() { let _ = primary.focus(); }
            }}>"Focus primary"</Button>
            <Button on_click={close_window}>"Close this window"</Button>
        </VStack>
    };
    Element::new()
        .class("flex flex-col w-full h-full")
        .child(title_bar("Desktop shell"))
        .child(content)
}

fn main() {
    app()
        .title("Desktop shell · primary")
        .size(520.0, 400.0)
        .min_size(320.0, 240.0)
        .frameless()
        .resize_border(5.0)
        .mount(window_ui);
}
