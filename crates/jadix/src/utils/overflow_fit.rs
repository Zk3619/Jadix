//! Helpers for folding overflow rows (`+N`) against a real horizontal budget.

use crate::core::tree::Element;
use crate::layout::dimension::{dim_kind, dim_length_px, DimKind};

/// Propagate a horizontal budget down the tree. Only ancestors with a definite
/// width, `max-width`, or clipping overflow tighten the budget — `w:auto`
/// shrink-to-fit nodes pass the incoming constraint through unchanged.
pub fn tighten_horizontal_constraint(el: &Element, parent: Option<f32>) -> Option<f32> {
    let bounds_w = el.layout_bounds[2];
    if bounds_w <= 0.0 {
        return parent;
    }

    let width_kind = dim_kind(el.style.size.width);
    let has_definite_w = matches!(width_kind, DimKind::Length(_) | DimKind::Percent(_));

    let overflow_x = el
        .attrs
        .get("overflow_x")
        .or_else(|| el.attrs.get("overflow"))
        .map(|s| s.as_str())
        .unwrap_or("visible");
    let clips = matches!(overflow_x, "hidden" | "clip" | "auto" | "scroll");

    let mut budget = parent;

    if has_definite_w || clips {
        budget = Some(match budget {
            Some(b) => b.min(bounds_w),
            None => bounds_w,
        });
    }

    if let Some(px) = dim_length_px(el.style.max_size.width) {
        budget = Some(match budget {
            Some(b) => b.min(px),
            None => px,
        });
    } else if let DimKind::Percent(p) = dim_kind(el.style.max_size.width) {
        if let Some(b) = budget {
            budget = Some(b * p);
        }
    }

    // Shrink-to-fit auto nodes can still be capped by layout when max-width binds.
    if !has_definite_w
        && !clips
        && !matches!(dim_kind(el.style.max_size.width), DimKind::Auto)
        && bounds_w > 0.0
    {
        budget = Some(match budget {
            Some(b) => b.min(bounds_w),
            None => bounds_w,
        });
    }

    budget
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_shrink_to_fit_does_not_expand_budget() {
        let mut auto = Element::new().w("auto");
        auto.layout_bounds[2] = 120.0;
        assert_eq!(tighten_horizontal_constraint(&auto, Some(80.0)), Some(80.0));
    }

    #[test]
    fn definite_width_tightens_budget() {
        let mut panel = Element::new().w(60.0);
        panel.layout_bounds[2] = 60.0;
        assert_eq!(
            tighten_horizontal_constraint(&panel, Some(200.0)),
            Some(60.0)
        );
    }

    #[test]
    fn percent_max_width_uses_fractional_taffy_units() {
        let mut panel = Element::new().max_w("80%");
        panel.layout_bounds[2] = 240.0;
        assert_eq!(tighten_horizontal_constraint(&panel, Some(300.0)), Some(240.0));
        panel = panel.max_w("100%");
        assert_eq!(tighten_horizontal_constraint(&panel, Some(300.0)), Some(240.0));
    }
}
