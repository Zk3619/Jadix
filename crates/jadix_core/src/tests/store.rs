use super::*;
use crate::reactivity::{batch, create_effect, create_memo, create_root};
use std::cell::Cell;
use std::rc::Rc;

#[test]
fn store_write_flushes_root_and_overlapping_paths_together() {
    let (_owner, ()) = create_root(|| {
        let store = create_store(((1, 2), 0));
        let point = store.field("point", |s| s.0, |s, v| s.0 = v);
        let x = store.field("point.x", |s| s.0 .0, |s, v| s.0 .0 = v);
        let y = store.field("point.y", |s| s.0 .1, |s, v| s.0 .1 = v);
        let root_x = create_memo(move || store.get().0 .0);
        let field_x = create_memo({
            let x = x.clone();
            move || x.get()
        });
        let seen = Rc::new(RefCell::new(Vec::new()));
        create_effect({
            let seen = Rc::clone(&seen);
            let point = point.clone();
            move || {
                seen.borrow_mut().push((
                    store.get().0,
                    point.get(),
                    root_x.get(),
                    field_x.get(),
                    y.get(),
                ));
            }
        });

        x.set(3);
        store.set(((4, 5), 0));
        store.update(|s| s.0 = (6, 7));
        store.produce(|s| s.0 = (8, 9));
        batch(|| {
            x.update(|v| *v = 10);
            store.update(|s| s.0 .1 = 11);
            assert_eq!(seen.borrow().len(), 5, "outer batch must defer effects");
        });
        point.set((12, 13));

        assert_eq!(
            *seen.borrow(),
            vec![
                ((1, 2), (1, 2), 1, 1, 2),
                ((3, 2), (3, 2), 3, 3, 2),
                ((4, 5), (4, 5), 4, 4, 5),
                ((6, 7), (6, 7), 6, 6, 7),
                ((8, 9), (8, 9), 8, 8, 9),
                ((10, 11), (10, 11), 10, 10, 11),
                ((12, 13), (12, 13), 12, 12, 13),
            ],
            "one write must expose one consistent root/path/memo snapshot"
        );
    });
}

#[test]
#[allow(deprecated)]
fn store_update_and_select() {
    let (_owner, ()) = create_root(|| {
        let store = create_store(vec![1, 2, 3]);
        let len = store_select(store, |v| v.len());
        assert_eq!(len.get(), 3);
        batch(|| {
            store.update(|v| v.push(4));
        });
        assert_eq!(len.get(), 4);
        assert_eq!(store.get(), vec![1, 2, 3, 4]);
        store.produce(|v| v.push(5));
        assert_eq!(store.get(), vec![1, 2, 3, 4, 5]);
    });
}

#[test]
fn produce_notifies_select_only_when_slice_changes() {
    #[derive(Clone, Debug, PartialEq)]
    struct Form {
        name: String,
        age: u32,
    }
    let (_owner, ()) = create_root(|| {
        let store = create_store(Form {
            name: "a".into(),
            age: 1,
        });
        let name = store.field("name", |f| f.name.clone(), |f, v| f.name = v);
        let age = store.field("age", |f| f.age, |f, v| f.age = v);
        assert_eq!(name.get(), "a");
        assert_eq!(age.get(), 1);

        store.produce(|f| f.age = 2);
        assert_eq!(age.get(), 2);
        assert_eq!(name.get(), "a", "name select must stay equal");

        store.produce(|f| f.name = "b".into());
        assert_eq!(name.get(), "b");
        assert_eq!(age.get(), 2);
    });
}

#[test]
fn field_sibling_set_does_not_rerun_sibling_compute() {
    #[derive(Clone, Debug, PartialEq)]
    struct Form {
        name: String,
        age: u32,
    }
    let (_owner, ()) = create_root(|| {
        let store = create_store(Form {
            name: "a".into(),
            age: 1,
        });
        let name = store.field("name", |f| f.name.clone(), |f, v| f.name = v);
        let age = store.field("age", |f| f.age, |f, v| f.age = v);

        let name_computes = Rc::new(Cell::new(0u32));
        let age_computes = Rc::new(Cell::new(0u32));
        let name_memo = create_memo({
            let name = name.clone();
            let name_computes = Rc::clone(&name_computes);
            move || {
                name_computes.set(name_computes.get() + 1);
                name.get()
            }
        });
        let age_memo = create_memo({
            let age = age.clone();
            let age_computes = Rc::clone(&age_computes);
            move || {
                age_computes.set(age_computes.get() + 1);
                age.get()
            }
        });

        assert_eq!(name_memo.get(), "a");
        assert_eq!(age_memo.get(), 1);
        let name_baseline = name_computes.get();
        let age_baseline = age_computes.get();
        assert!(name_baseline >= 1);
        assert!(age_baseline >= 1);

        age.set(2);
        assert_eq!(age_memo.get(), 2);
        assert!(
            age_computes.get() > age_baseline,
            "age compute must re-run on age.set"
        );
        assert_eq!(
            name_computes.get(),
            name_baseline,
            "changing age must not re-run name compute"
        );
        assert_eq!(name_memo.get(), "a");

        let age_after = age_computes.get();
        name.set("b".into());
        assert_eq!(name_memo.get(), "b");
        assert!(name_computes.get() > name_baseline);
        assert_eq!(
            age_computes.get(),
            age_after,
            "changing name must not re-run age compute"
        );
        assert_eq!(age_memo.get(), 2);
    });
}

#[test]
fn field_nested_parent_set_notifies_descendants_not_unrelated() {
    #[derive(Clone, Debug, PartialEq)]
    struct User {
        name: String,
        age: u32,
    }
    #[derive(Clone, Debug, PartialEq)]
    struct App {
        user: User,
        title: String,
    }
    let (_owner, ()) = create_root(|| {
        let store = create_store(App {
            user: User {
                name: "a".into(),
                age: 1,
            },
            title: "t".into(),
        });
        let user = store.field("user", |a| a.user.clone(), |a, v| a.user = v);
        let user_name = store.field("user.name", |a| a.user.name.clone(), |a, v| a.user.name = v);
        let title = store.field("title", |a| a.title.clone(), |a, v| a.title = v);

        let name_computes = Rc::new(Cell::new(0u32));
        let title_computes = Rc::new(Cell::new(0u32));
        let name_memo = create_memo({
            let user_name = user_name.clone();
            let name_computes = Rc::clone(&name_computes);
            move || {
                name_computes.set(name_computes.get() + 1);
                user_name.get()
            }
        });
        let title_memo = create_memo({
            let title = title.clone();
            let title_computes = Rc::clone(&title_computes);
            move || {
                title_computes.set(title_computes.get() + 1);
                title.get()
            }
        });
        assert_eq!(name_memo.get(), "a");
        assert_eq!(title_memo.get(), "t");
        let name_baseline = name_computes.get();
        let title_baseline = title_computes.get();

        user.set(User {
            name: "b".into(),
            age: 9,
        });
        assert_eq!(name_memo.get(), "b");
        assert!(
            name_computes.get() > name_baseline,
            "replacing user must re-run user.name"
        );
        assert_eq!(
            title_computes.get(),
            title_baseline,
            "replacing user must not re-run unrelated title"
        );
        assert_eq!(title_memo.get(), "t");
    });
}

#[test]
fn field_set_updates_root_peek_without_tracking_root_in_effect() {
    #[derive(Clone, Debug, PartialEq)]
    struct Form {
        name: String,
        age: u32,
    }
    let (_owner, ()) = create_root(|| {
        let store = create_store(Form {
            name: "a".into(),
            age: 1,
        });
        let age = store.field("age", |f| f.age, |f, v| f.age = v);
        let runs = Rc::new(Cell::new(0u32));
        create_effect({
            let age = age.clone();
            let runs = Rc::clone(&runs);
            move || {
                runs.set(runs.get() + 1);
                let _ = age.get();
            }
        });
        assert_eq!(runs.get(), 1);
        store.produce(|f| f.name = "b".into());
        assert_eq!(
            runs.get(),
            1,
            "whole-store produce must skip unchanged field observers"
        );
        age.set(3);
        assert_eq!(runs.get(), 2);
        assert_eq!(store.peek().age, 3);
        assert_eq!(store.peek().name, "b");
    });
}

#[test]
fn whole_store_writes_only_notify_changed_projections() {
    let (_owner, ()) = create_root(|| {
        let store = create_store(((1, 2), 0));
        let point = store.field("point", |s| s.0, |s, v| s.0 = v);
        let x = store.field("point.x", |s| s.0 .0, |s, v| s.0 .0 = v);
        let y = store.field("point.y", |s| s.0 .1, |s, v| s.0 .1 = v);
        let title = store.field("title", |s| s.1, |s, v| s.1 = v);
        let runs = Rc::new(Cell::new([0; 4]));
        for (index, read) in [
            Box::new(move || {
                point.get();
            }) as Box<dyn Fn()>,
            Box::new(move || {
                x.get();
            }),
            Box::new(move || {
                y.get();
            }),
            Box::new(move || {
                title.get();
            }),
        ]
        .into_iter()
        .enumerate()
        {
            let runs = Rc::clone(&runs);
            create_effect(move || {
                read();
                let mut counts = runs.get();
                counts[index] += 1;
                runs.set(counts);
            });
        }
        store.set(((3, 2), 0));
        assert_eq!(runs.get(), [2, 2, 1, 1]);
        store.update(|s| s.0 .1 = 4);
        assert_eq!(runs.get(), [3, 2, 2, 1]);
        store.produce(|s| s.1 = 5);
        assert_eq!(runs.get(), [3, 2, 2, 2]);
        store.set(store.peek());
        store.update(|_| {});
        assert_eq!(runs.get(), [3, 2, 2, 2], "equal writes do not notify");
        batch(|| {
            store.update(|s| s.0 .0 = 6);
            store.update(|s| s.0 .0 = 7);
        });
        assert_eq!(
            runs.get(),
            [4, 3, 2, 2],
            "outer batch deduplicates observers"
        );
    });
}

#[test]
fn registered_path_outlives_first_reader_and_does_not_clone_root() {
    #[derive(PartialEq)]
    struct Data {
        value: i32,
        clones: Rc<Cell<usize>>,
    }
    impl Clone for Data {
        fn clone(&self) -> Self {
            self.clones.set(self.clones.get() + 1);
            Self {
                value: self.value,
                clones: Rc::clone(&self.clones),
            }
        }
    }
    let (owner, (store, field, clones)) = create_root(|| {
        let clones = Rc::new(Cell::new(0));
        let store = create_store(Data {
            value: 1,
            clones: Rc::clone(&clones),
        });
        let (reader, field) = create_root(|| store.field("value", |d| d.value, |d, v| d.value = v));
        reader.dispose();
        (store, field, clones)
    });
    let runs = Rc::new(Cell::new(0));
    let (reader, ()) = create_root({
        let field = field.clone();
        let runs = Rc::clone(&runs);
        move || {
            create_effect(move || {
                field.get();
                runs.set(runs.get() + 1);
            });
        }
    });
    assert_eq!(field.get(), 1);
    assert_eq!(
        clones.get(),
        0,
        "path reads/comparisons must borrow the root"
    );
    store.set(Data {
        value: 2,
        clones: Rc::clone(&clones),
    });
    assert_eq!(runs.get(), 2);
    assert_eq!(clones.get(), 0, "whole-value set needs no extra root clone");
    reader.dispose();
    owner.dispose();
    assert!(!store.read_only().is_alive());
    assert!(!field.trigger.is_alive());
    STORE_PATHS.with(|map| assert!(!map.borrow().contains_key(&store.root.id())));
}
