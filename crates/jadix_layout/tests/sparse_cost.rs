//! Actual allocation traffic for operation-driven layout of unchanged records.
use jadix_core::presentation::{style::NodeStyle, *};
use jadix_layout::presentation::*;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
struct CountingAllocator;
thread_local! {
    static BYTES: Cell<Option<usize>> = const { Cell::new(None) };
    static DESCRIPTIONS: Cell<usize> = const { Cell::new(0) };
}
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        BYTES.with(|n| {
            if let Some(bytes) = n.get() {
                n.set(Some(bytes + layout.size()));
                if (2200..2240).contains(&layout.size()) {
                    DESCRIPTIONS.with(|v| v.set(v.get() + layout.size()));
                }
            }
        });
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        BYTES.with(|n| {
            if let Some(bytes) = n.get() {
                n.set(Some(bytes + size));
                if (2200..2240).contains(&size) {
                    DESCRIPTIONS.with(|v| v.set(v.get() + size));
                }
            }
        });
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;
fn measured<T>(run: impl FnOnce() -> T) -> (T, usize, usize) {
    BYTES.with(|n| n.set(Some(0)));
    DESCRIPTIONS.with(|n| n.set(0));
    let result = run();
    let bytes = BYTES.with(|n| n.replace(None).unwrap());
    (result, bytes, DESCRIPTIONS.with(Cell::get))
}

fn descriptor(content: NodeContent) -> NodeDescriptor {
    NodeDescriptor {
        content,
        frame: Rect::default(),
        accessible_description: None,
        accessible_name: None,
        accessible_role: None,
        accessible_selected: None,
        accessible_expanded: None,
        interaction: Default::default(),
        keyboard: Default::default(),
        navigation_keys: Vec::new(),
        accessibility: Default::default(),
        accessibility_relations: Default::default(),
        style: NodeStyle {
            width: Some(320.),
            height: Some(20.),
            shrink: 0.,
            ..Default::default()
        },
    }
}
fn measure(
    request: MeasureRequest,
    descriptor: &NodeDescriptor,
) -> Result<MeasureResult, LayoutError> {
    let width = match &descriptor.content {
        NodeContent::Text(text) => text.len() as f64 * 8.,
        _ => 0.,
    };
    Ok(MeasureResult {
        key: request.key,
        width: width.min(request.key.max_width),
        height: 20f64.min(request.key.max_height),
        baseline: None,
    })
}
#[test]
fn unchanged_business_descriptions_are_not_copied_into_a_second_candidate_forest() {
    let mut costs = vec![];
    for rows in [32, 256, 1024, 3072] {
        let life = RootLifetime::new(RootId::new());
        let root = NodeId::new(life.id());
        let mut container = descriptor(NodeContent::Container);
        container.style.height = None;
        let mut operations = vec![Operation::Create {
            node: root,
            parent: None,
            index: 0,
            descriptor: container,
        }];
        let mut first = None;
        let mut payload = 0;
        for index in 0..rows {
            let id = NodeId::new(life.id());
            let mut d = descriptor(NodeContent::Text(format!("Record {index}: 0")));
            d.accessible_name = Some(format!(
                "Record {index}: {}",
                "Business description. ".repeat(100)
            ));
            payload += d.accessible_name.as_ref().unwrap().len();
            if first.is_none() {
                first = Some((id, d.clone()));
            }
            operations.push(Operation::Create {
                node: id,
                parent: Some(root),
                index,
                descriptor: d,
            });
        }
        let mut layout = StructuralLayout::new(life.clone());
        layout
            .commit(
                &CommitBatch {
                    root: life.id(),
                    previous: 0,
                    revision: 1,
                    operations,
                },
                [320., 240.],
                0,
                measure,
            )
            .unwrap();
        let (changed, mut updated) = first.unwrap();
        updated.content = NodeContent::Text("Record 0: 1".into());
        let batch = CommitBatch {
            root: life.id(),
            previous: 1,
            revision: 2,
            operations: vec![Operation::Update {
                node: changed,
                descriptor: updated,
            }],
        };
        let start = std::time::Instant::now();
        let (result, bytes, copied_payload) =
            measured(|| layout.commit(&batch, [320., 240.], 0, measure));
        assert_eq!(result.unwrap().sync_nodes, 1);
        assert!(
            bytes <= 65536,
            "warm one-leaf commit allocated {bytes} bytes at {rows} rows"
        );
        let elapsed = start.elapsed().as_nanos();
        let mut reference = layout.roots().to_vec();
        PresentationLayout::new(life)
            .layout(&mut reference, [320., 240.], 0, measure)
            .unwrap();
        assert_eq!(layout.roots(), reference);
        // Count actual requests in the description-string size class. Other
        // engine work buffers remain in `bytes`; do not misclassify them as
        // additional descriptor copies or claim total allocation is O(k).
        let budget = 16 * 2240;
        println!("rows={rows} payload={payload} copied_payload={copied_payload} allocated={bytes} budget={budget} nanos={elapsed}");
        costs.push((rows, copied_payload, budget));
    }
    assert!(
        costs.iter().all(|(_, bytes, budget)| bytes <= budget),
        "unchanged payload copied into an extra candidate forest: {costs:?}"
    );
}
