use super::{HybridGeometry, Presentation, RootLifetime};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SnapshotTicket {
    owner: super::super::RootId,
    node: super::super::NodeId,
    sequence: u64,
    revision: u64,
    width: u32,
    height: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SnapshotError {
    Disposed,
    PolicyDenied,
    Interactive,
    Geometry,
    Budget,
    Busy,
    Stale,
    Pixels,
}

pub struct SnapshotStore {
    owner: super::super::RootId,
    lifetime: RootLifetime,
    budget: usize,
    revision: u64,
    next: u64,
    pending: Option<SnapshotTicket>,
    pixels: Option<(SnapshotTicket, Rc<[u8]>)>,
}

impl SnapshotStore {
    pub fn new(lifetime: RootLifetime, budget: usize) -> Result<Self, SnapshotError> {
        if budget == 0 || budget > 16 * 1024 * 1024 {
            return Err(SnapshotError::Budget);
        }
        Ok(Self {
            owner: super::super::RootId::new(),
            lifetime,
            budget,
            revision: 0,
            next: 0,
            pending: None,
            pixels: None,
        })
    }

    pub fn invalidate(&mut self, revision: u64) -> Result<(), SnapshotError> {
        if self.lifetime.is_disposed() {
            self.clear();
            return Err(SnapshotError::Disposed);
        }
        if revision <= self.revision {
            return Err(SnapshotError::Stale);
        }
        self.revision = revision;
        self.clear();
        Ok(())
    }

    pub fn request(
        &mut self,
        geometry: &HybridGeometry,
        size: [u32; 2],
        capture_allowed: bool,
    ) -> Result<SnapshotTicket, SnapshotError> {
        if self.lifetime.is_disposed() {
            self.clear();
            return Err(SnapshotError::Disposed);
        }
        if !capture_allowed {
            return Err(SnapshotError::PolicyDenied);
        }
        if geometry.interactive {
            return Err(SnapshotError::Interactive);
        }
        if geometry.node.root != self.lifetime.id()
            || geometry.presentation != Presentation::Native
            || !geometry.is_valid()
            || !geometry.visible
            || geometry.revision != self.revision
        {
            return Err(SnapshotError::Geometry);
        }
        let [width, height] = size;
        let bytes = (width as usize)
            .checked_mul(height as usize)
            .and_then(|size| size.checked_mul(4))
            .ok_or(SnapshotError::Budget)?;
        if width == 0 || height == 0 || bytes > self.budget {
            return Err(SnapshotError::Budget);
        }
        if self.pending.is_some() {
            return Err(SnapshotError::Busy);
        }
        self.next = self.next.checked_add(1).ok_or(SnapshotError::Budget)?;
        self.pixels = None;
        let ticket = SnapshotTicket {
            owner: self.owner,
            node: geometry.node,
            sequence: self.next,
            revision: self.revision,
            width,
            height,
        };
        self.pending = Some(ticket);
        Ok(ticket)
    }

    pub fn complete(&mut self, ticket: SnapshotTicket, rgba: Vec<u8>) -> Result<(), SnapshotError> {
        if self.lifetime.is_disposed() {
            self.clear();
            return Err(SnapshotError::Disposed);
        }
        if self.pending != Some(ticket) || ticket.revision != self.revision {
            return Err(SnapshotError::Stale);
        }
        if rgba.len() != ticket.width as usize * ticket.height as usize * 4 {
            return Err(SnapshotError::Pixels);
        }
        self.pending = None;
        self.pixels = Some((ticket, rgba.into()));
        Ok(())
    }

    pub fn pixels(&self, node: super::super::NodeId, revision: u64) -> Option<&[u8]> {
        if self.lifetime.is_disposed() || revision != self.revision {
            return None;
        }
        self.pixels
            .as_ref()
            .filter(|(ticket, _)| ticket.node == node)
            .map(|(_, pixels)| pixels.as_ref())
    }

    pub fn cancel(&mut self, ticket: SnapshotTicket) -> bool {
        if self.lifetime.is_disposed() || self.pending != Some(ticket) {
            return false;
        }
        self.pending = None;
        true
    }

    pub fn clear(&mut self) {
        self.pending = None;
        self.pixels = None;
    }
}
