use super::{button, Base, Element, Signal};
use jadix::presentation::NodeContent;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, rc::Rc};

pub const MAX_NAVIGATION_DEPTH: usize = 32;
const MAX_CHECKPOINT_BYTES: usize = 32 * 1024;

/// A finite route registry and bounded back stack. Platform back gestures are host policy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "NavigationCheckpoint")]
pub struct NavigationHistory {
    schema: u32,
    routes: Vec<String>,
    entries: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NavigationCheckpoint {
    schema: u32,
    routes: Vec<String>,
    entries: Vec<String>,
}

impl TryFrom<NavigationCheckpoint> for NavigationHistory {
    type Error = String;
    fn try_from(value: NavigationCheckpoint) -> Result<Self, String> {
        let history = Self {
            schema: value.schema,
            routes: value.routes,
            entries: value.entries,
        };
        history.validate()?;
        Ok(history)
    }
}

impl NavigationHistory {
    pub fn new(
        routes: impl IntoIterator<Item = impl Into<String>>,
        initial: &str,
    ) -> Result<Self, String> {
        let value = Self {
            schema: 1,
            routes: routes.into_iter().map(Into::into).collect(),
            entries: vec![initial.into()],
        };
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<(), String> {
        let routes: BTreeSet<_> = self.routes.iter().collect();
        if self.schema != 1
            || self.routes.is_empty()
            || self.routes.len() > 128
            || routes.len() != self.routes.len()
            || self.routes.iter().any(|route| {
                route.is_empty() || route.len() > 256 || route.chars().any(char::is_control)
            })
            || self.entries.is_empty()
            || self.entries.len() > MAX_NAVIGATION_DEPTH
            || self.entries.iter().any(|route| !routes.contains(route))
            || serde_json::to_vec(self)
                .map_err(|_| "Navigation encoding failed")?
                .len()
                > MAX_CHECKPOINT_BYTES
        {
            return Err("Invalid navigation registry or history".into());
        }
        Ok(())
    }

    pub fn current(&self) -> &str {
        self.entries.last().expect("validated navigation history")
    }
    pub fn depth(&self) -> usize {
        self.entries.len()
    }
    pub fn can_back(&self) -> bool {
        self.entries.len() > 1
    }
    pub fn contains(&self, route: &str) -> bool {
        self.routes.iter().any(|entry| entry == route)
    }
    pub fn can_push(&self, route: &str) -> bool {
        if self.current() == route {
            return true;
        }
        if !self.contains(route) || self.entries.len() == MAX_NAVIGATION_DEPTH {
            return false;
        }
        let mut candidate = self.clone();
        candidate.entries.push(route.into());
        candidate.validate().is_ok()
    }

    /// Repeated pushes of the current route do not grow the stack.
    pub fn push(&mut self, route: &str) -> Result<bool, String> {
        if !self.can_push(route) {
            return Err("Unknown route or navigation stack full".into());
        }
        if self.current() == route {
            return Ok(false);
        }
        self.entries.push(route.into());
        Ok(true)
    }

    pub fn back(&mut self) -> bool {
        if !self.can_back() {
            return false;
        }
        self.entries.pop();
        true
    }

    pub fn reset(&mut self, route: &str) -> Result<(), String> {
        if !self.contains(route) {
            return Err("Unknown route".into());
        }
        self.entries = vec![route.into()];
        Ok(())
    }

    pub fn encode(&self) -> String {
        serde_json::to_string(self).expect("navigation strings serialize")
    }

    /// Restore only checkpoints from this application's exact route registry.
    pub fn restore(&mut self, source: &str) -> Result<(), String> {
        if source.len() > MAX_CHECKPOINT_BYTES {
            return Err("Navigation checkpoint exceeds 32 KiB".into());
        }
        let candidate: Self =
            serde_json::from_str(source).map_err(|_| "Malformed navigation checkpoint")?;
        candidate.validate()?;
        if candidate.routes != self.routes {
            return Err("Navigation route registry changed".into());
        }
        *self = candidate;
        Ok(())
    }
}

/// Rust page state is retained while hidden; native nodes are remounted on return.
pub fn navigation_page(
    id: &str,
    history: Signal<NavigationHistory>,
    route: &str,
    child: Element,
) -> Element {
    let route = route.to_owned();
    let mut page = Element::new().id(id).flex_col().child(child);
    page.set_attr("data-slot", "navigation-page");
    if !history.get().contains(&route) {
        page.set_attr("jadix_native_unsupported", "Unknown navigation page route");
    }
    page.visibility = Some(Rc::new(move || history.get().current() == route));
    page
}

#[derive(Default)]
pub struct NavigationPageProps {
    pub id: String,
    pub history: Option<Signal<NavigationHistory>>,
    pub route: Option<String>,
    pub children: Option<Element>,
    pub base: Base,
}

#[allow(non_snake_case)]
pub fn NavigationPage(props: NavigationPageProps) -> Element {
    let Some(history) = props.history else {
        return missing_history();
    };
    let page = navigation_page(
        &props.id,
        history,
        &props.route.unwrap_or_default(),
        props.children.unwrap_or_default(),
    );
    match props.base {
        Some(base) => base(page),
        None => page,
    }
}

fn missing_history() -> Element {
    let mut element = Element::new();
    element.set_attr("jadix_native_unsupported", "Navigation requires history");
    element
}

pub fn navigation_link(
    id: &str,
    history: Signal<NavigationHistory>,
    route: &str,
    caption: &str,
) -> Element {
    let route = route.to_owned();
    let target = route.clone();
    let mut link: Element = button(id)
        .text(caption)
        .on_click(move || {
            let mut next = history.get();
            if next.push(&target).is_ok() {
                history.set(next);
            }
        })
        .into();
    link.set_attr("data-slot", "navigation-link");
    link.descriptor_binding = Some(Rc::new(move |descriptor| {
        if let NodeContent::Button { enabled, .. } = &mut descriptor.content {
            *enabled = history.get().can_push(&route);
        }
    }));
    link
}

#[derive(Default)]
pub struct NavigationLinkProps {
    pub id: String,
    pub history: Option<Signal<NavigationHistory>>,
    pub route: Option<String>,
    pub children: Option<Element>,
    pub base: Base,
}

#[allow(non_snake_case)]
pub fn NavigationLink(props: NavigationLinkProps) -> Element {
    let Some(history) = props.history else {
        return missing_history();
    };
    let route = props.route.unwrap_or_default();
    let mut link = navigation_link(&props.id, history, &route, "");
    link.children = vec![props
        .children
        .unwrap_or_else(|| Element::new().text(&route))];
    match props.base {
        Some(base) => base(link),
        None => link,
    }
}

pub fn navigation_back(id: &str, history: Signal<NavigationHistory>, caption: &str) -> Element {
    let mut link: Element = button(id)
        .text(caption)
        .on_click(move || {
            let mut next = history.get();
            if next.back() {
                history.set(next);
            }
        })
        .into();
    link.set_attr("data-slot", "navigation-back");
    link.descriptor_binding = Some(Rc::new(move |descriptor| {
        if let NodeContent::Button { enabled, .. } = &mut descriptor.content {
            *enabled = history.get().can_back();
        }
    }));
    link
}

#[derive(Default)]
pub struct NavigationBackProps {
    pub id: String,
    pub history: Option<Signal<NavigationHistory>>,
    pub children: Option<Element>,
    pub base: Base,
}

#[allow(non_snake_case)]
pub fn NavigationBack(props: NavigationBackProps) -> Element {
    let Some(history) = props.history else {
        return missing_history();
    };
    let mut link = navigation_back(&props.id, history, "");
    link.children = vec![props
        .children
        .unwrap_or_else(|| Element::new().text("Back"))];
    match props.base {
        Some(base) => base(link),
        None => link,
    }
}
