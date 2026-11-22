//! Portable announcement viewport. Showing a toast never requests keyboard focus.
use super::{button, Element};
pub use jadix::portable::toast::{Announcement, Toast, ToastHold, ToastOptions, ToastQueue};
use jadix::presentation::{accessibility::{LiveRegion, SemanticRole}, keyboard::KeyPhase};

pub struct ToastViewportBuilder { id: String, queue: ToastQueue, return_focus: Option<String> }
pub fn toast_viewport(id: impl Into<String>, queue: ToastQueue) -> ToastViewportBuilder {
    ToastViewportBuilder { id: id.into(), queue, return_focus: None }
}
impl ToastViewportBuilder {
    /// Only restore when the removed toast actually owned focus. A business
    /// callback that moves focus elsewhere keeps that focus.
    pub fn return_focus(mut self, id: impl Into<String>) -> Self { self.return_focus = Some(id.into()); self }
    pub fn build(self) -> Element {
        let keys = self.queue.clone(); let queue = self.queue; let id = self.id.clone();
        Element::new().id(self.id).flex_col().gap(8.).bind_keyed_children(
            move || keys.items().iter().map(|toast| toast.id.to_string()).collect(),
            move |key, _| {
                let toast_id: u64 = key.parse().expect("toast identity");
                let toast = queue.items().into_iter().find(|toast| toast.id == toast_id).expect("live toast");
                let mut row = Element::new().id(format!("{id}__toast_{toast_id}")).flex_row().gap(8.)
                    .child(Element::new().text(toast.message));
                row.descriptor.accessibility.role = Some(SemanticRole::Alert);
                row.descriptor.accessibility.live = match toast.announcement { Announcement::Polite => LiveRegion::Polite, Announcement::Assertive => LiveRegion::Assertive };
                row.focus_restore_target = self.return_focus.clone();
                let dismiss = queue.clone();
                row.children.push(button(&format!("{id}__dismiss_{toast_id}")).text("Dismiss")
                    .on_click(move || dismiss.dismiss(toast_id)).into());
                let hover = queue.clone(); let focus = queue.clone(); let escape = queue.clone();
                row.descriptor.keyboard.key_down = true;
                row.on_hover(move |inside| hover.hold(toast_id, ToastHold::Hover, inside))
                    .and_on_focus_change(move |inside, _| focus.hold(toast_id, ToastHold::Focus, inside))
                    .and_on_key(move |event| { if event.phase == KeyPhase::Down && event.key == "Escape" { escape.dismiss(toast_id); } })
            })
    }
}
impl From<ToastViewportBuilder> for Element { fn from(builder: ToastViewportBuilder) -> Self { builder.build() } }
