//! Shared declarative appearance timing used by native platform adapters.
use crate::{animation::cubic_bezier_y_at_x, Spring, SpringPhysics};
use jadix_core::presentation::style::{ViewAnimation, ViewKeyframes, ViewTransition};

pub const VISUAL_SAMPLES: usize = 256;

pub fn transition_curve(transition: &ViewTransition) -> Option<(f32, Vec<f32>)> {
    if !transition.is_valid() { return None; }
    let spring = transition.spring.map(|s| Spring::from_physics(0., 1., 0., SpringPhysics {
        stiffness:s.stiffness, damping:s.damping, mass:s.mass,
    }).with_rest(0.008, 0.0008));
    let duration = spring.as_ref().map_or(transition.duration, Spring::calculated_duration_ms);
    let [x1,y1,x2,y2] = transition.easing;
    let samples = (0..=VISUAL_SAMPLES).map(|i| {
        if i == 0 { return 0.; }
        if i == VISUAL_SAMPLES { return 1.; }
        let t=i as f32/VISUAL_SAMPLES as f32;
        spring.as_ref().map_or_else(||cubic_bezier_y_at_x(t,x1,y1,x2,y2), |s|s.next(t*duration).value)
    }).collect();
    Some((duration,samples))
}

/// Rotation degrees, uniform scale, opacity multiplier and vertical height fraction.
pub fn keyframe_curve(animation: &ViewAnimation) -> Option<Vec<[f32;4]>> {
    if !animation.is_valid() { return None; }
    let [x1,y1,x2,y2]=animation.easing;
    Some((0..=VISUAL_SAMPLES).map(|i| {
        let phase=i as f32/VISUAL_SAMPLES as f32;
        if (i==0 || i==VISUAL_SAMPLES) && matches!(animation.keyframes,ViewKeyframes::Pulse|ViewKeyframes::Bounce) { return [0.,1.,1.,0.]; }
        let ease=|t|cubic_bezier_y_at_x(t,x1,y1,x2,y2);
        match animation.keyframes {
            ViewKeyframes::Spin => [360.*ease(phase),1.,1.,0.],
            ViewKeyframes::Ping => { let t=ease((phase/0.75).min(1.)); [0.,1.+t,1.-t,0.] },
            ViewKeyframes::Pulse => { let t=if phase<0.5 {phase*2.} else {(1.-phase)*2.}; [0.,1.,1.-0.5*cubic_bezier_y_at_x(t,0.4,0.,0.6,1.),0.] },
            ViewKeyframes::Bounce => { let t=if phase<0.5 {cubic_bezier_y_at_x(phase*2.,0.8,0.,1.,1.)} else {1.-cubic_bezier_y_at_x((phase-0.5)*2.,0.,0.,0.2,1.)}; [0.,1.,1.,-0.25*t] },
        }
    }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spring_overshoots_then_settles_and_keyframes_return_to_rest() {
        let transition=ViewTransition {spring:Some(Default::default()),..Default::default()};
        let (duration,curve)=transition_curve(&transition).unwrap();
        assert!(duration>0. && curve.iter().all(|v|v.is_finite()));
        assert!(curve.iter().any(|v|*v>1.));
        assert_eq!(curve[0],0.); assert_eq!(curve[VISUAL_SAMPLES],1.);
        for kind in [ViewKeyframes::Pulse,ViewKeyframes::Bounce] {
            let curve=keyframe_curve(&ViewAnimation{keyframes:kind,..Default::default()}).unwrap();
            assert_eq!(curve[0],[0.,1.,1.,0.]); assert_eq!(curve[VISUAL_SAMPLES],[0.,1.,1.,0.]);
        }
    }
}
