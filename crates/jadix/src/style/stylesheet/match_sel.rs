//! Match registered rules against an element's attrs / class list.

use super::ir::{MatchedStyleRule, Selector};
use super::registry::{registry_is_empty, with_registry};
use crate::core::tree::Element;
use crate::style::with_class_list;

const ATTR_SHEET_APPLIED: &str = "data-jadix-sheet";

/// Apply matching stylesheet utilities **after** component defaults and **before**
/// caller RSX `class` (see `paint_slot` / `From<BaseComponentBuilder>`).
///
/// Idempotent: a second call is a no-op (`data-jadix-sheet`).
pub fn apply_stylesheet_rules(mut el: Element) -> Element {
    if el.attrs.get(ATTR_SHEET_APPLIED).map(String::as_str) == Some("1") {
        return el;
    }
    if registry_is_empty() {
        return el;
    }
    let classes = collect_matching_classes(&el);
    if !classes.is_empty() {
        let sheet_classes = classes.clone();
        // Tree mounts reach slots after caller classes have been applied. Keep those
        // utilities last, including their hover/dark rules, instead of replacing them.
        let classes = match el.attrs.get("jadix_utility_class") {
            Some(caller) if !caller.trim().is_empty() => format!("{classes} {caller}"),
            _ => classes,
        };
        el = with_class_list(el, &classes);
        el.set_attr("jadix_stylesheet_class", sheet_classes);
    }
    el.set_attr(ATTR_SHEET_APPLIED, "1");
    el
}

/// Walk `el` and every descendant. Covers slots stamped with `set_attr("data-slot")`
/// inside a `#[component]` body (table cells, command rows, dialog chrome, …).
///
/// Empty registry: identity. Already-stamped nodes (`data-jadix-sheet`) stay no-ops.
pub fn apply_stylesheet_rules_tree(el: Element) -> Element {
    if registry_is_empty() {
        return el;
    }
    walk_stylesheet_rules(el)
}

fn walk_stylesheet_rules(mut el: Element) -> Element {
    el = apply_stylesheet_rules(el);
    let kids = std::mem::take(&mut el.children);
    el.children = kids.into_iter().map(walk_stylesheet_rules).collect();
    el
}

/// Hit list for DevTools Styles (selector + classes + source path).
pub fn collect_matching_rules(el: &Element) -> Vec<MatchedStyleRule> {
    if registry_is_empty() {
        return Vec::new();
    }
    let mut hits: Vec<(u16, usize, MatchedStyleRule)> = Vec::new();
    with_registry(|sheets| {
        let mut order = 0usize;
        for sheet in sheets {
            for rule in &sheet.rules {
                if let Some(sel) = rule.selectors.iter().find(|sel| selector_matches(el, sel)) {
                    hits.push((
                        rule.priority,
                        order,
                        MatchedStyleRule {
                            selector: sel.to_string(),
                            classes: rule.classes.clone(),
                            priority: rule.priority,
                            source: sheet.source.clone(),
                        },
                    ));
                    order += 1;
                }
            }
        }
    });
    hits.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    hits.into_iter().map(|(_, _, r)| r).collect()
}

fn collect_matching_classes(el: &Element) -> String {
    let mut hits: Vec<(u16, usize, String)> = Vec::new();
    with_registry(|sheets| {
        let mut order = 0usize;
        for sheet in sheets {
            for rule in &sheet.rules {
                if rule.selectors.iter().any(|sel| selector_matches(el, sel)) {
                    hits.push((rule.priority, order, rule.classes.clone()));
                    order += 1;
                }
            }
        }
    });
    hits.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut out = String::new();
    for (_, _, classes) in hits {
        if classes.trim().is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(classes.trim());
    }
    out
}

fn selector_matches(el: &Element, sel: &Selector) -> bool {
    match sel {
        Selector::DataSlot(slot) => el.attrs.get("data-slot").map(String::as_str) == Some(slot),
        Selector::DataEq { key, value } => attr_eq(el, key, value),
        Selector::Class(class) => element_has_class(el, class),
        Selector::Id(id) => {
            el.attrs.get("id").map(String::as_str) == Some(id)
                || el.focus_id.as_deref() == Some(id.as_str())
        }
        Selector::And(parts) => parts.iter().all(|p| selector_matches(el, p)),
    }
}

fn attr_eq(el: &Element, key: &str, value: &str) -> bool {
    if el.attrs.get(key).map(String::as_str) == Some(value) {
        return true;
    }
    attr_aliases(key)
        .iter()
        .any(|alias| el.attrs.get(*alias).map(String::as_str) == Some(value))
}

/// Non-standard Bubble attrs (`jadix_bubble_variant`) match `data-variant` selectors.
fn attr_aliases(key: &str) -> &'static [&'static str] {
    match key {
        "data-variant" => &[
            "data-bubble-variant",
            "jadix_bubble_variant",
            "jadix_badge_variant",
        ],
        "data-bubble-variant" => &["data-variant", "jadix_bubble_variant"],
        "jadix_bubble_variant" => &["data-variant", "data-bubble-variant"],
        "jadix_badge_variant" => &["data-variant"],
        _ => &[],
    }
}

fn element_has_class(el: &Element, class: &str) -> bool {
    if let Some(cs) = el.class_set.as_ref() {
        if cs.base.iter().any(|c| c == class) {
            return true;
        }
    }
    el.attrs
        .get("class")
        .map(|s| s.split_whitespace().any(|c| c == class))
        .unwrap_or(false)
}
