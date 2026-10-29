use super::*;

#[test]
fn prepared_path_preserves_disconnected_subpaths() {
    let commands = vec![
        PathCommand::MoveTo((0.0, 0.0)),
        PathCommand::LineTo((10.0, 0.0)),
        PathCommand::MoveTo((20.0, 0.0)),
        PathCommand::LineTo((30.0, 0.0)),
    ];
    let path = PreparedPath::new(&commands);
    assert_eq!(path.length(), 20.0);
    assert_eq!(path.sample(0.75).position, (25.0, 0.0));
    assert_eq!(
        path.trim(0.25, 0.75),
        vec![
            PathCommand::MoveTo((5.0, 0.0)),
            PathCommand::LineTo((10.0, 0.0)),
            PathCommand::MoveTo((20.0, 0.0)),
            PathCommand::LineTo((25.0, 0.0))
        ]
    );
    assert_eq!(path.trim(0.0, 1.0), commands);
}

#[test]
fn morph_checks_topology_and_samples_without_framework() {
    let morph = PathMorph::new(
        &[PathCommand::MoveTo((0.0, 0.0))],
        &[PathCommand::MoveTo((10.0, 20.0))],
    )
    .unwrap();
    assert_eq!(morph.sample(0.5), vec![PathCommand::MoveTo((5.0, 10.0))]);
    assert!(matches!(
        PathMorph::new(&[PathCommand::Close], &[PathCommand::MoveTo((0.0, 0.0))]),
        Err(PathMorphError::IncompatibleTopology { command: 0 })
    ));
    assert!(matches!(
        PathMorph::new(
            &[PathCommand::MoveTo((f32::NAN, 0.0))],
            &[PathCommand::MoveTo((0.0, 0.0))]
        ),
        Err(PathMorphError::NonFiniteCoordinates)
    ));
}
