//! Host-independent paging and snap selection in logical document points.
use serde::{Deserialize, Serialize};

/// Retains subpixel input when a platform scrolls in whole physical pixels.
/// Each axis owns its remainder; callers reset it when the scroll owner changes.
#[derive(Clone, Copy, Debug, Default)]
pub struct ScrollRemainder([f64; 2]);
impl ScrollRemainder {
    pub fn pixels(&mut self, axis: usize, delta: f64) -> i32 {
        let Some(remainder) = self.0.get_mut(axis) else { return 0; };
        if !delta.is_finite() { return 0; }
        let total = (*remainder + delta).clamp(i32::MIN as f64, i32::MAX as f64);
        let whole = total.trunc();
        *remainder = total - whole;
        whole as i32
    }
}

#[cfg(test)]
mod fractional_input_tests {
    use super::ScrollRemainder;
    #[test]
    fn fine_input_preserves_distance_and_axis_independence() {
        let mut input = ScrollRemainder::default();
        assert_eq!((0..120).map(|_| input.pixels(1, 0.4)).sum::<i32>(), 48);
        assert_eq!(input.pixels(0, -0.75), 0);
        assert_eq!(input.pixels(1, f64::NAN), 0);
        assert_eq!(input.pixels(0, -0.75), -1);
        assert_eq!(input.pixels(0, 0.5), 0);
        assert_eq!(input.pixels(0, f64::MAX), i32::MAX);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapAlignment { #[default] Start, Center, End }

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScrollSnap {
    pub paging: bool,
    pub interval: Option<f64>,
    /// Sorted absolute offsets, bounded independently of the document length.
    pub offsets: Vec<f64>,
    pub alignment: SnapAlignment,
    /// Velocity retained each millisecond. None uses .998 for pages, .99 otherwise.
    pub deceleration: Option<f64>,
    pub one_step: bool,
}
impl ScrollSnap {
    pub fn is_valid(&self) -> bool {
        self.interval.is_none_or(|n| n.is_finite() && n > 0.0 && n <= 1e12)
            && self.offsets.len() <= 4096
            && self.offsets.iter().all(|n| n.is_finite() && (0.0..=1e12).contains(n))
            && self.offsets.windows(2).all(|p| p[0] <= p[1])
            && self.deceleration.is_none_or(|n| n.is_finite() && (0.0..1.0).contains(&n))
    }
    /// Bounded logarithmic search; tiny intervals never allocate a point per snap.
    /// Velocity is in document points/second, positive towards the end.
    pub fn target(&self, offset: f64, velocity: f64, viewport: f64, content: f64) -> Option<f64> {
        if !self.is_valid() || [offset,velocity,viewport,content].iter().any(|n| !n.is_finite() || n.abs()>1e12)
            || viewport <= 0.0 || content < 0.0 { return None; }
        let max=(content-viewport).max(0.0);
        let offset=offset.clamp(0.0,max);
        let (interval, shift) = if let Some(interval)=self.interval {
            (interval,match self.alignment { SnapAlignment::Start=>0.0,SnapAlignment::Center=>(viewport-interval)/2.0,SnapAlignment::End=>viewport-interval })
        } else if self.offsets.is_empty() && self.paging { (viewport,0.0) }
        else if self.offsets.is_empty() { return Some(offset); }
        else { (0.0,0.0) };
        let rate=self.deceleration.unwrap_or(if self.interval.is_some() || !self.offsets.is_empty(){0.99}else{0.998});
        let step=self.one_step || (self.paging && self.interval.is_none() && self.offsets.is_empty());
        let from=if step {offset} else { (offset+velocity/1000.0*rate/(1.0-rate)).clamp(-1e15,1e15) };
        let forward=velocity>300.0;
        let backward=velocity< -300.0;
        let boundary=if forward {from+if step {0.5}else{-0.5}} else if backward {from+if step {-0.5}else{0.5}}else{from};
        if interval > 0.0 {
            let coordinate=(boundary+shift)/interval;
            // Below representable index resolution the snap grid is continuous.
            // Do not cap the grid length: that incorrectly truncates long documents.
            if !coordinate.is_finite() || coordinate.abs() >= (1u64<<52) as f64 {
                return Some(boundary.clamp(0.0,max));
            }
            let last=((max+shift)/interval).ceil().max(0.0);
            let point=|index:f64| (index.clamp(0.0,last)*interval-shift).clamp(0.0,max);
            if backward && coordinate <= 0.0 { return Some(0.0); }
            return Some(if forward {point(if step {coordinate.floor()+1.0}else{coordinate.ceil()})}
                else if backward {point(if step {coordinate.ceil()-1.0}else{coordinate.floor()})}
                else {let a=point(coordinate.floor());let b=point(coordinate.ceil());if (from-a).abs()<=(b-from).abs(){a}else{b}});
        }
        let count=self.offsets.len() as u64;
        let point=|i:u64| self.offsets[i as usize].clamp(0.0,max);
        let mut lo=0;let mut hi=count;
        while lo<hi {
            let mid=lo+(hi-lo)/2;let p=point(mid);
            let before=if forward&&step {p<=boundary} else if backward&&!step {p<=boundary} else {p<boundary};
            if before {lo=mid+1}else{hi=mid}
        }
        if forward {Some(if lo<count {point(lo)}else{max})}
        else if backward {Some(if lo>0 {point(lo-1)}else{0.0})}
        else {
            let a=point(lo.saturating_sub(1));let b=point(lo.min(count-1));
            Some(if (from-a).abs()<=(b-from).abs(){a}else{b})
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paging_velocity_alignment_and_bounds() {
        let page=ScrollSnap{paging:true,..Default::default()};
        assert_eq!(page.target(120.0,8000.0,100.0,950.0),Some(200.0));
        assert_eq!(page.target(120.0,-8000.0,100.0,950.0),Some(100.0));
        assert_eq!(page.target(850.0,900.0,100.0,950.0),Some(850.0));
        assert_eq!(page.target(150.0,0.0,100.0,950.0),Some(100.0));
        let grid=ScrollSnap{interval:Some(60.0),alignment:SnapAlignment::Center,..Default::default()};
        assert_eq!(grid.target(45.0,0.0,100.0,950.0),Some(40.0));
        let points=ScrollSnap{offsets:vec![0.0,80.0,500.0],one_step:true,..Default::default()};
        assert_eq!(points.target(80.0,900.0,100.0,400.0),Some(300.0));
        assert_eq!(points.target(80.0,-900.0,100.0,400.0),Some(0.0));
    }
    #[test]
    fn malformed_and_extreme_inputs_are_bounded() {
        assert!(!ScrollSnap{offsets:vec![2.0,1.0],..Default::default()}.is_valid());
        assert!(!ScrollSnap{deceleration:Some(1.0),..Default::default()}.is_valid());
        assert!(!ScrollSnap{interval:Some(0.0),..Default::default()}.is_valid());
        let tiny=ScrollSnap{interval:Some(1e-6),..Default::default()};
        let value=tiny.target(1e6,999.0,100.0,1e12).unwrap();
        assert!((1e6..=1e12-100.0).contains(&value));
        let microscopic=ScrollSnap{interval:Some(1e-300),..Default::default()};
        assert_eq!(microscopic.target(1e6,0.0,100.0,1e12),Some(1e6));
        assert_eq!(tiny.target(f64::NAN,0.0,100.0,900.0),None);
        assert_eq!(tiny.target(0.0,0.0,0.0,900.0),None);
    }
}

/// Controlled refresh state; distances are logical pixels in the viewport.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RefreshControl {
    pub refreshing: bool,
    pub threshold: f64,
    pub inset: f64,
}
impl Default for RefreshControl {
    fn default() -> Self { Self { refreshing: false, threshold: 64.0, inset: 56.0 } }
}
impl RefreshControl {
    pub fn is_valid(self) -> bool {
        self.threshold.is_finite() && self.threshold > 0.0 && self.threshold <= 4096.0
            && self.inset.is_finite() && (0.0..=4096.0).contains(&self.inset)
    }
    pub fn release(self, distance: f64, cancelled: bool) -> bool {
        self.is_valid() && !self.refreshing && !cancelled && distance.is_finite() && distance >= self.threshold
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "camelCase", deny_unknown_fields)]
pub enum ScrollActivity {
    DragStart, DragEnd, MomentumStart, MomentumEnd,
    Pull { distance: f64 }, Refresh,
}
impl ScrollActivity {
    pub fn is_valid(self, refresh: Option<RefreshControl>) -> bool {
        match self {
            Self::Pull { distance } => refresh.is_some() && distance.is_finite() && (0.0..=8192.0).contains(&distance),
            Self::Refresh => refresh.is_some_and(|policy| policy.is_valid() && !policy.refreshing),
            _ => true,
        }
    }
}
#[cfg(test)]
mod refresh_tests {
    use super::*;
    #[test] fn refresh_cancel_and_controlled_state() {
        let policy = RefreshControl::default();
        assert!(policy.release(64.0, false));
        assert!(!policy.release(64.0, true));
        assert!(!policy.release(f64::NAN, false));
        assert!(!RefreshControl { refreshing: true, ..policy }.release(200.0, false));
        assert!(!ScrollActivity::Pull { distance: f64::INFINITY }.is_valid(Some(policy)));
        assert!(!ScrollActivity::Refresh.is_valid(None));
        let grid = ScrollSnap { interval: Some(200.0), alignment: SnapAlignment::End, one_step: true, ..Default::default() };
        assert_eq!(grid.target(0.0, -900.0, 100.0, 950.0), Some(0.0));
    }
}
