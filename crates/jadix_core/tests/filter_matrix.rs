use jadix_core::presentation::style::ViewFilter;

fn pixel(filter: ViewFilter, rgba: [f32; 4]) -> [f32; 4] {
    let m = filter.color_matrix().unwrap();
    std::array::from_fn(|r| (0..4).map(|c| m[r*5+c]*rgba[c]).sum::<f32>()+m[r*5+4])
}

fn near(left: [f32; 4], right: [f32; 4]) {
    for (a,b) in left.into_iter().zip(right) { assert!((a-b).abs()<0.002,"{left:?} != {right:?}"); }
}

#[test]
fn color_filters_preserve_alpha_and_use_normalized_offsets() {
    near(pixel(ViewFilter::Invert(1.), [0.2,0.6,1.,0.5]),[0.8,0.4,0.,0.5]);
    near(pixel(ViewFilter::Contrast(0.), [0.2,0.6,1.,0.5]),[0.5,0.5,0.5,0.5]);
    near(pixel(ViewFilter::Grayscale(1.), [1.,0.,0.,0.5]),[0.2126,0.2126,0.2126,0.5]);
    near(pixel(ViewFilter::Opacity(0.2), [1.,0.4,0.,0.5]),[1.,0.4,0.,0.1]);
}

#[test]
fn filter_angles_are_radians_and_spatial_operations_remain_ordered() {
    near(pixel(ViewFilter::HueRotate(std::f32::consts::TAU),[0.2,0.6,1.,0.5]),[0.2,0.6,1.,0.5]);
    near(pixel(ViewFilter::Saturate(1.),[0.2,0.6,1.,0.5]),[0.2,0.6,1.,0.5]);
    near(pixel(ViewFilter::Sepia(0.),[0.2,0.6,1.,0.5]),[0.2,0.6,1.,0.5]);
    assert!(ViewFilter::Blur(2.).color_matrix().is_none());
}

#[test]
fn progressive_backdrop_reverses_and_validates_its_local_ramp() {
    use jadix_core::presentation::style::{ViewAppearance, ViewProgressiveBlur};
    let mut ramp = ViewProgressiveBlur { start: 0.2, end: 0.8, min_mix: 0.1 };
    assert!(ramp.is_valid());
    assert!((ramp.mix(0.5) - 0.55).abs() < 0.001);
    assert_eq!(ramp.mix(0.), 0.1);
    assert_eq!(ramp.mix(1.), 1.);
    std::mem::swap(&mut ramp.start, &mut ramp.end);
    assert_eq!(ramp.mix(0.), 1.);
    assert_eq!(ramp.mix(1.), 0.1);
    ramp.end = ramp.start;
    assert_eq!(ramp.mix(ramp.start), 1.);
    ramp.min_mix = f32::NAN;
    assert!(!ViewAppearance { backdrop_progressive: Some(ramp), ..Default::default() }.is_valid());
}

#[test]
fn progressive_samples_preserve_coverage_and_the_requested_radius() {
    use jadix_core::presentation::style::ViewProgressiveBlur;
    for (start,end) in [(0.2,0.8),(0.8,0.2),(0.5,0.5),(0.,0.),(1.,1.)] {
        for min_mix in [0.,0.17,0.5,1.] {
            let ramp=ViewProgressiveBlur { start,end,min_mix };
            let samples=ramp.sample_masks().unwrap();
            assert!(samples.len()<=9);
            for i in 0..=100 {
                let y=i as f32/100.;
                let weights:Vec<_>=samples.iter().map(|(strength,stops)| {
                    let before=stops.iter().rposition(|stop| stop[0]<=y).unwrap();
                    let a=stops[before];
                    let weight=if let Some(b)=stops.get(before+1) {a[1]+(b[1]-a[1])*(y-a[0])/(b[0]-a[0])}else{a[1]};
                    (*strength,weight)
                }).collect();
                let coverage:f32=weights.iter().map(|(_,w)| w).sum();
                let radius:f32=weights.iter().map(|(s,w)| s*w).sum();
                assert!((coverage-1.).abs()<0.00001,"{ramp:?} y={y} coverage={coverage}");
                assert!((radius-ramp.mix(y)).abs()<0.00001,"{ramp:?} y={y} radius={radius}");
            }
        }
    }
    near(pixel(ViewFilter::Contrast(0.).at_strength(0.),[0.2,0.6,1.,0.5]),[0.2,0.6,1.,0.5]);
    assert_eq!(ViewFilter::Blur(20.).at_strength(0.5),ViewFilter::Blur(10.));
}
