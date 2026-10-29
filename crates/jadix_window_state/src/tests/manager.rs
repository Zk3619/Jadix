use super::*;
use crate::view::MonitorRect;
use std::cell::Cell;
use std::fs;

#[derive(Default)]
struct MockWindow {
    size: Cell<(u32, u32)>,
    pos: Cell<(i32, i32)>,
    maximized: Cell<bool>,
    minimized: Cell<bool>,
    visible: Cell<bool>,
    decorated: Cell<bool>,
    fullscreen: Cell<bool>,
    monitors: Vec<MonitorRect>,
}

impl MockWindow {
    fn desktop() -> Self {
        Self {
            size: Cell::new((800, 600)),
            pos: Cell::new((40, 50)),
            visible: Cell::new(true),
            decorated: Cell::new(true),
            monitors: vec![MonitorRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            }],
            ..Self::default()
        }
    }
}

impl WindowView for MockWindow {
    fn inner_size(&self) -> (u32, u32) {
        self.size.get()
    }
    fn outer_position(&self) -> Option<(i32, i32)> {
        Some(self.pos.get())
    }
    fn is_maximized(&self) -> bool {
        self.maximized.get()
    }
    fn is_minimized(&self) -> bool {
        self.minimized.get()
    }
    fn is_visible(&self) -> bool {
        self.visible.get()
    }
    fn is_decorated(&self) -> bool {
        self.decorated.get()
    }
    fn is_fullscreen(&self) -> bool {
        self.fullscreen.get()
    }
    fn available_monitors(&self) -> Vec<MonitorRect> {
        self.monitors.clone()
    }
    fn set_outer_position(&self, x: i32, y: i32) {
        self.pos.set((x, y));
    }
    fn set_inner_size(&self, width: u32, height: u32) {
        self.size.set((width, height));
    }
    fn set_maximized(&self, maximized: bool) {
        self.maximized.set(maximized);
    }
    fn set_fullscreen(&self, fullscreen: bool) {
        self.fullscreen.set(fullscreen);
    }
    fn set_decorations(&self, decorated: bool) {
        self.decorated.set(decorated);
    }
    fn set_visible(&self, visible: bool) {
        self.visible.set(visible);
    }
    fn focus(&self) {}
}

fn unique_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "jadix-ws-mgr-{}-{}.toml",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[test]
fn restore_skips_placeholder_then_captures() {
    let path = unique_path();
    let mgr = Builder::new()
        .with_path(&path)
        .with_autosave(false)
        .build()
        .unwrap();
    let w = MockWindow::desktop();
    mgr.restore("main", &w).unwrap();
    let s = mgr.get("main").unwrap();
    assert_eq!((s.width, s.height), (800, 600));
    assert_eq!((s.x, s.y), (40, 50));
    let _ = fs::remove_file(&path);
}

#[test]
fn restore_applies_saved_geometry() {
    let path = unique_path();
    let mgr = Builder::new()
        .with_path(&path)
        .with_autosave(false)
        .build()
        .unwrap();
    let src = MockWindow::desktop();
    src.size.set((1440, 900));
    src.pos.set((80, 60));
    mgr.capture("main", &src).unwrap();
    mgr.save().unwrap();

    let mgr2 = Builder::new()
        .with_path(&path)
        .with_autosave(false)
        .build()
        .unwrap();
    let dst = MockWindow::desktop();
    mgr2.restore("main", &dst).unwrap();
    assert_eq!(dst.size.get(), (1440, 900));
    assert_eq!(dst.pos.get(), (80, 60));
    let _ = fs::remove_file(&path);
}

#[test]
fn startup_restore_keeps_window_hidden_and_preserves_saved_visibility() {
    let path = unique_path();
    let mgr = Builder::new()
        .with_path(&path)
        .with_autosave(false)
        .build()
        .unwrap();
    let src = MockWindow::desktop();
    src.size.set((1440, 900));
    src.pos.set((80, 60));
    mgr.capture("main", &src).unwrap();
    let dst = MockWindow::desktop();
    dst.visible.set(false);
    mgr.restore_before_show("main", &dst).unwrap();
    assert_eq!(dst.size.get(), (1440, 900));
    assert_eq!(dst.pos.get(), (80, 60));
    assert!(
        !dst.visible.get(),
        "startup restore must leave first show to the renderer"
    );
    assert!(mgr.get("main").unwrap().visible);
    mgr.restore("main", &dst).unwrap();
    assert!(
        dst.visible.get(),
        "explicit restore still restores visibility"
    );
}

#[test]
fn maximized_restore_uses_prev_origin() {
    let path = unique_path();
    let mgr = Builder::new()
        .with_path(&path)
        .with_autosave(false)
        .build()
        .unwrap();
    mgr.note_moved("main", 120, 80);
    mgr.note_moved("main", 0, 0);
    mgr.note_resized("main", 1280, 720, false, false);
    {
        let mut g = mgr.lock();
        if let Some(s) = g.windows.get_mut("main") {
            s.maximized = true;
        }
    }
    let dst = MockWindow::desktop();
    mgr.restore("main", &dst).unwrap();
    assert!(dst.maximized.get());
    assert_eq!(dst.pos.get(), (120, 80));
    let _ = fs::remove_file(&path);
}

#[test]
fn denylist_skips_tracking() {
    let path = unique_path();
    let mgr = Builder::new()
        .with_path(&path)
        .with_denylist(&["splash"])
        .with_autosave(false)
        .build()
        .unwrap();
    let w = MockWindow::desktop();
    mgr.restore("splash", &w).unwrap();
    assert!(mgr.get("splash").is_none());
    let _ = fs::remove_file(&path);
}

#[test]
fn map_label_groups_windows() {
    let path = unique_path();
    let mgr = Builder::new()
        .with_path(&path)
        .map_label(|l| {
            if l.starts_with("editor-") {
                "editor".into()
            } else {
                l.into()
            }
        })
        .with_autosave(false)
        .build()
        .unwrap();
    let w = MockWindow::desktop();
    w.size.set((100, 100));
    mgr.capture("editor-1", &w).unwrap();
    w.size.set((200, 200));
    mgr.capture("editor-2", &w).unwrap();
    let s = mgr.get("editor-1").unwrap();
    assert_eq!(s.width, 200);
    assert!(mgr.get("editor").is_some());
    let _ = fs::remove_file(&path);
}

#[test]
fn offscreen_position_is_not_applied() {
    let path = unique_path();
    let mgr = Builder::new()
        .with_path(&path)
        .with_autosave(false)
        .build()
        .unwrap();
    let src = MockWindow::desktop();
    src.pos.set((8000, 8000));
    src.size.set((400, 300));
    mgr.capture("main", &src).unwrap();
    let dst = MockWindow::desktop();
    dst.pos.set((10, 10));
    mgr.restore("main", &dst).unwrap();
    assert_eq!(dst.pos.get(), (10, 10));
    assert_eq!(dst.size.get(), (400, 300));
    let _ = fs::remove_file(&path);
}
