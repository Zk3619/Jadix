use super::*;

impl<Host: PlatformHost> DataTable<Host> {
    pub fn focus_cell(&mut self, row: u64, column: u64) -> Result<bool, String> {
        let indices = [
            self.grid
                .columns
                .index(&column)
                .ok_or("unknown table column")?,
            self.grid.rows.index(&row).ok_or("unknown table row")?,
        ];
        if self.drafts.values().any(Draft::is_composing) {
            return Ok(false);
        }
        let frozen = self.frozen_size()?;
        let viewports = [
            f64::from(self.viewport[0]) - frozen[0],
            f64::from(self.viewport[1] - self.chrome_height()) - frozen[1],
        ];
        if viewports.iter().any(|extent| *extent <= 0.0) {
            return Err(String::from("table has no scrollable viewport"));
        }
        let axes = [&self.grid.columns, &self.grid.rows];
        let mut offset = self.offset;
        for axis in 0..2 {
            if indices[axis] < self.frozen[axis] {
                continue;
            }
            let start = axes[axis]
                .position(indices[axis])
                .ok_or("missing table position")?
                - frozen[axis];
            let end = axes[axis]
                .position(indices[axis] + 1)
                .ok_or("missing table extent")?
                - frozen[axis];
            if start < offset[axis] || end - start >= viewports[axis] {
                offset[axis] = start;
            } else if end > offset[axis] + viewports[axis] {
                offset[axis] = end - viewports[axis];
            }
            offset[axis] = offset[axis].clamp(
                0.0,
                (axes[axis].total() - frozen[axis] - viewports[axis]).max(0.0),
            );
        }
        let previous = (self.pinned, self.offset);
        self.pinned = Some((row, column));
        self.offset = offset;
        if let Err(error) = self.render() {
            (self.pinned, self.offset) = previous;
            return Err(error);
        }
        self.sync_panes()?;
        let input = self
            .cell(row, column)
            .ok_or("navigation target was not mounted")?;
        self.root
            .focus(Some(input))
            .map_err(|error| error.to_string())?;
        self.poll_events()?;
        Ok(true)
    }

    pub fn navigate_cell(
        &mut self,
        current: (u64, u64),
        key: NavigationKey,
        shift: bool,
        control: bool,
    ) -> Result<bool, String> {
        let mut row = self
            .grid
            .rows
            .index(&current.0)
            .ok_or("unknown table row")?;
        let mut column = self
            .grid
            .columns
            .index(&current.1)
            .ok_or("unknown table column")?;
        if self.drafts.values().any(Draft::is_composing) {
            return Ok(false);
        }
        let last_row = self.grid.rows.len() - 1;
        let last_column = self.grid.columns.len() - 1;
        match key {
            NavigationKey::Tab if !control => {
                if shift && column == 0 && row == 0
                    || !shift && column == last_column && row == last_row
                {
                    let action = if shift { self.actions.len() - 1 } else { 0 };
                    self.root
                        .focus(Some(self.actions[action]))
                        .map_err(|error| error.to_string())?;
                    self.poll_events()?;
                    return Ok(true);
                }
                if shift {
                    if column == 0 {
                        row -= 1;
                        column = last_column;
                    } else {
                        column -= 1;
                    }
                } else if column == last_column {
                    row += 1;
                    column = 0;
                } else {
                    column += 1;
                }
            }
            NavigationKey::Enter if !control => {
                row = if shift {
                    row.saturating_sub(1)
                } else {
                    (row + 1).min(last_row)
                };
            }
            NavigationKey::Up if control && !shift => row = row.saturating_sub(1),
            NavigationKey::Down if control && !shift => row = (row + 1).min(last_row),
            NavigationKey::Left if control && !shift => column = column.saturating_sub(1),
            NavigationKey::Right if control && !shift => column = (column + 1).min(last_column),
            NavigationKey::Home if control && !shift => {
                row = 0;
                column = 0;
            }
            NavigationKey::End if control && !shift => {
                row = last_row;
                column = last_column;
            }
            NavigationKey::PageUp | NavigationKey::PageDown if !control && !shift => {
                let page = (f64::from(self.viewport[1] - self.chrome_height())
                    - self.frozen_size()?[1])
                    .max(1.0);
                let top = self
                    .grid
                    .rows
                    .position(row)
                    .ok_or("missing table row position")?;
                let offset = if key == NavigationKey::PageUp {
                    (top - page).max(0.0)
                } else {
                    top + page
                };
                let anchor = self
                    .grid
                    .rows
                    .anchor(offset)
                    .map_err(|error| format!("{error:?}"))?
                    .ok_or("missing table page anchor")?;
                let next = self
                    .grid
                    .rows
                    .index(&anchor.key)
                    .ok_or("unknown table page anchor")?;
                row = if key == NavigationKey::PageUp {
                    next.min(row.saturating_sub(1))
                } else {
                    next.max((row + 1).min(last_row))
                };
            }
            _ => return Ok(false),
        }
        let row_key = *self.grid.rows.key(row).ok_or("missing navigation row")?;
        let column_key = *self
            .grid
            .columns
            .key(column)
            .ok_or("missing navigation column")?;
        self.focus_cell(row_key, column_key)
    }
}
