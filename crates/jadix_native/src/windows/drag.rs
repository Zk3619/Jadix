use super::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetCapture, ReleaseCapture, SetCapture};

#[derive(Clone, Copy)]
struct Gesture {
    node: NodeId,
    window: HWND,
    start: [i32; 2],
    current: [i32; 2],
    active: bool,
}

#[derive(Default)]
pub(super) struct DragTracker {
    targets: RefCell<BTreeSet<NodeId>>,
    gesture: RefCell<Option<Gesture>>,
}

impl DragTracker {
    fn push(&self, context: &WindowContext, gesture: Gesture, phase: DragPhase) {
        context.push(RawEvent::Drag {
            control: gesture.node,
            phase,
            screen_position: gesture.current.map(f64::from),
        });
    }

    pub(super) fn clear(&self) {
        self.targets.borrow_mut().clear();
    }

    pub(super) fn cancel(&self, context: &WindowContext, reset_button: bool) -> bool {
        let Some(gesture) = self.gesture.borrow_mut().take() else {
            return false;
        };
        if gesture.active {
            self.push(context, gesture, DragPhase::Cancelled);
        }
        if reset_button {
            unsafe {
                SendMessageW(gesture.window, WM_CANCELMODE, 0, 0);
            }
        }
        if unsafe { GetCapture() } == gesture.window {
            unsafe {
                ReleaseCapture();
            }
        }
        true
    }
}

unsafe fn screen_position(window: HWND, data: LPARAM) -> Option<[i32; 2]> {
    let mut point = POINT {
        x: data as u16 as i16 as i32,
        y: (data >> 16) as u16 as i16 as i32,
    };
    (unsafe { ClientToScreen(window, &mut point) } != 0).then_some([point.x, point.y])
}

unsafe extern "system" fn drag_proc(
    window: HWND,
    message: u32,
    parameter: WPARAM,
    data: LPARAM,
    _: usize,
    reference: usize,
) -> LRESULT {
    let _dpi = DpiScope::window(window);
    let context = unsafe { &*(reference as *const WindowContext) };
    if message == WM_LBUTTONDOWN
        && unsafe { IsWindowEnabled(window) } != 0
        && unsafe { IsWindowVisible(window) } != 0
    {
        if let Some(node) = context
            .target(window)
            .filter(|node| context.drag.targets.borrow().contains(node))
        {
            context.drag.cancel(context, true);
            if let Some(position) = unsafe { screen_position(window, data) } {
                *context.drag.gesture.borrow_mut() = Some(Gesture {
                    node,
                    window,
                    start: position,
                    current: position,
                    active: false,
                });
            }
        }
    } else if message == WM_MOUSEMOVE {
        let gesture = *context.drag.gesture.borrow();
        if let Some(mut gesture) = gesture.filter(|gesture| gesture.window == window) {
            if parameter & 1 == 0 {
                context.drag.cancel(context, false);
            } else if let Some(position) = unsafe { screen_position(window, data) } {
                gesture.current = position;
                let threshold = 6.0 * context.scale.get();
                let distance: f64 = position
                    .into_iter()
                    .zip(gesture.start)
                    .map(|(current, start)| (f64::from(current) - f64::from(start)).powi(2))
                    .sum();
                if !gesture.active && distance >= threshold * threshold {
                    if unsafe { GetCapture() } != window {
                        unsafe {
                            SetCapture(window);
                        }
                    }
                    if unsafe { GetCapture() } != window {
                        context.drag.cancel(context, true);
                        return 0;
                    }
                    gesture.active = true;
                    context.drag.push(
                        context,
                        Gesture {
                            current: gesture.start,
                            ..gesture
                        },
                        DragPhase::Started,
                    );
                }
                *context.drag.gesture.borrow_mut() = Some(gesture);
                if gesture.active {
                    context.drag.push(context, gesture, DragPhase::Moved);
                    return 0;
                }
            }
        }
    } else if message == WM_LBUTTONUP {
        let gesture = *context.drag.gesture.borrow();
        if let Some(mut gesture) = gesture.filter(|gesture| gesture.window == window) {
            context.drag.gesture.borrow_mut().take();
            if gesture.active {
                if let Some(position) = unsafe { screen_position(window, data) } {
                    gesture.current = position;
                }
                context.drag.push(context, gesture, DragPhase::Ended);
                unsafe {
                    DefSubclassProc(window, WM_CANCELMODE, 0, 0);
                }
                if unsafe { GetCapture() } == window {
                    unsafe {
                        ReleaseCapture();
                    }
                }
                return 0;
            }
        }
    } else if matches!(message, WM_CAPTURECHANGED | WM_CANCELMODE | WM_NCDESTROY)
        || message == WM_ENABLE && parameter == 0
    {
        let owns = context
            .drag
            .gesture
            .borrow()
            .is_some_and(|gesture| gesture.window == window);
        if owns {
            context.drag.cancel(context, false);
        }
        if message == WM_NCDESTROY {
            unsafe {
                RemoveWindowSubclass(window, Some(drag_proc), 3);
            }
        }
    }
    unsafe { DefSubclassProc(window, message, parameter, data) }
}

impl WindowsHost {
    pub fn bind_drag(&mut self, nodes: &[NodeId]) -> Result<(), String> {
        if self.is_closed() || nodes.len() > 128 {
            return Err(String::from(
                "invalid native drag target budget or closed root",
            ));
        }
        let wanted: BTreeSet<_> = nodes.iter().copied().collect();
        if wanted.len() != nodes.len()
            || wanted.iter().any(|node| {
                self.controls.get(node).is_none_or(|control| {
                    !matches!(control.descriptor.content, NodeContent::Button { .. })
                })
            })
        {
            return Err(String::from(
                "native drag targets must be unique mounted buttons in this root",
            ));
        }
        let previous = self.context.drag.targets.borrow().clone();
        let stale = self
            .context
            .drag
            .gesture
            .borrow()
            .is_some_and(|gesture| !wanted.contains(&gesture.node));
        if stale {
            self.context.drag.cancel(&self.context, true);
        }
        for node in previous.difference(&wanted) {
            if let Some(control) = self.controls.get(node) {
                unsafe {
                    RemoveWindowSubclass(control.window, Some(drag_proc), 3);
                }
            }
        }
        for node in wanted.difference(&previous) {
            if unsafe {
                SetWindowSubclass(
                    self.controls[node].window,
                    Some(drag_proc),
                    3,
                    (&*self.context as *const WindowContext) as usize,
                )
            } == 0
            {
                self.context.failed.set(true);
                return Err(native_error("subclass native drag button"));
            }
        }
        *self.context.drag.targets.borrow_mut() = wanted;
        Ok(())
    }

    pub fn cancel_pointer_drag(&self) -> bool {
        self.context.drag.cancel(&self.context, true)
    }

    pub fn control_screen_frame(&self, node: NodeId) -> Result<Rect, String> {
        let _dpi = DpiScope::window(self.window);
        let control = self.controls.get(&node).ok_or("unknown native control")?;
        let mut frame = RECT::default();
        if self.is_closed() || unsafe { GetWindowRect(control.window, &mut frame) } == 0 {
            return Err(String::from("native control has no screen bounds"));
        }
        Ok(Rect {
            x: f64::from(frame.left),
            y: f64::from(frame.top),
            width: f64::from(frame.right - frame.left),
            height: f64::from(frame.bottom - frame.top),
        })
    }

    pub fn owns_screen_point(&self, position: [f64; 2]) -> bool {
        if self.is_closed()
            || position
                .into_iter()
                .any(|value| !value.is_finite() || value.abs() > 1_000_000.0)
        {
            return false;
        }
        let _dpi = DpiScope::window(self.window);
        let target = unsafe {
            WindowFromPoint(POINT {
                x: position[0].round() as i32,
                y: position[1].round() as i32,
            })
        };
        target == self.window || unsafe { IsChild(self.window, target) } != 0
    }
}
