#![allow(non_snake_case)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Component gallery MVP：Button / Input / DatePicker / Command / Progress。

use rui::app::run;
use rui::components::*;
use rui::core::reactivity::use_signal;
use rui::core::tree::Element;
use rui::ui;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    run(|| {
        let cmd_open = use_signal("gallery_cmd_open", || false);
        let progress_val = use_signal("gallery_progress", || 0.45f32);

        let open_cmd = cmd_open;
        let bump = progress_val;

        let progress_el: Element = progress("gal-progress").value_signal(progress_val).into();
        let cmd_el: Element = command("gal-cmd")
            .open_signal(cmd_open)
            .action(CommandAction::new("hello", "Say hello", || {
                log::info!("gallery command: hello");
            }))
            .action(CommandAction::new("toast", "Log ping", || {
                log::info!("gallery command: ping");
            }))
            .into();
        let chart_el: Element = bar_chart("gal-bars")
            .values([0.2, 0.5, 0.8, 0.4, 0.9])
            .labels(["Mon", "Tue", "Wed", "Thu", "Fri"])
            .h("6rem")
            .into();

        ui! {
            <VStack w="100%" h="100%" p="1.5rem" gap="1.25rem" bg="background">
                <Label text_size="1.5rem" font_weight="700">{ "RUI Gallery" }</Label>
                <Label text_size="0.875rem" text_color="muted-foreground">{
                    "Button · Input · DatePicker · Command · Progress"
                }</Label>

                <VStack gap="0.5rem" w="100%" max_w="28rem">
                    <Label font_weight="600">{ "Button" }</Label>
                    <HStack gap="0.5rem">
                        <Button id="gal-btn-default">{ "Default" }</Button>
                        <Button id="gal-btn-outline" variant="outline">{ "Outline" }</Button>
                        <Button id="gal-btn-ghost" variant="ghost">{ "Ghost" }</Button>
                    </HStack>
                </VStack>

                <VStack gap="0.5rem" w="100%" max_w="28rem">
                    <Label font_weight="600">{ "Input" }</Label>
                    <Input id="gal-input" placeholder="Type here…" />
                </VStack>

                <VStack gap="0.5rem" w="100%" max_w="28rem">
                    <Label font_weight="600">{ "DatePicker" }</Label>
                    <DatePicker id="gal-date" kind="date" />
                    <TimePicker id="gal-time" />
                    <ColorPicker id="gal-color" value="#3b82f6" />
                </VStack>

                <VStack gap="0.5rem" w="100%" max_w="28rem">
                    <Label font_weight="600">{ "Progress" }</Label>
                    { progress_el }
                    <Button id="gal-progress-bump" variant="outline" on_click={move || {
                        let v = bump.peek();
                        bump.set(if v >= 1.0 { 0.0 } else { (v + 0.1).min(1.0) });
                    }}>{ "Bump +10%" }</Button>
                </VStack>

                <VStack gap="0.5rem" w="100%" max_w="28rem">
                    <Label font_weight="600">{ "Command" }</Label>
                    <Button id="gal-cmd-open" variant="secondary" on_click={move || {
                        open_cmd.set(true);
                    }}>{ "Open command palette" }</Button>
                    { cmd_el }
                </VStack>

                <VStack gap="0.5rem" w="100%" max_w="28rem">
                    <Label font_weight="600">{ "BarChart" }</Label>
                    { chart_el }
                </VStack>
            </VStack>
        }
    });
}
