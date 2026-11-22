use super::Element;
pub use jadix::portable::error_boundary::{ErrorBoundary, throw_error};

/// Keep content mounted until it fails or the boundary is reset. The fallback
/// receives a reset handle and belongs to the enclosing error boundary.
pub fn error_boundary(
    id: impl Into<String>,
    state: ErrorBoundary,
    mut fallback: impl FnMut(String, ErrorBoundary) -> Element + 'static,
    mut children: impl FnMut() -> Element + 'static,
) -> Element {
    let key = state.clone();
    Element::new().id(id.into()).bind_children(
        move || (key.generation(), key.error()),
        move |_| {
            match state.run(&mut children) {
                Some(child) => vec![child],
                None => vec![fallback(state.error().unwrap_or_else(|| "content unavailable".into()), state.clone())],
            }
        },
    )
}
