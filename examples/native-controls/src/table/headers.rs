use super::*;

impl<Host: PlatformHost> DataTable<Host> {
    pub(super) fn build_headers(
        &mut self,
        mounted: &[usize],
        widths: [f64; 2],
    ) -> Result<(Option<PresentationNode>, BTreeMap<u64, Header>), String> {
        let mut headers = BTreeMap::new();
        if !self.headers_enabled {
            return Ok((None, headers));
        }
        let identifier = self.root.model().id();
        for node in &mut self.header_chrome {
            if self.root.model().node(*node).is_none() {
                *node = NodeId::new(identifier);
            }
        }
        let mut bodies: [PresentationNode; 2] = std::array::from_fn(|side| {
            let mut body = node(
                self.header_chrome[3 + side],
                NodeContent::Container,
                widths[side],
                32.0,
            );
            body.descriptor.style.axis = style::Axis::Row;
            body
        });
        let mut horizontal = [0.0; 2];
        for index in mounted {
            let key = *self
                .grid
                .columns
                .key(*index)
                .ok_or("missing header column key")?;
            let side = usize::from(*index >= self.frozen[0]);
            let left = self
                .grid
                .columns
                .position(*index)
                .ok_or("missing header position")?
                - if side == 1 { widths[0] } else { 0.0 };
            let width = self
                .grid
                .columns
                .extent(&key)
                .ok_or("missing header width")?;
            let header = self.headers.get(&key).cloned().unwrap_or_else(|| Header {
                label: NodeId::new(identifier),
                gap: NodeId::new(identifier),
            });
            bodies[side].children.push(node(
                header.gap,
                NodeContent::Container,
                (left - horizontal[side]).max(0.0),
                32.0,
            ));
            let mut label = node(
                header.label,
                NodeContent::Text(format!("Column {key:04}")),
                width,
                32.0,
            );
            label.descriptor.accessible_role = Some(AccessibleRole::GridColumnHeader {
                column: u32::try_from(*index)
                    .map_err(|_| "header column exceeds semantic range")?,
            });
            label.descriptor.accessible_name = Some(format!("Column {key:04}"));
            bodies[side].children.push(label);
            horizontal[side] = left + width;
            headers.insert(key, header);
        }
        let mut row = node(
            self.header_chrome[0],
            NodeContent::Container,
            f64::from(self.viewport[0]),
            32.0,
        );
        row.descriptor.style.axis = style::Axis::Row;
        for (side, mut body) in bodies.into_iter().enumerate() {
            if side == 0 && self.frozen[0] == 0 {
                continue;
            }
            body.children.push(node(
                self.header_chrome[5 + side],
                NodeContent::Container,
                (widths[side] - horizontal[side]).max(0.0),
                32.0,
            ));
            let mut scroll = node(
                self.header_chrome[1 + side],
                NodeContent::Scroll,
                if side == 0 {
                    widths[0]
                } else {
                    f64::from(self.viewport[0]) - widths[0]
                },
                32.0,
            );
            scroll.descriptor.style.scrollbars = [false; 2];
            scroll.children.push(body);
            row.children.push(scroll);
        }
        Ok((Some(row), headers))
    }
}
