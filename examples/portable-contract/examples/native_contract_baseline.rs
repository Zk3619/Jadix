use jadix_native::presentation::hybrid::{host::*, policy::*, *};
use jadix_native::{layout::*, presentation::*, HeadlessRoot};
use std::time::Instant;

struct ReferenceAdapter;

impl NativeAdapter for ReferenceAdapter {
    fn capabilities(&self) -> EmbeddingCapabilities {
        EmbeddingCapabilities {
            overlay: true,
            rectangular_clip: true,
            platform_semantics: true,
            ..Default::default()
        }
    }
    fn prepare(&mut self, _: u64, _: &[HybridGeometry]) -> Result<(), CompositionError> {
        Ok(())
    }
    fn publish(&mut self, _: u64) -> Result<(), CompositionError> {
        Ok(())
    }
    fn cancel(&mut self, _: u64) -> Result<(), CompositionError> {
        Ok(())
    }
    fn dispose(&mut self) {}
}

fn node(root: RootId, content: NodeContent) -> PresentationNode {
    PresentationNode {
        id: NodeId::new(root),
        descriptor: NodeDescriptor {
            content,
            frame: Rect::default(),
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            accessible_description: None,
            accessible_name: None,
            style: Default::default(),
        },
        children: vec![],
    }
}

fn measure(request: MeasureRequest, _: &NodeDescriptor) -> Result<MeasureResult, LayoutError> {
    Ok(MeasureResult {
        key: request.key,
        width: 100.0f64.min(request.key.max_width),
        height: 24.0f64.min(request.key.max_height),
        baseline: None,
    })
}

fn percentile(samples: &mut [f64], percent: usize) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[(samples.len() * percent).div_ceil(100).saturating_sub(1)]
}

fn main() {
    for rows in [100usize, 1000] {
        let mut backend = HeadlessRoot::new();
        let lifetime = backend.lifetime();
        let mut layout = NativeLayout::new(lifetime.clone());
        let mut hybrid = HybridHost::new(lifetime.clone(), ReferenceAdapter);
        let mut root = node(lifetime.id(), NodeContent::Container);
        root.descriptor.style.width = Some(320.0);
        root.children = (0..rows)
            .map(|index| node(lifetime.id(), NodeContent::Text(format!("row {index}"))))
            .collect();
        let mut roots = vec![root];
        let mut changed = vec![];
        let mut idle = vec![];
        let mut composition = vec![];
        let mut idle_measurements = 0;
        for iteration in 0..35 {
            roots[0].children[0].descriptor.content =
                NodeContent::Text(format!("edit {iteration}"));
            let started = Instant::now();
            layout
                .layout(&mut roots, [320.0, 800.0], 1, measure)
                .unwrap();
            assert!(backend.reconcile(&roots).unwrap());
            if iteration >= 5 {
                changed.push(started.elapsed().as_secs_f64() * 1000.0);
            }
            let revision = iteration + 1;
            let geometry = roots[0]
                .children
                .iter()
                .enumerate()
                .map(|(index, child)| HybridGeometry {
                    node: child.id,
                    revision,
                    presentation: if index % 2 == 0 {
                        Presentation::Gpu
                    } else {
                        Presentation::Native
                    },
                    frame: child.descriptor.frame,
                    clips: vec![],
                    visible: true,
                    interactive: true,
                    z_index: if index % 2 == 0 { 0 } else { 1 },
                })
                .collect();
            let started = Instant::now();
            hybrid
                .stage(revision, CompositionMode::Overlay, geometry)
                .unwrap();
            assert!(!hybrid.present(revision, false).unwrap());
            assert!(hybrid.present(revision, true).unwrap());
            if iteration >= 5 {
                composition.push(started.elapsed().as_secs_f64() * 1000.0);
            }
            let started = Instant::now();
            let stats = layout
                .layout(&mut roots, [320.0, 800.0], 1, measure)
                .unwrap();
            idle_measurements += stats.measurements;
            assert!(!backend.reconcile(&roots).unwrap());
            if iteration >= 5 {
                idle.push(started.elapsed().as_secs_f64() * 1000.0);
            }
        }
        assert_eq!(idle_measurements, 0);
        let p50 = percentile(&mut changed, 50);
        let p95 = percentile(&mut changed, 95);
        let idle_p95 = percentile(&mut idle, 95);
        let hybrid_p95 = percentile(&mut composition, 95);
        println!("{{\"rows\":{rows},\"samples\":30,\"changed_p50_ms\":{p50},\"changed_p95_ms\":{p95},\"idle_p95_ms\":{idle_p95},\"hybrid_p95_ms\":{hybrid_p95},\"idle_measurements\":{idle_measurements},\"idle_commits\":0}}");
        backend.dispose();
    }
}
