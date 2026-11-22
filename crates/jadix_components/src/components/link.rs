//! RSX adapter for the shared, unstyled semantic link primitive.
use crate::core::tree::Element;
pub use crate::primitives::link::{link, LinkBuilder};
crate::__component_contract! {
LinkProps [];
/// Semantic link; navigation policy belongs to the application's activation handler.
#[allow(non_snake_case)]
pub fn Link(
identity [] id: String,
default [] text: String,
default [] disabled: bool,
plain [] href: Option<String>,
plain [] accessibility_label: Option<String>,
plain [mut] on_activate: Option<crate::core::tree::Handler>,
plain [mut] open_with: Option<crate::core::tree::Handler<String>>,
plain [] children: Option<Element>,
) -> Element {
    let mut builder = link(id).text(text).disabled(disabled);
    if let Some(href) = href { builder = builder.href(href); }
    if let Some(label) = accessibility_label { builder = builder.accessibility_label(label); }
    if let Some(mut activate) = on_activate.take() { builder = builder.on_activate(move || activate.call(())); }
    if let Some(mut open) = open_with.take() { builder = builder.open_with(move |href| open.call(href.to_owned())); }
    if let Some(children) = children { builder = builder.child(children); }
    builder.into()
}
}
