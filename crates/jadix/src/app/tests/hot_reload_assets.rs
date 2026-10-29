use super::*;

#[test]
fn keyframe_file_replacement_removal_and_failure_are_atomic() {
    let path = std::env::temp_dir().join(format!(
        "jadix-hr-keyframes-{}.keyframes.css",
        std::process::id()
    ));
    let valid = "@keyframes hr-data-test { from { opacity: 0; } to { opacity: 1; } }";
    std::fs::write(&path, valid).unwrap();
    commit(prepare(&path).unwrap());
    let first = crate::style::keyframes::lookup_keyframes("hr-data-test").unwrap();
    let revision = crate::style::keyframes::keyframe_revision("hr-data-test");
    commit(prepare(&path).unwrap());
    assert_eq!(
        crate::style::keyframes::keyframe_revision("hr-data-test"),
        revision
    );
    std::fs::write(&path, format!("{valid} @keyframes broken {{ from {{")).unwrap();
    assert!(prepare(&path).is_err());
    assert_eq!(
        crate::style::keyframes::lookup_keyframes("hr-data-test"),
        Some(first)
    );
    assert!(!crate::style::keyframes::keyframes_exist("broken"));
    std::fs::write(&path, "").unwrap();
    commit(prepare(&path).unwrap());
    assert!(!crate::style::keyframes::keyframes_exist("hr-data-test"));
    std::fs::remove_file(path).unwrap();
}
