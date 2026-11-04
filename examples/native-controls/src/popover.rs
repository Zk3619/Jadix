use super::*;

pub fn page(count: Signal<usize>, value: Signal<String>) -> Element {
    let main = ui! {
        <Popover id="Profile">
            <PopoverTrigger id="profile-trigger"><Button id="profile-button" h={44.0}>"Edit profile"</Button></PopoverTrigger>
            <PopoverContent side="bottom" align="start" width={280.0}>
                <PopoverHeader><PopoverTitle>"Profile details"</PopoverTitle><PopoverDescription>"Native 中文 😀"</PopoverDescription></PopoverHeader>
                <Input id="profile-name" value={value} placeholder="Profile name" h={44.0} />
                <Button id="save-profile" on_click={popover_close} h={44.0}>"Save profile"</Button>
                <Popover id="Details">
                    <PopoverTrigger id="details-trigger"><Button id="details-button" h={44.0}>"More options"</Button></PopoverTrigger>
                    <PopoverContent side="right" align="start" width={180.0}>
                        <PopoverTitle>"Nested details"</PopoverTitle>
                        <Button id="apply-option" on_click={move || { count.update(|value| *value += 1); popover_close(); }} h={44.0}>"Apply option"</Button>
                    </PopoverContent>
                </Popover>
            </PopoverContent>
        </Popover>
    };
    let other = popover("Other").child(|| {
        let trigger = PopoverTrigger(PopoverTriggerProps {
            id: "other-trigger".into(),
            children: Some(super::label("Other panel", 44.0)),
            ..Default::default()
        });
        let panel = popover_content(
            PopoverContentOpts {
                side: "right".into(),
                align: "start".into(),
                width: Some("220".into()),
                ..Default::default()
            },
            move || {
                let field: Element = input("other-name")
                    .value_signal(value)
                    .placeholder("Other field")
                    .into();
                let close: Element = button("close-other")
                    .on_click(popover_close)
                    .child(text("Close other"))
                    .into();
                Element::new()
                    .gap(8.0)
                    .child(field.h(44.0))
                    .child(close.h(44.0))
            },
        );
        Element::new().child(trigger).child(panel)
    });
    let bottom = popover("Bottom").child(|| {
        let trigger = PopoverTrigger(PopoverTriggerProps {
            id: "bottom-trigger".into(),
            children: Some(super::label("Bottom panel", 44.0)),
            ..Default::default()
        });
        let panel = popover_content(
            PopoverContentOpts {
                side: "bottom".into(),
                align: "end".into(),
                width: Some("260".into()),
                ..Default::default()
            },
            || {
                button("close-bottom")
                    .on_click(popover_close)
                    .child(text("Close bottom"))
                    .into()
            },
        );
        Element::new().child(trigger).child(panel)
    });
    let background: Element = button("background-action")
        .on_click(move || count.update(|value| *value += 1))
        .child(text("Background action"))
        .into();
    let reset: Element = button("reset-name")
        .on_click(move || value.set("Reset 中文 😀".into()))
        .child(text("Reset name"))
        .into();
    scroll_area("popover-scroll").child(
        Element::new()
            .p(24.0)
            .gap(12.0)
            .flex_shrink(0.0)
            .child(super::label("Native Popover controls", 28.0))
            .child(
                jadix_components::app::reactive::bind_text("count", move || {
                    format!("Count: {} / Name: {}", count.get(), value.get())
                })
                .h(44.0),
            )
            .child(background.h(44.0))
            .child(main)
            .child(other)
            .child(reset.h(44.0))
            .child(Element::new().h(650.0).flex_shrink(0.0))
            .child(bottom)
            .child(Element::new().h(20.0)),
    )
}
