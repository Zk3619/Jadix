#![allow(non_snake_case)]

use ui_runtime::prelude::*;

#[test]
fn renamed_runtime_builders_keep_media_and_portal_available() {
    let (_owner, ()) = ui_runtime::core::reactivity::create_root(|| {
        let video_node: Element = video().id("external-video").rotation("90").into();
        let portal_node: Element = portal("external-portal")
            .mount("external-host")
            .child(video_node)
            .into();
        assert_eq!(portal_node.template_id.as_deref(), Some("external-portal"));
        assert_eq!(
            portal_node
                .get_attr("jadix_video_rotation")
                .map(String::as_str),
            Some("90")
        );
        assert_eq!(
            portal_node
                .get_attr("jadix_portal_mount")
                .map(String::as_str),
            Some("external-host")
        );
        let audio_node: Element = audio().id("external-audio").into();
        assert_eq!(
            audio_node.get_attr("data-media-kind").map(String::as_str),
            Some("audio")
        );
    });
}

#[cfg(feature = "macros")]
#[ui_runtime::component(crate_path = "ui_runtime")]
fn Caption(value: String) -> Element {
    text(value).into()
}

#[cfg(feature = "macros")]
#[test]
fn renamed_runtime_retains_optional_props_and_primitive_macros() {
    let (_owner, ()) = ui_runtime::core::reactivity::create_root(|| {
        let caption = Caption(CaptionProps {
            value: Some("external-caption".into()),
            ..Default::default()
        });
        assert_eq!(caption.text.as_deref(), Some("external-caption"));
        let portal_node = ui_runtime::ui! {
            <Portal id="external-macro-portal" mount="external-host">
                <Video id="external-macro-video" rotation={MediaRotation::Clockwise90} />
            </Portal>
        };
        assert_eq!(
            portal_node
                .get_attr("jadix_video_rotation")
                .map(String::as_str),
            Some("90")
        );
        assert_eq!(
            portal_node.get_attr("jadix_portal").map(String::as_str),
            Some("1")
        );
    });
}
