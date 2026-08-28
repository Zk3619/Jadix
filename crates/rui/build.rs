//! Compile-time Lucide asset tables (phf maps + optional animated recipe codegen).
//!
//! Embeds the **full** Lucide catalogs from `assets/lucide*.json` until icons move to a
//! dedicated crate (see `docs/assets_and_icons_architecture.md`).

#[path = "build/lucide_assets.rs"]
mod lucide_assets;

use std::env;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let assets = manifest_dir.join("assets");

    println!("cargo:rerun-if-changed=assets/lucide.json");
    println!("cargo:rerun-if-changed=assets/lucide_paths.json");
    println!("cargo:rerun-if-changed=assets/lucide_animated.json");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build/lucide_assets.rs");

    lucide_assets::write_glyph_map(
        &assets.join("lucide.json"),
        &out_dir.join("lucide_glyph_map.rs"),
    );

    let bundled_paths = env::var("CARGO_FEATURE_BUNDLED_LUCIDE").is_ok();
    if bundled_paths {
        lucide_assets::write_paths_map(
            &assets.join("lucide_paths.json"),
            &out_dir.join("lucide_paths_map.rs"),
        );
    }

    if env::var("CARGO_FEATURE_LUCIDE_ANIMATED").is_ok() {
        lucide_assets::write_animated_recipes(
            &assets.join("lucide_animated.json"),
            &out_dir.join("lucide_animated_recipes.rs"),
        );
    }
}
