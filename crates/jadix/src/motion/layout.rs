use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use crate::core::reactivity::use_signal;
use crate::core::tree::{Element, LayoutStatus};
use crate::style::{AnimatedNumber, ValueDriver};

/// Layout is committed once; only its visual projection animates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LayoutAnimation {
    #[default]
    Position,
    Size,
    Both,
}

impl LayoutAnimation {
    fn animates(self, index: usize) -> bool {
        match self {
            Self::Position => index < 2,
            Self::Size => index >= 2,
            Self::Both => true,
        }
    }
}

#[derive(Clone, Copy)]
struct ProjectionContext {
    top_left: [f32; 2],
    origin: [f32; 2],
    scale: [f32; 2],
    author_scale: [f32; 2],
    visual: [f32; 2],
    axis_aligned: bool,
}

impl Default for ProjectionContext {
    fn default() -> Self {
        Self {
            top_left: [0.0; 2],
            origin: [0.0; 2],
            scale: [1.0; 2],
            author_scale: [1.0; 2],
            visual: [0.0; 2],
            axis_aligned: true,
        }
    }
}

impl ProjectionContext {
    fn base(self, bounds: [f32; 4]) -> [f32; 2] {
        std::array::from_fn(|axis| {
            self.top_left[axis]
                + self.origin[axis]
                + self.scale[axis] * (bounds[axis] + bounds[axis + 2] * 0.5 - self.origin[axis])
                - bounds[axis + 2] * 0.5
        })
    }
}

struct LayoutInner {
    mode: LayoutAnimation,
    driver: ValueDriver,
    target: Option<[f32; 4]>,
    channels: [AnimatedNumber; 4],
    last_sample: Option<[f32; 4]>,
    context: ProjectionContext,
}

/// Retain one handle per keyed element, including across list reordering.
#[derive(Clone)]
pub struct LayoutMotion(Rc<RefCell<LayoutInner>>);

impl Default for LayoutMotion {
    fn default() -> Self {
        Self::new(LayoutAnimation::Position)
    }
}

impl LayoutMotion {
    pub fn new(mode: LayoutAnimation) -> Self {
        Self(Rc::new(RefCell::new(LayoutInner {
            mode,
            driver: ValueDriver::spring(),
            target: None,
            channels: std::array::from_fn(|_| AnimatedNumber::follow(0.0)),
            last_sample: None,
            context: ProjectionContext::default(),
        })))
    }

    pub fn transition(self, driver: ValueDriver) -> Self {
        assert!(
            !matches!(driver, ValueDriver::Inertia(_)),
            "layout targets require a spring or tween"
        );
        self.0.borrow_mut().driver = driver;
        self
    }

    pub fn is_active(&self) -> bool {
        self.0
            .borrow()
            .channels
            .iter()
            .any(AnimatedNumber::is_active)
    }

    /// Current parent-local presentation rectangle, or `None` before first layout.
    pub fn bounds(&self) -> Option<[f32; 4]> {
        let state = self.0.borrow();
        state.target?;
        Some(std::array::from_fn(|index| state.channels[index].peek()))
    }

    pub(crate) fn velocities(&self) -> [f32; 4] {
        let state = self.0.borrow();
        std::array::from_fn(|index| state.channels[index].velocity())
    }

    pub(crate) fn update(&self, bounds: [f32; 4]) {
        if !bounds.iter().all(|value| value.is_finite()) {
            return;
        }
        let mut state = self.0.borrow_mut();
        let reduced = crate::style::prefers_reduced_motion();
        for (index, (&value, channel)) in bounds.iter().zip(&state.channels).enumerate() {
            let animate = state.mode.animates(index);
            let changed = state
                .target
                .is_none_or(|target| (target[index] - value).abs() > 1e-4);
            if reduced
                || !animate
                || state.target.is_none()
                || state
                    .target
                    .is_some_and(|target| target[2] <= 1e-4 || target[3] <= 1e-4)
                || bounds[2] <= 1e-4
                || bounds[3] <= 1e-4
            {
                if changed || channel.is_active() {
                    channel.jump(value);
                }
            } else if changed {
                channel.restart(value, state.driver.clone());
            }
        }
        state.target = Some(bounds);
        if state.channels.iter().any(AnimatedNumber::is_active) {
            crate::app::invalidation::request_frame();
        }
    }

    fn adopt(&self, previous: &SharedLayout, bounds: [f32; 4], element: &Element) {
        let mut state = self.0.borrow_mut();
        let context = state.context;
        let origin = crate::style::origin_px(&element.style.transform_origin, bounds[2], bounds[3]);
        let mut initial = previous.bounds;
        for axis in 0..2 {
            let author_scale = positive_scale(element.style.scale[axis]);
            initial[axis + 2] /= context.author_scale[axis] * author_scale;
            let ratio = initial[axis + 2] / bounds[axis + 2].max(1e-4);
            initial[axis] = (initial[axis]
                - context.visual[axis]
                - element.style.translate[axis]
                - bounds[axis + 2] * element.style.translate_pct[axis]
                - origin[axis] * ratio * context.author_scale[axis] * (1.0 - author_scale))
                / context.author_scale[axis];
        }
        let valid = bounds[2] > 1e-4 && bounds[3] > 1e-4 && initial[2] > 1e-4 && initial[3] > 1e-4;
        for (index, channel) in state.channels.iter().enumerate() {
            if valid
                && state.mode.animates(index)
                && !crate::style::prefers_reduced_motion()
                && (initial[index] - bounds[index]).abs() > 1e-4
            {
                let axis = index % 2;
                let velocity = previous.velocity[index] / context.author_scale[axis];
                channel.restart_from(
                    initial[index],
                    bounds[index],
                    velocity,
                    state.driver.clone(),
                );
            } else {
                channel.jump(bounds[index]);
            }
        }
        state.target = Some(bounds);
        state.last_sample = None;
    }

    fn sync(&self) -> LayoutStatus {
        let mut state = self.0.borrow_mut();
        if state.target.is_none() {
            return LayoutStatus::Clean;
        }
        if crate::style::prefers_reduced_motion() {
            for (channel, target) in state.channels.iter().zip(state.target.unwrap()) {
                if channel.is_active() {
                    channel.jump(target);
                }
            }
        }
        let sample = std::array::from_fn(|index| state.channels[index].peek());
        if state.last_sample.replace(sample) == Some(sample) {
            LayoutStatus::Clean
        } else {
            LayoutStatus::StyleChanged
        }
    }

    pub(crate) fn projection(&self, origin: [f32; 2]) -> ([f32; 2], [f32; 2]) {
        let state = self.0.borrow();
        let Some(target) = state.target else {
            return ([0.0; 2], [1.0; 2]);
        };
        let sample: [f32; 4] = std::array::from_fn(|index| state.channels[index].peek());
        let ratio: [f32; 2] = std::array::from_fn(|axis| {
            if target[axis + 2] > 1e-4 {
                (sample[axis + 2] / target[axis + 2]).max(0.001)
            } else {
                1.0
            }
        });
        let context = state.context;
        let base = context.base(target);
        let scale = std::array::from_fn(|axis| {
            ratio[axis] * context.author_scale[axis] / context.scale[axis]
        });
        let translate = std::array::from_fn(|axis| {
            context.visual[axis] + sample[axis] * context.author_scale[axis] - base[axis]
                + origin[axis] * (ratio[axis] * context.author_scale[axis] - 1.0)
        });
        (translate, scale)
    }
}

pub fn use_layout_motion(id: &str, mode: LayoutAnimation) -> LayoutMotion {
    use_signal(id, move || LayoutMotion::new(mode)).peek()
}

impl Element {
    /// Share the last presented rectangle with the same ID in this window's layout tree.
    /// Only one visible element may own an ID; duplicates disable shared handoff.
    pub fn layout_id(mut self, id: impl Into<String>) -> Self {
        self.set_attr("jadix_layout_id", id.into());
        if self.layout_motion.is_none() {
            self = self.layout_motion(&LayoutMotion::new(LayoutAnimation::Both));
        }
        self
    }

    pub fn layout_motion(mut self, motion: &LayoutMotion) -> Self {
        self.layout_motion = Some(motion.clone());
        let previous = self.reactive_sync.take();
        self.reactive_sync(move |element, tree| {
            previous
                .as_ref()
                .map_or(LayoutStatus::Clean, |sync| sync.run(element, tree))
                .merge(
                    element
                        .layout_motion
                        .as_ref()
                        .map_or(LayoutStatus::Clean, LayoutMotion::sync),
                )
        })
    }
}

fn positive_scale(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        1.0
    }
}

struct SharedLayout {
    owner: Weak<RefCell<LayoutInner>>,
    parent: Option<taffy::NodeId>,
    bounds: [f32; 4],
    velocity: [f32; 4],
    missing_frames: u8,
}

#[derive(Default)]
pub(crate) struct LayoutRegistry {
    shared: HashMap<String, SharedLayout>,
}

impl LayoutRegistry {
    pub(crate) fn measure(&mut self, root: &mut Element, taffy: &taffy::TaffyTree<()>) {
        let mut counts = HashMap::<String, usize>::new();
        crate::core::tree::walk_elements_pruned(root, |element| {
            if element.collect_paint_hash != 0 && !element.subtree_layout_motion {
                return false;
            }
            if element.style.display != taffy::style::Display::None {
                if let Some(id) = element.attrs.get("jadix_layout_id") {
                    *counts.entry(id.clone()).or_default() += 1;
                }
                true
            } else {
                false
            }
        });
        let mut next = HashMap::new();
        let mut stack = vec![(root, ProjectionContext::default(), None)];
        while let Some((element, context, parent)) = stack.pop() {
            if element.collect_paint_hash != 0 && !element.subtree_layout_motion {
                continue;
            }
            if element.style.display == taffy::style::Display::None {
                continue;
            }
            let Some(layout) = element.layout_node.and_then(|node| taffy.layout(node).ok()) else {
                continue;
            };
            let bounds = [
                layout.location.x,
                layout.location.y,
                layout.size.width,
                layout.size.height,
            ];
            let axis_aligned = context.axis_aligned
                && element.style.rotate_deg.abs() < 1e-4
                && element
                    .style
                    .transform
                    .as_ref()
                    .is_none_or(|list| list.is_empty());
            let id = element
                .attrs
                .get("jadix_layout_id")
                .filter(|id| counts.get(*id) == Some(&1) && axis_aligned);
            if let Some(motion) = &element.layout_motion {
                motion.0.borrow_mut().context = context;
                if let Some(previous) = id.and_then(|id| self.shared.get(id)) {
                    if !previous.owner.ptr_eq(&Rc::downgrade(&motion.0))
                        || previous.parent != parent
                    {
                        motion.adopt(previous, bounds, element);
                    }
                }
                motion.update(bounds);
            }
            let origin =
                crate::style::origin_px(&element.style.transform_origin, bounds[2], bounds[3]);
            let base = context.base(bounds);
            let (projection_translate, projection_scale) = element
                .layout_motion
                .as_ref()
                .map_or(([0.0; 2], [1.0; 2]), |motion| motion.projection(origin));
            let translate: [f32; 2] = std::array::from_fn(|axis| {
                element.style.translate[axis] + projection_translate[axis]
            });
            let scale: [f32; 2] = std::array::from_fn(|axis| {
                positive_scale(element.style.scale[axis]) * projection_scale[axis]
            });
            let total_scale: [f32; 2] = std::array::from_fn(|axis| {
                (context.scale[axis] * positive_scale(scale[axis])).clamp(0.01, 8.0)
            });
            let top_left: [f32; 2] = std::array::from_fn(|axis| {
                base[axis] + translate[axis] + bounds[axis + 2] * element.style.translate_pct[axis]
            });
            let visual: [f32; 2] = std::array::from_fn(|axis| {
                top_left[axis] + origin[axis] * (1.0 - total_scale[axis])
            });
            if let (Some(id), Some(motion)) = (id, &element.layout_motion) {
                let velocity = motion.velocities();
                next.insert(
                    id.clone(),
                    SharedLayout {
                        owner: Rc::downgrade(&motion.0),
                        parent,
                        bounds: [
                            visual[0],
                            visual[1],
                            bounds[2] * total_scale[0],
                            bounds[3] * total_scale[1],
                        ],
                        velocity: std::array::from_fn(|index| {
                            velocity[index] * context.author_scale[index % 2]
                        }),
                        missing_frames: 0,
                    },
                );
            }
            let child_context = ProjectionContext {
                top_left: std::array::from_fn(|axis| top_left[axis] - element.scroll_offset[axis]),
                origin,
                scale: total_scale,
                author_scale: std::array::from_fn(|axis| {
                    context.author_scale[axis] * positive_scale(element.style.scale[axis])
                }),
                visual: std::array::from_fn(|axis| visual[axis] - element.scroll_offset[axis]),
                axis_aligned,
            };
            let parent_node = element.layout_node;
            for child in element.children.iter_mut().rev() {
                stack.push((child, child_context, parent_node));
            }
        }
        for (id, mut previous) in self.shared.drain() {
            if !counts.contains_key(&id) && previous.missing_frames == 0 {
                previous.missing_frames = 1;
                next.insert(id, previous);
            }
        }
        self.shared = next;
    }
}
