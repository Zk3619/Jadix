use jadix_2d::*;
#[test]
fn morphology_matches_independent_2d_window_including_canvas_edges() {
    let bytes: Vec<u8> = (0..7 * 5)
        .flat_map(|i| {
            let a = ((i * 37 + 29) % 256) as u8;
            [a / 2, a / 3, a / 4, a]
        })
        .collect();
    for radius in [[0, 0], [1, 0], [0, 2], [1, 2], [8, 6], [u32::MAX, 1]] {
        for operator in [Morphology::Dilate, Morphology::Erode] {
            let mut c = RasterCanvas::new(7, 5);
            c.push_layer_with_options(LayerOptions {
                filters: vec![LayerFilter::Morphology { radius, operator }],
                ..Default::default()
            });
            c.put_image_data(0, 0, 7, 5, &bytes);
            let original = c.buf.clone();
            c.pop_layer();
            for y in 0..5i32 {
                for x in 0..7i32 {
                    for k in 0..4 {
                        let mut value = if operator == Morphology::Dilate {
                            0f32
                        } else {
                            1f32
                        };
                        for dy in -(radius[1].min(5) as i32)..=radius[1].min(5) as i32 {
                            for dx in -(radius[0].min(7) as i32)..=radius[0].min(7) as i32 {
                                let (xx, yy) = (x + dx, y + dy);
                                let sample = if (0..7).contains(&xx) && (0..5).contains(&yy) {
                                    original[((yy * 7 + xx) * 4) as usize + k]
                                } else {
                                    0.
                                };
                                value = if operator == Morphology::Dilate {
                                    value.max(sample)
                                } else {
                                    value.min(sample)
                                };
                            }
                        }
                        assert!(
                            (c.buf[((y * 7 + x) * 4) as usize + k] - value).abs() < 0.000001,
                            "{radius:?} {operator:?} {x}/{y}/{k}"
                        );
                    }
                }
            }
        }
    }
}
#[test]
fn fractional_offset_and_shadow_only_are_distinct_from_content_over_shadow() {
    for shadow in [false, true] {
        let mut c = RasterCanvas::new_linear(8, 8);
        c.push_layer_with_options(LayerOptions {
            filters: vec![if shadow {
                LayerFilter::DropShadowOnly {
                    sigma: [0.; 2],
                    offset: [1.5, 0.],
                    color: [1., 0., 0., 0.5],
                }
            } else {
                LayerFilter::Offset { offset: [1.5, 0.] }
            }],
            ..Default::default()
        });
        c.put_image_data(2, 2, 1, 1, &[0, 0, 255, 128]);
        c.pop_layer();
        let pixel = |x| c.get_image_data(x, 2, 1, 1).unwrap();
        assert_eq!(pixel(2), vec![0; 4]);
        assert_eq!(
            pixel(3),
            if shadow {
                vec![255, 0, 0, 32]
            } else {
                vec![0, 0, 255, 64]
            }
        );
        assert_eq!(pixel(4), pixel(3));
    }
}
