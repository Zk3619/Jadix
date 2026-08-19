//! Reactivity tab (P1): WP-R.8 graph summary.

use crate::core::reactivity::{
    reactivity_debug_snapshot, SignalEffectEdge, SignalMemoEdge,
};
use crate::core::tree::Element;
use crate::primitives::{hstack, r#box, vstack};

use crate::style::parse_text_color;

use super::snapshot::{node_label, snapshot_subtree};

pub fn build_reactivity_panel(root: Option<&Element>, path: &[usize]) -> Element {
    let snap = reactivity_debug_snapshot();
    let selected = root.and_then(|r| snapshot_subtree(r, path));
    let filter_focus = selected
        .as_ref()
        .and_then(|n| n.focus_id.clone())
        .or_else(|| selected.as_ref().and_then(|n| n.template_id.clone()));

    let summary = format!(
        "owners {} · effects {} · memos {} · signals {} · pending e/m {}/{}",
        snap.live_owners,
        snap.live_effects,
        snap.live_memos,
        snap.live_signals,
        snap.pending_effects,
        snap.pending_memos,
    );

    let mut rows: Vec<Element> = vec![
        label_line("Summary", &summary),
        label_line(
            "Selected",
            &selected
                .as_ref()
                .map(node_label)
                .unwrap_or_else(|| "(none)".into()),
        ),
    ];

    rows.push(section_title("Signal → Effect"));
    for edge in filter_edges_effect(&snap.signal_effect_edges, filter_focus.as_deref()) {
        rows.push(edge_row(
            &format!("s{}", edge.signal_id),
            &format!("effects [{}]", fmt_ids(&edge.effect_ids)),
        ));
    }

    rows.push(section_title("Signal → Memo"));
    for edge in filter_edges_memo(&snap.signal_memo_edges, filter_focus.as_deref()) {
        rows.push(edge_row(
            &format!("s{}", edge.signal_id),
            &format!("memos [{}]", fmt_ids(&edge.memo_ids)),
        ));
    }

    if rows.len() <= 4 {
        rows.push(label_line("", "No edges (or none match selection)."));
    }

    scroll_column(rows)
}

fn filter_edges_effect(edges: &[SignalEffectEdge], _focus: Option<&str>) -> Vec<SignalEffectEdge> {
    // Future: map focus/template to named signals. For now show all (cap 24).
    edges.iter().take(24).cloned().collect()
}

fn filter_edges_memo(edges: &[SignalMemoEdge], _focus: Option<&str>) -> Vec<SignalMemoEdge> {
    edges.iter().take(24).cloned().collect()
}

fn fmt_ids(ids: &[usize]) -> String {
    ids.iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn section_title(text: &str) -> Element {
    Element::from(
        r#box()
            .w("100%")
            .pt("0.5rem")
            .pb("0.25rem")
            .child(
                crate::components::label(text)
                    .text_size("0.75rem")
                    .font_weight("700")
                    .color(parse_text_color("muted-foreground")),
            ),
    )
}

fn label_line(key: &str, val: &str) -> Element {
    Element::from(
        vstack()
            .w("100%")
            .gap("0.125rem")
            .py("0.25rem")
            .children([
                if key.is_empty() {
                    Element::new()
                } else {
                    crate::components::label(key)
                        .text_size("0.6875rem")
                        .color(parse_text_color("muted-foreground"))
                        .into()
                },
                crate::components::label(val)
                    .text_size("0.75rem")
                    .font_weight("500")
                    .w("100%")
                    .into(),
            ]),
    )
}

fn edge_row(left: &str, right: &str) -> Element {
    Element::from(
        hstack()
            .w("100%")
            .gap("0.25rem")
            .py("0.125rem")
            .children([
                crate::components::label(left)
                    .text_size("0.6875rem")
                    .font_weight("600")
                    .into(),
                crate::components::label(right)
                    .text_size("0.6875rem")
                    .color(parse_text_color("muted-foreground"))
                    .w("100%")
                    .into(),
            ]),
    )
}

fn scroll_column(children: Vec<Element>) -> Element {
    Element::from(vstack().w("100%").children(children))
}
