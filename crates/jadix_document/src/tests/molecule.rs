use super::*;
#[test]
fn xyz_water_has_two_inferred_bonds() {
    let molecule =
        Molecule::from_xyz("3\nwater\nO 0 0 0\nH 0.958 0 0\nH -0.239 0.927 0\n").unwrap();
    assert_eq!(molecule.bonds().len(), 2);
    assert!(molecule.bonds().iter().all(|bond| bond.atoms[0] == 0));
}
#[test]
fn pdb_keeps_connections_after_first_model_without_loading_other_models() {
    let source = "MODEL        1\nHETATM    1  O   HOH A   1       0.000   0.000   0.000  1.00  0.00           O\nHETATM    2  H   HOH A   1       1.000   0.000   0.000  1.00  0.00           H\nENDMDL\nMODEL        2\nHETATM    3  O   HOH A   1       2.000   0.000   0.000  1.00  0.00           O\nENDMDL\nCONECT    1    2\n";
    let molecule = Molecule::from_pdb(source).unwrap();
    assert_eq!(molecule.atoms().len(), 2);
    assert_eq!(molecule.bonds()[0].atoms, [0, 1]);
}
#[test]
fn xyz_and_json_roundtrip_and_reject_invalid_coordinates() {
    let molecule = Molecule::from_xyz("3\nwater\nO 0 0 0\nH 1 0 0\nH 0 1 0\n").unwrap();
    assert_eq!(
        Molecule::from_json(&molecule.to_json())
            .unwrap()
            .atoms()
            .len(),
        3
    );
    assert!(Molecule::from_xyz("1\ninvalid\nH NaN 0 0\n").is_err());
    assert!(Molecule::from_json(
        r#"{"atoms":[{"element":"C","position":[0,0,0]}],"bonds":[{"atoms":[0,2]}]}"#
    )
    .is_err());
}
