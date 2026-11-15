use super::input_cost_tests::field;
use super::*;
use jadix_core::presentation::{CommitBatch, Operation, RootId};
#[test]
fn structural_editors_stage_stale_limits_and_multi_field_failures_before_layout_acceptance() {
    let root = RootId::new();
    let life = RootLifetime::new(root);
    let mut a = field(NodeId::new(root), "draft", 0, 0.);
    let mut b = field(NodeId::new(root), "notes", 0, 0.);
    for n in [&mut a, &mut b] {
        n.descriptor.frame = Default::default();
        n.descriptor.style.width = Some(32.);
        n.descriptor.style.height = Some(2.);
        n.descriptor.style.shrink = 0.;
    }
    let mut surface = CellSurface::new(life, [1., 1.]).unwrap();
    let area = Rect::new(3, 2, 40, 12);
    let operations = [&a, &b]
        .iter()
        .enumerate()
        .map(|(index, n)| Operation::Create {
            node: n.id,
            parent: None,
            index,
            descriptor: n.descriptor.clone(),
        })
        .collect();
    surface
        .commit_auto(
            &CommitBatch {
                root,
                previous: 0,
                revision: 1,
                operations,
            },
            area,
            0,
        )
        .unwrap();
    surface.focus_node(a.id).unwrap();
    surface.handle(Event::Paste("中文".into())).unwrap();
    let checkpoint = surface.fields[&a.id].editor.clone();
    let bytes = surface.editor_bytes;
    let accepted = surface.structural_layout.as_ref().unwrap().roots().to_vec();
    let mut da = a.descriptor.clone();
    if let NodeContent::Input {
        value, revision, ..
    } = &mut da.content
    {
        *value = "SERVER".into();
        *revision = 1;
    }
    let mut db = b.descriptor.clone();
    if let NodeContent::Input { max_length, .. } = &mut db.content {
        *max_length = Some(TerminalEditor::MAX_BYTES + 1);
    }
    let batch = |da: NodeDescriptor, db: NodeDescriptor| CommitBatch {
        root,
        previous: 1,
        revision: 2,
        operations: vec![
            Operation::Update {
                node: a.id,
                descriptor: da,
            },
            Operation::Update {
                node: b.id,
                descriptor: db,
            },
        ],
    };
    // Use exact accepted structural sequence; invalid second record cannot publish first.
    let mut bad = batch(da.clone(), db);
    bad.revision = 2;
    assert!(surface.commit_auto(&bad, area, 0).is_err());
    assert_eq!(surface.revision(), 1);
    assert_eq!(surface.structural_layout.as_ref().unwrap().revision(), 1);
    assert_eq!(
        surface.structural_layout.as_ref().unwrap().roots(),
        accepted
    );
    assert_eq!(
        surface.fields[&a.id].editor.document(),
        checkpoint.document()
    );
    assert_eq!(surface.editor_bytes, bytes);
    let mut stale = a.descriptor.clone();
    if let NodeContent::Input { value, .. } = &mut stale.content {
        *value = "stale".into();
    }
    let retry = CommitBatch {
        root,
        previous: 1,
        revision: 2,
        operations: vec![Operation::Update {
            node: a.id,
            descriptor: stale,
        }],
    };
    assert_eq!(
        surface.commit_auto(&retry, area, 0).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(surface.revision(), 1);
    let retry = CommitBatch {
        root,
        previous: 1,
        revision: 2,
        operations: vec![Operation::Update {
            node: a.id,
            descriptor: da.clone(),
        }],
    };
    surface.commit_auto(&retry, area, 0).unwrap();
    assert_eq!(surface.fields[&a.id].editor.document().text(), "SERVER");
    assert_eq!(surface.fields[&a.id].revision, 1);
    assert_eq!(
        surface.editor_bytes,
        surface
            .fields
            .values()
            .map(|f| f.editor.retained_bytes())
            .sum::<usize>()
    );
    // Same-value higher revision keeps the accepted history; a new maxLength is
    // applied atomically while existing text is preserved.
    if let NodeContent::Input {
        revision,
        max_length,
        ..
    } = &mut da.content
    {
        *revision = 2;
        *max_length = Some(2);
    }
    surface
        .commit_auto(
            &CommitBatch {
                root,
                previous: 2,
                revision: 3,
                operations: vec![Operation::Update {
                    node: a.id,
                    descriptor: da,
                }],
            },
            area,
            0,
        )
        .unwrap();
    assert_eq!(surface.fields[&a.id].editor.max_length(), Some(2));
    assert_eq!(surface.fields[&a.id].editor.document().text(), "SERVER");
    assert_eq!(surface.focused(), Some(a.id));
}
