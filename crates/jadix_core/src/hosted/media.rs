use super::MediaFit;

impl MediaFit {
    /// Destination rectangle and normalized source rectangle, both in x/y/width/height
    /// form. Unknown dimensions use the full frame; invalid target bounds are rejected.
    pub fn geometry(self, target: [f64; 4], source: [f64; 2]) -> Option<([f64; 4], [f64; 4])> {
        if !target.iter().all(|v| v.is_finite()) || target[2] <= 0.0 || target[3] <= 0.0 {
            return None;
        }
        let full = [0.0, 0.0, 1.0, 1.0];
        if !source.iter().all(|v| v.is_finite() && *v > 0.0) { return Some((target, full)); }
        let ratio = source[0] / source[1];
        let target_ratio = target[2] / target[3];
        // Ratios can overflow or underflow even for finite, positive dimensions.
        // Compare logarithms only for that extreme range; keep ordinary pixel math exact.
        if !ratio.is_finite() || !target_ratio.is_finite() || ratio == 0.0 || target_ratio == 0.0 {
            let difference = (source[0].ln() - source[1].ln()) - (target[2].ln() - target[3].ln());
            let fraction = (-difference.abs()).exp();
            return match self {
                Self::Fill => Some((target, full)),
                Self::Cover => Some((target, if difference > 0.0 { [(1.0-fraction)/2.0, 0.0, fraction, 1.0] } else { [0.0, (1.0-fraction)/2.0, 1.0, fraction] })),
                Self::Contain => {
                    let (width, height) = if difference > 0.0 { (target[2], target[3]*fraction) } else { (target[2]*fraction, target[3]) };
                    let rect = [target[0]+(target[2]-width)/2.0, target[1]+(target[3]-height)/2.0, width, height];
                    rect.iter().all(|v| v.is_finite()).then_some((rect, full))
                }
            };
        }
        match self {
            Self::Fill => Some((target, full)),
            Self::Contain => {
                let (width, height) = if ratio > target_ratio {
                    (target[2], target[2] / ratio)
                } else { (target[3] * ratio, target[3]) };
                let rect = [target[0] + (target[2] - width) / 2.0,
                    target[1] + (target[3] - height) / 2.0, width, height];
                rect.iter().all(|v| v.is_finite()).then_some((rect, full))
            }
            Self::Cover => {
                let (width, height) = if ratio > target_ratio { (target_ratio / ratio, 1.0) }
                    else { (1.0, ratio / target_ratio) };
                Some((target, [(1.0 - width) / 2.0, (1.0 - height) / 2.0, width, height]))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extreme_finite_aspect_ratios_never_produce_nan() {
        for source in [[f64::MAX, f64::MIN_POSITIVE], [f64::MIN_POSITIVE, f64::MAX]] {
            for target in [[0.,0.,f64::MAX,f64::MIN_POSITIVE], [0.,0.,f64::MIN_POSITIVE,f64::MAX], [0.,0.,100.,100.]] {
                for fit in [MediaFit::Fill, MediaFit::Cover, MediaFit::Contain] {
                    let (destination, crop) = fit.geometry(target, source).unwrap();
                    assert!(destination.into_iter().chain(crop).all(f64::is_finite));
                    assert!((0.0..=1.0).contains(&crop[2]) && (0.0..=1.0).contains(&crop[3]));
                }
            }
        }
    }
    #[test]
    fn video_fit_preserves_aspect_and_centers_crop_with_nonzero_origins() {
        assert_eq!(MediaFit::Contain.geometry([10., 20., 100., 100.], [200., 100.]),
            Some(([10., 45., 100., 50.], [0., 0., 1., 1.])));
        assert_eq!(MediaFit::Cover.geometry([10., 20., 100., 100.], [200., 100.]),
            Some(([10., 20., 100., 100.], [0.25, 0., 0.5, 1.])));
        assert_eq!(MediaFit::Cover.geometry([0., 0., 100., 100.], [100., 200.]).unwrap().1,
            [0., 0.25, 1., 0.5]);
        assert_eq!(MediaFit::Fill.geometry([0., 0., 100., 100.], [200., 100.]).unwrap().0,
            [0., 0., 100., 100.]);
        assert!(MediaFit::Contain.geometry([0., 0., f64::NAN, 2.], [1., 1.]).is_none());
        assert!(MediaFit::Cover.geometry([0.; 4], [1., 1.]).is_none());
        assert!(MediaFit::Contain.geometry([f64::MAX, f64::MAX, f64::MAX, f64::MAX], [2., 1.]).is_none());
        assert_eq!(MediaFit::Contain.geometry([0., 0., 2., 3.], [0., 0.]).unwrap().1, [0., 0., 1., 1.]);
    }
}
