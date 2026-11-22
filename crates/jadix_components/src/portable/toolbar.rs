//! Toolbars use the shared live-tree roving navigation contract.
use super::Element;
use jadix::presentation::accessibility::SemanticRole;

pub struct ToolbarBuilder { id: String, vertical: bool }
pub fn toolbar(id: impl Into<String>) -> ToolbarBuilder { ToolbarBuilder { id: id.into(), vertical: false } }
impl ToolbarBuilder {
    pub fn vertical(mut self, vertical: bool) -> Self { self.vertical = vertical; self }
    pub fn children(self, children: Vec<Element>) -> Element {
        let mut root = Element::new().id(self.id).children(children).roving_navigation(self.vertical);
        root = if self.vertical { root.flex_col() } else { root.flex_row() };
        root.descriptor.accessibility.role = Some(SemanticRole::Toolbar);
        root
    }
}
