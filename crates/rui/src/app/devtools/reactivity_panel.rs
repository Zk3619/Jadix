//! Reactivity tab (P1): WP-R.8 graph summary.

use crate::core::reactivity::{
    reactivity_debug_snapshot, NamedSignalDebugEntry, SignalEffectEdge, SignalMemoEdge,
};
use crate::core::tree::Element;
use crate::primitives::{hstack, r#box, vstack};

use std::collections::HashMap;

use crate::style::parse_text_color;

use super::snapshot::{node_label, path_to_key, snapshot_subtree};

const EDGE_CAP: usize = 32;
const NAME_CAP: usize = 20;

pub fn build_reactivity_panel(root: Option<&Element>, path: &[usize]) -> Element {
    let snap = reactivity_debug_snapshot();
    let selected = root.and_then(|r| snapshot_subtree(r, path));
    let selected_label = selected
        .as_ref()
        .map(node_label)
        .unwrap_or_else(|| "(none)".into());
    let selected_path_key = path_to_key(path);
    let selected_focus = selected
        .as_ref()
        .and_then(|n| n.focus_id.clone())
        .unwrap_or_else(|| "—".into());
    let selected_tpl = selected
        .as_ref()
        .and_then(|n| n.template_id.clone())
        .unwrap_or_else(|| "—".into());

    let signal_names = build_signal_name_map(&snap.named_signal_entries);
    let filter_terms = build_filter_terms(
        selected.as_ref().and_then(|n| n.focus_id.as_deref()),
        selected.as_ref().and_then(|n| n.template_id.as_deref()),
        &selected_path_key,
    );
    let effect_rows = filter_edges_effect(&snap.signal_effect_edges, &signal_names, &filter_terms);
    let memo_rows = filter_edges_memo(&snap.signal_memo_edges, &signal_names, &filter_terms);

    let summary = format!(
        "owners {} · effects {} · memos {} · signals {} (named {}) · pending e/m {}/{} · ui-root deps {}",
        snap.live_owners,
        snap.live_effects,
        snap.live_memos,
        snap.live_signals,
        snap.named_signals,
        snap.pending_effects,
        snap.pending_memos,
        snap.ui_root_dep_count,
    );

    let mut rows: Vec<Element> = vec![
        label_line("Summary", &summary),
        label_line("Selected", &selected_label),
        label_line("Selected path", &selected_path_key),
        label_line("Selected focus_id", &selected_focus),
        label_line("Selected template_id", &selected_tpl),
    ];
    let filter_hint = if filter_terms.is_empty() {
        "none (show all edges)".to_string()
    } else {
        filter_terms.join(" | ")
    };
    rows.push(label_line("Filter terms", &filter_hint));

    rows.push(section_title("Signal → Effect"));
    for edge in effect_rows.iter().take(EDGE_CAP) {
        rows.push(edge_row(
            &format!("s{} ({})", edge.signal_id, edge.fanout),
            &format!("effects [{}] · {}", fmt_ids(&edge.targets), edge.names),
        ));
    }
    if let Some(note) = truncation_note(effect_rows.len(), EDGE_CAP, "effect edges") {
        rows.push(label_line("", &note));
    }

    rows.push(section_title("Signal → Memo"));
    for edge in memo_rows.iter().take(EDGE_CAP) {
        rows.push(edge_row(
            &format!("s{} ({})", edge.signal_id, edge.fanout),
            &format!("memos [{}] · {}", fmt_ids(&edge.targets), edge.names),
        ));
    }
    if let Some(note) = truncation_note(memo_rows.len(), EDGE_CAP, "memo edges") {
        rows.push(label_line("", &note));
    }

    rows.push(section_title("Named signals"));
    for line in named_signal_lines(&snap.named_signal_entries)
        .into_iter()
        .take(NAME_CAP)
    {
        rows.push(edge_row("key", &line));
    }
    if let Some(note) = truncation_note(snap.named_signal_entries.len(), NAME_CAP, "named signals")
    {
        rows.push(label_line("", &note));
    }

    if effect_rows.is_empty() && memo_rows.is_empty() {
        rows.push(label_line(
            "",
            "No edges (or no named-signal edges match current selection).",
        ));
    }

    scroll_column(rows)
}

#[derive(Clone)]
struct EdgeRowData {
    signal_id: usize,
    fanout: usize,
    targets: Vec<usize>,
    names: String,
}

fn filter_edges_effect(
    edges: &[SignalEffectEdge],
    signal_names: &HashMap<usize, Vec<String>>,
    filter_terms: &[String],
) -> Vec<EdgeRowData> {
    let mut out = Vec::new();
    for edge in edges {
        if !matches_filter(edge.signal_id, signal_names, filter_terms) {
            continue;
        }
        out.push(EdgeRowData {
            signal_id: edge.signal_id,
            fanout: edge.effect_ids.len(),
            targets: edge.effect_ids.clone(),
            names: names_label(edge.signal_id, signal_names),
        });
    }
    sort_edge_rows(&mut out);
    out
}

fn filter_edges_memo(
    edges: &[SignalMemoEdge],
    signal_names: &HashMap<usize, Vec<String>>,
    filter_terms: &[String],
) -> Vec<EdgeRowData> {
    let mut out = Vec::new();
    for edge in edges {
        if !matches_filter(edge.signal_id, signal_names, filter_terms) {
            continue;
        }
        out.push(EdgeRowData {
            signal_id: edge.signal_id,
            fanout: edge.memo_ids.len(),
            targets: edge.memo_ids.clone(),
            names: names_label(edge.signal_id, signal_names),
        });
    }
    sort_edge_rows(&mut out);
    out
}

fn sort_edge_rows(rows: &mut [EdgeRowData]) {
    rows.sort_by(|a, b| b.fanout.cmp(&a.fanout).then(a.signal_id.cmp(&b.signal_id)));
}

fn build_signal_name_map(entries: &[NamedSignalDebugEntry]) -> HashMap<usize, Vec<String>> {
    let mut out: HashMap<usize, Vec<String>> = HashMap::new();
    for entry in entries {
        out.entry(entry.signal_id)
            .or_default()
            .push(entry.key.clone());
    }
    for names in out.values_mut() {
        names.sort();
        names.dedup();
    }
    out
}

fn build_filter_terms(
    focus_id: Option<&str>,
    template_id: Option<&str>,
    path_key: &str,
) -> Vec<String> {
    let mut terms = Vec::new();
    push_term(&mut terms, focus_id);
    push_term(&mut terms, template_id);
    if !path_key.is_empty() {
        terms.push(path_key.to_ascii_lowercase());
    }
    terms
}

fn push_term(out: &mut Vec<String>, raw: Option<&str>) {
    let Some(v) = raw else {
        return;
    };
    let t = v.trim();
    if t.is_empty() || t == "—" {
        return;
    }
    let lower = t.to_ascii_lowercase();
    if !out.iter().any(|x| x == &lower) {
        out.push(lower);
    }
}

fn matches_filter(
    signal_id: usize,
    signal_names: &HashMap<usize, Vec<String>>,
    filter_terms: &[String],
) -> bool {
    if filter_terms.is_empty() {
        return true;
    }
    let Some(names) = signal_names.get(&signal_id) else {
        return false;
    };
    names.iter().any(|name| {
        let lower = name.to_ascii_lowercase();
        filter_terms
            .iter()
            .any(|term| lower.contains(&term.to_ascii_lowercase()))
    })
}

fn names_label(signal_id: usize, signal_names: &HashMap<usize, Vec<String>>) -> String {
    let Some(names) = signal_names.get(&signal_id) else {
        return "unnamed".into();
    };
    if names.is_empty() {
        return "unnamed".into();
    }
    names.join(" | ")
}

fn named_signal_lines(entries: &[NamedSignalDebugEntry]) -> Vec<String> {
    entries
        .iter()
        .map(|entry| format!("s{} ← {}", entry.signal_id, entry.key))
        .collect()
}

fn truncation_note(total: usize, cap: usize, label: &str) -> Option<String> {
    if total > cap {
        Some(format!("… +{} more {label}", total - cap))
    } else {
        None
    }
}

fn fmt_ids(ids: &[usize]) -> String {
    ids.iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn section_title(text: &str) -> Element {
    Element::from(
        r#box().w("100%").pt("0.5rem").pb("0.25rem").child(
            crate::components::label(text)
                .text(text)
                .text_size("0.75rem")
                .font_weight("700")
                .color(parse_text_color("muted-foreground")),
        ),
    )
}

fn label_line(key: &str, val: &str) -> Element {
    Element::from(
        vstack().w("100%").gap("0.125rem").py("0.25rem").children([
            if key.is_empty() {
                Element::new()
            } else {
                crate::components::label(key)
                    .text(key)
                    .text_size("0.6875rem")
                    .color(parse_text_color("muted-foreground"))
                    .into()
            },
            crate::components::label(val)
                .text(val)
                .text_size("0.75rem")
                .font_weight("500")
                .w("100%")
                .into(),
        ]),
    )
}

fn edge_row(left: &str, right: &str) -> Element {
    Element::from(
        hstack().w("100%").gap("0.25rem").py("0.125rem").children([
            crate::components::label(left)
                .text(left)
                .text_size("0.6875rem")
                .font_weight("600")
                .into(),
            crate::components::label(right)
                .text(right)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_edges_effect_sorts_by_fanout_then_signal_id() {
        let edges = vec![
            SignalEffectEdge {
                signal_id: 4,
                effect_ids: vec![1],
            },
            SignalEffectEdge {
                signal_id: 2,
                effect_ids: vec![3, 4, 5],
            },
            SignalEffectEdge {
                signal_id: 3,
                effect_ids: vec![8, 9],
            },
        ];
        let names = HashMap::from([
            (2usize, vec!["alpha".to_string()]),
            (3usize, vec!["beta".to_string()]),
            (4usize, vec!["gamma".to_string()]),
        ]);

        let rows = filter_edges_effect(&edges, &names, &[]);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].signal_id, 2);
        assert_eq!(rows[0].fanout, 3);
        assert_eq!(rows[1].signal_id, 3);
        assert_eq!(rows[2].signal_id, 4);
    }

    #[test]
    fn filter_edges_memo_matches_named_signal_terms_case_insensitive() {
        let edges = vec![
            SignalMemoEdge {
                signal_id: 10,
                memo_ids: vec![1],
            },
            SignalMemoEdge {
                signal_id: 11,
                memo_ids: vec![2],
            },
        ];
        let names = HashMap::from([
            (10usize, vec!["__rui_focus_visible".to_string()]),
            (11usize, vec!["counter_value".to_string()]),
        ]);
        let terms = vec!["FOCUS".to_string()];

        let rows = filter_edges_memo(&edges, &names, &terms);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].signal_id, 10);
        assert_eq!(rows[0].names, "__rui_focus_visible");
    }

    #[test]
    fn truncation_note_only_returns_when_total_exceeds_cap() {
        assert_eq!(
            truncation_note(40, EDGE_CAP, "effect edges"),
            Some("… +8 more effect edges".to_string())
        );
        assert_eq!(truncation_note(EDGE_CAP, EDGE_CAP, "effect edges"), None);
        assert_eq!(truncation_note(3, EDGE_CAP, "effect edges"), None);
    }
}
