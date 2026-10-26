use std::cell::{Cell, RefCell};
use std::rc::Rc;
use ui_runtime::core::tree::Element;
use ui_runtime::utils::drag::runtime::{
    self, DragKey, DragRuntime, MoveMode, RuntimeState, Update,
};

pub struct BoardDrag {
    tree: Rc<RefCell<Element>>,
    active: Rc<Cell<bool>>,
}

pub fn install(tree: Rc<RefCell<Element>>) {
    runtime::register(Rc::new(BoardDrag {
        tree,
        active: Rc::new(Cell::new(true)),
    }));
}

impl DragRuntime for BoardDrag {
    fn state(&self) -> RuntimeState {
        RuntimeState {
            dragging: self.active.get(),
            active: self.active.get(),
            ..Default::default()
        }
    }

    fn prepare_move(
        &self,
        root: Option<&Element>,
        pointer: [f32; 2],
        _viewport: [f32; 2],
        _mode: MoveMode,
    ) -> Update {
        let label = root.and_then(|root| root.text.clone()).unwrap_or_default();
        let tree = self.tree.clone();
        Box::new(move || {
            tree.borrow_mut().text = Some(format!("{label} moved to {pointer:?}"));
            true
        })
    }

    fn prepare_end(&self, _root: &Element) -> Update {
        let tree = self.tree.clone();
        let active = self.active.clone();
        Box::new(move || {
            tree.borrow_mut().text = Some("board committed".into());
            active.set(false);
            runtime::unregister();
            true
        })
    }

    fn refresh_geometry(&self, _root: &Element) {}

    fn prepare_autoscroll(&self, _root: &Element, _viewport: [f32; 2]) -> Update {
        let tree = self.tree.clone();
        Box::new(move || {
            tree.borrow_mut().text = Some("board scrolled".into());
            true
        })
    }

    fn prepare_inertia(&self, _root: &Element, _viewport: [f32; 2]) -> Update {
        let tree = self.tree.clone();
        Box::new(move || {
            tree.borrow_mut().text = Some("board coasted".into());
            true
        })
    }

    fn key(&self, key: DragKey) -> bool {
        if key != DragKey::Cancel {
            return false;
        }
        self.active.set(false);
        runtime::unregister();
        true
    }
}
