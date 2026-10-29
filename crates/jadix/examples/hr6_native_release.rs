use jadix::core::tree::Element;

fn scene() -> Element {
    let mut rows = Element::new().w(600.0).flex_col();
    for index in 0..120 {
        rows = rows.child(
            Element::new()
                .w(580.0)
                .h(24.0)
                .flex_shrink(0.0)
                .bg(if index % 2 == 0 { "#eeeeee" } else { "#ffffff" })
                .text(format!("HR6 native row {index:03}: 0000")),
        );
    }
    let viewport = jadix::core::scroll_viewport::activate_scroll_viewport(
        Element::new().w(620.0).h(460.0).child(rows),
        "hr6-cost-scroll",
        "hidden",
        "scroll",
        "jump",
        "scrollbar",
    );
    Element::new()
        .w(640.0)
        .h(480.0)
        .bg("#ffffff")
        .child(viewport)
}

fn main() {
    jadix::app::app().size(640.0, 480.0).run(scene);
}
