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
        (10usize, vec!["__jadix_focus_visible".to_string()]),
        (11usize, vec!["counter_value".to_string()]),
    ]);
    let terms = vec!["FOCUS".to_string()];

    let rows = filter_edges_memo(&edges, &names, &terms);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].signal_id, 10);
    assert_eq!(rows[0].names, "__jadix_focus_visible");
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
