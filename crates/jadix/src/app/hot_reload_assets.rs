use crate::style::keyframes::KeyframeStop;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;

type Definitions = Vec<(String, Vec<KeyframeStop>)>;
thread_local! {
    static FILES: RefCell<HashMap<String, Definitions>> = RefCell::new(HashMap::new());
    static BASE: RefCell<HashMap<String, Option<Vec<KeyframeStop>>>> = RefCell::new(HashMap::new());
}

pub(super) enum PreparedAsset {
    Keyframes {
        path: String,
        definitions: Definitions,
    },
    Images(Vec<(String, crate::media::DecodedImage)>),
}

pub(super) fn prepare(path: &Path) -> Result<PreparedAsset, String> {
    if path
        .file_name()
        .is_some_and(|name| name.to_string_lossy().ends_with(".keyframes.css"))
    {
        let source = std::fs::read_to_string(path).map_err(|reason| reason.to_string())?;
        let definitions = crate::style::parse_keyframe_definitions(&source)?;
        let path = path
            .canonicalize()
            .map_err(|reason| reason.to_string())?
            .to_string_lossy()
            .into_owned();
        let conflict = FILES.with(|files| {
            files.borrow().iter().any(|(owner, existing)| {
                owner != &path
                    && existing
                        .iter()
                        .any(|(name, _)| definitions.iter().any(|(candidate, _)| candidate == name))
            })
        });
        if conflict {
            return Err("keyframe name is owned by a different data file".into());
        }
        Ok(PreparedAsset::Keyframes { path, definitions })
    } else {
        Ok(PreparedAsset::Images(
            super::pending_images::prepare_local_reload(path)?,
        ))
    }
}

#[cfg(feature = "hot-reload")]
pub(super) fn image_keys(asset: &PreparedAsset) -> Vec<String> {
    match asset {
        PreparedAsset::Images(images) => images.iter().map(|(key, _)| key.clone()).collect(),
        _ => Vec::new(),
    }
}

pub(super) fn commit(asset: PreparedAsset) {
    match asset {
        PreparedAsset::Images(images) => super::pending_images::commit_local_reload(images),
        PreparedAsset::Keyframes { path, definitions } => {
            let previous = FILES.with(|files| {
                files
                    .borrow_mut()
                    .insert(path, definitions.clone())
                    .unwrap_or_default()
            });
            for (name, _) in &previous {
                if definitions.iter().any(|(candidate, _)| candidate == name) {
                    continue;
                }
                let baseline = BASE.with(|base| base.borrow_mut().remove(name).flatten());
                if let Some(stops) = baseline {
                    crate::style::keyframes::register_keyframes(name.clone(), stops);
                } else {
                    crate::style::keyframes::remove_keyframes(name);
                }
            }
            for (name, stops) in definitions {
                BASE.with(|base| {
                    base.borrow_mut()
                        .entry(name.clone())
                        .or_insert_with(|| crate::style::keyframes::lookup_keyframes(&name));
                });
                crate::style::keyframes::register_keyframes(name, stops);
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/hot_reload_assets.rs"]
mod tests;
