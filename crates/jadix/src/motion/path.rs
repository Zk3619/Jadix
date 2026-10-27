use std::rc::Rc;

use crate::core::tree::{Element, LayoutStatus};
use crate::primitives::path::{
    encode_path_d, PathCommand, ATTR_PATH_D, ATTR_PATH_KIND, ATTR_PATH_POINTS,
};
use crate::style::AnimatedNumber;

pub use jadix_animation::path::{PathMorph, PathMorphError, PathSample, PreparedPath};

fn bind_geometry(
    mut element: Element,
    progress: AnimatedNumber,
    geometry: impl Fn(f32) -> Vec<PathCommand> + 'static,
) -> Element {
    let apply = move |element: &mut Element| {
        if crate::style::prefers_reduced_motion()
            && (progress.is_active() || progress.peek() != 1.0)
        {
            progress.jump(1.0);
        }
        let data = encode_path_d(&geometry(progress.peek()));
        if element.attrs.get(ATTR_PATH_D) == Some(&data) {
            return LayoutStatus::Clean;
        }
        element.set_attr(ATTR_PATH_D, data);
        element.attrs.remove(ATTR_PATH_POINTS);
        element.attrs.remove(ATTR_PATH_KIND);
        LayoutStatus::StyleChanged
    };
    apply(&mut element);
    let previous = element.reactive_sync.take();
    element.reactive_sync(move |element, tree| {
        previous
            .as_ref()
            .map_or(LayoutStatus::Clean, |sync| sync.run(element, tree))
            .merge(apply(element))
    })
}

pub fn bind_path_reveal(
    element: Element,
    path: Rc<PreparedPath>,
    progress: AnimatedNumber,
) -> Element {
    bind_geometry(element, progress, move |progress| path.trim(0.0, progress))
}

pub fn bind_path_morph(
    element: Element,
    morph: Rc<PathMorph>,
    progress: AnimatedNumber,
) -> Element {
    bind_geometry(element, progress, move |progress| morph.sample(progress))
}

/// Compose a measured path offset onto an element's authored translation/rotation.
pub fn bind_path_follow(
    mut element: Element,
    path: Rc<PreparedPath>,
    progress: AnimatedNumber,
    rotate: bool,
) -> Element {
    let base_translate = element.style.translate;
    let base_rotate = element.style.rotate_deg;
    let apply = move |element: &mut Element| {
        if crate::style::prefers_reduced_motion()
            && (progress.is_active() || progress.peek() != 1.0)
        {
            progress.jump(1.0);
        }
        let sample = path.sample(progress.peek());
        let translate = [
            base_translate[0] + sample.position.0,
            base_translate[1] + sample.position.1,
        ];
        let rotation = base_rotate + if rotate { sample.tangent_degrees } else { 0.0 };
        if element.style.translate == translate && element.style.rotate_deg == rotation {
            return LayoutStatus::Clean;
        }
        element.style.translate = translate;
        element.style.rotate_deg = rotation;
        LayoutStatus::StyleChanged
    };
    apply(&mut element);
    let previous = element.reactive_sync.take();
    element.reactive_sync(move |element, tree| {
        previous
            .as_ref()
            .map_or(LayoutStatus::Clean, |sync| sync.run(element, tree))
            .merge(apply(element))
    })
}
