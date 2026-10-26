//! Custom window chrome. Keep interactive controls outside `WindowDragRegion`.
//! Resize bands are enabled on the window with `resize_border`, independently
//! from the title bar. Native macOS traffic lights are not emulated here.

use super::base::SlotChrome;
use crate::app::current_window;
use crate::core::tree::{Element, IntoElementList};
use jadix_macros::component;

/// Apply drag/double-click/system-menu behavior to a non-interactive title region.
/// Captures the owning weak window handle, so later calls target the right window.
pub fn window_drag_region(child: Element) -> Element {
    let drag = current_window();
    let menu = drag.clone();
    child
        .on_mousedown(move |event| {
            if event.button_index() == 0 {
                if let Some(window) = &drag {
                    event.stop_propagation();
                    // Native move loops can swallow mouse-up. Act on the second
                    // press, rather than depending on a later synthetic dblclick.
                    if event.detail.is_multiple_of(2) {
                        let _ = window.titlebar_double_click();
                    } else {
                        let _ = window.start_drag();
                    }
                }
            }
        })
        .on_context_menu(move |event| {
            if let Some(window) = &menu {
                if window
                    .show_system_menu(event.x as f64, event.y as f64)
                    .is_ok()
                {
                    event.stop_propagation();
                }
            }
        })
}

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn WindowDragRegion(children: Option<Element>) -> Element {
    window_drag_region(children.unwrap_or_default())
}

pub struct TitleBarBuilder {
    title: String,
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn title_bar(title: impl Into<String>) -> TitleBarBuilder {
    TitleBarBuilder {
        title: title.into(),
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn TitleBar(title: String, children: Option<Element>) -> Element {
    let mut builder = title_bar(title);
    if let Some(child) = children {
        builder = builder.child(child);
    }
    builder.into()
}

impl TitleBarBuilder {
    crate::impl_slot_chrome_methods!();

    /// Actions added between the title drag region and the window controls.
    pub fn child(mut self, child: Element) -> Self {
        self.children.extend(crate::utils::take_packed(child));
        self
    }
}

fn caption_button(label: &str, glyph: &str, action: impl FnMut() + 'static) -> Element {
    Element::from(
        super::button::button("")
            .variant("ghost")
            .size("icon-xs")
            .text(glyph)
            .on_click(action),
    )
    .aria_label(label)
}

impl From<TitleBarBuilder> for Element {
    fn from(builder: TitleBarBuilder) -> Self {
        let window = current_window();
        let minimize = window.clone();
        let zoom = window.clone();
        let close = window.clone();
        let policy = window.as_ref().and_then(|w| w.chrome_state().ok());
        let mut root = Element::new()
            .class(
                "flex w-full h-10 items-center gap-1 px-2 shrink-0 bg-background text-foreground",
            )
            .child(window_drag_region(
                Element::new()
                    .class("flex flex-1 h-full items-center min-w-0 select-none")
                    .child(Element::new().text(builder.title)),
            ));
        for child in builder.children {
            root = root.child(child);
        }
        // Decorated AppKit windows own traffic lights; decorated peers have controls
        // from their OS or Winit frame (including Wayland's fallback CSD).
        if policy.is_some_and(|p| p.decorated) {
            return builder.chrome.apply(root);
        }
        let minimize_button = caption_button("Minimize window", "−", move || {
            if let Some(window) = &minimize {
                if window
                    .chrome_state()
                    .is_ok_and(|p| p.buttons.contains(winit::window::WindowButtons::MINIMIZE))
                {
                    let _ = window.set_minimized(true);
                }
            }
        });
        let mut maximize_button = caption_button("Maximize or restore window", "□", move || {
            if let Some(window) = &zoom {
                let _ = window.toggle_maximized();
            }
        });
        // An absent host is useful in offscreen/component tests, but is not interactive.
        if !policy.is_some_and(|p| p.can_maximize()) {
            crate::primitives::interactive::apply_disabled(&mut maximize_button, true);
        }
        let close_button = caption_button("Close window", "×", move || {
            if let Some(window) = &close {
                if window
                    .chrome_state()
                    .is_ok_and(|p| p.buttons.contains(winit::window::WindowButtons::CLOSE))
                {
                    let _ = window.close();
                }
            }
        });
        let mut minimize_button = minimize_button;
        let mut close_button = close_button;
        for (button, flag) in [
            (&mut minimize_button, winit::window::WindowButtons::MINIMIZE),
            (&mut close_button, winit::window::WindowButtons::CLOSE),
        ] {
            if !policy.is_some_and(|p| p.buttons.contains(flag)) {
                crate::primitives::interactive::apply_disabled(button, true);
            }
        }
        builder.chrome.apply(
            root.child(minimize_button)
                .child(maximize_button)
                .child(close_button),
        )
    }
}

impl IntoElementList for TitleBarBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::{Label, LabelProps};

    #[test]
    fn title_bar_keeps_controls_outside_drag_region_and_supports_both_surfaces() {
        crate::init();
        let builder: Element = title_bar("Editor")
            .child(Element::new().text("Action"))
            .into();
        let rsx = crate::ui! { <TitleBar title="Editor"><Label>"Action"</Label></TitleBar> };
        for bar in [builder, rsx] {
            assert_eq!(bar.children.len(), 5);
            assert!(
                bar.on_mousedown.is_none(),
                "content controls must never bubble into a drag handler"
            );
            assert!(bar.children[0].on_mousedown.is_some());
            assert!(bar.children[1].on_mousedown.is_none());
            for (index, label) in [
                (2, "Minimize window"),
                (3, "Maximize or restore window"),
                (4, "Close window"),
            ] {
                assert_eq!(
                    bar.children[index]
                        .attrs
                        .get("aria-label")
                        .map(String::as_str),
                    Some(label)
                );
            }
        }
    }
}
