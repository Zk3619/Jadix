//! # System tray (stub)
//!
//! 系统托盘图标 + 菜单项的 **API 占位**。真实平台实现依赖额外 crate（如
//! `tray-icon` / `muda`）；默认构建仅提供可编译 stub，见 [`docs/tray.md`](../../../../docs/tray.md)。

use std::rc::Rc;

/// 托盘菜单项。
#[derive(Clone)]
pub struct TrayMenuItem {
    pub id: String,
    pub label: String,
    pub on_click: Option<Rc<dyn Fn()>>,
}

impl TrayMenuItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            on_click: None,
        }
    }

    pub fn on_click(mut self, f: impl Fn() + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }
}

/// [`TrayIcon`] 构建器。
pub struct TrayIconBuilder {
    tooltip: String,
    menu: Vec<TrayMenuItem>,
}

/// 系统托盘图标 stub。
pub struct TrayIcon {
    pub tooltip: String,
    pub menu: Vec<TrayMenuItem>,
}

/// 开始构建托盘图标。
pub fn tray_icon() -> TrayIconBuilder {
    TrayIconBuilder {
        tooltip: String::new(),
        menu: Vec::new(),
    }
}

impl TrayIconBuilder {
    pub fn tooltip(mut self, s: impl Into<String>) -> Self {
        self.tooltip = s.into();
        self
    }

    pub fn menu_item(mut self, item: TrayMenuItem) -> Self {
        self.menu.push(item);
        self
    }

    pub fn menu(mut self, items: Vec<TrayMenuItem>) -> Self {
        self.menu = items;
        self
    }

    /// 构建 stub 句柄。平台实现启用后将在此注册原生托盘。
    pub fn build(self) -> TrayIcon {
        #[cfg(feature = "system-tray")]
        {
            // Feature reserved for a future platform crate integration.
            log::debug!("rui tray: system-tray feature enabled (native path not wired yet)");
        }
        #[cfg(not(feature = "system-tray"))]
        {
            log::debug!("rui tray: stub TrayIcon (enable feature system-tray for native)");
        }
        TrayIcon {
            tooltip: self.tooltip,
            menu: self.menu,
        }
    }
}

impl TrayIcon {
    /// Stub：无原生副作用。
    pub fn show(&self) {}
    /// Stub：无原生副作用。
    pub fn hide(&self) {}
    /// Stub：更新 tooltip 本地字段。
    pub fn set_tooltip(&mut self, s: impl Into<String>) {
        self.tooltip = s.into();
    }
}
