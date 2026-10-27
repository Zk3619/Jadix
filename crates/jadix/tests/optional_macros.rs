use jadix::prelude::*;

#[test]
fn builder_media_and_portal_work_without_ui_authoring() {
    let (_owner, ()) = jadix::core::reactivity::create_root(|| {
        let audio_node: Element = audio().id("builder-audio").into();
        assert_eq!(
            audio_node.get_attr("data-media-kind").map(String::as_str),
            Some("audio")
        );
        let video_node: Element = video().id("builder-video").rotation("90").into();
        assert_eq!(
            video_node
                .get_attr("jadix_video_rotation")
                .map(String::as_str),
            Some("90")
        );
        let portal_node: Element = portal("builder-portal")
            .mount("named-host")
            .child(text("portal-content"))
            .into();
        assert_eq!(portal_node.template_id.as_deref(), Some("builder-portal"));
        assert_eq!(
            portal_node.get_attr("jadix_portal").map(String::as_str),
            Some("1")
        );
        assert_eq!(
            portal_node
                .get_attr("jadix_portal_mount")
                .map(String::as_str),
            Some("named-host")
        );
    });
}

#[cfg(feature = "macros")]
#[test]
fn optional_macro_surface_retains_media_and_portal_props() {
    let (_owner, ()) = jadix::core::reactivity::create_root(|| {
        let macro_audio = ui! { <Audio id="macro-audio" /> };
        let builder_audio: Element = audio().id("macro-audio").into();
        for (name, value) in &builder_audio.attrs {
            assert_eq!(macro_audio.attrs.get(name), Some(value));
        }
        assert_eq!(macro_audio.template_id, builder_audio.template_id);
        let macro_video = ui! { <Video id="macro-video" rotation={MediaRotation::Clockwise90} /> };
        let builder_video: Element = video().id("macro-video").rotation("90").into();
        for (name, value) in &builder_video.attrs {
            assert_eq!(macro_video.attrs.get(name), Some(value));
        }
        assert_eq!(macro_video.template_id, builder_video.template_id);
        let macro_portal = ui! { <Portal id="macro-portal" mount="named-host">"child"</Portal> };
        let builder_portal: Element = portal("macro-portal")
            .mount("named-host")
            .child(text("child"))
            .into();
        for (name, value) in &builder_portal.attrs {
            assert_eq!(macro_portal.attrs.get(name), Some(value));
        }
        assert_eq!(macro_portal.template_id, builder_portal.template_id);
        assert_eq!(macro_portal.style.size, builder_portal.style.size);
    });
}

#[test]
fn portal_builder_preserves_empty_single_and_multiple_children() {
    let empty: Element = portal("empty").into();
    assert_eq!(
        empty.get_attr("jadix_portal").map(String::as_str),
        Some("1")
    );
    assert!(empty.children.is_empty());
    let single: Element = portal("single").child(text("single-content")).into();
    assert_eq!(single.text.as_deref(), Some("single-content"));
    let multiple: Element = portal("multiple")
        .child(text("first"))
        .child(text("second"))
        .into();
    assert_eq!(multiple.children.len(), 2);
    assert_eq!(multiple.children[0].text.as_deref(), Some("first"));
    assert_eq!(multiple.children[1].text.as_deref(), Some("second"));
}
