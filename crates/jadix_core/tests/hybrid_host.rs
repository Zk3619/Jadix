use jadix_core::presentation::{
    hybrid::{host::*, policy::*, *},
    *,
};
use std::{cell::RefCell, rc::Rc};

fn geometry(node: NodeId, revision: u64, presentation: Presentation, x: f64) -> HybridGeometry {
    HybridGeometry {
        node,
        revision,
        presentation,
        frame: Rect {
            x,
            y: 0.0,
            width: 20.0,
            height: 20.0,
        },
        clips: Vec::new(),
        visible: true,
        interactive: true,
        z_index: if presentation == Presentation::Native {
            1
        } else {
            0
        },
    }
}

#[derive(Clone, Default)]
struct Probe {
    calls: Rc<RefCell<Vec<&'static str>>>,
    fail: bool,
}
impl NativeAdapter for Probe {
    fn capabilities(&self) -> EmbeddingCapabilities {
        EmbeddingCapabilities {
            overlay: true,
            rectangular_clip: true,
            platform_semantics: true,
            ..Default::default()
        }
    }
    fn prepare(&mut self, _: u64, _: &[HybridGeometry]) -> Result<(), CompositionError> {
        self.calls.borrow_mut().push("prepare");
        Ok(())
    }
    fn publish(&mut self, _: u64) -> Result<(), CompositionError> {
        self.calls.borrow_mut().push("publish");
        if self.fail {
            Err(CompositionError::AdapterFailure)
        } else {
            Ok(())
        }
    }
    fn cancel(&mut self, _: u64) -> Result<(), CompositionError> {
        self.calls.borrow_mut().push("cancel");
        Ok(())
    }
    fn dispose(&mut self) {
        self.calls.borrow_mut().push("dispose");
    }
}

#[test]
fn registered_adapter_waits_for_gpu_and_damage_covers_old_and_new_holes() {
    let lifetime = RootLifetime::new(RootId::new());
    let native = NodeId::new(lifetime.id());
    let gpu = NodeId::new(lifetime.id());
    let probe = Probe::default();
    let mut host = HybridHost::new(lifetime, probe.clone());
    host.stage(
        1,
        CompositionMode::Overlay,
        vec![
            geometry(gpu, 1, Presentation::Gpu, 0.0),
            geometry(native, 1, Presentation::Native, 0.0),
        ],
    )
    .unwrap();
    assert!(!host.present(1, false).unwrap());
    assert_eq!(*probe.calls.borrow(), ["prepare"]);
    assert!(host.coordinator().hit(5.0, 5.0).is_none());
    assert!(host.present(1, true).unwrap());
    assert_eq!(host.coordinator().hit(5.0, 5.0).unwrap().node, native);
    assert_eq!(
        host.coordinator().semantic_owner(native),
        Some(Presentation::Native)
    );
    host.stage(
        2,
        CompositionMode::Overlay,
        vec![geometry(native, 2, Presentation::Native, 40.0)],
    )
    .unwrap();
    host.present(2, false).unwrap();
    assert_eq!(host.damage().len(), 2);
    assert_eq!(host.damage()[0].x, 0.0);
    assert_eq!(host.damage()[1].x, 40.0);
    host.stage(
        3,
        CompositionMode::Overlay,
        vec![geometry(native, 3, Presentation::Native, 80.0)],
    )
    .unwrap();
    host.cancel(3).unwrap();
    assert_eq!(host.coordinator().visible_revision(), 2);
    assert_eq!(host.coordinator().hit(45.0, 5.0).unwrap().node, native);
}

#[test]
fn platform_failure_revokes_only_its_root_and_disposal_is_once() {
    let lifetime = RootLifetime::new(RootId::new());
    let other = RootLifetime::new(RootId::new());
    let probe = Probe {
        fail: true,
        ..Default::default()
    };
    let mut host = HybridHost::new(lifetime.clone(), probe.clone());
    host.stage(
        1,
        CompositionMode::Overlay,
        vec![geometry(
            NodeId::new(lifetime.id()),
            1,
            Presentation::Native,
            0.0,
        )],
    )
    .unwrap();
    assert_eq!(host.present(1, true), Err(CompositionError::AdapterFailure));
    assert!(lifetime.is_disposed());
    assert!(!other.is_disposed());
    assert!(host.coordinator().hit(1.0, 1.0).is_none());
    host.dispose();
    drop(host);
    assert_eq!(
        probe
            .calls
            .borrow()
            .iter()
            .filter(|call| **call == "dispose")
            .count(),
        1
    );
}

#[test]
fn composition_started_after_staging_is_checked_before_platform_publish() {
    let lifetime = RootLifetime::new(RootId::new());
    let node = NodeId::new(lifetime.id());
    let probe = Probe::default();
    let mut host = HybridHost::new(lifetime.clone(), probe.clone());
    host.stage(
        1,
        CompositionMode::Overlay,
        vec![geometry(node, 1, Presentation::Native, 0.0)],
    )
    .unwrap();
    host.present(1, true).unwrap();
    assert!(host.focus(1, node));
    host.stage(2, CompositionMode::Overlay, vec![]).unwrap();
    assert!(host.set_composing(1, node, true));
    assert_eq!(host.present(2, true), Err(CompositionError::Revision));
    assert_eq!(*probe.calls.borrow(), ["prepare", "publish", "prepare"]);
    assert!(!lifetime.is_disposed());
    assert!(host.set_composing(1, node, false));
    assert!(host.present(2, true).unwrap());
}

#[test]
fn composition_caps_refuse_unsupported_interleaving_and_snapshot_input() {
    let root = RootId::new();
    let native = geometry(NodeId::new(root), 1, Presentation::Native, 0.0);
    let mut gpu = geometry(NodeId::new(root), 1, Presentation::Gpu, 0.0);
    gpu.z_index = 2;
    let caps = EmbeddingCapabilities {
        overlay: true,
        snapshot: true,
        platform_semantics: true,
        ..Default::default()
    };
    assert_eq!(
        caps.validate(CompositionMode::Overlay, &[native.clone(), gpu]),
        Err(CompositionError::OverlapOrder)
    );
    assert_eq!(
        caps.validate(CompositionMode::Snapshot, std::slice::from_ref(&native)),
        Err(CompositionError::InteractiveSnapshot)
    );
    let mut clipped = native;
    clipped.clips.push(clipped.frame);
    assert_eq!(
        caps.validate(CompositionMode::Overlay, &[clipped]),
        Err(CompositionError::UnsupportedClip)
    );
    assert_eq!(
        select(
            PresentationIntent::RequireNative,
            Presentation::Gpu,
            true,
            false
        ),
        Err(CompositionError::UnsupportedPresenter)
    );
    assert_eq!(
        select(
            PresentationIntent::PreferNative,
            Presentation::Gpu,
            true,
            false
        ),
        Ok(Selection {
            presentation: Presentation::Gpu,
            fallback: true
        })
    );
}

#[test]
fn capture_routes_once_outside_bounds_and_composition_requires_explicit_completion() {
    let root = RootId::new();
    let native = NodeId::new(root);
    let gpu = NodeId::new(root);
    let mut coordinator = HybridCoordinator::new(root);
    assert!(coordinator.stage(
        1,
        vec![
            geometry(native, 1, Presentation::Native, 0.0),
            geometry(gpu, 1, Presentation::Gpu, 40.0)
        ]
    ));
    assert!(coordinator.confirm(1, true, true));
    assert!(coordinator.focus(1, native));
    assert!(coordinator.set_composing(1, native, true));
    assert!(!coordinator.focus(1, gpu));
    assert!(!coordinator.blur(1, native));
    assert!(!coordinator.stage(2, vec![]));
    assert!(coordinator.capture_pointer(1, native));
    assert_eq!(coordinator.pointer_target(45.0, 1.0).unwrap().node, native);
    assert!(!coordinator.release_pointer(0, native));
    assert!(coordinator.release_pointer(1, native));
    assert_eq!(coordinator.pointer_target(45.0, 1.0).unwrap().node, gpu);
    assert!(coordinator.set_composing(1, native, false));
    assert!(coordinator.stage(2, vec![]));
    assert!(coordinator.confirm(2, true, true));
    assert!(coordinator.focused().is_none());
    assert!(coordinator.pointer_target(1.0, 1.0).is_none());
}

#[test]
fn geometry_converts_scroll_offset_safe_area_and_scale_once() {
    let transform = ViewportTransform {
        scale: 2.0,
        offset: [-5.0, -10.0],
        safe_area: [8.0, 0.0, 0.0, 3.0],
    };
    let frame = transform
        .physical(Rect {
            x: 10.0,
            y: 20.0,
            width: 30.0,
            height: 40.0,
        })
        .unwrap();
    assert_eq!(
        frame,
        Rect {
            x: 16.0,
            y: 36.0,
            width: 60.0,
            height: 80.0
        }
    );
    assert_eq!(
        ViewportTransform {
            scale: f64::NAN,
            ..transform
        }
        .physical(frame),
        Err(CompositionError::InvalidGeometry)
    );
}
