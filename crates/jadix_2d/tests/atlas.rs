use jadix_2d::*;
fn bitmap(w: u32, h: u32, color: [u8; 4]) -> Image2D {
    Image2D::from_rgba8(w, h, &color.repeat((w * h) as usize)).unwrap()
}
#[test]
fn atlas_padding_pages_sampling_and_shared_identity() {
    let images: Vec<_> = (0..9)
        .map(|i| bitmap(2, 2, [i * 20, 255 - i * 20, 64, 128]))
        .collect();
    let atlas = ImageAtlas::pack(
        &images,
        AtlasLimits {
            page_size: 8,
            max_pages: 3,
            max_entries: 9,
        },
    )
    .unwrap();
    assert_eq!(atlas.pages().len(), 3);
    assert_eq!(atlas.byte_len(), 3 * 8 * 8 * 4);
    assert_eq!(atlas.clone().pages(), atlas.pages());
    for (i, r) in atlas.regions().iter().enumerate() {
        let [x, y, w, h] = r.rect;
        let pixels = atlas.pages()[r.page].rgba8();
        for yy in y - 1..=y + h {
            for xx in x - 1..=x + w {
                let at = ((yy * 8 + xx) * 4) as usize;
                assert_eq!(
                    &pixels[at..at + 4],
                    if xx >= x && yy >= y && xx < x + w && yy < y + h {
                        &images[i].rgba8()[..4]
                    } else {
                        &[0; 4]
                    }
                );
            }
        }
    }
    let mut c = RasterCanvas::new_linear(8, 8);
    c.set_fill("#ff0000");
    atlas.draw(&mut c, 3, [2., 2., 2., 2.], ImageSampling::Nearest);
    c.fill_rect(6., 6., 1., 1.);
    let pixels = c.get_image_data(0, 0, 8, 8).unwrap();
    assert_eq!(
        &pixels[(2 * 8 + 2) * 4..(2 * 8 + 2) * 4 + 4],
        &images[3].rgba8()[..4]
    );
    assert_eq!(
        &pixels[(6 * 8 + 6) * 4..(6 * 8 + 6) * 4 + 4],
        &[255, 0, 0, 255]
    );
}
#[test]
fn atlas_budgets_and_invalid_destinations() {
    let limits = AtlasLimits {
        page_size: 8,
        max_pages: 1,
        max_entries: 4,
    };
    assert_eq!(
        ImageAtlas::pack(&[bitmap(7, 1, [255; 4])], limits).unwrap_err(),
        AtlasError::EntryTooLarge
    );
    assert_eq!(
        ImageAtlas::pack(&vec![bitmap(6, 6, [255; 4]); 2], limits).unwrap_err(),
        AtlasError::BudgetExceeded
    );
    assert_eq!(
        ImageAtlas::pack(&vec![bitmap(1, 1, [255; 4]); 5], limits).unwrap_err(),
        AtlasError::BudgetExceeded
    );
    assert_eq!(
        ImageAtlas::pack(
            &[],
            AtlasLimits {
                page_size: 0,
                ..limits
            }
        )
        .unwrap_err(),
        AtlasError::InvalidLimits
    );
    let atlas = ImageAtlas::pack(&[bitmap(2, 2, [255; 4])], limits).unwrap();
    let mut c = RasterCanvas::new_linear(8, 8);
    for dest in [
        [0., 0., -1., 2.],
        [f32::NAN, 0., 2., 2.],
        [0., 0., f32::INFINITY, 2.],
    ] {
        atlas.draw(&mut c, 0, dest, ImageSampling::Bilinear);
    }
    atlas.draw(&mut c, 100, [0., 0., 2., 2.], ImageSampling::Bilinear);
    assert!(c.into_pixels().iter().all(|v| *v == 0.));
}

#[test]
fn cropped_image_rectangles_validate_and_replay_without_new_bitmaps() {
    let image = Image2D::from_rgba8(
        3,
        2,
        &[
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255, 0, 255, 255, 255,
            255, 0, 255, 255,
        ],
    )
    .unwrap();
    let mut c = RasterCanvas::new_linear(8, 8);
    let mut g = GpuCanvas::new_linear(8, 8, None);
    for target in [&mut c as &mut dyn Canvas2D, &mut g as &mut dyn Canvas2D] {
        target.draw_image(
            &CanvasImageSource::Region {
                image: &image,
                rect: [1, 0, 1, 2],
            },
            2.,
            2.,
            1.,
            2.,
        );
        for rect in [[3, 0, 1, 1], [0, 0, 0, 1], [u32::MAX, 0, 2, 1]] {
            target.draw_image(
                &CanvasImageSource::Region {
                    image: &image,
                    rect,
                },
                0.,
                0.,
                8.,
                8.,
            );
        }
    }
    let actual = c.get_image_data(0, 0, 8, 8).unwrap();
    assert_eq!(
        &actual[(2 * 8 + 2) * 4..(2 * 8 + 2) * 4 + 4],
        &[0, 255, 0, 255]
    );
    assert_eq!(
        &actual[(3 * 8 + 2) * 4..(3 * 8 + 2) * 4 + 4],
        &[0, 255, 255, 255]
    );
    assert_eq!(c.get_image_data(0, 0, 8, 8), g.get_image_data(0, 0, 8, 8));
    let ops = g.into_ops();
    assert_eq!(ops.len(), 1);
    let PathGpuOp::Image(op) = &ops[0] else {
        panic!("image op");
    };
    assert_eq!(op.image, image);
    assert_eq!(op.source_rect, Some([1, 0, 1, 2]));
}
