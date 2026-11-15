//! Real Element rendering and production pointer dispatch for a terminal host.
use crate::{RgbaFrame, TerminalViewport};
use jadix::{
    core::tree::Element,
    renderer::offscreen::{OffscreenGpu, SceneRenderer},
};
use std::io;

/// Windowless Jadix scene. The caller owns its Element state and event loop;
/// rendering does not create an App, window, native view, or GPUI runtime.
pub struct JadixScene {
    renderer: SceneRenderer,
}

impl JadixScene {
    pub fn new() -> io::Result<Self> {
        OffscreenGpu::new()
            .map(|gpu| Self {
                renderer: SceneRenderer::new(gpu),
            })
            .ok_or_else(|| io::Error::other("no headless GPU adapter available"))
    }

    pub fn renderer(&self) -> &SceneRenderer {
        &self.renderer
    }
    /// Preserve keyed layout identities and unmount hooks across rebuilt views.
    pub fn replace_root(
        &mut self,
        root: &mut Element,
        next: Element,
    ) -> jadix::core::tree::LayoutStatus {
        self.renderer.replace_root(root, next)
    }
    pub fn hit_text(&mut self, leaf: &Element, pointer: [f32; 2]) -> usize {
        self.renderer.hit_text(leaf, pointer)
    }
    pub fn caret_wakeup_after(&self) -> std::time::Duration {
        self.renderer.caret_wakeup_after()
    }
    pub fn restart_caret(&self) {
        self.renderer.restart_caret();
    }

    pub fn render(
        &mut self,
        root: &mut Element,
        viewport: TerminalViewport,
        pointer: [f32; 2],
    ) -> io::Result<RgbaFrame> {
        let image = self.renderer.render(
            root,
            viewport.pixels(),
            viewport.scale() as f32,
            pointer,
            wgpu::Color::BLACK,
        )?;
        RgbaFrame::new(image.width, image.height, image.pixels)
    }
}

/// Pointer gesture state for Element callbacks. Uses Jadix's clipped hit paths,
/// capture/target/bubble dispatch and down/up click intersection. Coordinates
/// are logical pixels from `TerminalInput`, never terminal cells.
#[derive(Default)]
pub struct SceneInput {
    down: Vec<jadix::app::pointer_events::HitPathEntry>,
    down_origin: Option<[f32; 2]>,
    focus: Option<String>,
    modals: Vec<(String, Option<String>)>,
}

impl SceneInput {
    pub fn modal_depth(&self) -> usize {
        self.modals.len()
    }
    pub fn has_modal(&self) -> bool {
        !self.modals.is_empty()
    }
    fn scope<'a>(&self, root: &'a Element) -> Option<&'a Element> {
        match self.modals.last() {
            Some((id, _)) => jadix::app::pointer_events::find_by_focus_id(root, id),
            None => Some(root),
        }
    }
    fn focus_ids(root: &Element) -> Vec<String> {
        let mut ids = Vec::new();
        jadix::utils::input_hit::collect_focusable_ids(root, &mut ids);
        ids.retain(|id| {
            jadix::app::pointer_events::find_by_focus_id(root, id).is_some_and(|el| {
                let b = el.viewport_layout_bounds();
                b.iter().all(|v| v.is_finite()) && b[2] > 0.0 && b[3] > 0.0
            })
        });
        ids
    }
    /// Explicit modal focus scope, opened after the tree has been laid out.
    /// Eight nested scopes maximum; no duplicate IDs or empty focus scopes.
    pub fn push_modal(&mut self, root: &Element, id: &str) -> io::Result<()> {
        if self.modals.len() == 8 || self.modals.iter().any(|(m, _)| m == id) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "duplicate modal or modal depth limit",
            ));
        }
        let scope = jadix::app::pointer_events::find_by_focus_id(root, id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "modal root missing"))?;
        let first = Self::focus_ids(scope).into_iter().next().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "modal has no laid-out focus target",
            )
        })?;
        self.modals.push((id.into(), self.focus.take()));
        self.focus = Some(first);
        self.cancel();
        Ok(())
    }
    pub fn pop_modal(&mut self, root: &Element) -> bool {
        let Some((_, previous)) = self.modals.pop() else {
            return false;
        };
        let ids = self.scope(root).map(Self::focus_ids).unwrap_or_default();
        self.focus = previous
            .filter(|id| ids.contains(id))
            .or_else(|| ids.into_iter().next());
        self.cancel();
        true
    }
    pub fn cancel(&mut self) {
        self.down.clear();
        self.down_origin = None;
    }
    /// After laying out a rebuilt tree, retain a press only when its deepest
    /// explicit identity still hits the original point. Replace all saved
    /// callbacks with current-tree callbacks. Unnamed/replaced targets cancel.
    /// Navigation/root replacement can still explicitly cancel before rebuild.
    pub fn refresh_gesture(&mut self, root: &Element) {
        while self
            .modals
            .last()
            .is_some_and(|(id, _)| jadix::app::pointer_events::find_by_focus_id(root, id).is_none())
        {
            self.pop_modal(root);
        }
        let Some([x, y]) = self.down_origin else {
            return;
        };
        let path = self.hit_path(root, x, y);
        let old = self.down.iter().rev().find_map(|entry| entry.target_key());
        let new = path.iter().rev().find_map(|entry| entry.target_key());
        if old.is_some() && old == new {
            self.down = path;
        } else {
            self.cancel();
        }
    }
    pub fn focused_id(&self) -> Option<&str> {
        self.focus.as_deref()
    }
    pub fn focus(&mut self, id: Option<String>) {
        self.focus = id;
        self.cancel();
    }
    fn hit_path(
        &self,
        root: &Element,
        x: f32,
        y: f32,
    ) -> Vec<jadix::app::pointer_events::HitPathEntry> {
        let path = jadix::app::pointer_events::hit_path(root, x, y);
        if let Some((id, _)) = self.modals.last() {
            path.iter()
                .position(|entry| entry.focus_id.as_ref() == Some(id))
                .map_or_else(Vec::new, |at| path[at..].to_vec())
        } else {
            path
        }
    }

    /// Dispatch primary-button down/up/move, wheel, Tab focus and Enter activation.
    /// Text editing and application scheduling remain host responsibilities;
    /// Explicit modal scopes trap focus and isolate pointer callbacks; Escape
    /// restores prior focus. False means unhandled. No OS pointer capture.
    pub fn dispatch(&mut self, root: &Element, input: &crate::TerminalInput) -> bool {
        use crate::crossterm::event::{KeyModifiers, MouseButton, MouseEventKind};
        use jadix::app::pointer_events::{self as p, PointerKind};
        if let crate::TerminalInput::Key(key) = input {
            use crate::crossterm::event::{KeyCode, KeyEventKind};
            if key.kind == KeyEventKind::Release {
                return false;
            }
            if key.code == KeyCode::Esc && self.has_modal() {
                return self.pop_modal(root);
            }
            if matches!(key.code, KeyCode::Tab | KeyCode::BackTab)
                && !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            {
                let ids = self.scope(root).map(Self::focus_ids).unwrap_or_default();
                if ids.is_empty() {
                    self.focus = None;
                    return false;
                }
                let backwards =
                    key.code == KeyCode::BackTab || key.modifiers.contains(KeyModifiers::SHIFT);
                let at = ids
                    .iter()
                    .position(|id| Some(id.as_str()) == self.focused_id());
                let next = at.map_or(if backwards { ids.len() - 1 } else { 0 }, |i| {
                    if backwards {
                        (i + ids.len() - 1) % ids.len()
                    } else {
                        (i + 1) % ids.len()
                    }
                });
                self.focus = Some(ids[next].clone());
                self.cancel();
                return true;
            }
            if key.code == KeyCode::Enter && key.modifiers.is_empty() {
                if let Some(callback) = self
                    .focused_id()
                    .and_then(|id| {
                        self.scope(root)
                            .and_then(|scope| p::find_by_focus_id(scope, id))
                    })
                    .and_then(|el| el.on_click.clone())
                {
                    callback.borrow_mut()();
                    return true;
                }
            }
        }
        let crate::TerminalInput::Pointer {
            kind,
            logical,
            modifiers,
        } = input
        else {
            if matches!(
                input,
                crate::TerminalInput::Focus(false) | crate::TerminalInput::Resize(_)
            ) {
                self.cancel();
            }
            return false;
        };
        let [x, y] = logical.map(|v| v as f32);
        if !x.is_finite() || !y.is_finite() {
            self.cancel();
            return false;
        }
        let path = self.hit_path(root, x, y);
        if path.is_empty() && self.has_modal() {
            self.cancel();
            return true;
        }
        let delta = match kind {
            MouseEventKind::ScrollUp => Some([0.0, -40.0]),
            MouseEventKind::ScrollDown => Some([0.0, 40.0]),
            MouseEventKind::ScrollLeft => Some([-40.0, 0.0]),
            MouseEventKind::ScrollRight => Some([40.0, 0.0]),
            _ => None,
        };
        if let Some(delta) = delta {
            self.cancel();
            return p::dispatch_wheel(&path, delta);
        }
        let mut event = p::PointerEvent::new(x, y, p::MouseButton::Left, 1);
        event.ctrl_key = modifiers.contains(KeyModifiers::CONTROL);
        event.alt_key = modifiers.contains(KeyModifiers::ALT);
        event.shift_key = modifiers.contains(KeyModifiers::SHIFT);
        event.meta_key = modifiers.contains(KeyModifiers::SUPER);
        match kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(id) = path
                    .iter()
                    .rev()
                    .filter_map(|entry| entry.focus_id.as_ref())
                    .find(|id| p::find_by_focus_id(root, id).is_some_and(|el| el.focusable))
                {
                    self.focus = Some(id.clone());
                }
                self.down = path.clone();
                self.down_origin = Some([x, y]);
                event.buttons = 1;
                event.event_type = "mousedown";
                p::dispatch_pointer(&path, &mut event, PointerKind::Down);
                if event.default_prevented() || event.is_consumed() {
                    self.cancel();
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                event.event_type = "mouseup";
                p::dispatch_pointer(&path, &mut event, PointerKind::Up);
                let down = std::mem::take(&mut self.down);
                self.down_origin = None;
                let click = p::click_dispatch_path(&down, &path);
                if !event.default_prevented() && !event.is_consumed() {
                    // Label `for` lookups must stay inside the modal too;
                    // trimming the hit path alone does not constrain them.
                    if let Some(callback) = self
                        .scope(root)
                        .and_then(|scope| p::resolve_click_from_path(scope, &click).0)
                    {
                        callback.borrow_mut()();
                    }
                }
            }
            MouseEventKind::Moved | MouseEventKind::Drag(MouseButton::Left) => {
                event.buttons = u16::from(matches!(kind, MouseEventKind::Drag(_)));
                event.event_type = "mousemove";
                p::dispatch_pointer(&path, &mut event, PointerKind::Move);
            }
            _ => return false,
        }
        !path.is_empty()
    }
}
