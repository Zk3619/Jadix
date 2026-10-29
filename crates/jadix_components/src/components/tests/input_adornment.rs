use super::*;
use crate::core::reactivity::create_root;

#[test]
fn align_parse() {
    assert_eq!(
        InputAdornmentAlign::parse("inline-end"),
        InputAdornmentAlign::InlineEnd
    );
    assert_eq!(
        InputAdornmentAlign::parse("block-start"),
        InputAdornmentAlign::BlockStart
    );
    assert_eq!(
        InputAdornmentAlign::parse(""),
        InputAdornmentAlign::InlineStart
    );
}

#[test]
fn dual_surface_compiles_shape() {
    create_root(|| {
        let rsx = InputAdornment(InputAdornmentProps {
            id: String::new(),
            children: Some({
                let mut frag = Element::new();
                frag.set_attr("jadix_fragment", "1");
                frag.children(vec![
                    InputAdornmentAddon(InputAdornmentAddonProps {
                        align: Some("inline-start".into()),
                        children: Some(InputAdornmentText(InputAdornmentTextProps {
                            children: Some(Element::new().text("https://")),
                            base: None,
                            ..Default::default()
                        })),
                        base: None,
                        ..Default::default()
                    }),
                    InputAdornmentInput(InputAdornmentInputProps {
                        id: "t-ia-in".into(),
                        placeholder: Some("example.com".into()),
                        ..Default::default()
                    }),
                ])
            }),
            base: None,
            ..Default::default()
        });
        let built: Element = input_adornment()
            .addon(
                "inline-start",
                InputAdornmentText(InputAdornmentTextProps {
                    children: Some(Element::new().text("https://")),
                    base: None,
                    ..Default::default()
                }),
            )
            .child(
                input("t-ia-b")
                    .placeholder("example.com")
                    .plain_chrome(true),
            )
            .into();
        assert_eq!(
            rsx.attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_ROOT)
        );
        assert_eq!(
            built.attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_ROOT)
        );
    });
}

#[test]
fn reorder_puts_inline_start_before_control() {
    create_root(|| {
        let el: Element = InputAdornment(InputAdornmentProps {
            id: String::new(),
            children: Some({
                let mut frag = Element::new();
                frag.set_attr("jadix_fragment", "1");
                // DOM: control first, addon after (matches shadcn); visual order still start → control
                frag.children(vec![
                    InputAdornmentInput(InputAdornmentInputProps {
                        id: "t-ia-order".into(),
                        ..Default::default()
                    }),
                    InputAdornmentAddon(InputAdornmentAddonProps {
                        align: Some("inline-start".into()),
                        children: Some(Element::new().text("pre")),
                        base: None,
                        ..Default::default()
                    }),
                ])
            }),
            base: None,
            ..Default::default()
        });
        let row = el.children.first().expect("shell child");
        assert!(
            row.children.len() >= 2,
            "expected addon + control, got {}",
            row.children.len()
        );
        assert_eq!(
            row.children[0].attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_ADDON)
        );
        assert_eq!(
            row.children[1].attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_CONTROL)
        );
    });
}
