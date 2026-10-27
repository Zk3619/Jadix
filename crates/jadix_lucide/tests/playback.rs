use jadix::prelude::*;
use jadix_lucide::*;

#[cfg(feature = "animated")]
#[test]
fn enabled_lucide_recipes_support_builder_macro_and_hover() {
    assert!(jadix_lucide::font_path().is_file());
    jadix_lucide::install().unwrap();
    assert!(animated_icon_names().count() >= 400);
    assert!(animated_icon_recipe("bell").is_some());
    let mut icon: Element = animated_icon("bell").into();
    (icon.on_hover.as_ref().unwrap().borrow_mut())(true);
    sync_icon_hover(&mut icon);
    assert_eq!(
        icon.attrs[jadix_lucide::ATTR_ANIMATED_ICON_STATE],
        "animate"
    );
    (icon.on_hover.as_ref().unwrap().borrow_mut())(false);
    sync_icon_hover(&mut icon);
    assert_eq!(icon.attrs[jadix_lucide::ATTR_ANIMATED_ICON_STATE], "normal");
    let controlled: Element = animated_icon("bell").controlled(true).into();
    #[cfg(feature = "macros")]
    {
        let macro_controlled = jadix_lucide::ui! { <AnimatedIcon name="bell" controlled={true} /> };
        assert_eq!(
            macro_controlled.attrs[jadix_lucide::ATTR_ANIMATED_ICON],
            "bell"
        );
        assert!(macro_controlled.on_hover.is_none());
    }
    assert_eq!(controlled.attrs[jadix_lucide::ATTR_ANIMATED_ICON], "bell");
    assert!(controlled.on_hover.is_none());
}
