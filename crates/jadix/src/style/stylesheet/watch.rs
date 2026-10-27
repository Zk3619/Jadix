//! Debounced mtime poll for theme / stylesheet files (no `notify` in the lib).
//!
//! Commit [`super::apply_style_file_batch`] on the UI thread. The next redraw
//! refreshes subscribed window roots and islands so baked token colors re-resolve.

use std::cell::Cell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

const POLL_MS: u64 = 400;

static WATCH_PATHS: OnceLock<Mutex<Vec<PathBuf>>> = OnceLock::new();
thread_local! {
    static NEED_REBUILD: Cell<bool> = const { Cell::new(false) };
}
static LOOP_STARTED: AtomicBool = AtomicBool::new(false);

fn watch_paths() -> &'static Mutex<Vec<PathBuf>> {
    WATCH_PATHS.get_or_init(|| Mutex::new(Vec::new()))
}

/// True when this path is a theme-only file (`theme.toml`), not a rule sheet.
pub fn is_theme_override_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.eq_ignore_ascii_case("theme.toml"))
        .unwrap_or(false)
}

/// Remember paths and start a background poll loop (idempotent).
pub fn watch_stylesheets<I, P>(paths: I)
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    {
        let mut guard = watch_paths().lock().unwrap_or_else(|e| e.into_inner());
        for p in paths {
            let p = p.as_ref().to_path_buf();
            if !guard.iter().any(|x| x == &p) {
                guard.push(p);
            }
        }
    }
    if LOOP_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("jadix-style-watch".into())
        .spawn(style_watch_loop);
}

fn style_watch_loop() {
    let mut stamps: HashMap<PathBuf, Option<SystemTime>> = HashMap::new();
    let mut seeded = false;
    loop {
        std::thread::sleep(Duration::from_millis(POLL_MS));
        let paths = watch_paths()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if paths.is_empty() {
            continue;
        }
        let mut changed = Vec::new();
        for p in &paths {
            let m = std::fs::metadata(p).and_then(|m| m.modified()).ok();
            let prev = stamps.get(p);
            if prev != Some(&m) {
                if seeded && prev.is_some() {
                    changed.push(p.clone());
                }
                stamps.insert(p.clone(), m);
            }
        }
        if !seeded {
            seeded = true;
            continue;
        }
        if changed.is_empty() {
            continue;
        }
        let reload = paths.clone();
        crate::app::ui_bridge::run_on_ui_thread(move || {
            if let Err(err) = super::apply_style_file_batch(&reload) {
                log::warn!("stylesheet watch reload failed: {err}");
                return;
            }
            mark_style_hot_rebuild();
        });
    }
}

pub(crate) fn mark_style_hot_rebuild() {
    NEED_REBUILD.with(|pending| pending.set(true));
    crate::app::runtime::request_redraw_all();
}

/// Consume the “need full rebuild after style/theme file change” flag.
pub(crate) fn take_style_hot_rebuild() -> bool {
    NEED_REBUILD.with(|pending| pending.replace(false))
}

fn style_generation() -> crate::core::reactivity::Signal<u64> {
    crate::core::reactivity::use_signal("__jadix_style_generation", || 0)
}

/// Subscribe outside isolated component bodies so all window roots and islands refresh.
pub(crate) fn track_style_changes() {
    let _ = style_generation().get();
}

/// Flush only while window trees are free to borrow. Multiple mutations coalesce per frame.
pub(crate) fn flush_style_rebuild() {
    if take_style_hot_rebuild() {
        crate::app::reactive::reset_static_templates();
        let generation = style_generation();
        generation.set(generation.peek().wrapping_add(1));
        crate::app::runtime::sync_window_theme();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_names() {
        assert!(is_theme_override_file(Path::new("theme.toml")));
        assert!(!is_theme_override_file(Path::new("/app/globals.css")));
        assert!(!is_theme_override_file(Path::new("styles.toml")));
    }

    #[test]
    fn theme_changes_queue_rebuild() {
        let prior = crate::style::get_theme();
        take_style_hot_rebuild();
        crate::style::set_theme_mode(crate::style::ThemeMode::Dark);
        assert!(
            take_style_hot_rebuild(),
            "existing trees need rebuilding, not only repainting"
        );
        crate::style::set_theme(prior);
        take_style_hot_rebuild();
    }

    #[test]
    fn sheet_changes_queue_rebuild() {
        super::super::clear_stylesheets();
        take_style_hot_rebuild();
        let sheet = super::super::parse_stylesheet(
            "[[rule]]\nselectors = [\"[data-slot=test]\"]\nclasses = \"bg-primary\"",
        )
        .unwrap();
        super::super::register_stylesheet(sheet).unwrap();
        assert!(
            take_style_hot_rebuild(),
            "registration must update mounted slots"
        );
        super::super::clear_stylesheets();
        assert!(
            take_style_hot_rebuild(),
            "clearing must remove mounted rules"
        );
    }

    #[test]
    fn style_flush_updates_roots_in_every_window_scope() {
        use crate::core::reactivity::{
            bind_current_owner_window_key_scope, create_effect, create_root,
        };
        use crate::core::tree::Element;
        use std::cell::RefCell;
        use std::rc::Rc;

        let prior = crate::style::get_theme();
        super::super::clear_stylesheets();
        crate::style::set_theme_mode(crate::style::ThemeMode::Light);
        flush_style_rebuild();
        let mut owners = Vec::new();
        let mut roots = Vec::new();
        for scope in [0, 42] {
            let tree = Rc::new(RefCell::new(Element::new()));
            let live = tree.clone();
            let (owner, _) = create_root(move || {
                bind_current_owner_window_key_scope(scope);
                create_effect(move || {
                    track_style_changes();
                    let mut slot = Element::new().bg("background").text("text");
                    slot.text_style.color = Some(crate::style::parse_text_color("foreground"));
                    slot.set_attr("data-slot", "test");
                    *live.borrow_mut() = super::super::apply_stylesheet_rules_tree(slot);
                });
            });
            owners.push(owner);
            roots.push(tree);
        }
        crate::style::set_theme_mode(crate::style::ThemeMode::Dark);
        flush_style_rebuild();
        for tree in &roots {
            assert_eq!(
                tree.borrow().style.bg_color,
                crate::style::get_theme().colors.background
            );
            assert_eq!(
                tree.borrow().text_style.color,
                Some(crate::style::parse_text_color("foreground"))
            );
        }
        super::super::register_stylesheet(
            super::super::parse_stylesheet(
                "[[rule]]\nselectors = [\"[data-slot=test]\"]\nclasses = \"bg-primary\"",
            )
            .unwrap(),
        )
        .unwrap();
        flush_style_rebuild();
        for tree in &roots {
            assert_eq!(
                tree.borrow().style.bg_color,
                crate::style::get_theme().colors.primary
            );
        }
        super::super::clear_stylesheets();
        flush_style_rebuild();
        for tree in &roots {
            assert_eq!(
                tree.borrow().style.bg_color,
                crate::style::get_theme().colors.background
            );
        }
        drop(owners);
        crate::style::set_theme(prior);
        take_style_hot_rebuild();
    }

    #[test]
    fn failed_theme_validation_does_not_notify_live_trees() {
        let prior = crate::style::get_theme();
        take_style_hot_rebuild();
        let patch = crate::style::parse_theme_overrides_str(
            "[light]\nprimary = \"#123456\"\nsecondary = \"unknown-color\"",
        )
        .unwrap();
        assert!(crate::style::install_theme_overrides(patch).is_err());
        assert_eq!(
            crate::style::get_theme().colors.primary,
            prior.colors.primary
        );
        assert!(!take_style_hot_rebuild());
    }

    #[test]
    fn identical_theme_does_not_rebuild() {
        take_style_hot_rebuild();
        crate::style::set_theme(crate::style::get_theme());
        assert!(!take_style_hot_rebuild());
    }

    #[test]
    #[cfg(feature = "product-tests")]
    fn sidebar_selection_survives_theme_refresh_and_parent_rebuild() {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::components::SidebarMenuButtonProps;
        use crate::core::reactivity::{create_effect, create_root, Signal};
        use crate::core::tree::{find_by_template_id, Element, LayoutTree};
        use crate::prelude::*;
        use std::{cell::RefCell, rc::Rc};

        let prior = crate::style::get_theme();
        reset_registered_anchors();
        crate::style::set_theme_mode(ThemeMode::Light);
        flush_style_rebuild();
        let live = Rc::new(RefCell::new(Element::new()));
        crate::app::runtime::install_test_reconcile_root(live.clone());
        let mounted = live.clone();
        let (owner, (section, rebuild)) = create_root(move || {
            let section = Signal::new("basics");
            let rebuild = Signal::new(0);
            create_effect(move || {
                track_style_changes();
                let _ = rebuild.get();
                let mut next = ui! {
                    <VStack w="200px" h="200px">
                        <SidebarMenuButton id="selection-basics" isActive={section.get() == "basics"}
                            on_click={move || section.set("basics")}>"Basics"</SidebarMenuButton>
                        <SidebarMenuButton id="selection-events" isActive={section.get() == "events"}
                            on_click={move || section.set("events")}>"Events"</SidebarMenuButton>
                    </VStack>
                };
                next = crate::style::apply_stylesheet_rules_tree(next);
                let mut old = mounted.borrow_mut();
                crate::app::test_support::reconcile_elements(&mut next, &mut old);
                *old = next;
            });
            (section, rebuild)
        });
        let mut layout = LayoutTree::new();
        for cycle in 0..4 {
            if cycle > 0 {
                crate::style::toggle_theme_mode();
                flush_style_rebuild();
                rebuild.set(cycle);
            }
            for selected in ["events", "basics"] {
                let id = format!("selection-{selected}");
                // Release the tree borrow before the click flushes its binding effects.
                let click = {
                    let tree = live.borrow();
                    find_by_template_id(&tree, &id)
                        .unwrap()
                        .on_click
                        .clone()
                        .unwrap()
                };
                (click.borrow_mut())();
                drain_deferred_view_remounts();
                assert_eq!(section.peek(), selected);
                let mut tree = live.borrow_mut();
                let _ = layout.build_and_layout(&mut tree, 200.0, 200.0, [-1.0; 2]);
                for item in ["basics", "events"] {
                    let node = find_by_template_id(&tree, &format!("selection-{item}")).unwrap();
                    let active = item == selected;
                    assert_eq!(
                        node.attrs.get("data-active").map(String::as_str),
                        Some(if active { "true" } else { "false" }),
                        "cycle={cycle}, item={item}"
                    );
                    assert_eq!(
                        node.style.bg_color,
                        if active {
                            crate::style::parse_color("sidebar-accent")
                        } else {
                            [0.0; 4]
                        },
                        "cycle={cycle}, item={item}: selection must repaint"
                    );
                }
            }
        }
        drop(owner);
        crate::app::runtime::clear_test_reconcile_root();
        reset_registered_anchors();
        crate::style::set_theme(prior);
        take_style_hot_rebuild();
    }

    #[test]
    fn theme_refresh_rebuilds_islands_static_and_list_items() {
        use crate::app::reactive::{
            for_keyed_reactive, index_reactive, reactive_view, show_reactive,
        };
        use crate::core::reactivity::{create_effect, create_root};
        use crate::core::tree::Element;
        use std::cell::RefCell;
        use std::rc::Rc;

        let prior = crate::style::get_theme();
        crate::app::reactive::reset_registered_anchors();
        crate::style::set_theme_mode(crate::style::ThemeMode::Light);
        flush_style_rebuild();
        let tree = Rc::new(RefCell::new(Element::new()));
        crate::app::runtime::install_test_reconcile_root(tree.clone());
        let live = tree.clone();
        let (owner, _) = create_root(move || {
            create_effect(move || {
                track_style_changes();
                let mut next = Element::new().children(vec![
                    reactive_view("theme:island", || Element::new().bg("card").text("island")),
                    show_reactive(
                        "theme:static",
                        || true,
                        || Element::new().bg("muted").text("static"),
                    ),
                    for_keyed_reactive(
                        "theme:keyed",
                        || vec![1],
                        |i| *i,
                        |_| Element::new().bg("primary").text("keyed"),
                    ),
                    index_reactive(
                        "theme:index",
                        || vec![1],
                        |_, _| Element::new().bg("accent").text("index"),
                    ),
                ]);
                let mut old = live.borrow_mut();
                crate::app::test_support::reconcile_elements(&mut next, &mut old);
                *old = next;
            });
        });
        for mode in [
            crate::style::ThemeMode::Dark,
            crate::style::ThemeMode::Light,
        ] {
            crate::style::set_theme_mode(mode);
            flush_style_rebuild();
            crate::app::reactive::drain_deferred_view_remounts();
            for (text, token) in [
                ("island", "card"),
                ("static", "muted"),
                ("keyed", "primary"),
                ("index", "accent"),
            ] {
                let tree = tree.borrow();
                let node =
                    crate::core::tree::find_in_tree(&tree, |el| el.text.as_deref() == Some(text))
                        .expect("all reactive children stay mounted after theme refresh");
                assert_eq!(
                    node.style.bg_color,
                    crate::style::parse_color(token),
                    "{text}"
                );
            }
        }
        drop(owner);
        crate::app::runtime::clear_test_reconcile_root();
        crate::app::reactive::reset_registered_anchors();
        crate::style::set_theme(prior);
        take_style_hot_rebuild();
    }
}
