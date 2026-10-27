use std::{cell::Cell, rc::Rc};

use jadix_core::{
    create_context, create_effect, create_root, create_store, dispose_effect, provide_context,
    store_field, use_context, with_child_owner, ReadStore,
};

#[derive(Clone, PartialEq)]
struct User {
    name: String,
    age: u32,
}

#[derive(Clone, PartialEq)]
struct Form {
    user: User,
    dirty: bool,
}

#[test]
fn checked_nested_fields_notify_readers_without_notifying_siblings() {
    let (owner, (view, name)) = create_root(|| {
        let store = create_store(Form {
            user: User {
                name: "Ada".into(),
                age: 25,
            },
            dirty: false,
        });
        let evaluations = Cell::new(0);
        let name = store_field!(
            {
                evaluations.set(evaluations.get() + 1);
                store
            },
            user.name
        );
        assert_eq!(evaluations.get(), 1);
        assert_eq!(name.path(), "user.name");
        let age = store_field!(store, user.age);
        let name_runs = Rc::new(Cell::new(0));
        create_effect({
            let name = name.clone();
            let name_runs = name_runs.clone();
            move || {
                name.get();
                name_runs.set(name_runs.get() + 1);
            }
        });
        let view = store.read_only();
        let snapshots = Rc::new(Cell::new(0));
        create_effect({
            let snapshots = snapshots.clone();
            move || {
                view.get();
                snapshots.set(snapshots.get() + 1);
            }
        });
        age.set(26);
        assert_eq!(name_runs.get(), 1);
        assert_eq!(snapshots.get(), 2);
        name.set("Grace".into());
        assert_eq!(name_runs.get(), 2);
        store.update(|form| form.user.name = "Lin".into());
        assert_eq!(name_runs.get(), 3);
        assert_eq!(view.with(|form| form.user.age), 26);
        (view, name)
    });
    assert_eq!(view.peek().user.name, "Lin");
    assert_eq!(name.get(), "Lin");
    owner.dispose();
    assert!(!view.is_alive());
    assert!(view.try_get().is_none());
    assert!(view.try_peek().is_none());
}

#[test]
fn parent_owned_context_survives_effect_disposal_and_isolates_other_roots() {
    let context = create_context::<ReadStore<u32>>();
    let (owner, store) = create_root(|| {
        let store = create_store(1_u32);
        provide_context(context, store.read_only());
        let view = with_child_owner(|| use_context(context).unwrap());
        let effect = create_effect(move || {
            assert_eq!(use_context(context).unwrap().get(), 1);
        });
        dispose_effect(effect);
        store.set(2);
        assert_eq!(view.get(), 2);
        let sibling_view = with_child_owner(|| use_context(context).unwrap());
        assert_eq!(sibling_view.peek(), 2);
        store
    });
    let (other, missing) = create_root(|| use_context(context));
    assert!(missing.is_none());
    other.dispose();
    owner.dispose();
    assert!(!store.read_only().is_alive());
}

#[test]
fn indexed_paths_keep_parent_child_batches_and_ignore_similar_prefixes() {
    let (owner, ()) = create_root(|| {
        let store = create_store(Form {
            user: User {
                name: "Ada".into(),
                age: 25,
            },
            dirty: false,
        });
        let user = store_field!(store, user);
        let name = store_field!(store, user.name);
        let age = store_field!(store, user.age);
        let length = store.field(
            "user.name.length",
            |form| form.user.name.len(),
            |form, length| form.user.name.truncate(length),
        );
        let unrelated_projections = Rc::new(Cell::new(0));
        for path in [
            "user!",
            "user.name!",
            "user.nameplate",
            "user0",
            "user_name",
        ] {
            let projections = unrelated_projections.clone();
            store.field(
                path,
                move |form| {
                    projections.set(projections.get() + 1);
                    form.dirty
                },
                |form, dirty| form.dirty = dirty,
            );
        }
        unrelated_projections.set(0);
        let snapshots = Rc::new(std::cell::RefCell::new(Vec::new()));
        create_effect({
            let user = user.clone();
            let name = name.clone();
            let length = length.clone();
            let snapshots = snapshots.clone();
            move || {
                snapshots
                    .borrow_mut()
                    .push((user.get(), name.get(), length.get()));
            }
        });
        let age_runs = Rc::new(Cell::new(0));
        create_effect({
            let age_runs = age_runs.clone();
            move || {
                age.get();
                age_runs.set(age_runs.get() + 1);
            }
        });
        name.set("Grace".into());
        assert_eq!(snapshots.borrow().len(), 2);
        assert_eq!(snapshots.borrow()[1].0.name, "Grace");
        assert_eq!(snapshots.borrow()[1].1, "Grace");
        assert_eq!(snapshots.borrow()[1].2, 5);
        assert_eq!(age_runs.get(), 1);
        assert_eq!(unrelated_projections.get(), 0);
        user.set(User {
            name: "Lin".into(),
            age: 26,
        });
        assert_eq!(snapshots.borrow().len(), 3);
        assert_eq!(snapshots.borrow()[2].1, "Lin");
        assert_eq!(snapshots.borrow()[2].2, 3);
        assert_eq!(age_runs.get(), 2);
        assert_eq!(unrelated_projections.get(), 2);
        store.update(|form| form.dirty = true);
        assert_eq!(unrelated_projections.get(), 7);
        assert_eq!(snapshots.borrow().len(), 3);
    });
    owner.dispose();
}
