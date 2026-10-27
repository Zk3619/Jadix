use jadix::{
    prelude::{create_store, store_field, ReadStore},
    reactivity::create_root,
};

#[test]
fn prelude_exposes_checked_fields_and_read_views_without_procedural_macros() {
    #[derive(Clone, PartialEq)]
    struct Settings {
        label: String,
    }

    let (_owner, ()) = create_root(|| {
        let settings = create_store(Settings {
            label: "before".into(),
        });
        let view: ReadStore<Settings> = settings.read_only();
        let label = store_field!(settings, label);
        label.set("after".into());
        assert_eq!(view.get().label, "after");
    });
}
