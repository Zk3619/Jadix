//! Wrapping focus movement shared by retained and GPU component adapters.
pub fn roving_move(len: usize, current: usize, delta: i32) -> Option<usize> {
    if len == 0 { return None; }
    Some(((current.min(len - 1) as i128 + delta as i128).rem_euclid(len as i128)) as usize)
}
pub fn roving_edge(len: usize, to_end: bool) -> Option<usize> {
    (len > 0).then(|| if to_end { len - 1 } else { 0 })
}
pub fn roving_move_skipping(len: usize, current: usize, delta: i32, disabled: impl Fn(usize) -> bool) -> Option<usize> {
    if len == 0 { return None; }
    let enabled: Vec<_> = (0..len).filter(|index| !disabled(*index)).collect();
    if enabled.is_empty() { return None; }
    let current = current.min(len - 1);
    let forward = delta >= 0;
    let first = if forward { enabled.iter().position(|index| *index > current).unwrap_or(0) }
        else { enabled.iter().rposition(|index| *index < current).unwrap_or(enabled.len() - 1) };
    let remaining = delta.unsigned_abs().max(1) as i128 - 1;
    let next = (first as i128 + if forward { remaining } else { -remaining }).rem_euclid(enabled.len() as i128);
    Some(enabled[next as usize])
}
