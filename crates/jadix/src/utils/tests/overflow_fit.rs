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
    assert_eq!(
        tighten_horizontal_constraint(&panel, Some(300.0)),
        Some(240.0)
    );
    panel = panel.max_w("100%");
    assert_eq!(
        tighten_horizontal_constraint(&panel, Some(300.0)),
        Some(240.0)
    );
}
