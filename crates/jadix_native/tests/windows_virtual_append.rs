#![cfg(all(windows, feature = "windows"))]

use jadix_core::virtualization::VirtualAxis;
use jadix_native::{host::NativeRoot, presentation::*, windows::WindowsHost};
use std::{collections::BTreeMap, time::Instant};

struct Window {
    root: NativeRoot<WindowsHost>,
    editor: NodeId,
    rows: BTreeMap<u64, NodeId>,
}

impl Window {
    fn new(title: &str) -> Self {
        let root = NativeRoot::new(WindowsHost::new(title, [420, 340]).unwrap());
        let editor = NodeId::new(root.model().id());
        root.host().show();
        Self {
            root,
            editor,
            rows: BTreeMap::new(),
        }
    }

    fn present(&mut self, axis: &VirtualAxis<u64>) -> Vec<NodeId> {
        let root_id = self.root.model().id();
        let mut nodes = vec![node(
            self.editor,
            NodeContent::Input {
                value: "seed".into(),
                placeholder: String::new(),
                enabled: true,
                password: false,
                max_length: None,
                revision: 0,
            },
            32.0,
        )];
        let window = axis
            .window(axis.total() - 240.0, 240.0, 0.0, &[], 10)
            .unwrap();
        self.rows
            .retain(|key, _| window.visible.contains(&(*key as usize)));
        for index in window.mounted {
            let key = *axis.key(index).unwrap();
            let id = *self.rows.entry(key).or_insert_with(|| NodeId::new(root_id));
            nodes.push(node(id, NodeContent::Text(format!("Row {key}")), 24.0));
        }
        let environment = self.root.host().environment();
        self.root
            .present(&mut nodes, [420.0, 340.0], environment)
            .unwrap();
        self.root.host().pump();
        nodes.into_iter().skip(1).map(|row| row.id).collect()
    }

    fn read_rows(&self, rows: &[NodeId]) -> Vec<(String, Rect)> {
        rows.iter()
            .map(|id| {
                (
                    self.root.host().text(*id).unwrap(),
                    self.root.host().control_frame(*id).unwrap(),
                )
            })
            .collect()
    }
}

fn node(id: NodeId, content: NodeContent, height: f32) -> PresentationNode {
    PresentationNode {
        id,
        descriptor: NodeDescriptor {
            content,
            frame: Rect::default(),
            accessible_description: None,
            accessible_name: None,
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            accessibility_relations: Default::default(),
            style: style::NodeStyle {
                width: Some(360.0),
                height: Some(height),
                shrink: 0.0,
                ..Default::default()
            },
        },
        children: vec![],
    }
}

fn median(samples: &mut [u128]) -> u128 {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

#[test]
fn virtual_append_commits_the_same_visible_rows_to_real_windows() {
    let mut appended = Window::new("Jadix keyed append acceptance");
    let mut replaced = Window::new("Jadix replacement acceptance");
    println!("existing,added,append_host_ns,replacement_host_ns,replacement_over_append");
    for count in [1_024u64, 16_384, 131_072] {
        for added in [1u64, 32] {
            let mut append_times = Vec::new();
            let mut replace_times = Vec::new();
            for round in 0..13 {
                let mut axis = VirtualAxis::new(0..count, 24.0).unwrap();
                let mut reference = VirtualAxis::new(0..count, 24.0).unwrap();
                axis.measure(&7, 41.0).unwrap();
                reference.measure(&7, 41.0).unwrap();
                let baseline = appended.present(&axis);
                let other_baseline = replaced.present(&reference);
                assert_eq!(
                    appended.read_rows(&baseline),
                    replaced.read_rows(&other_baseline)
                );
                let identities = [
                    appended.root.host().control_identity(appended.editor),
                    replaced.root.host().control_identity(replaced.editor),
                ];
                for window in [&appended, &replaced] {
                    window
                        .root
                        .host()
                        .select_range(window.editor, [1, 3])
                        .unwrap();
                }
                let append_run = |window: &mut Window, axis: &mut VirtualAxis<u64>| {
                    let start = Instant::now();
                    axis.append((count..count + added).map(|key| (key, 24.0)))
                        .unwrap();
                    let rows = window.present(axis);
                    (start.elapsed().as_nanos(), rows)
                };
                let replace_run = |window: &mut Window, axis: &mut VirtualAxis<u64>| {
                    let start = Instant::now();
                    axis.replace(0..count + added, 24.0).unwrap();
                    let rows = window.present(axis);
                    (start.elapsed().as_nanos(), rows)
                };
                let ((a, rows), (b, other_rows)) = if round % 2 == 0 {
                    (
                        append_run(&mut appended, &mut axis),
                        replace_run(&mut replaced, &mut reference),
                    )
                } else {
                    let b = replace_run(&mut replaced, &mut reference);
                    (append_run(&mut appended, &mut axis), b)
                };
                let rows = appended.read_rows(&rows);
                let other_rows = replaced.read_rows(&other_rows);
                assert_eq!(rows, other_rows);
                assert_eq!(rows.len(), 10);
                assert_eq!(rows.last().unwrap().0, format!("Row {}", count + added - 1));
                assert_eq!(axis.total(), reference.total());
                for (window, identity) in [&appended, &replaced].into_iter().zip(identities) {
                    assert_eq!(window.root.host().control_identity(window.editor), identity);
                    assert_eq!(window.root.host().selection(window.editor).unwrap(), [1, 3]);
                    assert_eq!(window.root.host().text(window.editor).unwrap(), "seed");
                }
                if round > 1 {
                    append_times.push(a);
                    replace_times.push(b);
                }
            }
            let a = median(&mut append_times);
            let b = median(&mut replace_times);
            println!("{count},{added},{a},{b},{:.2}", b as f64 / a.max(1) as f64);
        }
    }
    // Preserve a live edit and native undo through one more actual commit.
    let mut axis = VirtualAxis::new(0..131_104u64, 24.0).unwrap();
    axis.measure(&7, 41.0).unwrap();
    axis.append([(131_104, 24.0)]).unwrap();
    for window in [&mut appended, &mut replaced] {
        window.root.focus(Some(window.editor)).unwrap();
        window
            .root
            .host()
            .replace_selection(window.editor, "XYZ")
            .unwrap();
        assert_eq!(window.root.host().text(window.editor).unwrap(), "sXYZd");
        let identity = window.root.host().control_identity(window.editor).unwrap();
        window.present(&axis);
        assert_eq!(window.root.host().text(window.editor).unwrap(), "sXYZd");
        assert_eq!(window.root.host().selection(window.editor).unwrap(), [4, 4]);
        assert_eq!(
            unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetFocus() } as usize,
            identity
        );
        window.root.host().undo(window.editor).unwrap();
        assert_eq!(window.root.host().text(window.editor).unwrap(), "seed");
    }
}
