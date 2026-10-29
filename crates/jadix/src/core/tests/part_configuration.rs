use super::*;

#[test]
fn configuration_binds_runtime_geometry_without_assembling_or_styling_a_widget() {
    let (_owner, _) = crate::core::reactivity::create_root(|| {
        for axis in ["x", "y"] {
            let target = format!("foundation-parts-{axis}");
            for key in [
                scroll_viewport_w_signal_key(&target),
                scroll_viewport_h_signal_key(&target),
            ] {
                use_signal(&key, || 100.0_f32);
            }
            for key in [
                scroll_content_w_signal_key(&target),
                scroll_content_h_signal_key(&target),
            ] {
                use_signal(&key, || 400.0_f32);
            }
            for key in [
                scroll_offset_x_signal_key(&target),
                scroll_offset_y_signal_key(&target),
            ] {
                use_signal(&key, || 50.0_f32);
            }
            let mut track = Element::new()
                .bg([0.1, 0.2, 0.3, 1.0])
                .border([0.4, 0.5, 0.6, 1.0], 2.0)
                .absolute()
                .right(3.0);
            let original_style = track.style.clone();
            let mut thumb = Element::new().bg([0.7, 0.8, 0.9, 1.0]);
            configure_scrollbar_parts(&mut track, &mut thumb, Some(&target), axis, "always", 10.0);
            assert_eq!(track.style.bg_color, original_style.bg_color);
            assert_eq!(track.style.border_width, original_style.border_width);
            assert_eq!(track.style.inset, original_style.inset);
            assert_eq!(thumb.style.bg_color, [0.7, 0.8, 0.9, 1.0]);
            assert!(track.children.is_empty());
            assert_eq!(
                track.focus_id.as_deref(),
                Some(format!("scrollbar-track:{target}:{axis}").as_str())
            );
            let (main, thumb_main, position) = if axis == "x" {
                (
                    track.style.size.width,
                    thumb.style.size.width,
                    thumb.style.margin.left,
                )
            } else {
                (
                    track.style.size.height,
                    thumb.style.size.height,
                    thumb.style.margin.top,
                )
            };
            assert_eq!(crate::layout::dim_length_px(main), Some(100.0));
            assert_eq!(crate::layout::dim_length_px(thumb_main), Some(25.0));
            assert!((crate::layout::lpa_length_px(position).unwrap() - 12.5).abs() < 0.001);
            assert_eq!(
                use_signal(&scrollbar_visibility_signal_key(&target, axis), String::new).peek(),
                "always"
            );
        }
    });
}

#[test]
fn unmeasured_parts_keep_finite_initial_geometry() {
    let (_owner, _) = crate::core::reactivity::create_root(|| {
        for target in [None, Some("unmeasured-foundation-parts")] {
            let mut track = Element::new();
            let mut thumb = Element::new();
            configure_scrollbar_parts(&mut track, &mut thumb, target, "y", "auto", 8.0);
            for size in [
                track.style.size.width,
                track.style.size.height,
                thumb.style.size.width,
                thumb.style.size.height,
            ] {
                let value = crate::layout::dim_length_px(size).unwrap();
                assert!(value.is_finite() && value > 0.0);
            }
        }
    });
}
