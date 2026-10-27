use jadix_components::components::{
    scroll_viewport, scrollbar, ScrollViewportBuilder, ScrollbarBuilder, ScrollbarOrientation,
};
use jadix_components::core::reactivity::{create_root, use_signal};
use jadix_components::core::scroll_viewport::{
    activate_scroll_viewport, scroll_behavior_signal_key, scroll_content_h_signal_key,
    scroll_viewport_h_signal_key,
};
use jadix_components::core::scrollbar::scrollbar_visibility_signal_key;
use jadix_components::core::tree::Element;

#[test]
fn viewport_component_and_core_activation_share_the_scroll_contract() {
    let (_owner, _) = create_root(|| {
        let builder: ScrollViewportBuilder = scroll_viewport("component-scroll")
            .w(200.0)
            .h(120.0)
            .overflow_x("hidden")
            .overflow_y("auto")
            .scroll_behavior("jump");
        let component: Element = builder.into();
        let raw = activate_scroll_viewport(
            Element::new().w(200.0).h(120.0),
            "raw-scroll",
            "hidden",
            "auto",
            "jump",
            "scrollbar",
        );
        assert_eq!(component.style.size, raw.style.size);
        assert_eq!(component.style.overflow, raw.style.overflow);
        assert_eq!(
            component.focus_id.as_deref(),
            Some("scroll:component-scroll")
        );
        assert_eq!(raw.focus_id.as_deref(), Some("scroll:raw-scroll"));
        assert!(component.children.is_empty());
        assert_eq!(
            use_signal(&scroll_behavior_signal_key("component-scroll"), String::new).peek(),
            "jump"
        );
    });
}

#[test]
fn scrollbar_component_keeps_target_geometry_and_visibility_contracts() {
    let (_owner, _) = create_root(|| {
        use_signal(&scroll_viewport_h_signal_key("scroll-target"), || 120.0_f32);
        use_signal(&scroll_content_h_signal_key("scroll-target"), || 480.0_f32);
        let builder: ScrollbarBuilder = scrollbar("bar")
            .target("scroll-target")
            .orientation(ScrollbarOrientation::Vertical)
            .thickness(12.0)
            .rect()
            .always();
        let track: Element = builder.into();
        assert_eq!(
            track.focus_id.as_deref(),
            Some("scrollbar-track:scroll-target:y")
        );
        assert_eq!(track.children.len(), 1);
        assert_eq!(
            track.children[0].focus_id.as_deref(),
            Some("scrollbar-thumb:scroll-target:y")
        );
        assert_eq!(
            track.style.size.width,
            taffy::style::Dimension::length(12.0)
        );
        assert_eq!(
            track.style.size.height,
            taffy::style::Dimension::length(120.0)
        );
        assert_eq!(
            use_signal(
                &scrollbar_visibility_signal_key("scroll-target", "y"),
                String::new
            )
            .peek(),
            "always"
        );
        let auto: Element = scrollbar("unmeasured-bar")
            .target("unmeasured")
            .auto()
            .into();
        assert_eq!(auto.children.len(), 1);
        let never: Element = scrollbar("hidden-bar").never().into();
        assert!(never.children.is_empty());
        assert_eq!(never.style.size.width, taffy::style::Dimension::length(0.0));
        assert_eq!(
            never.style.size.height,
            taffy::style::Dimension::length(0.0)
        );
    });
}

#[test]
fn scroll_component_tags_remain_available_from_prelude() {
    use jadix_components::prelude::*;
    let (_owner, _) = create_root(|| {
        let viewport = ui! {
            <ScrollViewport id="tag-scroll" overflow_y="auto" scroll_behavior="jump">
                <Box h={480.0} />
            </ScrollViewport>
        };
        let bar = ui! {
            <Scrollbar target="tag-scroll" orientation="vertical" visibility="always" thickness={12.0} />
        };
        assert_eq!(viewport.focus_id.as_deref(), Some("scroll:tag-scroll"));
        assert_eq!(
            bar.focus_id.as_deref(),
            Some("scrollbar-track:tag-scroll:y")
        );
    });
}

#[test]
fn arrow_switches_work_in_both_scroll_component_tags() {
    use jadix_components::prelude::*;
    let (_owner, ()) = create_root(|| {
        let bar = ui! {
            <Scrollbar target="arrows" orientation="horizontal" visibility="always" arrows={true}
                arrow_class="hover:bg-primary" />
        };
        assert_eq!(bar.children.len(), 3);
        let area = ui! {
            <ScrollArea id="arrows" w={240.0} h={160.0} scrollbar_arrows={true}
                scrollbar_arrow_class="hover:bg-primary">
                <Div w={600.0} h={700.0} />
            </ScrollArea>
        };
        assert!(jadix_components::core::tree::find_in_tree(&area, |el| el
            .get_attr("data-slot")
            .map(String::as_str)
            == Some("scrollbar-arrow-end"))
        .is_some());
    });
}

#[test]
fn product_viewports_default_to_smooth_and_allow_explicit_jump() {
    use jadix_components::prelude::*;
    let (_owner, ()) = create_root(|| {
        for behavior in ["smooth", "jump"] {
            let list: Element = if behavior == "jump" {
                virtual_list("default-list").scroll_behavior("jump").into()
            } else {
                virtual_list("default-list").into()
            };
            let masonry: Element = if behavior == "jump" {
                virtual_masonry("default-masonry")
                    .scroll_behavior("jump")
                    .into()
            } else {
                virtual_masonry("default-masonry").into()
            };
            let messages = if behavior == "jump" {
                ui! { <MessageScrollerProvider id="default-messages">
                    <MessageScrollerViewport scroll_behavior="jump" />
                </MessageScrollerProvider> }
            } else {
                ui! { <MessageScrollerProvider id="default-messages">
                    <MessageScrollerViewport />
                </MessageScrollerProvider> }
            };
            for root in [&list, &masonry, &messages] {
                let viewport = jadix_components::core::tree::find_in_tree(root, |node| {
                    node.focus_id
                        .as_deref()
                        .is_some_and(|id| id.starts_with("scroll:"))
                })
                .unwrap();
                let id = viewport
                    .focus_id
                    .as_deref()
                    .unwrap()
                    .strip_prefix("scroll:")
                    .unwrap();
                let keys =
                    jadix_components::core::scroll_viewport::ScrollViewportSignalKeys::new(id);
                assert_eq!(
                    use_signal(keys.behavior.as_str(), String::new).peek(),
                    behavior
                );
                use_signal(keys.viewport_h.as_str(), || 100.0_f32).set_silent(100.0);
                use_signal(keys.content_h.as_str(), || 500.0_f32).set_silent(500.0);
                use_signal(keys.offset_y.as_str(), || 0.0_f32).set_silent(0.0);
                use_signal(keys.target_y.as_str(), || 0.0_f32).set_silent(0.0);
                jadix_components::authoring::input::scroll_by(id, 0.0, 40.0);
                assert_eq!(use_signal(keys.target_y.as_str(), || 0.0_f32).peek(), 40.0);
                assert_eq!(
                    use_signal(keys.offset_y.as_str(), || 0.0_f32).peek(),
                    if behavior == "smooth" { 0.0 } else { 40.0 }
                );
            }
        }
    });
}
