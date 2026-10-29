use super::*;
use crate::prelude::{Label, LabelProps};

#[test]
fn title_bar_keeps_controls_outside_drag_region_and_supports_both_surfaces() {
    crate::init();
    let builder: Element = title_bar("Editor")
        .child(Element::new().text("Action"))
        .into();
    let rsx = crate::ui! { <TitleBar title="Editor"><Label>"Action"</Label></TitleBar> };
    for bar in [builder, rsx] {
        assert_eq!(bar.children.len(), 5);
        assert!(
            bar.on_mousedown.is_none(),
            "content controls must never bubble into a drag handler"
        );
        assert!(bar.children[0].on_mousedown.is_some());
        assert!(bar.children[1].on_mousedown.is_none());
        for (index, label) in [
            (2, "Minimize window"),
            (3, "Maximize or restore window"),
            (4, "Close window"),
        ] {
            assert_eq!(
                bar.children[index]
                    .attrs
                    .get("aria-label")
                    .map(String::as_str),
                Some(label)
            );
        }
    }
}
