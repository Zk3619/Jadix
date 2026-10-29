use super::*;
#[test]
fn molecular_graph_raster_is_nonblank_without_per_atom_ui_nodes() {
    let molecule = Molecule::from_xyz("3\nwater\nO 0 0 0\nH 1 0 0\nH 0 1 0\n").unwrap();
    let element: Element = molecule_view(molecule).into();
    assert!(element.children.is_empty());
    let (pixels, _, _) =
        crate::primitives::canvas::rasterize_canvas_premul_for_test(&element.attrs, 200.0, 160.0)
            .unwrap();
    assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] > 0.5));
}

#[test]
fn water_raster_contains_bonds_between_atoms() {
    let molecule =
        Molecule::from_xyz("3\nwater\nO 0 0 0\nH 0.958 0 0\nH -0.239 0.927 0\n").unwrap();
    let element: Element = molecule_view(molecule).into();
    assert_eq!(
        element.get_attr("data-bond-count").map(String::as_str),
        Some("2")
    );
    let (pixels, _, _) =
        crate::primitives::canvas::rasterize_canvas_premul_for_test(&element.attrs, 200.0, 160.0)
            .unwrap();
    assert!(pixels[(34 * 200 + 112) * 4 + 3] > 0.5);
}
