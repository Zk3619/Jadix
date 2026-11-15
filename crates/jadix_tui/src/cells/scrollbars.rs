//! Retained Scroll extents, edge-cell geometry, paint and pointer capture.
use super::*;
use crossterm::event::MouseEvent;

#[derive(Clone, Copy)]
pub(super) struct Capture {
    node: NodeId,
    axis: usize,
    grab: i64,
}
struct Track {
    rect: Rect,
    start: i64,
    length: i64,
    thumb: i64,
    position: i64,
    max: f64,
}
impl CellSurface {
    pub(super) fn descendant(&self, child: usize, ancestor: usize) -> bool {
        let mut at = Some(child);
        while let Some(i) = at {
            if i == ancestor {
                return true;
            }
            at = self.nodes[i].parent;
        }
        false
    }
    pub(super) fn update_scroll_limits(&mut self) {
        // Preorder storage permits one forward visibility pass and one reverse
        // union. Nested Scroll clips and portals bound propagation to parents.
        let mut visible = Vec::with_capacity(self.nodes.len());
        let mut ends = Vec::with_capacity(self.nodes.len());
        for n in &self.nodes {
            visible.push(n.parent.is_none_or(|p| visible[p]) && shown(&n.descriptor));
            let f = n.descriptor.frame;
            ends.push([f.x + f.width, f.y + f.height]);
        }
        self.scroll_limits.resize(self.nodes.len(), [0.; 2]);
        for i in (0..self.nodes.len()).rev() {
            let n = &self.nodes[i];
            let f = n.descriptor.frame;
            self.scroll_limits[i] = [
                (ends[i][0] - f.x - f.width).max(0.),
                (ends[i][1] - f.y - f.height).max(0.),
            ];
            if !visible[i]
                || matches!(
                    n.descriptor.content,
                    NodeContent::Dialog { .. } | NodeContent::Popover { .. }
                )
            {
                continue;
            }
            if let Some(p) = n.parent {
                let end = if matches!(n.descriptor.content, NodeContent::Scroll) {
                    [f.x + f.width, f.y + f.height]
                } else {
                    ends[i]
                };
                for (axis, value) in end.into_iter().enumerate() {
                    ends[p][axis] = ends[p][axis].max(value);
                }
            }
        }
    }
    fn track(&self, index: usize, axis: usize) -> Option<Track> {
        if !self.scrollbars || !matches!(self.nodes[index].descriptor.content, NodeContent::Scroll)
        {
            return None;
        }
        let g = self.geometry.get(index)?;
        if !g.live {
            return None;
        }
        let max = self.scroll_limits[index][axis];
        if max <= 0. {
            return None;
        }
        let f = g.frame;
        let vertical = axis == 1;
        let other = self.scroll_limits[index][1 - axis] > 0.;
        let (start, length, edge) = if vertical {
            (f[1], f[3] - f[1] - i64::from(other), f[2] - 1)
        } else {
            (f[0], f[2] - f[0] - i64::from(other), f[3] - 1)
        };
        if length < 2 {
            return None;
        }
        let rect = if vertical {
            [edge, start, edge + 1, start + length]
        } else {
            [start, edge, start + length, edge + 1]
        };
        // Partial clipping must not stretch the thumb into a false range.
        let exact = Rect::new(
            u16::try_from(rect[0]).ok()?,
            u16::try_from(rect[1]).ok()?,
            u16::try_from(rect[2] - rect[0]).ok()?,
            u16::try_from(rect[3] - rect[1]).ok()?,
        );
        if intersection(rect, g.clip) != exact {
            return None;
        }
        let viewport = if vertical {
            self.nodes[index].descriptor.frame.height
        } else {
            self.nodes[index].descriptor.frame.width
        };
        let thumb =
            ((length as f64 * viewport / (viewport + max)).floor() as i64).clamp(1, length - 1);
        let offset = self
            .scroll
            .get(&self.nodes[index].id)
            .map_or(0., |v| v[axis]);
        let position = ((length - thumb) as f64 * offset / max).round() as i64;
        Some(Track {
            rect: intersection(rect, g.clip),
            start,
            length,
            thumb,
            position,
            max,
        })
    }
    pub(super) fn paint_scrollbars(&self, index: usize, buffer: &mut Buffer) {
        for axis in 0..2 {
            let Some(track) = self.track(index, axis) else {
                continue;
            };
            for n in 0..track.length {
                let x = track.rect.x + if axis == 0 { n as u16 } else { 0 };
                let y = track.rect.y + if axis == 1 { n as u16 } else { 0 };
                if !buffer.area.contains((x, y).into()) {
                    continue;
                }
                let thumb = (track.position..track.position + track.thumb).contains(&n);
                buffer[(x, y)]
                    .set_symbol(if thumb {
                        "█"
                    } else if axis == 1 {
                        "│"
                    } else {
                        "─"
                    })
                    .set_style(Style::default().fg(if thumb {
                        Color::Gray
                    } else {
                        Color::DarkGray
                    }));
            }
        }
    }
    pub(super) fn validate_scrollbar_capture(&mut self) {
        if self.scrollbar_capture.is_some_and(|c| {
            self.node_indices
                .get(&c.node)
                .is_none_or(|&i| !self.in_scope(i) || self.track(i, c.axis).is_none())
        }) {
            self.scrollbar_capture = None;
        }
    }
    pub(super) fn handle_scrollbar(&mut self, mouse: MouseEvent) -> Option<Option<PlatformEvent>> {
        if let Some(c) = self.scrollbar_capture {
            if mouse.kind == MouseEventKind::Up(MouseButton::Left) {
                self.scrollbar_capture = None;
                return Some(None);
            }
            if mouse.kind == MouseEventKind::Drag(MouseButton::Left) {
                let i = *self.node_indices.get(&c.node)?;
                let track = self.track(i, c.axis)?;
                let coordinate = i64::from(if c.axis == 1 { mouse.row } else { mouse.column });
                let position =
                    (coordinate - track.start - c.grab).clamp(0, track.length - track.thumb);
                let offset = position as f64 * track.max / (track.length - track.thumb) as f64;
                let old = self.scroll.get(&c.node).map_or(0., |v| v[c.axis]);
                let mut delta = [0.; 2];
                delta[c.axis] = offset - old;
                return Some(self.scroll_node(i, delta[0], delta[1]));
            }
        }
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return None;
        }
        self.scrollbar_capture = None;
        let hit = self.hit(mouse.column, mouse.row)?;
        for &i in self.visible_paint_order.iter().rev() {
            if !self.in_scope(i) || !self.descendant(hit, i) {
                continue;
            }
            for axis in 0..2 {
                let Some(track) = self.track(i, axis) else {
                    continue;
                };
                if !track.rect.contains((mouse.column, mouse.row).into()) {
                    continue;
                }
                self.capture = None;
                let coordinate = i64::from(if axis == 1 { mouse.row } else { mouse.column });
                let at = coordinate - track.start;
                if (track.position..track.position + track.thumb).contains(&at) {
                    self.scrollbar_capture = Some(Capture {
                        node: self.nodes[i].id,
                        axis,
                        grab: at - track.position,
                    });
                    return Some(None);
                }
                let f = self.nodes[i].descriptor.frame;
                let mut delta = [0.; 2];
                delta[axis] = if axis == 1 { f.height } else { f.width }
                    * if at < track.position { -1. } else { 1. };
                return Some(self.scroll_node(i, delta[0], delta[1]));
            }
        }
        None
    }
}
