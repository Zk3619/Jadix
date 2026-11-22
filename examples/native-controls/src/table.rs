use std::collections::BTreeMap;

use crate::editing::Draft;

use jadix_core::virtualization::{VirtualAxis, VirtualGrid};
use jadix_native::{
    host::{NativeRoot, PlatformHost},
    presentation::*,
    ResourceLimits,
};

mod headers;
mod navigation;

#[derive(Clone)]
struct Cell {
    input: NodeId,
    gap: NodeId,
    semantics: NodeId,
}

#[derive(Clone)]
struct Header {
    label: NodeId,
    gap: NodeId,
}

#[derive(Clone)]
struct Row {
    container: [NodeId; 2],
    gap: [NodeId; 2],
    tail: [NodeId; 2],
    cells: BTreeMap<u64, Cell>,
}

pub struct DataTable<Host: PlatformHost> {
    pub root: NativeRoot<Host>,
    grid: VirtualGrid<u64, u64>,
    rows: BTreeMap<u64, Row>,
    drafts: BTreeMap<(u64, u64), Draft>,
    scroll: [NodeId; 4],
    body: [NodeId; 4],
    bottom: [NodeId; 4],
    bands: [NodeId; 2],
    outer: NodeId,
    toolbar: NodeId,
    actions: [NodeId; 4],
    descending: bool,
    even_only: bool,
    expanded: bool,
    offset: [f64; 2],
    viewport: [f32; 2],
    environment: u64,
    pinned: Option<(u64, u64)>,
    maximum: [u64; 2],
    frozen: [usize; 2],
    headers_enabled: bool,
    headers: BTreeMap<u64, Header>,
    keyboard_navigation: bool,
    header_chrome: [NodeId; 7],
}

fn node(identifier: NodeId, content: NodeContent, width: f64, height: f64) -> PresentationNode {
    PresentationNode {
        id: identifier,
        descriptor: NodeDescriptor {
            content,
            frame: Rect::default(),
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            accessible_description: None,
            accessible_name: None,
            interaction: Default::default(),
            keyboard: Default::default(),
            navigation_keys: Vec::new(),
            accessibility: Default::default(),
            accessibility_relations: Default::default(),
            style: style::NodeStyle {
                width: Some(width as f32),
                height: Some(height as f32),
                shrink: 0.0,
                ..Default::default()
            },
        },
        children: vec![],
    }
}

impl<Host: PlatformHost> DataTable<Host> {
    pub fn new(host: Host, viewport: [f32; 2], rows: u64, columns: u64) -> Result<Self, String> {
        let grid = VirtualGrid {
            rows: VirtualAxis::new(0..rows, 32.0).map_err(|error| format!("{error:?}"))?,
            columns: VirtualAxis::new(0..columns, 160.0).map_err(|error| format!("{error:?}"))?,
        };
        let root = NativeRoot::with_limits(
            host,
            ResourceLimits {
                retired_nodes: 4_000_000,
                ..Default::default()
            },
        );
        let identifier = root.model().id();
        let mut table = Self {
            root,
            grid,
            rows: BTreeMap::new(),
            drafts: BTreeMap::new(),
            scroll: std::array::from_fn(|_| NodeId::new(identifier)),
            body: std::array::from_fn(|_| NodeId::new(identifier)),
            bottom: std::array::from_fn(|_| NodeId::new(identifier)),
            bands: std::array::from_fn(|_| NodeId::new(identifier)),
            outer: NodeId::new(identifier),
            toolbar: NodeId::new(identifier),
            actions: std::array::from_fn(|_| NodeId::new(identifier)),
            descending: false,
            even_only: false,
            expanded: false,
            offset: [0.0; 2],
            viewport,
            environment: 1,
            pinned: None,
            maximum: [columns, rows],
            frozen: [0; 2],
            headers_enabled: false,
            headers: BTreeMap::new(),
            keyboard_navigation: false,
            header_chrome: std::array::from_fn(|_| NodeId::new(identifier)),
        };
        table.render()?;
        Ok(table)
    }

    pub fn cell(&self, row: u64, column: u64) -> Option<NodeId> {
        self.rows
            .get(&row)?
            .cells
            .get(&column)
            .map(|cell| cell.input)
    }

    pub fn value(&self, row: u64, column: u64) -> Option<&str> {
        self.drafts
            .get(&(row, column))
            .map(|draft| draft.value.as_str())
    }

    pub fn scroll_node(&self) -> NodeId {
        self.scroll[3]
    }

    pub fn semantics_node(&self) -> NodeId {
        self.outer
    }

    pub fn cell_semantics(&self, row: u64, column: u64) -> Option<NodeId> {
        self.rows
            .get(&row)?
            .cells
            .get(&column)
            .map(|cell| cell.semantics)
    }

    pub fn frozen_scroll_nodes(&self) -> [NodeId; 4] {
        self.scroll
    }

    pub fn column_header(&self, key: u64) -> Option<NodeId> {
        self.headers.get(&key).map(|header| header.label)
    }

    fn chrome_height(&self) -> f32 {
        36.0 + if self.headers_enabled { 32.0 } else { 0.0 }
    }

    pub fn set_headers(&mut self, enabled: bool) -> Result<(), String> {
        let previous = self.headers_enabled;
        self.headers_enabled = enabled;
        if let Err(error) = self.render() {
            self.headers_enabled = previous;
            return Err(error);
        }
        self.sync_panes()
    }

    fn frozen_size(&self) -> Result<[f64; 2], String> {
        let size = [
            self.grid
                .columns
                .position(self.frozen[0])
                .ok_or("frozen columns exceed inventory")?,
            self.grid
                .rows
                .position(self.frozen[1])
                .ok_or("frozen rows exceed inventory")?,
        ];
        if size[0] >= f64::from(self.viewport[0])
            || size[1] >= f64::from((self.viewport[1] - self.chrome_height()).max(0.0))
        {
            return Err(String::from("frozen panes must leave a scrolling viewport"));
        }
        Ok(size)
    }

    pub fn set_frozen(&mut self, rows: usize, columns: usize) -> Result<(), String> {
        self.sync_panes()?;
        let previous = self.frozen;
        self.frozen = [columns, rows];
        if let Err(error) = self.render() {
            self.frozen = previous;
            return Err(error);
        }
        self.sync_panes()
    }

    fn sync_panes(&mut self) -> Result<(), String> {
        let offsets = [
            [0.0, 0.0],
            [self.offset[0], 0.0],
            [0.0, self.offset[1]],
            self.offset,
        ];
        for (pane, offset) in offsets.into_iter().enumerate() {
            if self.root.model().node(self.scroll[pane]).is_some() {
                self.root
                    .scroll_to(self.scroll[pane], offset)
                    .map_err(|error| error.to_string())?;
            }
        }
        for side in 0..2 {
            let scroll = self.header_chrome[1 + side];
            if self.root.model().node(scroll).is_some() {
                self.root
                    .scroll_to(scroll, [if side == 0 { 0.0 } else { self.offset[0] }, 0.0])
                    .map_err(|error| error.to_string())?;
            }
        }
        self.poll_events()?;
        Ok(())
    }

    pub fn scroll_to(&mut self, offset: [f64; 2]) -> Result<(), String> {
        self.root
            .scroll_to(self.scroll[3], offset)
            .map_err(|error| error.to_string())?;
        self.poll_events()?;
        self.sync_panes()
    }

    pub fn resize(&mut self, viewport: [f32; 2], environment: u64) -> Result<(), String> {
        let previous = (self.viewport, self.environment);
        self.viewport = viewport;
        self.environment = environment;
        if let Err(error) = self.render() {
            (self.viewport, self.environment) = previous;
            return Err(error);
        }
        Ok(())
    }

    pub fn set_rows(&mut self, keys: Vec<u64>) -> Result<(), String> {
        if keys.iter().any(|key| *key >= self.maximum[1]) {
            return Err(String::from("unknown table row"));
        }
        let anchor = self
            .grid
            .rows
            .anchor(self.offset[1] + self.frozen_size()?[1])
            .map_err(|error| format!("{error:?}"))?;
        let replacement = self
            .grid
            .rows
            .replacement(keys, 32.0)
            .map_err(|error| format!("{error:?}"))?;
        let frozen = replacement
            .position(self.frozen[1])
            .ok_or("frozen rows exceed replacement")?;
        let previous = std::mem::replace(&mut self.grid.rows, replacement);
        let before = (self.offset, self.pinned);
        self.offset[1] = anchor
            .as_ref()
            .and_then(|anchor| {
                self.grid
                    .rows
                    .restore_anchor(
                        anchor,
                        (f64::from(self.viewport[1] - self.chrome_height()) - frozen).max(0.0),
                    )
                    .ok()
            })
            .map(|offset| (offset - frozen).max(0.0))
            .unwrap_or(0.0);
        if self
            .pinned
            .is_some_and(|(row, _)| self.grid.rows.index(&row).is_none())
        {
            self.pinned = None;
        }
        if let Err(error) = self.render() {
            self.grid.rows = previous;
            (self.offset, self.pinned) = before;
            return Err(error);
        }
        self.scroll_to(self.offset)
    }

    pub fn set_columns(&mut self, keys: Vec<u64>) -> Result<(), String> {
        if keys.iter().any(|key| *key >= self.maximum[0]) {
            return Err(String::from("unknown table column"));
        }
        let anchor = self
            .grid
            .columns
            .anchor(self.offset[0] + self.frozen_size()?[0])
            .map_err(|error| format!("{error:?}"))?;
        let replacement = self
            .grid
            .columns
            .replacement(keys, 160.0)
            .map_err(|error| format!("{error:?}"))?;
        let frozen = replacement
            .position(self.frozen[0])
            .ok_or("frozen columns exceed replacement")?;
        let previous = std::mem::replace(&mut self.grid.columns, replacement);
        let before = (self.offset, self.pinned);
        self.offset[0] = anchor
            .as_ref()
            .and_then(|anchor| {
                self.grid
                    .columns
                    .restore_anchor(anchor, (f64::from(self.viewport[0]) - frozen).max(0.0))
                    .ok()
            })
            .map(|offset| (offset - frozen).max(0.0))
            .unwrap_or(0.0);
        if self
            .pinned
            .is_some_and(|(_, column)| self.grid.columns.index(&column).is_none())
        {
            self.pinned = None;
        }
        if let Err(error) = self.render() {
            self.grid.columns = previous;
            (self.offset, self.pinned) = before;
            return Err(error);
        }
        self.scroll_to(self.offset)
    }

    pub fn measure_row(&mut self, key: u64, height: f64) -> Result<(), String> {
        let previous = self.grid.rows.extent(&key).ok_or("unknown table row")?;
        let before = self.offset;
        let anchor = self
            .grid
            .rows
            .anchor(self.offset[1] + self.frozen_size()?[1])
            .map_err(|error| format!("{error:?}"))?;
        self.grid
            .rows
            .measure(&key, height)
            .map_err(|error| format!("{error:?}"))?;
        let frozen = self
            .grid
            .rows
            .position(self.frozen[1])
            .ok_or("missing frozen rows")?;
        self.offset[1] = anchor
            .as_ref()
            .and_then(|anchor| {
                self.grid
                    .rows
                    .restore_anchor(
                        anchor,
                        (f64::from(self.viewport[1] - self.chrome_height()) - frozen).max(0.0),
                    )
                    .ok()
            })
            .map(|offset| (offset - frozen).max(0.0))
            .unwrap_or(0.0);
        if let Err(error) = self.render() {
            self.grid
                .rows
                .measure(&key, previous)
                .map_err(|error| format!("{error:?}"))?;
            self.offset = before;
            return Err(error);
        }
        self.scroll_to(self.offset)
    }

    pub fn measure_column(&mut self, key: u64, width: f64) -> Result<(), String> {
        let previous = self
            .grid
            .columns
            .extent(&key)
            .ok_or("unknown table column")?;
        let before = self.offset;
        let anchor = self
            .grid
            .columns
            .anchor(self.offset[0] + self.frozen_size()?[0])
            .map_err(|error| format!("{error:?}"))?;
        self.grid
            .columns
            .measure(&key, width)
            .map_err(|error| format!("{error:?}"))?;
        let frozen = self
            .grid
            .columns
            .position(self.frozen[0])
            .ok_or("missing frozen columns")?;
        self.offset[0] = anchor
            .as_ref()
            .and_then(|anchor| {
                self.grid
                    .columns
                    .restore_anchor(anchor, (f64::from(self.viewport[0]) - frozen).max(0.0))
                    .ok()
            })
            .map(|offset| (offset - frozen).max(0.0))
            .unwrap_or(0.0);
        if let Err(error) = self.render() {
            self.grid
                .columns
                .measure(&key, previous)
                .map_err(|error| format!("{error:?}"))?;
            self.offset = before;
            return Err(error);
        }
        self.scroll_to(self.offset)
    }

    pub fn render(&mut self) -> Result<(), String> {
        let frozen = self.frozen_size()?;
        let viewport = [
            f64::from(self.viewport[0]) - frozen[0],
            f64::from(self.viewport[1] - self.chrome_height()) - frozen[1],
        ];
        let mut pinned_rows: Vec<_> = (0..self.frozen[1])
            .filter_map(|index| self.grid.rows.key(index).copied())
            .collect();
        let mut pinned_columns: Vec<_> = (0..self.frozen[0])
            .filter_map(|index| self.grid.columns.key(index).copied())
            .collect();
        if let Some((row, column)) = self.pinned {
            pinned_rows.push(row);
            pinned_columns.push(column);
        }
        let [columns, rows] = self
            .grid
            .window(
                [self.offset[0] + frozen[0], self.offset[1] + frozen[1]],
                viewport,
                [0.0, 64.0],
                &pinned_rows,
                &pinned_columns,
                256,
            )
            .map_err(|error| format!("{error:?}"))?;
        let offset = [
            (columns.offset - frozen[0]).max(0.0),
            (rows.offset - frozen[1]).max(0.0),
        ];
        let widths = [frozen[0], (columns.total - frozen[0]).max(0.0)];
        let heights = [frozen[1], (rows.total - frozen[1]).max(0.0)];
        let identifier = self.root.model().id();
        for pane in 0..4 {
            if self.root.model().node(self.scroll[pane]).is_none() {
                self.scroll[pane] = NodeId::new(identifier);
                self.body[pane] = NodeId::new(identifier);
                self.bottom[pane] = NodeId::new(identifier);
            }
        }
        for band in &mut self.bands {
            if self.root.model().node(*band).is_none() {
                *band = NodeId::new(identifier);
            }
        }
        let mut desired = BTreeMap::new();
        let mut bodies: [PresentationNode; 4] = std::array::from_fn(|pane| {
            node(
                self.body[pane],
                NodeContent::Container,
                widths[pane % 2],
                heights[pane / 2],
            )
        });
        let mut vertical = [0.0; 2];
        for index in rows.mounted {
            let key = *self.grid.rows.key(index).ok_or("missing row key")?;
            let band = usize::from(index >= self.frozen[1]);
            let mut row = self.rows.get(&key).cloned().unwrap_or_else(|| Row {
                container: std::array::from_fn(|_| NodeId::new(identifier)),
                gap: std::array::from_fn(|_| NodeId::new(identifier)),
                tail: std::array::from_fn(|_| NodeId::new(identifier)),
                cells: BTreeMap::new(),
            });
            for side in 0..2 {
                if self.root.model().node(row.container[side]).is_none() {
                    row.container[side] = NodeId::new(identifier);
                    row.gap[side] = NodeId::new(identifier);
                    row.tail[side] = NodeId::new(identifier);
                }
            }
            let top = self
                .grid
                .rows
                .position(index)
                .ok_or("missing row position")?
                - if band == 1 { frozen[1] } else { 0.0 };
            let height = self.grid.rows.extent(&key).ok_or("missing row height")?;
            let mut containers: [PresentationNode; 2] = std::array::from_fn(|side| {
                let mut container = node(
                    row.container[side],
                    NodeContent::Container,
                    widths[side],
                    height,
                );
                container.descriptor.style.axis = style::Axis::Row;
                container
            });
            let mut horizontal = [0.0; 2];
            let mut cells = BTreeMap::new();
            for column_index in &columns.mounted {
                let column = *self
                    .grid
                    .columns
                    .key(*column_index)
                    .ok_or("missing column key")?;
                let side = usize::from(*column_index >= self.frozen[0]);
                let cell = row.cells.get(&column).cloned().unwrap_or_else(|| Cell {
                    input: NodeId::new(identifier),
                    gap: NodeId::new(identifier),
                    semantics: NodeId::new(identifier),
                });
                let left = self
                    .grid
                    .columns
                    .position(*column_index)
                    .ok_or("missing column position")?
                    - if side == 1 { frozen[0] } else { 0.0 };
                let width = self
                    .grid
                    .columns
                    .extent(&column)
                    .ok_or("missing column width")?;
                containers[side].children.push(node(
                    cell.gap,
                    NodeContent::Container,
                    (left - horizontal[side]).max(0.0),
                    height,
                ));
                let draft = self
                    .drafts
                    .get(&(key, column))
                    .cloned()
                    .unwrap_or_else(|| Draft::new(format!("R{key:06} C{column:04}")));
                let mut input = node(
                    cell.input,
                    NodeContent::Input {
                        max_length: None,
                        value: draft.display().into(),
                        placeholder: String::new(),
                        enabled: true,
                        password: false,
                        revision: draft.revision,
                    },
                    width,
                    height,
                );
                input.descriptor.accessible_name = Some(format!("Row {key}, column {column}"));
                let mut semantic = node(cell.semantics, NodeContent::Container, width, height);
                semantic.descriptor.accessible_name = Some(format!("Row {key}, column {column}"));
                semantic.descriptor.accessible_role = Some(AccessibleRole::GridCell {
                    row: u32::try_from(index).map_err(|_| "table row exceeds semantic range")?,
                    column: u32::try_from(*column_index)
                        .map_err(|_| "table column exceeds semantic range")?,
                    row_span: 1,
                    column_span: 1,
                });
                semantic.children.push(input);
                containers[side].children.push(semantic);
                horizontal[side] = left + width;
                cells.insert(column, cell);
            }
            for (side, mut container) in containers.into_iter().enumerate() {
                if side == 0 && self.frozen[0] == 0 {
                    continue;
                }
                let pane = band * 2 + side;
                bodies[pane].children.push(node(
                    row.gap[side],
                    NodeContent::Container,
                    widths[side],
                    (top - vertical[band]).max(0.0),
                ));
                container.children.push(node(
                    row.tail[side],
                    NodeContent::Container,
                    (widths[side] - horizontal[side]).max(0.0),
                    height,
                ));
                bodies[pane].children.push(container);
            }
            vertical[band] = top + height;
            desired.insert(key, Row { cells, ..row });
        }
        let mut bands: [PresentationNode; 2] = std::array::from_fn(|band| {
            let height = if band == 0 { frozen[1] } else { viewport[1] };
            let mut row = node(
                self.bands[band],
                NodeContent::Container,
                f64::from(self.viewport[0]),
                height,
            );
            row.descriptor.style.axis = style::Axis::Row;
            row
        });
        for (pane, mut body) in bodies.into_iter().enumerate() {
            let band = pane / 2;
            let side = pane % 2;
            if band == 0 && self.frozen[1] == 0 || side == 0 && self.frozen[0] == 0 {
                continue;
            }
            body.children.push(node(
                self.bottom[pane],
                NodeContent::Container,
                widths[side],
                (heights[band] - vertical[band]).max(0.0),
            ));
            let mut scroll = node(
                self.scroll[pane],
                NodeContent::Scroll,
                if side == 0 { frozen[0] } else { viewport[0] },
                if band == 0 { frozen[1] } else { viewport[1] },
            );
            scroll.descriptor.style.scrollbars = if self.frozen == [0; 2] {
                [true; 2]
            } else {
                [false; 2]
            };
            scroll.children.push(body);
            bands[band].children.push(scroll);
        }
        let mut toolbar = node(
            self.toolbar,
            NodeContent::Container,
            f64::from(self.viewport[0]),
            36.0,
        );
        toolbar.descriptor.style.axis = style::Axis::Row;
        for (identifier, label) in self.actions.iter().zip([
            "Reverse rows",
            "Even rows",
            "Resize row 0",
            if self.frozen == [0; 2] {
                "Freeze row/column"
            } else {
                "Unfreeze"
            },
        ]) {
            toolbar.children.push(node(
                *identifier,
                NodeContent::Button {
                    label: label.into(),
                    enabled: true,
                },
                f64::from(self.viewport[0]) / self.actions.len() as f64,
                36.0,
            ));
        }
        let mut outer = node(
            self.outer,
            NodeContent::Container,
            f64::from(self.viewport[0]),
            f64::from(self.viewport[1]),
        );
        outer.children.push(toolbar);
        let (header, headers) = self.build_headers(&columns.mounted, widths)?;
        outer.children.extend(header);
        outer.descriptor.accessible_name = Some("Editable data table".into());
        outer.descriptor.accessible_role = Some(AccessibleRole::Grid {
            rows: u32::try_from(self.grid.rows.len())
                .map_err(|_| "table rows exceed semantic range")?,
            columns: u32::try_from(self.grid.columns.len())
                .map_err(|_| "table columns exceed semantic range")?,
        });
        outer
            .children
            .extend(bands.into_iter().filter(|band| !band.children.is_empty()));
        self.root
            .present(&mut [outer], self.viewport, self.environment)
            .map_err(|error| error.to_string())?;
        self.rows = desired;
        self.headers = headers;
        self.offset = offset;
        let inputs = self
            .rows
            .values()
            .flat_map(|row| row.cells.values().map(|cell| cell.input))
            .collect::<Vec<_>>();
        self.keyboard_navigation = self.root.host_mut().bind_grid_navigation(&inputs)?;
        Ok(())
    }

    pub fn poll_events(&mut self) -> Result<usize, String> {
        let events = self.root.poll_events().map_err(|error| error.to_string())?;
        let mut changed = false;
        let mut scrolled = false;
        let mut actions = Vec::new();
        for event in &events {
            if event.payload == EventPayload::Activate {
                if let Some(action) = self
                    .actions
                    .iter()
                    .position(|identifier| *identifier == event.node)
                {
                    actions.push(action);
                }
                continue;
            }
            if let EventPayload::Scroll { x, y } = event.payload {
                if let Some(pane) = self.scroll.iter().position(|node| *node == event.node) {
                    let next = match pane {
                        1 => [x, self.offset[1]],
                        2 => [self.offset[0], y],
                        3 => [x, y],
                        _ => self.offset,
                    };
                    if next != self.offset {
                        self.offset = next;
                        changed = true;
                        scrolled = true;
                    }
                }
                continue;
            }
            let target = self.rows.iter().find_map(|(row, state)| {
                state
                    .cells
                    .iter()
                    .find(|(_, cell)| cell.input == event.node)
                    .map(|(column, _)| (*row, *column))
            });
            let Some(target) = target else {
                continue;
            };
            match &event.payload {
                EventPayload::Focus => {
                    self.pinned = Some(target);
                    changed = true;
                }
                EventPayload::Edit {
                    value,
                    revision,
                    composing_utf16,
                    ..
                } => {
                    self.pinned = Some(target);
                    self.drafts
                        .entry(target)
                        .or_insert_with(|| Draft::new(format!("R{:06} C{:04}", target.0, target.1)))
                        .update(value, *revision, composing_utf16.is_some());
                    changed = true;
                }
                EventPayload::Navigate {
                    key,
                    shift,
                    control,
                } => {
                    let current = self.pinned.unwrap_or(target);
                    self.navigate_cell(current, *key, *shift, *control)?;
                }
                _ => {}
            }
        }
        if changed {
            self.render()?;
        }
        if scrolled {
            self.sync_panes()?;
        }
        for action in actions {
            match action {
                0 | 1 => {
                    let previous = (self.descending, self.even_only);
                    if action == 0 {
                        self.descending = !self.descending;
                    } else {
                        self.even_only = !self.even_only;
                    }
                    let mut keys: Vec<_> = (0..self.maximum[1])
                        .filter(|key| !self.even_only || key % 2 == 0)
                        .collect();
                    if self.descending {
                        keys.reverse();
                    }
                    if let Err(error) = self.set_rows(keys) {
                        (self.descending, self.even_only) = previous;
                        return Err(error);
                    }
                }
                2 => {
                    self.expanded = !self.expanded;
                    if let Err(error) = self.measure_row(0, if self.expanded { 64.0 } else { 32.0 })
                    {
                        self.expanded = !self.expanded;
                        return Err(error);
                    }
                }
                3 => {
                    let count = usize::from(self.frozen == [0; 2]);
                    self.set_frozen(count, count)?;
                }
                _ => unreachable!(),
            }
        }
        Ok(events.len())
    }

    pub fn state(&self) -> serde_json::Value {
        serde_json::json!({"application": "native-data-table", "rows": self.grid.rows.len(), "columns": self.grid.columns.len(),
            "mounted_rows": self.rows.len(), "mounted_cells": self.rows.values().map(|row| row.cells.len()).sum::<usize>(),
            "edited_cells": self.drafts.values().filter(|draft| draft.committed).count(), "offset": self.offset, "pinned": self.pinned,
            "frozen_rows": self.frozen[1], "frozen_columns": self.frozen[0],
            "column_headers": self.headers_enabled, "mounted_column_headers": self.headers.len(),
            "keyboard_navigation": self.keyboard_navigation,
            "revision": self.root.model().revision()})
    }
}
