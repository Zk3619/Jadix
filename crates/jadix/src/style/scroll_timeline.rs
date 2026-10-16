//! Named CSS `scroll-timeline` / `view-timeline` registry helpers.
//!
//! Anonymous `scroll()` / `view()` stay on [`super::element_style::AnimationTimeline`].
//! This module normalizes `--name` idents, axis progress, and `animation-range`.

use crate::core::tree::Element;
use taffy::style::Overflow;

/// CSS `scroll-timeline-axis` / `view-timeline-axis`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ScrollTimelineAxis {
    #[default]
    Block,
    Inline,
    X,
    Y,
}

impl ScrollTimelineAxis {
    pub fn from_css(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "inline" => Self::Inline,
            "x" => Self::X,
            "y" => Self::Y,
            _ => Self::Block,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::Inline => "inline",
            Self::X => "x",
            Self::Y => "y",
        }
    }
}

/// Strip the CSS `--` prefix so `--page` and `page` share a lookup key.
pub fn normalize_timeline_name(raw: &str) -> String {
    let s = raw.trim();
    s.strip_prefix("--").unwrap_or(s).to_ascii_lowercase()
}

/// Parse `animation-range-*` (`10%`, `0%`, `normal`, `cover`, `entry`, `exit`).
pub fn parse_range_progress(raw: &str) -> Option<f32> {
    let s = raw.trim().to_ascii_lowercase();
    if s.is_empty() || s == "normal" || s == "cover" || s == "entry" || s == "contain" {
        return Some(0.0);
    }
    if s == "exit" {
        return Some(1.0);
    }
    if let Some(rest) = s.strip_suffix('%') {
        let v: f32 = rest.trim().parse().ok()?;
        return Some((v / 100.0).clamp(0.0, 1.0));
    }
    None
}

/// Map timeline progress through `animation-range-start` / `end`.
pub fn map_animation_range(raw: f32, start: Option<&str>, end: Option<&str>) -> f32 {
    let s = start.and_then(parse_range_progress).unwrap_or(0.0);
    let e = end.and_then(parse_range_progress).unwrap_or(1.0);
    if (e - s).abs() < 1e-6 {
        return 0.0;
    }
    ((raw - s) / (e - s)).clamp(0.0, 1.0)
}

/// Lookup nearest (last) named timeline progress.
pub fn lookup_named(named: &[(String, f32)], name: &str) -> Option<f32> {
    let key = normalize_timeline_name(name);
    named.iter().rev().find(|(n, _)| n == &key).map(|(_, p)| *p)
}

/// Scrollport progress on `axis` (0–1). `None` if this node is not a scroller.
pub fn scroller_progress_axis(el: &Element, axis: ScrollTimelineAxis) -> Option<f32> {
    let ox = !matches!(el.style.overflow.x, Overflow::Visible);
    let oy = !matches!(el.style.overflow.y, Overflow::Visible);
    if !ox && !oy {
        return None;
    }
    let view_w = el.layout_bounds[2].max(1.0);
    let view_h = el.layout_bounds[3].max(1.0);
    let content_w = el.used_layout.anim_width().max(view_w);
    let content_h = el.used_layout.anim_height().max(view_h);
    let max_x = (content_w - view_w).max(0.0);
    let max_y = (content_h - view_h).max(0.0);
    let px = if max_x < 1.0 {
        0.0
    } else {
        (el.scroll_offset[0] / max_x).clamp(0.0, 1.0)
    };
    let py = if max_y < 1.0 {
        0.0
    } else {
        (el.scroll_offset[1] / max_y).clamp(0.0, 1.0)
    };
    Some(match axis {
        ScrollTimelineAxis::X | ScrollTimelineAxis::Inline => px,
        ScrollTimelineAxis::Y | ScrollTimelineAxis::Block => {
            if oy {
                py
            } else {
                px
            }
        }
    })
}

/// Register this node's `scroll-timeline-name` / `view-timeline-name` onto `named`.
pub fn push_named_timelines(
    el: &Element,
    named: &mut Vec<(String, f32)>,
    scroll_acc: [f32; 2],
    viewport_h: f32,
) {
    if let Some(name) = el.style.scroll_timeline_name.as_deref() {
        if let Some(p) = scroller_progress_axis(el, el.style.scroll_timeline_axis) {
            named.push((normalize_timeline_name(name), p));
        }
    }
    if let Some(name) = el.style.view_timeline_name.as_deref() {
        named.push((
            normalize_timeline_name(name),
            view_timeline_progress(el, scroll_acc, viewport_h),
        ));
    }
}

pub fn view_timeline_progress(el: &Element, scroll_acc: [f32; 2], viewport_h: f32) -> f32 {
    let y = el.layout_bounds[1] - scroll_acc[1];
    let h = el.layout_bounds[3];
    let start = -h;
    let end = viewport_h.max(1.0);
    ((y - start) / (end - start).max(1.0)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_lookup_nearest_wins() {
        let named = vec![
            ("page".into(), 0.2),
            ("page".into(), 0.8),
            ("other".into(), 0.1),
        ];
        assert!((lookup_named(&named, "--page").unwrap() - 0.8).abs() < 1e-4);
        assert!((lookup_named(&named, "PAGE").unwrap() - 0.8).abs() < 1e-4);
        assert!(lookup_named(&named, "--missing").is_none());
    }

    #[test]
    fn animation_range_maps_progress() {
        let t = map_animation_range(0.3, Some("10%"), Some("90%"));
        assert!((t - 0.25).abs() < 1e-4, "got {t}");
        assert!((map_animation_range(0.0, Some("0%"), Some("100%"))).abs() < 1e-4);
        assert!((map_animation_range(1.0, None, None) - 1.0).abs() < 1e-4);
    }
}
