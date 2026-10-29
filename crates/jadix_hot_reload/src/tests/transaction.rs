use super::*;

fn gate() -> SessionGate {
    SessionGate::new(SessionInfo::new("build-window-owner".into()))
}

#[test]
fn ordinary_registration_and_serialized_adapter_share_commit_contract() {
    let mut plain = gate();
    let mut adapted = gate();
    let request = Transaction::new(plain.info()).unwrap();
    let wire = serde_json::to_string(&request).unwrap();
    let decoded = serde_json::from_str(&wire).unwrap();
    assert_eq!(
        plain.apply(&request, |_| Ok(CommitStatus::Applied)),
        adapted.apply(&decoded, |_| Ok(CommitStatus::Applied))
    );
}

#[test]
fn duplicate_delivery_does_not_repeat_side_effects() {
    let mut session = gate();
    let request = Transaction::new(session.info()).unwrap();
    let first = session.apply(&request, |_| Ok(CommitStatus::Deferred));
    assert_eq!(
        session.apply(&request, |_| panic!("duplicate must not commit")),
        first
    );
}

#[test]
fn failed_preflight_preserves_revision_and_allows_retry() {
    let mut session = gate();
    let request = Transaction::new(session.info()).unwrap();
    assert!(!session
        .apply(&request, |_| Err("unknown compiled slot".into()))
        .committed());
    assert_eq!(session.info().revision, 0);
    assert!(session
        .apply(&request, |_| Ok(CommitStatus::Applied))
        .committed());
}

#[test]
fn mismatched_build_schema_or_revision_never_calls_user_code() {
    let mut session = gate();
    for selection in 0..4 {
        let mut request = Transaction::new(session.info()).unwrap();
        match selection {
            0 => request.build.push('!'),
            1 => request.schema += 1,
            2 => request.base_revision += 1,
            _ => request.protocol += 1,
        }
        assert!(!session
            .apply(&request, |_| panic!("preflight must reject"))
            .committed());
        assert_eq!(session.info().revision, 0);
    }
}

#[test]
fn unsigned_network_templates_never_reach_the_commit_callback() {
    let mut session = gate();
    let mut request = Transaction::new(session.info()).unwrap();
    request.templates.push(TemplateFragment {
        id: "unregistered".into(),
        ..Default::default()
    });
    let acknowledgement =
        session.apply(&request, |_| panic!("unsigned capability must not commit"));
    assert_eq!(acknowledgement.status, CommitStatus::Rejected);
    assert_eq!(session.info().revision, 0);
}

#[test]
fn duplicate_template_keys_and_revision_overflow_are_rejected() {
    let mut session = gate();
    let mut request = Transaction::new(session.info()).unwrap();
    let template = TemplateFragment {
        id: "same".into(),
        ..Default::default()
    };
    request.templates = vec![template.clone(), template];
    assert!(!session
        .apply(&request, |_| panic!("duplicate key"))
        .committed());
    let mut overflow = SessionInfo::new("overflow".into());
    overflow.revision = u64::MAX;
    assert!(Transaction::new(&overflow).is_none());
}
