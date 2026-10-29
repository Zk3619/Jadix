use super::*;
use crate::layout::layout_tree::LayoutTree;

#[test]
fn scene3d_leaf_layout_bounds() {
    let leaf: Element = scene3d().w(120.0).h(90.0).into();
    assert!(leaf.children.is_empty());
    let mut root = Element::new().w(200.0).h(140.0).child(leaf);
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 200.0, 140.0, [-1.0, -1.0]);
    let [_, _, w, h] = root.children.first().expect("leaf").layout_bounds;
    assert!(
        w > 110.0 && w < 130.0 && h > 80.0 && h < 100.0,
        "scene3d leaf bounds [{w}, {h}]"
    );
    #[cfg(feature = "scene3d")]
    {
        assert_eq!(
            root.children[0].get_attr(ATTR_SCENE3D).map(|s| s.as_str()),
            Some("1")
        );
        let jobs = take_scene3d_jobs();
        assert!(
            !jobs.is_empty(),
            "layout collect must queue a GPU job without calling runtime::request_redraw"
        );
    }
    #[cfg(not(feature = "scene3d"))]
    {
        assert!(root.children[0].get_attr(ATTR_SCENE3D).is_none());
    }
}

#[test]
fn scene3d_camera_light_attrs() {
    let leaf: Element = scene3d()
        .w(80.0)
        .h(60.0)
        .camera(Scene3dCamera {
            fov_deg: 50.0,
            pitch: 0.2,
            distance: 3.0,
        })
        .light(Scene3dLight::default())
        .material(Scene3dMaterial {
            metallic: 0.4,
            roughness: 0.3,
        })
        .into();
    assert_eq!(
        leaf.get_attr(ATTR_SCENE3D_FOV_DEG).map(|s| s.as_str()),
        Some("50.000")
    );
    assert!(leaf.get_attr(ATTR_SCENE3D_CAMERA_Z).is_some());
    assert!(leaf.get_attr(ATTR_SCENE3D_LIGHT).is_some());
    assert!(leaf.get_attr(ATTR_SCENE3D_MATERIAL).is_some());
}

#[cfg(feature = "scene3d")]
#[test]
fn scene3d_equal_sizes_with_distinct_cameras_keep_separate_targets() {
    let first: Element = scene3d()
        .w(80.0)
        .h(60.0)
        .pitch_deg(12.0)
        .label("first")
        .into();
    let second: Element = scene3d()
        .w(80.0)
        .h(60.0)
        .pitch_deg(52.0)
        .label("second")
        .into();
    let first_key = scene3d_tex_from_element_attrs(&first.attrs, 80.0, 60.0)
        .unwrap()
        .0;
    let second_key = scene3d_tex_from_element_attrs(&second.attrs, 80.0, 60.0)
        .unwrap()
        .0;
    assert_ne!(first_key, second_key);
    assert_eq!(
        first_key,
        scene3d_tex_from_element_attrs(&first.attrs, 80.0, 60.0)
            .unwrap()
            .0
    );
    take_scene3d_jobs();
}

/// Layout collect used to call `runtime::request_redraw` while `AppState` was already
/// `borrow_mut` → panic at `runtime.rs` + GLES WGL Drop on Windows.
#[cfg(feature = "scene3d")]
#[test]
fn scene3d_collect_does_not_reenter_appstate_borrow() {
    let mut attrs = std::collections::HashMap::new();
    attrs.insert(ATTR_SCENE3D.to_string(), "1".into());
    crate::app::runtime::call_while_app_state_mutably_borrowed(|| {
        assert!(scene3d_tex_from_element_attrs(&attrs, 64.0, 48.0).is_some());
    });
    assert!(!take_scene3d_jobs().is_empty());
}
