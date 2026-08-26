//! Carousel autoplay 推进判定（产品层）。
//!
//! 与指针按住 / 悬停暂停互斥；无缝环动画进行中优先 settle。

use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoplayAction {
    Skip,
    /// 推进 loop settle / finish。
    FinishLoop,
    /// 到期翻页。
    Advance,
    /// 尚未排程，需 bump deadline。
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
}
