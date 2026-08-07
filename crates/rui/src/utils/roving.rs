//! Roving tabindex helpers for lists / menus (Stage 7 keyboard nav).
//!
//! Store the active index in a signal; widgets call [`roving_move`] on Arrow keys.

/// Move selection within `0..len`, wrapping at ends. Returns `None` if `len == 0`.
pub fn roving_move(len: usize, current: usize, delta: i32) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let n = len as i32;
    let cur = (current.min(len - 1)) as i32;
    let next = (cur + delta).rem_euclid(n);
    Some(next as usize)
}

/// Home / End targets.
pub fn roving_edge(len: usize, to_end: bool) -> Option<usize> {
    if len == 0 {
        None
    } else if to_end {
        Some(len - 1)
    } else {
        Some(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_and_edges() {
        assert_eq!(roving_move(3, 0, -1), Some(2));
        assert_eq!(roving_move(3, 2, 1), Some(0));
        assert_eq!(roving_edge(5, false), Some(0));
        assert_eq!(roving_edge(5, true), Some(4));
        assert_eq!(roving_move(0, 0, 1), None);
    }
}
