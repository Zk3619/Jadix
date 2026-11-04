use super::{NodeId, Rect, RootId, RootLifetime};

pub mod host;
pub mod policy;
pub mod snapshot;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Presentation {
    Gpu,
    Native,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HybridGeometry {
    pub node: NodeId,
    pub revision: u64,
    pub presentation: Presentation,
    pub frame: Rect,
    pub clips: Vec<Rect>,
    pub visible: bool,
    pub interactive: bool,
    pub z_index: i32,
}

impl HybridGeometry {
    pub fn is_valid(&self) -> bool {
        self.frame.is_valid() && self.clips.iter().all(|clip| clip.is_valid())
    }

    pub fn hit(&self, x: f64, y: f64) -> bool {
        self.visible
            && self.interactive
            && self.frame.contains(x, y)
            && self.clips.iter().all(|clip| clip.contains(x, y))
    }
}

pub struct HybridCoordinator {
    lifetime: RootLifetime,
    visible_revision: u64,
    visible: Vec<HybridGeometry>,
    pending: Option<(u64, Vec<HybridGeometry>)>,
    focus: Option<(NodeId, Presentation)>,
    capture: Option<(NodeId, Presentation)>,
    composing: bool,
}

impl HybridCoordinator {
    pub fn new(root: RootId) -> Self {
        Self::with_lifetime(RootLifetime::new(root))
    }

    pub fn with_lifetime(lifetime: RootLifetime) -> Self {
        Self {
            lifetime,
            visible_revision: 0,
            visible: Vec::new(),
            pending: None,
            focus: None,
            capture: None,
            composing: false,
        }
    }

    pub fn stage(&mut self, revision: u64, geometry: Vec<HybridGeometry>) -> bool {
        if self.lifetime.is_disposed()
            || revision <= self.visible_revision
            || self.pending.is_some()
            || (self.composing
                && self.focus.is_some_and(|(node, presenter)| {
                    !geometry.iter().any(|entry| {
                        entry.node == node
                            && entry.presentation == presenter
                            && entry.visible
                            && entry.interactive
                    })
                }))
            || geometry.iter().any(|entry| {
                entry.node.root != self.lifetime.id()
                    || entry.revision != revision
                    || !entry.is_valid()
                    || self.focus.is_some_and(|(node, presenter)| {
                        node == entry.node && presenter != entry.presentation
                    })
                    || self.capture.is_some_and(|(node, presenter)| {
                        node == entry.node && presenter != entry.presentation
                    })
            })
            || geometry.iter().enumerate().any(|(index, entry)| {
                geometry[..index]
                    .iter()
                    .any(|previous| previous.node == entry.node)
            })
        {
            return false;
        }
        self.pending = Some((revision, geometry));
        true
    }

    pub fn can_confirm(&self, revision: u64, gpu_ready: bool, native_ready: bool) -> bool {
        if self.lifetime.is_disposed() {
            return false;
        }
        let Some((pending_revision, geometry)) = self.pending.as_ref() else {
            return false;
        };
        if *pending_revision != revision
            || (self.composing
                && self.focus.is_some_and(|(node, presenter)| {
                    !geometry.iter().any(|entry| {
                        entry.node == node
                            && entry.presentation == presenter
                            && entry.visible
                            && entry.interactive
                    })
                }))
            || geometry.iter().any(|entry| match entry.presentation {
                Presentation::Gpu => !gpu_ready,
                Presentation::Native => !native_ready,
            })
            || geometry.iter().any(|entry| {
                self.focus.is_some_and(|(node, presenter)| {
                    node == entry.node && presenter != entry.presentation
                })
            })
            || geometry.iter().any(|entry| {
                self.capture.is_some_and(|(node, presenter)| {
                    node == entry.node && presenter != entry.presentation
                })
            })
        {
            return false;
        }
        true
    }

    pub fn confirm(&mut self, revision: u64, gpu_ready: bool, native_ready: bool) -> bool {
        if !self.can_confirm(revision, gpu_ready, native_ready) {
            return false;
        }
        let (_, geometry) = self.pending.take().unwrap();
        self.visible_revision = revision;
        self.visible = geometry;
        if self.focus.is_some_and(|(node, presenter)| {
            !self.visible.iter().any(|entry| {
                entry.node == node
                    && entry.presentation == presenter
                    && entry.visible
                    && entry.interactive
            })
        }) {
            self.focus = None;
            self.composing = false;
        }
        if self.capture.is_some_and(|(node, presenter)| {
            !self.visible.iter().any(|entry| {
                entry.node == node
                    && entry.presentation == presenter
                    && entry.visible
                    && entry.interactive
            })
        }) {
            self.capture = None;
        }
        true
    }

    pub fn cancel(&mut self, revision: u64) -> bool {
        if self.lifetime.is_disposed() {
            return false;
        }
        if self
            .pending
            .as_ref()
            .is_some_and(|(pending, _)| *pending == revision)
        {
            self.pending = None;
            true
        } else {
            false
        }
    }

    pub fn hit(&self, x: f64, y: f64) -> Option<&HybridGeometry> {
        if self.lifetime.is_disposed() {
            return None;
        }
        self.visible
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.hit(x, y))
            .max_by_key(|(index, entry)| (entry.z_index, *index))
            .map(|(_, entry)| entry)
    }

    pub fn visible_revision(&self) -> u64 {
        self.visible_revision
    }

    pub fn focus(&mut self, revision: u64, node: NodeId) -> bool {
        if self.lifetime.is_disposed() || revision != self.visible_revision {
            return false;
        }
        let Some(entry) = self
            .visible
            .iter()
            .find(|entry| entry.node == node && entry.visible && entry.interactive)
        else {
            return false;
        };
        if self.composing && self.focus.is_some_and(|(focused, _)| focused != node) {
            return false;
        }
        self.focus = Some((node, entry.presentation));
        true
    }

    pub fn focused(&self) -> Option<&HybridGeometry> {
        if self.lifetime.is_disposed() {
            return None;
        }
        let (node, presenter) = self.focus?;
        self.visible
            .iter()
            .find(|entry| entry.node == node && entry.presentation == presenter)
    }

    pub fn blur(&mut self, revision: u64, node: NodeId) -> bool {
        if self.lifetime.is_disposed()
            || revision != self.visible_revision
            || self.focus.is_none_or(|(focused, _)| focused != node)
            || self.composing
        {
            return false;
        }
        self.focus = None;
        true
    }

    pub fn set_composing(&mut self, revision: u64, node: NodeId, composing: bool) -> bool {
        if self.lifetime.is_disposed()
            || revision != self.visible_revision
            || self.focus.is_none_or(|(focused, _)| focused != node)
        {
            return false;
        }
        self.composing = composing;
        true
    }

    pub fn capture_pointer(&mut self, revision: u64, node: NodeId) -> bool {
        if self.lifetime.is_disposed() || revision != self.visible_revision {
            return false;
        }
        let Some(entry) = self
            .visible
            .iter()
            .find(|entry| entry.node == node && entry.visible && entry.interactive)
        else {
            return false;
        };
        self.capture = Some((node, entry.presentation));
        true
    }

    pub fn release_pointer(&mut self, revision: u64, node: NodeId) -> bool {
        if self.lifetime.is_disposed()
            || revision != self.visible_revision
            || self.capture.is_none_or(|(captured, _)| captured != node)
        {
            return false;
        }
        self.capture = None;
        true
    }

    pub fn pointer_target(&self, x: f64, y: f64) -> Option<&HybridGeometry> {
        if self.lifetime.is_disposed() {
            return None;
        }
        match self.capture {
            Some((node, presenter)) => self
                .visible
                .iter()
                .find(|entry| entry.node == node && entry.presentation == presenter),
            None => self.hit(x, y),
        }
    }

    pub fn semantic_owner(&self, node: NodeId) -> Option<Presentation> {
        if self.lifetime.is_disposed() {
            return None;
        }
        self.visible
            .iter()
            .find(|entry| entry.node == node && entry.visible)
            .map(|entry| entry.presentation)
    }

    pub fn dispose(&mut self) {
        self.lifetime.dispose();
        self.visible.clear();
        self.pending = None;
        self.focus = None;
        self.capture = None;
        self.composing = false;
    }
}
