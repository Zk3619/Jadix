//! Portable table controls using the same keyed model and geometry on every host.
pub use jadix::portable::table_model::{ColumnPin, TableColumn};
use jadix::{
    portable::{create_signal, table_model::{TableColumns, TableRows}, virtual_list::virtual_list_indexed, Element, Signal},
    presentation::{AccessibleRole, NavigationKey, style::ScrollBehavior},
};
use std::{cell::RefCell, rc::Rc};
pub use jadix::portable::table_model::TableError;
type CellRenderer = Rc<dyn Fn(&str, &str) -> Element>;
type SortHandler = Rc<dyn Fn(&str)>;

#[derive(Clone)]
pub struct TableState {
    rows: Rc<RefCell<TableRows>>,
    columns: Rc<RefCell<TableColumns>>,
    count: Signal<usize>,
    revision: Signal<u64>,
    width: Signal<f64>,
    offsets: [Signal<f64>; 3],
    focus: Signal<Option<(String, String)>>,
}
impl TableState {
    pub fn new(rows: Vec<String>, columns: Vec<TableColumn>) -> Result<Self, TableError> {
        let rows = TableRows::new(rows)?;
        Ok(Self { count: create_signal(rows.visible().len()), rows: Rc::new(RefCell::new(rows)),
            columns: Rc::new(RefCell::new(TableColumns::new(columns)?)), revision: create_signal(0),
            width: create_signal(640.), offsets: std::array::from_fn(|_| create_signal(0.)), focus: create_signal(None) })
    }
    pub fn read<R>(&self, read: impl FnOnce(&TableRows) -> R) -> R {
        self.revision.get(); read(&self.rows.borrow())
    }
    pub fn update<R>(&self, update: impl FnOnce(&mut TableRows) -> R) -> R {
        let result = update(&mut self.rows.borrow_mut());
        let count = self.rows.borrow().visible().len();
        if self.count.get() != count { self.count.set(count); }
        self.revision.set(self.revision.get().wrapping_add(1));
        result
    }
    pub fn set_columns(&self, columns: Vec<TableColumn>) -> Result<(), TableError> {
        let next = TableColumns::new(columns)?;
        let offset = next.remap_offset(&self.columns.borrow(), self.offsets[1].get(), self.width.get(), self.width.get())?;
        *self.columns.borrow_mut() = next;
        self.offsets[1].set(offset);
        self.offsets[0].set(0.); self.offsets[2].set(0.);
        self.revision.set(self.revision.get().wrapping_add(1));
        Ok(())
    }
    pub fn horizontal_offset(&self) -> f64 { self.offsets[1].get() }
    pub fn reveal(&self, row: &str, column: &str) -> Result<(), TableError> {
        if self.rows.borrow().index(row).is_none() { return Err(TableError::UnknownKey); }
        let columns = self.columns.borrow();
        let target = columns.get(column).ok_or(TableError::UnknownKey)?;
        let band = match target.pin { ColumnPin::Left => 0, ColumnPin::None => 1, ColumnPin::Right => 2 };
        let (_, offset) = columns.reveal_in_band(column, self.offsets[band].get(), self.width.get())?;
        drop(columns);
        self.offsets[band].set(offset);
        self.focus.set(Some((row.into(), column.into())));
        self.revision.set(self.revision.get().wrapping_add(1));
        Ok(())
    }
}

pub struct TableBuilder {
    id: String, state: TableState, height: f32, row_height: f32,
    cell: CellRenderer, sort: Option<SortHandler>,
}
pub fn table(id: impl Into<String>, state: TableState) -> TableBuilder {
    TableBuilder { id: id.into(), state, height: 320., row_height: 32.,
        cell: Rc::new(|row, column| Element::new().text(format!("{row}: {column}"))), sort: None }
}
pub fn data_table(id: impl Into<String>, state: TableState) -> TableBuilder { table(id, state) }
impl TableBuilder {
    pub fn h(mut self, height: f32) -> Self { self.height = height; self }
    pub fn row_height(mut self, height: f32) -> Self { self.row_height = height; self }
    pub fn cell(mut self, render: impl Fn(&str, &str) -> Element + 'static) -> Self { self.cell = Rc::new(render); self }
    pub fn on_sort(mut self, sort: impl Fn(&str) + 'static) -> Self { self.sort = Some(Rc::new(sort)); self }
    pub fn build(self) -> Element {
        let Self { id, state, height, row_height, cell, sort } = self;
        assert!(height.is_finite() && height >= 32. && row_height.is_finite() && row_height > 0.);
        let row_state = state.clone(); let row_id = id.clone();
        let keys = state.clone(); let focus = state.clone(); let row_cell = cell.clone();
        let body = virtual_list_indexed(state.count.get(), row_height as f64, move |index| {
            let row = row_state.read(|rows| rows.visible()[index.get()].clone());
            bands(&row_id, &row_state, Some(row), row_height, height - 32., &row_cell, &None)
        }).id(format!("{id}__body")).h(height - 32.).count(state.count).measure_rows().data_revision(state.revision)
            .key(move |index| keys.read(|rows| rows.visible()[index].clone()))
            .focused_index_request_fn(move || focus.focus.get().and_then(|(row, _)|
                focus.read(|rows| rows.index(&row)).map(|index| (index, focus.revision.get() as usize))))
            .build();
        let header = bands(&id, &state, None, 32., height - 32., &cell, &sort);
        let layout_state = state.clone(); let semantics = state.clone();
        let mut root = Element::new().id(id).flex_col().h(height).min_w(0.).children([header, body])
            .on_layout(move |rect| {
                let width = f64::from(rect.width);
                if (width - layout_state.width.get()).abs() <= 0.01 { return false; }
                let offset = { let columns = layout_state.columns.borrow();
                    columns.remap_offset(&columns, layout_state.offsets[1].get(), layout_state.width.get(), width).unwrap_or(0.) };
                layout_state.width.set(width); layout_state.offsets[1].set(offset); true
            });
        root.descriptor_binding = Some(Rc::new(move |descriptor| {
            semantics.revision.get();
            descriptor.accessible_role = Some(AccessibleRole::Grid {
                rows: semantics.count.get() as u32, columns: semantics.columns.borrow().order().len() as u32,
            });
        }));
        root
    }
}

fn bands(id: &str, state: &TableState, row: Option<String>, height: f32, viewport_height: f32,
    render: &CellRenderer, sort: &Option<SortHandler>) -> Element {
    let mut bands = Vec::new();
    for band in 0..3 {
        let keys_state = state.clone();
        let cells_state = state.clone(); let cells_row = row.clone(); let cells_id = id.to_string();
        let renderer = render.clone(); let sorter = sort.clone();
        let mut content = Element::new().flex_row().flex_shrink(0.).bind_keyed_children(move || {
            keys_state.revision.get();
            let columns = keys_state.columns.borrow();
                let window = columns.band_window(band, keys_state.offsets[band].get(), columns.band_viewports(keys_state.width.get()).unwrap()[band], 1).unwrap();
                std::iter::once("leading".into()).chain(window.keys.into_iter().map(|key| format!("c:{key}")))
                    .chain(std::iter::once("trailing".into())).collect()
        }, move |key, _| {
            if let Some(column) = key.strip_prefix("c:") {
                cell(&cells_id, &cells_state, cells_row.clone(), column.to_string(), height, viewport_height, &renderer, &sorter)
            } else {
                let state = cells_state.clone();
                let mut spacer = Element::new().h(height).flex_shrink(0.);
                spacer.descriptor_binding = Some(Rc::new(move |descriptor| {
                    let columns = state.columns.borrow();
                    let window = columns.band_window(band, state.offsets[band].get(), columns.band_viewports(state.width.get()).unwrap()[band], 1).unwrap();
                    descriptor.style.width = Some(if key == "leading" { window.leading } else { window.trailing } as f32);
                })); spacer
            }
        });
        let geometry = state.clone();
            let scroll = state.clone();
            content = jadix::portable::primitives::scroll().id(format!("{id}__scroll_{band}_{}", row.as_ref().map(|key| format!("row_{key}")).unwrap_or_else(|| "header".into())))
                .scrollbars(false).on_scroll(move |x, _| {
                    let columns = scroll.columns.borrow();
                    let offset = columns.band_window(band, x.max(0.), columns.band_viewports(scroll.width.get()).unwrap()[band], 0).unwrap().offset;
                    if (offset - scroll.offsets[band].get()).abs() > 0.01 { scroll.offsets[band].set(offset); }
                }).child(content).min_w(0.);
            content.descriptor_binding = Some(Rc::new(move |descriptor| {
                geometry.revision.get();
                descriptor.style.width = Some(geometry.columns.borrow().band_viewports(geometry.width.get()).unwrap()[band] as f32);
                descriptor.style.scroll_to = Some([geometry.offsets[band].get(), 0.]);
                descriptor.style.scroll_behavior = Some(ScrollBehavior::Instant);
            }));
        bands.push(content.h(height).flex_shrink(0.));
    }
    Element::new().flex_row().h(height).flex_shrink(0.).children(bands)
}

fn cell(id: &str, state: &TableState, row: Option<String>, column: String, height: f32, viewport_height: f32,
    render: &CellRenderer, sort: &Option<SortHandler>) -> Element {
    let identity = cell_id(id, row.as_deref(), &column);
    let action = state.clone(); let action_row = row.clone(); let action_column = column.clone(); let sorter = sort.clone();
    let mut element: Element = super::button(&identity).on_click(move || {
        if let Some(row) = &action_row { let _ = action.update(|rows| rows.select(row, false)); }
        else if let Some(sorter) = &sorter { sorter(&action_column); }
    }).into();
    element.attrs.insert("button:content".into(), "true".into());
    let content = if let Some(row) = &row { render(row, &column) } else {
        let state = state.clone(); let column = column.clone();
        let mut caption = Element::new().text("");
        caption.text_binding = Some(Rc::new(move || { state.revision.get(); state.columns.borrow().get(&column).map(|c| c.label.clone()).unwrap_or_default() })); caption
    };
    element.children.push(content);
    let binding = state.clone(); let binding_row = row.clone(); let binding_column = column.clone();
    element.descriptor_binding = Some(Rc::new(move |descriptor| {
        binding.revision.get();
        let columns = binding.columns.borrow();
        let Some(column) = columns.get(&binding_column) else { return; };
        descriptor.style.width = Some(column.width as f32);
        descriptor.accessible_name = Some(if let Some(row) = &binding_row { format!("{}: {row}", column.label) } else { column.label.clone() });
        let index = columns.index(&binding_column).unwrap() as u32;
        descriptor.accessible_role = Some(if let Some(row) = &binding_row {
            descriptor.accessible_selected = Some(binding.read(|rows| rows.selected().contains(row)));
            AccessibleRole::GridCell { row: binding.read(|rows| rows.index(row)).unwrap_or(0) as u32, column: index, row_span: 1, column_span: 1 }
        } else { AccessibleRole::GridColumnHeader { column: index, column_span: 1 } });
    }));
    if let Some(row) = row {
        let state = state.clone(); let id = id.to_string();
        element.on_navigation = Some(Rc::new(RefCell::new(Box::new(move |key| {
            let rows = state.rows.borrow(); let columns = state.columns.borrow();
            let mut r = rows.index(&row)?; let mut c = columns.index(&column)?;
            match key {
                NavigationKey::Up => r = r.saturating_sub(1),
                NavigationKey::Down => r = (r + 1).min(rows.visible().len().saturating_sub(1)),
                NavigationKey::Left => c = c.saturating_sub(1),
                NavigationKey::Right => c = (c + 1).min(columns.order().len().saturating_sub(1)),
                NavigationKey::Home => c = 0,
                NavigationKey::End => c = columns.order().len().saturating_sub(1),
                NavigationKey::PageUp => r = r.saturating_sub((viewport_height / height).floor().max(1.) as usize),
                NavigationKey::PageDown => r = r.saturating_add((viewport_height / height).floor().max(1.) as usize).min(rows.visible().len().saturating_sub(1)),
                _ => return None,
            }
            let row = rows.visible().get(r)?.clone(); let column = columns.order().get(c)?.clone();
            drop(rows); drop(columns);
            state.reveal(&row, &column).ok()?;
            Some(cell_id(&id, Some(&row), &column))
        }))));
    }
    element.h(height).flex_shrink(0.)
}
fn cell_id(id: &str, row: Option<&str>, column: &str) -> String {
    match row { Some(row) => format!("{id}__cell_{}:{row}{column}", row.len()), None => format!("{id}__header_{column}") }
}
impl From<TableBuilder> for Element { fn from(value: TableBuilder) -> Self { value.build() } }
impl jadix::portable::IntoElementList for TableBuilder { fn into_element_list(self) -> Vec<Element> { vec![self.build()] } }
