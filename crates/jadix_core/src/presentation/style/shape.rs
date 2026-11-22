//! Host-independent rounded outlines. Bounds and radii use the same units.

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(tag = "op", content = "points", rename_all = "camelCase")]
pub enum OutlineSegment {
    Move([f64; 2]),
    Line([f64; 2]),
    Cubic([f64; 6]),
    Close,
}

/// Fits elliptical corner radii using CSS's common scale factor, then constructs
/// circular or continuous corners. Continuous corners use 60% smoothing, reduced
/// locally when neighbouring corners consume the available edge.
pub fn rounded_outline(bounds: [f64; 4], mut radii: [f64; 8], continuous: bool) -> Option<Vec<OutlineSegment>> {
    if !bounds.iter().chain(radii.iter()).all(|v| v.is_finite())
        || radii.iter().any(|v| *v < 0.0) { return None; }
    let [x, y, width, height] = bounds;
    if width <= 0.0 || height <= 0.0 { return Some(Vec::new()); }
    if !(x + width).is_finite() || !(y + height).is_finite() { return None; }
    let mut fit = 1.0_f64;
    for (edge, a, b) in [(width,0,2), (width,6,4), (height,1,7), (height,3,5)] {
        // Divide first, so even very large finite radii cannot overflow their sum.
        let sum = radii[a] / edge + radii[b] / edge;
        if sum > 1.0 { fit = fit.min(1.0 / sum); }
    }
    for radius in &mut radii { *radius *= fit; }
    let share = |edge: f64, a: f64, b: f64| if a + b > 0.0 { edge * (a / (a + b)) } else { 0.0 };
    // Corner position, incoming/outgoing direction, incoming/outgoing radii,
    // and the edge budget allocated proportionally to adjacent corners.
    let corners = [
        ([x+width,y], [1.0,0.0], [0.0,1.0], radii[2],radii[3],share(width,radii[2],radii[0]),share(height,radii[3],radii[5])),
        ([x+width,y+height], [0.0,1.0], [-1.0,0.0], radii[5],radii[4],share(height,radii[5],radii[3]),share(width,radii[4],radii[6])),
        ([x,y+height], [-1.0,0.0], [0.0,-1.0], radii[6],radii[7],share(width,radii[6],radii[4]),share(height,radii[7],radii[1])),
        ([x,y], [0.0,-1.0], [1.0,0.0], radii[1],radii[0],share(height,radii[1],radii[7]),share(width,radii[0],radii[2])),
    ];
    let mut path = Vec::with_capacity(17);
    for (index, (point, incoming, outgoing, ru, rv, bu, bv)) in corners.into_iter().enumerate() {
        let radius = ru.max(rv);
        let mut add_start = |p| path.push(if index == 0 { OutlineSegment::Move(p) } else { OutlineSegment::Line(p) });
        if ru == 0.0 || rv == 0.0 { add_start(point); continue; }
        let (su, sv) = (ru/radius, rv/radius);
        let map = |a: f64, b: f64| [point[0]+incoming[0]*a*su+outgoing[0]*b*sv,
            point[1]+incoming[1]*a*su+outgoing[1]*b*sv];
        let budget = radius.max((bu/su).min(bv/sv));
        let smoothing = if continuous { (budget/radius-1.0).clamp(0.0,0.6) } else { 0.0 };
        let reach = ((1.0+smoothing)*radius).min(budget);
        let arc = std::f64::consts::FRAC_PI_2*(1.0-smoothing);
        let chord = (arc/2.0).sin()*radius*std::f64::consts::SQRT_2;
        let alpha = (std::f64::consts::FRAC_PI_2-arc)/2.0;
        let beta = std::f64::consts::FRAC_PI_4*smoothing;
        let c = radius*(alpha/2.0).tan()*beta.cos();
        let d = c*beta.tan();
        let b = ((reach-chord-c-d)/3.0).max(0.0);
        let a = 2.0*b;
        add_start(map(-reach,0.0));
        let mut cubic = |p: [f64;2], q: [f64;2], end: [f64;2]| path.push(OutlineSegment::Cubic([p[0],p[1],q[0],q[1],end[0],end[1]]));
        let transition = a+b+c > 1e-9*radius;
        if transition { cubic(map(-reach+a,0.0),map(-reach+a+b,0.0),map(-reach+a+b+c,d)); }
        let start = -std::f64::consts::FRAC_PI_4-arc/2.0;
        let end = -std::f64::consts::FRAC_PI_4+arc/2.0;
        let k = 4.0/3.0*(arc/4.0).tan()*radius;
        let p = [-radius+radius*start.cos(),radius+radius*start.sin()];
        let q = [-radius+radius*end.cos(),radius+radius*end.sin()];
        cubic(map(p[0]-k*start.sin(),p[1]+k*start.cos()),map(q[0]+k*end.sin(),q[1]-k*end.cos()),map(q[0],q[1]));
        if transition { cubic(map(0.0,reach-a-b),map(0.0,reach-a),map(0.0,reach)); }
    }
    path.push(OutlineSegment::Close);
    path.iter().all(|segment| match segment {
        OutlineSegment::Move(p)|OutlineSegment::Line(p)=>p.iter().all(|v|v.is_finite()),
        OutlineSegment::Cubic(p)=>p.iter().all(|v|v.is_finite()), OutlineSegment::Close=>true,
    }).then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn smoothing_fits_neighbours_and_degenerates_without_nan() {
        for radii in [[20.0;8],[1000.0,10.0,3.0,1000.0,0.0,0.0,50.0,50.0],[0.0;8]] {
            let path = rounded_outline([7.0,11.0,100.0,60.0],radii,true).unwrap();
            assert!(matches!(path.last(),Some(OutlineSegment::Close)));
            for segment in path { let points: &[f64] = match &segment {
                OutlineSegment::Move(p)|OutlineSegment::Line(p)=>p, OutlineSegment::Cubic(p)=>p, OutlineSegment::Close=>&[],
            }; for p in points.chunks_exact(2) { assert!(p[0].is_finite()&&p[1].is_finite()); assert!((7.0-1e-9..=107.0+1e-9).contains(&p[0])); assert!((11.0-1e-9..=71.0+1e-9).contains(&p[1])); } }
        }
        assert_ne!(rounded_outline([0.0,0.0,100.0,100.0],[10.0;8],true),rounded_outline([0.0,0.0,100.0,100.0],[10.0;8],false));
        assert!(rounded_outline([0.0,0.0,1.0,1.0],[f64::NAN;8],true).is_none());
        assert!(rounded_outline([0.0,0.0,0.0,1.0],[0.0;8],true).unwrap().is_empty());
    }
}
