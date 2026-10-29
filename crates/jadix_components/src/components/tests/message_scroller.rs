use super::*;

#[test]
fn message_navigation_defaults_to_smooth_and_retains_explicit_jump() {
    crate::core::reactivity::create_root(|| {
        let viewport = "message-navigation-policy";
        let keys = crate::core::scroll_viewport::ScrollViewportSignalKeys::new(viewport);
        set_scroll_y(viewport, 180.0, false);
        assert_eq!(use_signal(&keys.offset_y, || 0.0_f32).peek(), 0.0);
        assert_eq!(use_signal(&keys.target_y, || 0.0_f32).peek(), 180.0);
        set_scroll_y(viewport, 240.0, true);
        assert_eq!(use_signal(&keys.offset_y, || 0.0_f32).peek(), 240.0);
        use_signal(&keys.behavior, || String::new()).set_silent("jump".into());
        set_scroll_y(viewport, 300.0, false);
        assert_eq!(use_signal(&keys.offset_y, || 0.0_f32).peek(), 300.0);
    });
}

#[test]
fn latest_button_has_a_filled_default_and_accepts_explicit_variants() {
    crate::core::reactivity::create_root(|| {
        for mode in [
            crate::style::ThemeMode::Light,
            crate::style::ThemeMode::Dark,
        ] {
            crate::style::set_theme_mode(mode);
            let button = message_scroller_scope(
                "filled-latest",
                true,
                MessageScrollerDefaultPosition::End,
                DEFAULT_EDGE_THRESHOLD,
                DEFAULT_PREVIOUS_PEEK,
                || Element::from(message_scroller_button()),
            );
            assert!(
                button.children[0].style.bg_color[3] > 0.0,
                "{mode:?}: Latest needs a background"
            );
            assert_eq!(jump_button_variant(""), ButtonVariant::Secondary);
            assert_eq!(jump_button_variant("outline"), ButtonVariant::Outline);
        }
        crate::style::set_theme_mode(crate::style::ThemeMode::Light);
    });
}

#[test]
fn viewport_builder_style_sticks() {
    let el = message_scroller_scope(
        "test-ms",
        true,
        MessageScrollerDefaultPosition::End,
        DEFAULT_EDGE_THRESHOLD,
        DEFAULT_PREVIOUS_PEEK,
        || {
            message_scroller_viewport()
                .style(|b| b.text_size(11.0))
                .into()
        },
    );
    assert_eq!(el.text_style.size_resolved(), 11.0);
}
