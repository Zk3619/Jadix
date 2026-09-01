//! Carousel autoplay 推进判定（产品层）。
//!
//! 与指针按住 / 悬停暂停互斥；无缝环动画进行中优先 settle。
//! 离屏（本帧未 paint）时不 Advance：父级滚动把轮播滚出视口后时钟停在画面上，
//! 滚入时重排 deadline，避免把积压的翻页一次性补火（整页滚动条闪顶/底）。

use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoplayAction {
    Skip,
    /// Advance loop settle / finish.
    FinishLoop,
    /// Turn the page when it expires.
    Advance,
    /// Not yet scheduled, need to bump deadline.
    ArmDeadline,
}

pub fn decide_autoplay(
    autoplay_ms: u64,
    paused: bool,
    dragging: bool,
    has_loop_anim: bool,
    now: Instant,
    next_at: Option<Instant>,
) -> AutoplayAction {
    if autoplay_ms == 0 || paused || dragging {
        return AutoplayAction::Skip;
    }
    if has_loop_anim {
        return AutoplayAction::FinishLoop;
    }
    match next_at {
        None => AutoplayAction::ArmDeadline,
        Some(at) if now >= at => AutoplayAction::Advance,
        Some(_) => AutoplayAction::Skip,
    }
}

/// 视口内（含父级 ScrollArea 正在滚）照常推进；完全离屏则停表。
/// 刚滚入时把逾期 Advance 收成 ArmDeadline，避免进屏瞬间翻页。
pub fn gate_autoplay_visibility(
    visible: bool,
    just_entered: bool,
    action: AutoplayAction,
) -> AutoplayAction {
    if !visible {
        return AutoplayAction::Skip;
    }
    if just_entered {
        return AutoplayAction::ArmDeadline;
    }
    action
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn autoplay_mutex_and_advance() {
        let now = Instant::now();
        assert_eq!(
            decide_autoplay(1000, false, true, false, now, Some(now)),
            AutoplayAction::Skip
        );
        assert_eq!(
            decide_autoplay(1000, false, false, true, now, Some(now)),
            AutoplayAction::FinishLoop
        );
        assert_eq!(
            decide_autoplay(1000, false, false, false, now, None),
            AutoplayAction::ArmDeadline
        );
        assert_eq!(
            decide_autoplay(
                1000,
                false,
                false,
                false,
                now,
                Some(now - Duration::from_millis(1))
            ),
            AutoplayAction::Advance
        );
    }

    #[test]
    fn visibility_gate_freezes_offscreen_and_swallows_enter_advance() {
        assert_eq!(
            gate_autoplay_visibility(false, false, AutoplayAction::Advance),
            AutoplayAction::Skip
        );
        assert_eq!(
            gate_autoplay_visibility(true, true, AutoplayAction::Advance),
            AutoplayAction::ArmDeadline
        );
        assert_eq!(
            gate_autoplay_visibility(true, false, AutoplayAction::Advance),
            AutoplayAction::Advance
        );
        assert_eq!(
            gate_autoplay_visibility(true, false, AutoplayAction::Skip),
            AutoplayAction::Skip
        );
    }
}
