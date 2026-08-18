//! WP-R.5b / 5c: item-level keyed For owners + true Index accessor.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::app::reactive::{
    bind_text, for_keyed_reactive, index_reactive, keyed_list_item_owner_count,
    registered_anchor_count, reset_registered_anchors,
};
use crate::core::reactivity::{create_effect, dispose_effect, use_signal, Signal};
use crate::core::tree::Element;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Row {
    id: u32,
    label: String,
}

#[test]
fn keyed_for_unrelated_reorder_does_not_rerun_item_render() {
    reset_registered_anchors();
    let baseline_anchors = registered_anchor_count();

    let items = use_signal("__rui_wp_r5_keyed_items", || {
        vec![
            Row {
                id: 1,
                label: "a".into(),
            },
            Row {
                id: 2,
                label: "b".into(),
            },
            Row {
                id: 3,
                label: "c".into(),
            },
        ]
    });
    items.set_silent(items.peek());

    let render_counts: Rc<RefCell<HashMap<u32, u32>>> = Rc::new(RefCell::new(HashMap::new()));
    let mounted = Rc::new(RefCell::new(None::<Element>));

    let counts = Rc::clone(&render_counts);
    let mounted_a = Rc::clone(&mounted);
    let root = create_effect(move || {
        let counts = Rc::clone(&counts);
        let view = for_keyed_reactive(
            "wp_r5:keyed:list",
            move || items.get(),
            |row| row.id,
            move |row| {
                let mut map = counts.borrow_mut();
                *map.entry(row.id).or_insert(0) += 1;
                Element::new().text(row.label.clone())
            },
        );
        *mounted_a.borrow_mut() = Some(view);
    });

    assert_eq!(render_counts.borrow().get(&1), Some(&1));
    assert_eq!(render_counts.borrow().get(&2), Some(&1));
    assert_eq!(render_counts.borrow().get(&3), Some(&1));
    assert_eq!(keyed_list_item_owner_count(), 3);

    items.set(vec![
        Row {
            id: 3,
            label: "c".into(),
        },
        Row {
            id: 1,
            label: "a".into(),
        },
        Row {
            id: 2,
            label: "b".into(),
        },
    ]);

    assert_eq!(render_counts.borrow().get(&1), Some(&1));
    assert_eq!(render_counts.borrow().get(&2), Some(&1));
    assert_eq!(render_counts.borrow().get(&3), Some(&1));
    assert_eq!(keyed_list_item_owner_count(), 3);

    items.set(vec![
        Row {
            id: 1,
            label: "a".into(),
        },
        Row {
            id: 2,
            label: "b".into(),
        },
    ]);
    assert_eq!(render_counts.borrow().get(&3), Some(&1));
    assert_eq!(keyed_list_item_owner_count(), 2);

    reset_registered_anchors();
    dispose_effect(root);
    drop(mounted.borrow_mut().take());
    assert_eq!(keyed_list_item_owner_count(), 0);
    assert_eq!(registered_anchor_count(), baseline_anchors);
}

#[test]
fn keyed_for_insert_only_runs_new_item_render() {
    reset_registered_anchors();
    let items = use_signal("__rui_wp_r5_keyed_insert", || {
        vec![Row {
            id: 1,
            label: "a".into(),
        }]
    });
    items.set_silent(items.peek());

    let render_counts: Rc<RefCell<HashMap<u32, u32>>> = Rc::new(RefCell::new(HashMap::new()));
    let mounted = Rc::new(RefCell::new(None::<Element>));

    let counts = Rc::clone(&render_counts);
    let mounted_a = Rc::clone(&mounted);
    let root = create_effect(move || {
        let counts = Rc::clone(&counts);
        let view = for_keyed_reactive(
            "wp_r5:keyed:insert",
            move || items.get(),
            |row| row.id,
            move |row| {
                *counts.borrow_mut().entry(row.id).or_insert(0) += 1;
                Element::new().text(row.label.clone())
            },
        );
        *mounted_a.borrow_mut() = Some(view);
    });

    assert_eq!(render_counts.borrow().get(&1), Some(&1));
    items.set(vec![
        Row {
            id: 1,
            label: "a".into(),
        },
        Row {
            id: 2,
            label: "b".into(),
        },
    ]);
    assert_eq!(render_counts.borrow().get(&1), Some(&1));
    assert_eq!(render_counts.borrow().get(&2), Some(&1));

    reset_registered_anchors();
    dispose_effect(root);
    drop(mounted.borrow_mut().take());
}

#[test]
fn index_same_slot_value_change_only_reruns_bind() {
    reset_registered_anchors();
    let items = use_signal("__rui_wp_r5_index_items", || {
        vec!["a".to_string(), "b".to_string()]
    });
    items.set_silent(items.peek());

    let render_runs = Rc::new(Cell::new(0_u32));
    let bind_runs: Rc<RefCell<HashMap<usize, u32>>> = Rc::new(RefCell::new(HashMap::new()));
    let mounted = Rc::new(RefCell::new(None::<Element>));

    let render_a = Rc::clone(&render_runs);
    let bind_a = Rc::clone(&bind_runs);
    let mounted_a = Rc::clone(&mounted);
    let root = create_effect(move || {
        let render_a = Rc::clone(&render_a);
        let bind_a = Rc::clone(&bind_a);
        let view = index_reactive(
            "wp_r5:index:list",
            move || items.get(),
            move |index, item: Signal<String>| {
                render_a.set(render_a.get() + 1);
                bind_text(format!("wp_r5:index:row:{index}"), {
                    let bind_a = Rc::clone(&bind_a);
                    move || {
                        *bind_a.borrow_mut().entry(index).or_insert(0) += 1;
                        item.get()
                    }
                })
            },
        );
        *mounted_a.borrow_mut() = Some(view);
    });

    assert_eq!(render_runs.get(), 2);
    assert_eq!(*bind_runs.borrow().get(&0).unwrap_or(&0), 2);
    assert_eq!(*bind_runs.borrow().get(&1).unwrap_or(&0), 2);

    let before_row0 = *bind_runs.borrow().get(&0).unwrap_or(&0);
    let before_row1 = *bind_runs.borrow().get(&1).unwrap_or(&0);
    items.set(vec!["A".to_string(), "b".to_string()]);
    assert_eq!(
        render_runs.get(),
        2,
        "index render must not rerun on value change"
    );
    assert!(
        *bind_runs.borrow().get(&0).unwrap_or(&0) > before_row0,
        "row 0 bind should rerun"
    );
    assert_eq!(
        *bind_runs.borrow().get(&1).unwrap_or(&0),
        before_row1,
        "row 1 bind should stay"
    );

    reset_registered_anchors();
    dispose_effect(root);
    drop(mounted.borrow_mut().take());
}
