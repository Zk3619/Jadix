use jadix_components::prelude::*;

pub fn page(count: Signal<usize>, value: Signal<String>) -> Element {
    ui! {
        <VStack>
            <Text>"Portable Jadix"</Text>
            <Button id="increment" on_click={move || count.set(count.get() + 1)}>"Continue"</Button>
            <Input id="name" value={value} placeholder="Name" />
        </VStack>
    }
}

#[allow(
    clippy::useless_conversion,
    reason = "shared source also targets the GPU builder type"
)]
pub fn builder_page(count: Signal<usize>, value: Signal<String>) -> Element {
    vstack()
        .child(text("Portable Jadix"))
        .child(
            button("increment")
                .on_click(move || count.set(count.get() + 1))
                .child(text("Continue")),
        )
        .child(input("name").value_signal(value).placeholder("Name"))
        .into()
}
