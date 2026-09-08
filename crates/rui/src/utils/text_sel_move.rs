//! Input / Textarea: drag a non-empty selection to a new caret (Word-like move).
//!
//! Press inside the highlight, drag: a clone of the selected text follows the pointer
//! (offset down-right). Dropping inside the original range (or with an unchanged caret)
//! is a no-op (`not-allowed` cursor). Dropping elsewhere inserts the chunk before the
//! drop caret, leaves the caret at the end of the moved range, and clears the selection.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use crate::style::CursorKind;

const ARM_DIST_PX: f32 = 6.0;
const ARM_HOLD: Duration = Duration::from_millis(280);
const GHOST_DX: f32 = 14.0;
const GHOST_DY: f32 = 18.0;
const GHOST_MAX_CHARS: usize = 80;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Potential,
    Moving,
}

#[derive(Clone, Debug)]
struct Session {
    rest: String,
    sel: (usize, usize),
    text: String,
    press_xy: (f32, f32),
    press_at: Instant,
    press_caret: usize,
    phase: Phase,
    drop_caret: usize,
    allowed: bool,
    mouse: (f32, f32),
    size: f32,
    weight: u16,
    color: [u8; 4],
    line_height: f32,
}

#[derive(Clone, Debug)]
pub struct GhostPaint {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub weight: u16,
    pub color: [u8; 4],
    pub line_height: f32,
}

pub enum ReleaseAction {
    None,
    /// Click (no drag): collapse to this caret.
    Click {
        rest: String,
        caret: usize,
    },
    /// Commit the move.
    Move {
        rest: String,
        sel: (usize, usize),
        drop: usize,
    },
    /// Dragged but drop was illegal — keep the original selection.
    KeepSelection,
}

thread_local! {
    static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
}

pub fn begin_potential(
    rest: impl Into<String>,
    sel: (usize, usize),
    text: String,
    press_caret: usize,
    press_xy: (f32, f32),
    size: f32,
    weight: u16,
    color: [u8; 4],
    line_height: f32,
) {
    let lo = sel.0.min(sel.1);
    let hi = sel.0.max(sel.1);
    SESSION.with(|s| {
        *s.borrow_mut() = Some(Session {
            rest: rest.into(),
            sel: (lo, hi),
            text,
            press_xy,
            press_at: Instant::now(),
            press_caret,
            phase: Phase::Potential,
            drop_caret: press_caret,
            allowed: false,
            mouse: press_xy,
            size,
            weight,
            color,
            line_height,
        });
    });
}

pub fn is_active() -> bool {
    SESSION.with(|s| s.borrow().is_some())
}

/// True while a selection-move gesture owns the pointer (do not extend the selection).
pub fn blocks_select_drag() -> bool {
    is_active()
}

pub fn moving_drop_caret(rest: &str) -> Option<usize> {
    SESSION.with(|s| {
        s.borrow().as_ref().and_then(|st| {
            if st.phase == Phase::Moving && st.rest == rest {
                Some(st.drop_caret)
            } else {
                None
            }
        })
    })
}

pub fn active_cursor() -> Option<CursorKind> {
    SESSION.with(|s| {
        s.borrow().as_ref().and_then(|st| match st.phase {
            Phase::Potential => None,
            Phase::Moving => Some(if st.allowed {
                CursorKind::Alias
            } else {
                CursorKind::NotAllowed
            }),
        })
    })
}

pub fn ghost_paint() -> Option<GhostPaint> {
    SESSION.with(|s| {
        s.borrow().as_ref().and_then(|st| {
            if st.phase != Phase::Moving {
                return None;
            }
            let shown = truncate_ghost(&st.text);
            Some(GhostPaint {
                text: shown,
                x: st.mouse.0 + GHOST_DX,
                y: st.mouse.1 + GHOST_DY,
                size: st.size,
                weight: st.weight,
                color: st.color,
                line_height: st.line_height,
            })
        })
    })
}

fn truncate_ghost(text: &str) -> String {
    let mut out = String::new();
    for (i, ch) in text.chars().enumerate() {
        if i >= GHOST_MAX_CHARS {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

/// Update drop caret / arm Moving. Returns true if the overlay should redraw.
pub fn tick_pointer(mouse: (f32, f32), drop_caret: Option<usize>, value_len: usize) -> bool {
    SESSION.with(|s| {
        let mut slot = s.borrow_mut();
        let Some(st) = slot.as_mut() else {
            return false;
        };
        st.mouse = mouse;
        if let Some(d) = drop_caret {
            st.drop_caret = d.min(value_len);
        }
        st.allowed = drop_allowed(st.sel.0, st.sel.1, st.drop_caret, value_len);
        match st.phase {
            Phase::Potential => {
                let dx = mouse.0 - st.press_xy.0;
                let dy = mouse.1 - st.press_xy.1;
                let dist = (dx * dx + dy * dy).sqrt();
                let held = st.press_at.elapsed() >= ARM_HOLD;
                if dist >= ARM_DIST_PX || (held && dist >= 2.0) {
                    st.phase = Phase::Moving;
                    true
                } else {
                    false
                }
            }
            Phase::Moving => true,
        }
    })
}

pub fn on_release() -> ReleaseAction {
    SESSION.with(|s| {
        let Some(st) = s.borrow_mut().take() else {
            return ReleaseAction::None;
        };
        match st.phase {
            Phase::Potential => ReleaseAction::Click {
                rest: st.rest,
                caret: st.press_caret,
            },
            Phase::Moving => {
                if st.allowed {
                    ReleaseAction::Move {
                        rest: st.rest,
                        sel: st.sel,
                        drop: st.drop_caret,
                    }
                } else {
                    ReleaseAction::KeepSelection
                }
            }
        }
    })
}

pub fn cancel() {
    SESSION.with(|s| s.borrow_mut().take());
}

pub fn drop_allowed(lo: usize, hi: usize, drop: usize, len: usize) -> bool {
    if lo >= hi {
        return false;
    }
    let drop = drop.min(len);
    !(drop >= lo && drop <= hi)
}

/// Move `[lo, hi)` to sit before `drop`. Caret lands at the end of the moved chunk.
pub fn apply_move(value: &str, sel: (usize, usize), drop: usize) -> Option<(String, usize)> {
    let lo = sel.0.min(sel.1).min(value.len());
    let hi = sel.0.max(sel.1).min(value.len());
    if lo >= hi {
        return None;
    }
    let drop = drop.min(value.len());
    if !drop_allowed(lo, hi, drop, value.len()) {
        return None;
    }
    let chunk = &value[lo..hi];
    let mut out = String::with_capacity(value.len());
    if drop <= lo {
        out.push_str(&value[..drop]);
        out.push_str(chunk);
        out.push_str(&value[drop..lo]);
        out.push_str(&value[hi..]);
        Some((out, drop + chunk.len()))
    } else {
        out.push_str(&value[..lo]);
        out.push_str(&value[hi..drop]);
        out.push_str(chunk);
        out.push_str(&value[drop..]);
        Some((out, drop - (hi - lo) + chunk.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drop_inside_or_on_edge_is_forbidden() {
        assert!(!drop_allowed(2, 5, 2, 10));
        assert!(!drop_allowed(2, 5, 4, 10));
        assert!(!drop_allowed(2, 5, 5, 10));
        assert!(drop_allowed(2, 5, 1, 10));
        assert!(drop_allowed(2, 5, 6, 10));
        assert!(drop_allowed(2, 5, 10, 10));
    }

    #[test]
    fn move_before_selection() {
        let (s, caret) = apply_move("abcdef", (2, 4), 0).unwrap();
        assert_eq!(s, "cdabef");
        assert_eq!(caret, 2);
    }

    #[test]
    fn move_after_selection() {
        let (s, caret) = apply_move("abcdef", (1, 3), 6).unwrap();
        assert_eq!(s, "adefbc");
        assert_eq!(caret, 6);
    }

    #[test]
    fn move_into_selection_is_none() {
        assert!(apply_move("abcdef", (1, 4), 2).is_none());
    }

    #[test]
    fn caret_lands_at_end_of_moved_chunk() {
        // "hello world", move "world" in front of "hello "
        let (s, caret) = apply_move("hello world", (6, 11), 0).unwrap();
        assert_eq!(s, "worldhello ");
        assert_eq!(caret, "world".len());
    }
}
