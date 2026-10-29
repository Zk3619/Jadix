use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=JADIX_BUNDLED_FONT_DIR");
    let directory = env::var_os("JADIX_BUNDLED_FONT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("assets")
        });
    let groups: &[(&str, &[&str])] = &[
        ("CARGO_FEATURE_BUNDLED_CJK", &["NotoSansCJK.ttc"]),
        ("CARGO_FEATURE_BUNDLED_CJK_SERIF", &["NotoSerifCJK.ttc"]),
        (
            "CARGO_FEATURE_BUNDLED_EMOJI",
            &["NotoColorEmoji_WindowsCompatible.ttf", "NotoColorEmoji.ttf"],
        ),
        (
            "CARGO_FEATURE_BUNDLED_SYMBOLS",
            &[
                "NotoSansSymbols-wght.ttf",
                "NotoSansSymbols2-Regular.ttf",
                "NotoSansMath-Regular.ttf",
                "NotoMusic-Regular.ttf",
            ],
        ),
    ];
    for (feature, filenames) in groups {
        if env::var_os(feature).is_none() {
            continue;
        }
        for filename in *filenames {
            let path = directory.join(filename);
            assert!(path.is_file(), "Missing bundled font {}. Set JADIX_BUNDLED_FONT_DIR to a directory containing the documented licensed fonts before enabling bundled-* features.", path.display());
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
    println!(
        "cargo:rustc-env=JADIX_BUNDLED_FONT_DIR={}",
        directory.display()
    );
}
