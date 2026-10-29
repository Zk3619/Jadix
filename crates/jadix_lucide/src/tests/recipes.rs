use super::*;

#[test]
fn complete_recipe_asset_parses_and_retains_known_icons() {
    let parsed: HashMap<String, AnimatedIconRecipe> =
        serde_json::from_str(include_str!("../../assets/lucide_animated.json")).unwrap();
    assert!(parsed.len() >= 400);
    assert_eq!(animated_icon_names().count(), parsed.len());
    for name in ["heart", "check", "menu", "download"] {
        let recipe = animated_icon_recipe(name).unwrap();
        assert_eq!(recipe.id, name);
        assert!(!recipe.nodes.is_empty());
    }
}
