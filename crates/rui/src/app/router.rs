//! # Desktop Router
//!
//! Named routing table + history backward/forward, current route hangs [`Signal`].
//!
//! ## vs Navigator
//!
//! | | **Router**（本模块） | **Navigator** |
//! |---|---|---|
//! | Model | `name → render` routing table | typed page stack + transition |
//! | History | back / forward double-ended stack | push / pop (can keep-alive) |
//! | 呈现 | 替换当前视图 | Host / Sheet + transition |
//! | Applicable | Desktop multi-page / URL-like navigation | Wizard, modal page stack, mobile transition |
//!
//! The two can coexist: Router manages "which page to go to", and Navigator manages "in-page stack and animation".

use std::collections::HashMap;
use std::rc::Rc;

use crate::app::reactive::reactive_view;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;

type RouteRender = Rc<dyn Fn() -> Element>;

/// Desktop Router: Route Table + History.
#[derive(Clone)]
pub struct Router {
    id: String,
    routes: HashMap<String, RouteRender>,
    current: Signal<String>,
    back_stack: Signal<Vec<String>>,
    forward_stack: Signal<Vec<String>>,
}

impl Router {
    /// 创建 Router；`initial` 为默认路由名（不必预先 register）。
    pub fn new(id: impl Into<String>, initial: impl Into<String>) -> Self {
        let id = id.into();
        let initial = initial.into();
        let current = use_signal(format!("{id}__route").as_str(), || initial.clone());
        let back_stack = use_signal(format!("{id}__back").as_str(), Vec::<String>::new);
        let forward_stack = use_signal(format!("{id}__fwd").as_str(), Vec::<String>::new);
        Self {
            id,
            routes: HashMap::new(),
            current,
            back_stack,
            forward_stack,
        }
    }

    pub fn current_signal(&self) -> Signal<String> {
        self.current
    }

    pub fn current(&self) -> String {
        self.current.get()
    }

    pub fn register(&mut self, name: impl Into<String>, render: impl Fn() -> Element + 'static) {
        self.routes.insert(name.into(), Rc::new(render));
    }

    /// Navigate to the named route; push the back stack and clear the forward stack.
    pub fn navigate(&self, name: impl Into<String>) {
        let name = name.into();
        let cur = self.current.peek();
        if cur == name {
            return;
        }
        let mut back = self.back_stack.peek();
        back.push(cur);
        self.back_stack.set(back);
        self.forward_stack.set(Vec::new());
        self.current.set(name);
    }

    pub fn can_back(&self) -> bool {
        !self.back_stack.peek().is_empty()
    }

    pub fn can_forward(&self) -> bool {
        !self.forward_stack.peek().is_empty()
    }

    pub fn back(&self) -> bool {
        let mut back = self.back_stack.peek();
        let Some(prev) = back.pop() else {
            return false;
        };
        self.back_stack.set(back);
        let mut fwd = self.forward_stack.peek();
        fwd.push(self.current.peek());
        self.forward_stack.set(fwd);
        self.current.set(prev);
        true
    }

    pub fn forward(&self) -> bool {
        let mut fwd = self.forward_stack.peek();
        let Some(next) = fwd.pop() else {
            return false;
        };
        self.forward_stack.set(fwd);
        let mut back = self.back_stack.peek();
        back.push(self.current.peek());
        self.back_stack.set(back);
        self.current.set(next);
        true
    }

    /// Renders based on the current route; unknown routes return an empty Element.
    pub fn view(&self) -> Element {
        let routes = self.routes.clone();
        let current = self.current;
        let view_id = format!("{}__router_view", self.id);
        reactive_view(view_id, move || {
            let name = current.get();
            routes.get(&name).map(|f| f()).unwrap_or_default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::create_root;

    #[test]
    fn router_navigate_back_forward() {
        let _ = create_root(|| {
            let mut r = Router::new("test-router", "home");
            r.register("home", || Element::new().text("home"));
            r.register("settings", || Element::new().text("settings"));
            assert_eq!(r.current(), "home");
            r.navigate("settings");
            assert_eq!(r.current(), "settings");
            assert!(r.can_back());
            assert!(r.back());
            assert_eq!(r.current(), "home");
            assert!(r.can_forward());
            assert!(r.forward());
            assert_eq!(r.current(), "settings");
        });
    }

    #[test]
    fn router_two_hops_back_reaches_start() {
        let _ = create_root(|| {
            let mut r = Router::new("test-router-2hop", "home");
            r.register("home", || Element::new().text("home"));
            r.register("inbox", || Element::new().text("inbox"));
            r.register("settings", || Element::new().text("settings"));
            r.navigate("inbox");
            r.navigate("settings");
            assert_eq!(r.current(), "settings");
            assert!(r.back());
            assert_eq!(r.current(), "inbox");
            assert!(r.back());
            assert_eq!(r.current(), "home");
            assert!(!r.can_back());
            assert!(r.can_forward());
        });
    }
}
