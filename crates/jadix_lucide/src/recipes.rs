use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AnimatedIconRecipe {
    pub id: String,
    #[serde(default)]
    pub lucide: String,
    #[serde(default = "default_vb", alias = "viewBox")]
    pub view_box: String,
    #[serde(default = "default_sw", alias = "strokeWidth")]
    pub stroke_width: f32,
    #[serde(default = "default_cap", alias = "strokeLinecap")]
    pub stroke_linecap: String,
    #[serde(default = "default_join", alias = "strokeLinejoin")]
    pub stroke_linejoin: String,
    #[serde(default)]
    pub root: RecipeNodeGroup,
    #[serde(default)]
    pub nodes: Vec<RecipeNode>,
    #[serde(default)]
    pub animated: bool,
}

fn default_vb() -> String {
    "0 0 24 24".into()
}
fn default_sw() -> f32 {
    2.0
}
fn default_cap() -> String {
    "round".into()
}
fn default_join() -> String {
    "round".into()
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RecipeNodeGroup {
    #[serde(default)]
    pub tracks: Vec<RecipeTrack>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RecipeNode {
    #[serde(default)]
    pub id: String,
    #[serde(default = "default_path_tag")]
    pub tag: String,
    #[serde(default)]
    pub attrs: HashMap<String, String>,
    #[serde(default)]
    pub tracks: Vec<RecipeTrack>,
    /// Nested geometry under a Motion `<g>` (e.g. download arrow group).
    #[serde(default)]
    pub children: Vec<RecipeNode>,
}

fn default_path_tag() -> String {
    "path".into()
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RecipeTrack {
    pub prop: String,
    #[serde(default)]
    pub keyframes: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub transition: Option<RecipeTransition>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RecipeTransition {
    #[serde(default)]
    pub duration: Option<f32>,
    #[serde(default)]
    pub delay: Option<f32>,
    #[serde(default)]
    pub ease: Option<String>,
    /// `-1` = infinite
    #[serde(default)]
    pub repeat: Option<f32>,
    #[serde(default, rename = "type")]
    pub ty: Option<String>,
    #[serde(default)]
    pub stiffness: Option<f32>,
    #[serde(default)]
    pub damping: Option<f32>,
    /// Motion `times` — absolute keyframe offsets in [0,1], same length as value array.
    #[serde(default)]
    pub times: Option<Vec<f32>>,
    #[serde(default, rename = "perProp")]
    pub per_prop: Option<HashMap<String, PerPropTransition>>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PerPropTransition {
    #[serde(default)]
    pub duration: Option<f32>,
}

fn recipes() -> &'static HashMap<String, AnimatedIconRecipe> {
    static MAP: OnceLock<HashMap<String, AnimatedIconRecipe>> = OnceLock::new();
    MAP.get_or_init(|| {
        serde_json::from_str(include_str!("../assets/lucide_animated.json")).unwrap_or_default()
    })
}

pub fn animated_icon_recipe(name: &str) -> Option<&'static AnimatedIconRecipe> {
    recipes().get(name)
}

pub fn animated_icon_names() -> impl Iterator<Item = &'static str> {
    recipes().keys().map(|s| s.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_recipe_asset_parses_and_retains_known_icons() {
        let parsed: HashMap<String, AnimatedIconRecipe> =
            serde_json::from_str(include_str!("../assets/lucide_animated.json")).unwrap();
        assert!(parsed.len() >= 400);
        assert_eq!(animated_icon_names().count(), parsed.len());
        for name in ["heart", "check", "menu", "download"] {
            let recipe = animated_icon_recipe(name).unwrap();
            assert_eq!(recipe.id, name);
            assert!(!recipe.nodes.is_empty());
        }
    }
}
