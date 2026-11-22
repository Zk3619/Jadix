//! Adjacent panel resizing shared by retained and GPU components.
/// Preserve the pair total and satisfy both panels' limits. Invalid geometry
/// or an infeasible pair returns None instead of producing NaN or a jump.
pub fn resize_pair(start: [f64; 2], delta: f64, limits: [[f64; 2]; 2]) -> Option<[f64; 2]> {
    if !start.into_iter().chain([delta]).chain(limits.into_iter().flatten()).all(f64::is_finite)
        || start.iter().any(|value| *value < 0.)
        || limits.iter().any(|[min, max]| *min < 0. || min > max) { return None; }
    let total = start[0] + start[1];
    let lower = limits[0][0].max(total - limits[1][1]);
    let upper = limits[0][1].min(total - limits[1][0]);
    if !total.is_finite() || lower > upper { return None; }
    let first = (start[0] + delta).clamp(lower, upper);
    Some([first, total - first])
}
