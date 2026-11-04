use jadix_native::{
    host::PlatformHost,
    layout::{LayoutError, MeasureRequest},
    presentation::*,
};
use jadix_native_controls::{table::DataTable, workbench::Workbench};

#[derive(Default)]
struct Host {
    events: Vec<PlatformEvent>,
    fail_measure: bool,
    scrolls: std::collections::BTreeMap<NodeId, [f64; 2]>,
}

impl PlatformHost for Host {
    fn measure(
        &mut self,
        request: MeasureRequest,
        _: &NodeDescriptor,
    ) -> Result<MeasureResult, LayoutError> {
        if self.fail_measure {
            return Err(LayoutError::MeasurementUnavailable(request.key.node));
        }
        Ok(MeasureResult {
            key: request.key,
            width: 100.0,
            height: 32.0,
            baseline: None,
        })
    }
    fn publish(&mut self, _: RootId, _: u64, _: &[PresentationNode]) -> Result<(), String> {
        Ok(())
    }
    fn dispose(&mut self) {}
    fn take_events(&mut self) -> Result<Vec<PlatformEvent>, String> {
        Ok(std::mem::take(&mut self.events))
    }
    fn scroll_to(&mut self, node: NodeId, offset: [f64; 2]) -> Result<(), String> {
        if self.scrolls.insert(node, offset) != Some(offset) {
            self.events.push(PlatformEvent {
                node,
                payload: EventPayload::Scroll {
                    x: offset[0],
                    y: offset[1],
                },
            });
        }
        Ok(())
    }
}

fn edit(node: NodeId, value: &str, revision: u64, composing: bool) -> PlatformEvent {
    let end = value.encode_utf16().count();
    PlatformEvent {
        node,
        payload: EventPayload::Edit {
            value: value.into(),
            revision,
            selection_utf16: [end, end],
            composing_utf16: composing.then_some([0, end]),
        },
    }
}

#[test]
fn table_preedit_survives_relayout_without_committing_until_ime_commit() {
    let mut table = DataTable::new(Host::default(), [640.0, 480.0], 100_000, 1000).unwrap();
    let input = table.cell(0, 0).unwrap();
    table
        .root
        .host_mut()
        .events
        .push(edit(input, "中文 😀", 1, true));
    table.poll_events().unwrap();
    assert_eq!(table.value(0, 0), Some("R000000 C0000"));
    assert_eq!(table.state()["edited_cells"], 0);
    table.resize([800.0, 480.0], 2).unwrap();
    assert_eq!(table.cell(0, 0), Some(input));
    assert!(
        matches!(&table.root.model().node(input).unwrap().descriptor.content,
        NodeContent::Input { value, revision: 1, .. } if value == "中文 😀")
    );
    table
        .root
        .host_mut()
        .events
        .push(edit(input, "中文 😀!", 2, false));
    table.poll_events().unwrap();
    assert_eq!(table.value(0, 0), Some("中文 😀!"));
    assert_eq!(table.state()["edited_cells"], 1);
}

#[test]
fn workbench_cancelled_preedit_restores_committed_value_and_retains_identity() {
    let mut workbench = Workbench::new(Host::default(), [640.0, 480.0], 100_000).unwrap();
    let input = workbench.input_node(0).unwrap();
    workbench
        .root
        .host_mut()
        .events
        .push(edit(input, "saved", 1, false));
    workbench.poll_events().unwrap();
    workbench
        .root
        .host_mut()
        .events
        .push(edit(input, "未完成", 2, true));
    workbench.poll_events().unwrap();
    assert_eq!(workbench.value(0), Some("saved"));
    workbench.resize([800.0, 480.0], 2).unwrap();
    workbench
        .root
        .host_mut()
        .events
        .push(edit(input, "saved", 3, false));
    workbench.poll_events().unwrap();
    assert_eq!(workbench.value(0), Some("saved"));
    assert_eq!(workbench.input_node(0), Some(input));
}

#[test]
fn failed_table_axis_changes_and_oversized_viewports_preserve_the_live_model() {
    let mut table = DataTable::new(Host::default(), [640.0, 480.0], 100_000, 1000).unwrap();
    let input = table.cell(0, 0).unwrap();
    let revision = table.root.model().revision();
    table.root.host_mut().fail_measure = true;
    assert!(table.set_rows(vec![99_999]).is_err());
    assert!(table.set_columns(vec![999]).is_err());
    assert!(table.measure_row(0, 64.0).is_err());
    assert!(table.measure_column(0, 320.0).is_err());
    table.root.host_mut().fail_measure = false;
    assert_eq!(table.state()["rows"], 100_000);
    assert_eq!(table.state()["columns"], 1000);
    assert_eq!(table.root.model().revision(), revision);
    assert!(table.resize([100_000.0, 480.0], 2).is_err());
    table.render().unwrap();
    assert_eq!(table.cell(0, 0), Some(input));
    assert_eq!(table.root.model().revision(), revision);
    let mut workbench = Workbench::new(Host::default(), [640.0, 480.0], 100_000).unwrap();
    let input = workbench.input_node(0);
    assert!(workbench.resize([640.0, 100_000.0], 2).is_err());
    workbench.render().unwrap();
    assert_eq!(workbench.input_node(0), input);
}

#[test]
fn frozen_table_panes_follow_both_axes_and_retain_composition_across_reordering() {
    let mut table = DataTable::new(Host::default(), [800.0, 480.0], 100_000, 1000).unwrap();
    let input = table.cell(0, 0).unwrap();
    table
        .root
        .host_mut()
        .events
        .push(edit(input, "冻结 😀", 1, true));
    table.poll_events().unwrap();
    table.set_frozen(1, 1).unwrap();
    table.scroll_to([80_000.0, 1_600_000.0]).unwrap();
    let panes = table.frozen_scroll_nodes();
    for (pane, expected) in panes.into_iter().zip([
        [0.0, 0.0],
        [80_000.0, 0.0],
        [0.0, 1_600_000.0],
        [80_000.0, 1_600_000.0],
    ]) {
        assert_eq!(table.root.host().scrolls[&pane], expected);
        assert_eq!(
            table
                .root
                .model()
                .node(pane)
                .unwrap()
                .descriptor
                .style
                .scrollbars,
            [false; 2]
        );
    }
    assert!(table.cell(0, 501).is_some());
    assert!(table.cell(50_001, 0).is_some());
    let body = table.cell(50_001, 501).unwrap();
    table.set_rows((0..100_000).rev().collect()).unwrap();
    table.set_columns((0..1000).rev().collect()).unwrap();
    assert_eq!(table.cell(50_001, 501), Some(body));
    assert_eq!(
        table.state()["offset"],
        serde_json::json!([79_520.0, 1_599_904.0])
    );
    table.measure_row(99_999, 64.0).unwrap();
    table.measure_column(999, 220.0).unwrap();
    assert_eq!(
        table.state()["offset"],
        serde_json::json!([79_520.0, 1_599_904.0])
    );
    table.set_frozen(0, 0).unwrap();
    table.set_frozen(1, 1).unwrap();
    assert_eq!(table.cell(0, 0), Some(input));
    assert!(
        matches!(&table.root.model().node(input).unwrap().descriptor.content,
        NodeContent::Input { value, revision: 1, .. } if value == "冻结 😀")
    );
    assert_eq!(table.value(0, 0), Some("R000000 C0000"));
    assert!(table.state()["mounted_cells"].as_u64().unwrap() <= 256);
    assert_eq!(table.poll_events().unwrap(), 0);
}

#[test]
fn invalid_frozen_panes_leave_the_live_table_and_axis_measurements_intact() {
    let mut table = DataTable::new(Host::default(), [640.0, 480.0], 100_000, 1000).unwrap();
    table.set_frozen(1, 1).unwrap();
    let state = table.state();
    let input = table.cell(1, 1);
    assert!(table.set_frozen(1, 4).is_err());
    assert!(table.set_frozen(100_001, 1).is_err());
    assert!(table.set_rows(vec![]).is_err());
    assert!(table.set_columns(vec![]).is_err());
    assert!(table.measure_row(0, 500.0).is_err());
    assert!(table.measure_column(0, 640.0).is_err());
    assert!(table.resize([160.0, 480.0], 2).is_err());
    assert_eq!(table.state(), state);
    table.render().unwrap();
    assert_eq!(table.cell(1, 1), input);
    assert_eq!(table.state(), state);
}

#[test]
fn table_navigation_reveals_by_business_key_after_sort_filter_and_measured_frozen_axes() {
    let mut table = DataTable::new(Host::default(), [800.0, 480.0], 100_000, 1000).unwrap();
    table.set_headers(true).unwrap();
    table.set_frozen(1, 1).unwrap();
    table.measure_row(0, 64.0).unwrap();
    table.measure_column(0, 220.0).unwrap();
    assert!(table.focus_cell(0, 0).unwrap());
    assert!(
        table
            .navigate_cell((0, 0), NavigationKey::End, false, true)
            .unwrap()
    );
    let last = table.cell(99_999, 999).unwrap();
    assert_eq!(table.root.model().focused(), Some(last));
    assert!(table.state()["mounted_cells"].as_u64().unwrap() <= 256);
    assert!(table.state()["offset"][0].as_f64().unwrap() > 150_000.0);
    assert!(table.state()["offset"][1].as_f64().unwrap() > 3_000_000.0);
    table
        .set_rows((0..100_000).rev().filter(|key| key % 2 == 0).collect())
        .unwrap();
    table.set_columns((0..1000).rev().collect()).unwrap();
    assert!(table.focus_cell(99_998, 999).unwrap());
    assert!(
        table
            .navigate_cell((99_998, 999), NavigationKey::Tab, false, false)
            .unwrap()
    );
    assert_eq!(table.root.model().focused(), table.cell(99_998, 998));
    assert!(
        table
            .navigate_cell((99_998, 998), NavigationKey::Enter, false, false)
            .unwrap()
    );
    assert_eq!(table.root.model().focused(), table.cell(99_996, 998));
    assert!(
        table
            .navigate_cell((99_996, 998), NavigationKey::PageDown, false, false)
            .unwrap()
    );
    let paged = table.state()["pinned"][0].as_u64().unwrap();
    assert!(paged < 99_994);
    assert!(
        table
            .navigate_cell((paged, 998), NavigationKey::Home, false, true)
            .unwrap()
    );
    assert_eq!(table.root.model().focused(), table.cell(99_998, 999));
    assert!(
        table
            .navigate_cell((99_998, 999), NavigationKey::Tab, true, false)
            .unwrap()
    );
    let focus = table.root.model().focused().unwrap();
    assert!(matches!(
        table.root.model().node(focus).unwrap().descriptor.content,
        NodeContent::Button { .. }
    ));
    assert!(
        !table
            .navigate_cell((99_998, 999), NavigationKey::Right, false, false)
            .unwrap()
    );
}

#[test]
fn table_navigation_preserves_live_state_on_measurement_failure_and_during_preedit() {
    let mut table = DataTable::new(Host::default(), [640.0, 480.0], 100_000, 1000).unwrap();
    table.focus_cell(0, 0).unwrap();
    let input = table.cell(0, 0).unwrap();
    let before = table.state();
    let revision = table.root.model().revision();
    table.root.host_mut().fail_measure = true;
    assert!(table.focus_cell(99_999, 999).is_err());
    table.root.host_mut().fail_measure = false;
    assert_eq!(table.state(), before);
    assert_eq!(table.root.model().revision(), revision);
    assert_eq!(table.root.model().focused(), Some(input));
    table
        .root
        .host_mut()
        .events
        .push(edit(input, "未完成😀", 1, true));
    table.poll_events().unwrap();
    let composing = table.state();
    assert!(
        !table
            .navigate_cell((0, 0), NavigationKey::End, false, true)
            .unwrap()
    );
    assert!(!table.focus_cell(99_999, 999).unwrap());
    assert_eq!(table.state(), composing);
    assert_eq!(table.root.model().focused(), Some(input));
    table
        .root
        .host_mut()
        .events
        .push(edit(input, "提交😀", 2, false));
    table.root.host_mut().events.push(PlatformEvent {
        node: input,
        payload: EventPayload::Navigate {
            key: NavigationKey::Tab,
            shift: false,
            control: false,
        },
    });
    table.poll_events().unwrap();
    assert_eq!(table.value(0, 0), Some("提交😀"));
    assert_eq!(table.root.model().focused(), table.cell(0, 1));
    assert!(table.focus_cell(0, 0).unwrap());
    assert_eq!(table.cell(0, 0), Some(input));
}
