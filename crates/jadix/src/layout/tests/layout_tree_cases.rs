#[cfg(test)]
#[test]
fn captured_open_node_paint_ranges_are_empty_after_payload_drain() {
    begin_node_paint_collect();
    let mut element = Element::new().w(30.0).h(30.0);
    element.layout_bounds = [0.0, 0.0, 30.0, 30.0];
    let start = crate::layout::collect_cache::DlCursor {
        instances: 8,
        textured: 4,
        text: 16,
        chrome: 3,
        paint_ops: 9,
        ..Default::default()
    };
    record_node_paint(
        NodeId::from(42_u64),
        &element,
        start,
        &[],
        &[],
        &[],
        &[],
        &[],
        false,
        [0.0; 2],
        None,
        false,
    );
    let paints = take_node_paints();
    assert_eq!(paints.len(), 1);
    for range in [
        paints[0].instance,
        paints[0].textured,
        paints[0].text,
        paints[0].chrome,
        paints[0].paint_ops,
    ] {
        assert_eq!(
            range,
            (0, 0),
            "captured payload has no live main-pass range"
        );
    }
}

#[cfg(test)]
#[test]
fn shimmer_box_overlay_is_one_clipped_rounded_sweep() {
    let mut element = Element::new().rounded(8.0);
    crate::style::apply_class_list(&mut element, "shimmer-box shimmer-duration-1000");
    element.attrs.insert(
        crate::renderer::ATTR_SHIMMER_EPOCH.to_string(),
        (crate::style::global_time_s() - 0.5).to_string(),
    );
    for width in [40.0, 800.0, 8192.0] {
        let mut chrome = Vec::new();
        push_shimmer_box_overlay(
            &mut chrome,
            &element,
            [0.0, 0.0, width, 48.0],
            Some([10.0, 0.0, 200.0, 30.0]),
            crate::style::MAT2D_IDENTITY,
            0.75,
        );
        assert_eq!(chrome.len(), 1, "a sweep must not use overlapping slabs");
        let sweep = &chrome[0];
        assert_eq!(sweep.rect_size, [width, 48.0]);
        assert_eq!(sweep.radii, [8.0; 4]);
        assert_eq!(sweep.clip_pos, [10.0, 0.0]);
        assert_eq!(sweep.clip_size, [(width - 10.0).min(200.0), 30.0]);
        assert_eq!(sweep.glass[0], -1.0);
        for position in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let band = (1.0 - (position + sweep.glass[3] - sweep.glass[1]).abs() / sweep.glass[2])
                .max(0.0);
            let params = crate::renderer::ShimmerParams::box_from_attrs(&element.attrs).unwrap();
            let expected = crate::renderer::shimmer_box_mix_at(
                &params,
                crate::style::global_time_s(),
                position,
                width,
            );
            assert!((band * band - expected).abs() < 0.01);
        }
    }
}
