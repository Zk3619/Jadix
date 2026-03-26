use crate::core::tree::Element;

pub struct InteractiveBuilder {
    pub(crate) el: Element,
}

impl InteractiveBuilder {
    pub fn new(el: Element) -> Self {
        Self { el }
    }

    pub fn on_click(mut self, handler: impl FnMut() + 'static) -> Self {
        self.el = self.el.on_click(handler);
        self
    }

    pub fn on_hover(mut self, handler: impl FnMut(bool) + 'static) -> Self {
        self.el = self.el.on_hover(handler);
        self
    }

    pub fn focus_id(mut self, id: impl Into<String>) -> Self {
        self.el = self.el.focus_id(id);
        self
    }

    pub fn focusable(mut self, focusable: bool) -> Self {
        self.el = self.el.focusable(focusable);
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.el = self.el.child(child);
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        for c in children {
            self = self.child(c);
        }
        self
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.el = self.el.text(text);
        self
    }
}

impl From<InteractiveBuilder> for Element {
    fn from(builder: InteractiveBuilder) -> Self {
        builder.el
    }
}

impl crate::core::tree::IntoElementList for InteractiveBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

pub fn interactive(el: Element) -> InteractiveBuilder {
    InteractiveBuilder::new(el)
}
