use super::*;

#[test]
#[cfg(feature = "animation-base")]
fn toast_enter_opacity_tracks_translation_and_scale_on_every_frame() {
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    let mut card = Element::new();
    card.style.transition = toast_motion_entries(550.0, ToastCssPhase::Enter).to_vec();
    card.style.translate = [0.0, 100.0];
    card.style.scale = [0.8; 2];
    card.style.opacity = 0.0;
    let _ = card.tick_css_transitions(0.0);
    for frame in 0..=90 {
        crate::style::set_css_transition_timeline_ms(frame as f64 * 1000.0 / 120.0);
        card.style.translate = [0.0; 2];
        card.style.scale = [1.0; 2];
        card.style.opacity = 1.0;
        let _ = card.tick_css_transitions(0.0);
        let position_progress = 1.0 - card.style.translate[1] / 100.0;
        let scale_progress = (card.style.scale[0] - 0.8) / 0.2;
        assert!(
            (card.style.opacity - position_progress).abs() < 0.001,
            "opacity lag at frame {frame}"
        );
        assert!(
            (card.style.opacity - scale_progress).abs() < 0.001,
            "scale lag at frame {frame}"
        );
    }
    assert_eq!(card.style.opacity, 1.0);
    crate::style::set_css_transition_virtual_timeline(false);
}
