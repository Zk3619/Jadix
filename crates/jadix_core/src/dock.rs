use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pane {
    pub key: u64,
    pub title: String,
    pub closable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bounds {
    pub origin: [f64; 2],
    pub size: [f64; 2],
}

impl Bounds {
    fn valid(self) -> bool {
        self.origin
            .into_iter()
            .all(|value| value.is_finite() && value.abs() <= 1_000_000.0)
            && self
                .size
                .into_iter()
                .all(|value| value.is_finite() && (160.0..=16_384.0).contains(&value))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DockNode {
    Stack {
        key: u64,
        panes: Vec<u64>,
        active: u64,
    },
    Split {
        key: u64,
        axis: Axis,
        ratio: f64,
        first: Box<DockNode>,
        second: Box<DockNode>,
    },
}

impl DockNode {
    pub fn key(&self) -> u64 {
        match self {
            Self::Stack { key, .. } | Self::Split { key, .. } => *key,
        }
    }

    fn find_mut(&mut self, target: u64) -> Option<&mut Self> {
        if self.key() == target {
            return Some(self);
        }
        match self {
            Self::Stack { .. } => None,
            Self::Split { first, second, .. } => {
                first.find_mut(target).or_else(|| second.find_mut(target))
            }
        }
    }

    fn remove(self, pane: u64) -> Option<Self> {
        match self {
            Self::Stack {
                key,
                mut panes,
                mut active,
            } => {
                if let Some(position) = panes.iter().position(|candidate| *candidate == pane) {
                    panes.remove(position);
                    if active == pane && !panes.is_empty() {
                        active = panes[position.min(panes.len() - 1)];
                    }
                }
                (!panes.is_empty()).then_some(Self::Stack { key, panes, active })
            }
            Self::Split {
                key,
                axis,
                ratio,
                first,
                second,
            } => match (first.remove(pane), second.remove(pane)) {
                (Some(first), Some(second)) => Some(Self::Split {
                    key,
                    axis,
                    ratio,
                    first: Box::new(first),
                    second: Box::new(second),
                }),
                (remaining, None) | (None, remaining) => remaining,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DockSurface {
    pub key: u64,
    pub bounds: Bounds,
    pub root: DockNode,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    version: u32,
    panes: Vec<Pane>,
    surfaces: Vec<DockSurface>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DockLimits {
    pub panes: usize,
    pub windows: usize,
    pub depth: usize,
    pub history: usize,
    pub serialized_bytes: usize,
}

impl Default for DockLimits {
    fn default() -> Self {
        Self {
            panes: 128,
            windows: 8,
            depth: 32,
            history: 32,
            serialized_bytes: 256 * 1024,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DockError {
    UnknownPane,
    UnknownStack,
    UnknownSplit,
    UnknownSurface,
    InvalidLayout,
    InvalidRatio,
    InvalidBounds,
    InvalidPosition,
    ProtectedPane,
    MainWindowEmpty,
    StaleDrag,
    DragBusy,
    Budget,
    IdentityExhausted,
}

impl std::fmt::Display for DockError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for DockError {}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DropLocation {
    Tab { stack: u64, index: usize },
    Split { stack: u64, edge: Edge },
    Floating { bounds: Bounds },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DragToken {
    owner: u64,
    generation: u64,
    revision: u64,
    pane: u64,
}

pub struct Dock {
    owner: u64,
    state: Snapshot,
    limits: DockLimits,
    revision: u64,
    next_node: u64,
    next_surface: u64,
    next_drag: u64,
    drag: Option<DragToken>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl Dock {
    pub fn new(panes: Vec<Pane>, bounds: Bounds, limits: DockLimits) -> Result<Self, DockError> {
        let active = panes.first().ok_or(DockError::InvalidLayout)?.key;
        let state = Snapshot {
            version: 1,
            surfaces: vec![DockSurface {
                key: 1,
                bounds,
                root: DockNode::Stack {
                    key: 1,
                    panes: panes.iter().map(|pane| pane.key).collect(),
                    active,
                },
            }],
            panes,
        };
        Self::from_snapshot(state, limits)
    }

    pub fn from_json(bytes: &[u8], limits: DockLimits) -> Result<Self, DockError> {
        if bytes.len() > limits.serialized_bytes.min(1024 * 1024) {
            return Err(DockError::Budget);
        }
        let state = serde_json::from_slice(bytes).map_err(|_| DockError::InvalidLayout)?;
        Self::from_snapshot(state, limits)
    }

    fn from_snapshot(state: Snapshot, limits: DockLimits) -> Result<Self, DockError> {
        let (next_node, next_surface) = Self::validate(&state, limits)?;
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let owner = NEXT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| DockError::IdentityExhausted)?;
        Ok(Self {
            owner,
            state,
            limits,
            revision: 0,
            next_node,
            next_surface,
            next_drag: 0,
            drag: None,
            undo: Vec::new(),
            redo: Vec::new(),
        })
    }

    fn validate(state: &Snapshot, limits: DockLimits) -> Result<(u64, u64), DockError> {
        if state.version != 1
            || state.panes.is_empty()
            || state
                .surfaces
                .first()
                .is_none_or(|surface| surface.key != 1)
        {
            return Err(DockError::InvalidLayout);
        }
        if state.panes.len() > limits.panes.min(1024)
            || state.surfaces.len() > limits.windows.min(32)
            || limits.history > 128
        {
            return Err(DockError::Budget);
        }
        let mut declared = BTreeSet::new();
        for pane in &state.panes {
            if pane.key == 0
                || pane.title.is_empty()
                || pane.title.len() > 1024
                || pane.title.contains('\0')
                || !declared.insert(pane.key)
            {
                return Err(DockError::InvalidLayout);
            }
        }
        let mut nodes = BTreeSet::new();
        let mut surfaces = BTreeSet::new();
        let mut placed = BTreeSet::new();
        for surface in &state.surfaces {
            if surface.key == 0 || !surfaces.insert(surface.key) {
                return Err(DockError::InvalidLayout);
            }
            if !surface.bounds.valid() {
                return Err(DockError::InvalidBounds);
            }
            let mut pending = vec![(&surface.root, 0)];
            while let Some((node, depth)) = pending.pop() {
                if depth > limits.depth.min(48)
                    || nodes.len() >= limits.panes.min(1024).saturating_mul(2)
                {
                    return Err(DockError::Budget);
                }
                if node.key() == 0 || !nodes.insert(node.key()) {
                    return Err(DockError::InvalidLayout);
                }
                match node {
                    DockNode::Stack { panes, active, .. } => {
                        if panes.is_empty() || !panes.contains(active) {
                            return Err(DockError::InvalidLayout);
                        }
                        for pane in panes {
                            if !declared.contains(pane) || !placed.insert(*pane) {
                                return Err(DockError::InvalidLayout);
                            }
                        }
                    }
                    DockNode::Split {
                        ratio,
                        first,
                        second,
                        ..
                    } => {
                        if !ratio.is_finite() || !(0.1..=0.9).contains(ratio) {
                            return Err(DockError::InvalidRatio);
                        }
                        pending.push((first, depth + 1));
                        pending.push((second, depth + 1));
                    }
                }
            }
        }
        if declared != placed {
            return Err(DockError::InvalidLayout);
        }
        if serde_json::to_vec(state)
            .map_err(|_| DockError::InvalidLayout)?
            .len()
            > limits.serialized_bytes.min(1024 * 1024)
        {
            return Err(DockError::Budget);
        }
        let next_node = nodes
            .last()
            .copied()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(DockError::IdentityExhausted)?;
        let next_surface = surfaces
            .last()
            .copied()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(DockError::IdentityExhausted)?;
        Ok((next_node, next_surface))
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn surfaces(&self) -> &[DockSurface] {
        &self.state.surfaces
    }
    pub fn panes(&self) -> &[Pane] {
        &self.state.panes
    }
    pub fn dragging(&self) -> Option<u64> {
        self.drag.as_ref().map(|token| token.pane)
    }
    pub fn to_json(&self) -> Result<Vec<u8>, DockError> {
        serde_json::to_vec(&self.state).map_err(|_| DockError::InvalidLayout)
    }

    pub fn stack_of(&self, pane: u64) -> Option<(u64, u64)> {
        for surface in &self.state.surfaces {
            let mut pending = vec![&surface.root];
            while let Some(node) = pending.pop() {
                match node {
                    DockNode::Stack { key, panes, .. } if panes.contains(&pane) => {
                        return Some((surface.key, *key))
                    }
                    DockNode::Split { first, second, .. } => {
                        pending.push(second);
                        pending.push(first);
                    }
                    _ => {}
                }
            }
        }
        None
    }

    fn find_mut(state: &mut Snapshot, key: u64) -> Option<&mut DockNode> {
        state
            .surfaces
            .iter_mut()
            .find_map(|surface| surface.root.find_mut(key))
    }

    fn remove(state: &mut Snapshot, pane: u64) -> Result<(), DockError> {
        if !state.panes.iter().any(|record| record.key == pane) {
            return Err(DockError::UnknownPane);
        }
        let mut empty = Vec::new();
        for surface in &mut state.surfaces {
            if let Some(root) = surface.root.clone().remove(pane) {
                surface.root = root;
            } else if surface.key == 1 {
                return Err(DockError::MainWindowEmpty);
            } else {
                empty.push(surface.key);
            }
        }
        state
            .surfaces
            .retain(|surface| !empty.contains(&surface.key));
        Ok(())
    }

    fn commit(&mut self, state: Snapshot) -> Result<bool, DockError> {
        let (next_node, next_surface) = Self::validate(&state, self.limits)?;
        if state == self.state {
            return Ok(false);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(DockError::IdentityExhausted)?;
        if self.limits.history != 0 {
            if self.undo.len() == self.limits.history {
                self.undo.remove(0);
            }
            self.undo.push(std::mem::replace(&mut self.state, state));
        } else {
            self.state = state;
        }
        self.redo.clear();
        self.revision = revision;
        self.next_node = self.next_node.max(next_node);
        self.next_surface = self.next_surface.max(next_surface);
        self.drag = None;
        Ok(true)
    }

    pub fn activate(&mut self, pane: u64) -> Result<bool, DockError> {
        let (_, stack) = self.stack_of(pane).ok_or(DockError::UnknownPane)?;
        let mut state = self.state.clone();
        if let Some(DockNode::Stack { active, .. }) = Self::find_mut(&mut state, stack) {
            *active = pane;
        }
        self.commit(state)
    }

    pub fn relocate(&mut self, pane: u64, location: DropLocation) -> Result<bool, DockError> {
        let (_, source) = self.stack_of(pane).ok_or(DockError::UnknownPane)?;
        let mut state = self.state.clone();
        if let DropLocation::Tab { stack, index } = location {
            if let Some(DockNode::Stack { panes, active, .. }) = Self::find_mut(&mut state, stack) {
                if index > panes.len() {
                    return Err(DockError::InvalidPosition);
                }
                if stack == source {
                    let previous = panes
                        .iter()
                        .position(|key| *key == pane)
                        .ok_or(DockError::UnknownPane)?;
                    panes.remove(previous);
                    panes.insert(index.saturating_sub(usize::from(previous < index)), pane);
                    *active = pane;
                    return self.commit(state);
                }
            } else {
                return Err(DockError::UnknownStack);
            }
        }
        Self::remove(&mut state, pane)?;
        match location {
            DropLocation::Tab { stack, index } => {
                let Some(DockNode::Stack { panes, active, .. }) = Self::find_mut(&mut state, stack)
                else {
                    return Err(DockError::UnknownStack);
                };
                if index > panes.len() {
                    return Err(DockError::InvalidPosition);
                }
                panes.insert(index, pane);
                *active = pane;
            }
            DropLocation::Split { stack, edge } => {
                let new_stack = self.next_node;
                let split_key = new_stack
                    .checked_add(1)
                    .ok_or(DockError::IdentityExhausted)?;
                let target = Self::find_mut(&mut state, stack).ok_or(DockError::UnknownStack)?;
                if !matches!(target, DockNode::Stack { .. }) {
                    return Err(DockError::UnknownStack);
                }
                let new = DockNode::Stack {
                    key: new_stack,
                    panes: vec![pane],
                    active: pane,
                };
                let (first, second) = if matches!(edge, Edge::Left | Edge::Top) {
                    (new, target.clone())
                } else {
                    (target.clone(), new)
                };
                *target = DockNode::Split {
                    key: split_key,
                    axis: if matches!(edge, Edge::Left | Edge::Right) {
                        Axis::Horizontal
                    } else {
                        Axis::Vertical
                    },
                    ratio: 0.5,
                    first: Box::new(first),
                    second: Box::new(second),
                };
            }
            DropLocation::Floating { bounds } => {
                state.surfaces.push(DockSurface {
                    key: self.next_surface,
                    bounds,
                    root: DockNode::Stack {
                        key: self.next_node,
                        panes: vec![pane],
                        active: pane,
                    },
                });
            }
        }
        self.commit(state)
    }

    pub fn resize_split(&mut self, key: u64, ratio: f64) -> Result<bool, DockError> {
        if !ratio.is_finite() || !(0.1..=0.9).contains(&ratio) {
            return Err(DockError::InvalidRatio);
        }
        let mut state = self.state.clone();
        let Some(DockNode::Split { ratio: current, .. }) = Self::find_mut(&mut state, key) else {
            return Err(DockError::UnknownSplit);
        };
        *current = ratio;
        self.commit(state)
    }

    pub fn set_bounds(&mut self, surface: u64, bounds: Bounds) -> Result<bool, DockError> {
        let mut state = self.state.clone();
        state
            .surfaces
            .iter_mut()
            .find(|candidate| candidate.key == surface)
            .ok_or(DockError::UnknownSurface)?
            .bounds = bounds;
        self.commit(state)
    }

    pub fn close(&mut self, pane: u64) -> Result<bool, DockError> {
        if !self
            .state
            .panes
            .iter()
            .find(|record| record.key == pane)
            .ok_or(DockError::UnknownPane)?
            .closable
        {
            return Err(DockError::ProtectedPane);
        }
        let mut state = self.state.clone();
        Self::remove(&mut state, pane)?;
        state.panes.retain(|record| record.key != pane);
        self.commit(state)
    }

    pub fn dock_surface(&mut self, surface: u64, stack: u64) -> Result<bool, DockError> {
        self.dock_surface_with_active(surface, stack, None)
    }

    pub fn dock_surface_with_active(
        &mut self,
        surface: u64,
        stack: u64,
        selected: Option<u64>,
    ) -> Result<bool, DockError> {
        if surface == 1 {
            return Err(DockError::MainWindowEmpty);
        }
        let mut state = self.state.clone();
        let position = state
            .surfaces
            .iter()
            .position(|candidate| candidate.key == surface)
            .ok_or(DockError::UnknownSurface)?;
        let mut panes = Vec::new();
        let mut source_active = None;
        let mut pending = vec![&state.surfaces[position].root];
        while let Some(node) = pending.pop() {
            match node {
                DockNode::Stack {
                    panes: keys,
                    active,
                    ..
                } => {
                    source_active.get_or_insert(*active);
                    panes.extend(keys);
                }
                DockNode::Split { first, second, .. } => {
                    pending.push(second);
                    pending.push(first);
                }
            }
        }
        state.surfaces.remove(position);
        let Some(DockNode::Stack {
            panes: target,
            active,
            ..
        }) = Self::find_mut(&mut state, stack)
        else {
            return Err(DockError::UnknownStack);
        };
        let selected = selected.or(source_active).ok_or(DockError::UnknownPane)?;
        if !panes.contains(&selected) && !target.contains(&selected) {
            return Err(DockError::UnknownPane);
        }
        *active = selected;
        target.extend(panes);
        self.commit(state)
    }

    pub fn restore(&mut self, bytes: &[u8]) -> Result<bool, DockError> {
        if bytes.len() > self.limits.serialized_bytes.min(1024 * 1024) {
            return Err(DockError::Budget);
        }
        let state = serde_json::from_slice(bytes).map_err(|_| DockError::InvalidLayout)?;
        self.commit(state)
    }

    pub fn undo(&mut self) -> Result<bool, DockError> {
        if self.undo.is_empty() {
            return Ok(false);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(DockError::IdentityExhausted)?;
        let previous = self.undo.pop().ok_or(DockError::InvalidLayout)?;
        self.redo.push(std::mem::replace(&mut self.state, previous));
        self.revision = revision;
        self.drag = None;
        Ok(true)
    }

    pub fn redo(&mut self) -> Result<bool, DockError> {
        if self.redo.is_empty() {
            return Ok(false);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(DockError::IdentityExhausted)?;
        let next = self.redo.pop().ok_or(DockError::InvalidLayout)?;
        self.undo.push(std::mem::replace(&mut self.state, next));
        self.revision = revision;
        self.drag = None;
        Ok(true)
    }

    pub fn begin_drag(&mut self, pane: u64) -> Result<DragToken, DockError> {
        self.stack_of(pane).ok_or(DockError::UnknownPane)?;
        if self.drag.is_some() {
            return Err(DockError::DragBusy);
        }
        self.next_drag = self
            .next_drag
            .checked_add(1)
            .ok_or(DockError::IdentityExhausted)?;
        let token = DragToken {
            owner: self.owner,
            generation: self.next_drag,
            revision: self.revision,
            pane,
        };
        self.drag = Some(token.clone());
        Ok(token)
    }

    fn validate_drag(&self, token: &DragToken) -> Result<(), DockError> {
        if self.drag.as_ref() != Some(token)
            || token.owner != self.owner
            || token.revision != self.revision
        {
            Err(DockError::StaleDrag)
        } else {
            Ok(())
        }
    }

    pub fn cancel_drag(&mut self, token: &DragToken) -> Result<(), DockError> {
        self.validate_drag(token)?;
        self.drag = None;
        Ok(())
    }

    pub fn drop_drag(
        &mut self,
        token: &DragToken,
        location: DropLocation,
    ) -> Result<bool, DockError> {
        self.validate_drag(token)?;
        let changed = self.relocate(token.pane, location)?;
        self.drag = None;
        Ok(changed)
    }
}
