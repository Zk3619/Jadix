//! Shared menu definitions rendered and operated in terminal cells.
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
pub use jadix_core::menu::MenuItem;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Line,
};
use std::{
    cell::RefCell,
    io,
    rc::Rc,
    time::{Duration, Instant},
};

pub enum MenuInput {
    Pass,
    Handled,
    Command { id: String, run: Rc<dyn Fn()> },
}
#[derive(Clone, Copy)]
enum Mode {
    Bar,
    Context(u16, u16),
}
/// Menubar, context menus and nested popups share desktop MenuItem callbacks.
/// Call handle before routing input to the background surface, then paint last.
#[derive(Default)]
pub struct TerminalMenu {
    bar: Vec<MenuItem>,
    context: Vec<MenuItem>,
    mode: Option<Mode>,
    path: Vec<usize>,
    hits: RefCell<Vec<(Rect, Vec<usize>)>>,
    pressed: Option<Vec<usize>>,
    prefix: String,
    typed: Option<Instant>,
    dirty: bool,
    last_command: Option<String>,
    colors: crate::CellColors,
    locale: String,
}
fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "invalid terminal menu or accelerator",
    )
}
fn enabled(item: &MenuItem) -> bool {
    matches!(
        item,
        MenuItem::Command { enabled: true, .. } | MenuItem::Submenu { enabled: true, .. }
    )
}
fn label(item: &MenuItem) -> &str {
    match item {
        MenuItem::Command { label, .. } | MenuItem::Submenu { label, .. } => label,
        _ => "",
    }
}
pub(crate) fn chord(value: &str) -> io::Result<(KeyCode, KeyModifiers)> {
    let parts: Vec<_> = value.split('+').collect();
    let mut modifiers = KeyModifiers::NONE;
    for part in &parts[..parts.len().saturating_sub(1)] {
        let modifier = match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => KeyModifiers::CONTROL,
            "cmdorctrl" => {
                if cfg!(target_os = "macos") {
                    KeyModifiers::SUPER
                } else {
                    KeyModifiers::CONTROL
                }
            }
            "cmd" | "command" | "super" | "meta" => KeyModifiers::SUPER,
            "alt" | "option" => KeyModifiers::ALT,
            "shift" => KeyModifiers::SHIFT,
            _ => return Err(invalid()),
        };
        if modifiers.contains(modifier) {
            return Err(invalid());
        }
        modifiers |= modifier;
    }
    let key = parts.last().ok_or_else(invalid)?.to_ascii_lowercase();
    let code = if key.starts_with('f') && key.len() > 1 {
        let n = key[1..].parse().map_err(|_| invalid())?;
        if !(1..=24).contains(&n) {
            return Err(invalid());
        }
        KeyCode::F(n)
    } else if key.chars().count() == 1
        && modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
    {
        KeyCode::Char(key.chars().next().unwrap())
    } else {
        return Err(invalid());
    };
    Ok((code, modifiers))
}
fn validate(items: &[MenuItem]) -> io::Result<()> {
    jadix_core::menu::validate(items).map_err(io::Error::other)?;
    let mut keys = std::collections::HashSet::new();
    let mut pending: Vec<_> = items.iter().collect();
    while let Some(item) = pending.pop() {
        match item {
            MenuItem::Command {
                accelerator: Some(a),
                ..
            } if !keys.insert(chord(a)?) => return Err(invalid()),
            MenuItem::Submenu { items, .. } => pending.extend(items),
            _ => {}
        }
    }
    Ok(())
}
impl TerminalMenu {
    pub fn set_bar(&mut self, mut items: Vec<MenuItem>) -> io::Result<()> {
        validate(&items)?;
        if items.iter().any(|i| !matches!(i, MenuItem::Submenu { .. })) {
            return Err(invalid());
        }
        jadix_core::menu::localize(&mut items, &self.locale).map_err(io::Error::other)?;
        self.bar = items;
        self.close();
        self.dirty = true;
        Ok(())
    }
    pub fn set_context(&mut self, mut items: Vec<MenuItem>) -> io::Result<()> {
        validate(&items)?;
        jadix_core::menu::localize(&mut items, &self.locale).map_err(io::Error::other)?;
        self.context = items;
        self.close();
        Ok(())
    }
    /// Update built-in role captions in both menus, preserving commands and
    /// explicit labels. Close open popups so pointer geometry is recalculated.
    pub fn set_locale(&mut self, locale: impl Into<String>) {
        let locale = locale.into();
        if self.locale == locale {
            return;
        }
        // Installed menus are private and have already passed validation.
        jadix_core::menu::localize(&mut self.bar, &locale).expect("validated menubar");
        jadix_core::menu::localize(&mut self.context, &locale).expect("validated context menu");
        self.locale = locale;
        self.close();
        self.dirty = true;
    }
    pub fn content_area(&self, mut area: Rect) -> Rect {
        if !self.bar.is_empty() && area.height > 0 {
            area.y = area.y.saturating_add(1);
            area.height -= 1;
        }
        area
    }
    pub fn is_open(&self) -> bool {
        self.mode.is_some()
    }
    pub fn needs_frame(&self) -> bool {
        self.dirty
    }
    pub fn last_command(&self) -> Option<&str> {
        self.last_command.as_deref()
    }
    pub fn close(&mut self) {
        self.dirty |= self.mode.take().is_some();
        self.path.clear();
        self.hits.borrow_mut().clear();
        self.pressed = None;
        self.prefix.clear();
        self.typed = None;
    }
    fn root(&self) -> &[MenuItem] {
        if matches!(self.mode, Some(Mode::Context(..))) {
            &self.context
        } else {
            &self.bar
        }
    }
    fn list(&self, path: &[usize]) -> &[MenuItem] {
        let mut items = self.root();
        for &index in path {
            let Some(MenuItem::Submenu {
                items: children, ..
            }) = items.get(index)
            else {
                return &[];
            };
            items = children;
        }
        items
    }
    fn first(&self, parent: &[usize]) -> Option<usize> {
        self.list(parent).iter().position(enabled)
    }
    fn open_bar(&mut self, index: usize) {
        self.mode = Some(Mode::Bar);
        self.path = vec![index];
        if let Some(first) = self.first(&self.path) {
            self.path.push(first);
        }
        self.dirty = true;
        self.pressed = None;
    }
    fn root_move(&mut self, delta: isize) {
        let n = self.bar.len();
        if n == 0 {
            return;
        }
        let at = self.path.first().copied().unwrap_or(0);
        for step in 1..=n {
            let index = (at as isize + step as isize * delta).rem_euclid(n as isize) as usize;
            if enabled(&self.bar[index]) {
                self.open_bar(index);
                return;
            }
        }
    }
    fn step(&mut self, delta: isize) {
        let Some((&at, parent)) = self.path.split_last() else {
            return;
        };
        let items = self.list(parent);
        let n = items.len();
        if n == 0 {
            return;
        }
        let next = (1..=n)
            .map(|step| (at as isize + step as isize * delta).rem_euclid(n as isize) as usize)
            .find(|i| enabled(&items[*i]));
        if let Some(next) = next {
            *self.path.last_mut().unwrap() = next;
            self.dirty = true;
        }
    }
    fn choose(&mut self) -> MenuInput {
        let Some((&at, parent)) = self.path.split_last() else {
            return MenuInput::Handled;
        };
        match self.list(parent).get(at) {
            Some(MenuItem::Command {
                id,
                enabled: true,
                on_select,
                ..
            }) => {
                let (id, run) = (id.clone(), on_select.clone());
                self.close();
                self.last_command = Some(id.clone());
                MenuInput::Command { id, run }
            }
            Some(MenuItem::Submenu { enabled: true, .. }) => {
                if let Some(first) = self.first(&self.path) {
                    self.path.push(first);
                    self.dirty = true;
                }
                MenuInput::Handled
            }
            _ => MenuInput::Handled,
        }
    }
    /// `allowed` gates global menus/accelerators behind active modal scopes.
    /// `editing` preserves editor-owned shortcuts while a menu is closed.
    pub fn handle(&mut self, event: &Event, allowed: bool, editing: bool) -> MenuInput {
        if !allowed || matches!(event, Event::FocusLost | Event::Resize(..)) {
            self.close();
            return MenuInput::Pass;
        }
        if let Event::Key(key) = event {
            if key.kind != KeyEventKind::Press {
                return if self.is_open() {
                    MenuInput::Handled
                } else {
                    MenuInput::Pass
                };
            }
            if key.code == KeyCode::F(10) && key.modifiers.is_empty() && !self.bar.is_empty() {
                if self.is_open() {
                    self.close();
                } else if let Some(first) = self.bar.iter().position(enabled) {
                    self.open_bar(first);
                }
                return MenuInput::Handled;
            }
            if !self.is_open() {
                if editing {
                    return MenuInput::Pass;
                }
                let mut pending: Vec<_> = self.bar.iter().collect();
                while let Some(item) = pending.pop() {
                    match item {
                        MenuItem::Submenu {
                            enabled: true,
                            items,
                            ..
                        } => pending.extend(items),
                        MenuItem::Command {
                            id,
                            enabled: true,
                            accelerator: Some(a),
                            on_select,
                            ..
                        } => {
                            let code = match key.code {
                                KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
                                v => v,
                            };
                            if chord(a).is_ok_and(|v| v == (code, key.modifiers)) {
                                let (id, run) = (id.clone(), on_select.clone());
                                self.last_command = Some(id.clone());
                                return MenuInput::Command { id, run };
                            }
                        }
                        _ => {}
                    }
                }
                return MenuInput::Pass;
            }
            self.pressed = None;
            match key.code {
                KeyCode::Esc => {
                    if self.path.len()
                        > if matches!(self.mode, Some(Mode::Bar)) {
                            2
                        } else {
                            1
                        }
                    {
                        self.path.pop();
                        self.dirty = true;
                    } else {
                        self.close();
                    }
                }
                KeyCode::Tab => self.close(),
                KeyCode::Up => self.step(-1),
                KeyCode::Down => self.step(1),
                KeyCode::Home | KeyCode::End => {
                    let parent = &self.path[..self.path.len().saturating_sub(1)];
                    let selected = if key.code == KeyCode::Home {
                        self.list(parent).iter().position(enabled)
                    } else {
                        self.list(parent).iter().rposition(enabled)
                    };
                    if let Some(at) = selected {
                        *self.path.last_mut().unwrap() = at;
                        self.dirty = true;
                    }
                }
                KeyCode::Right => {
                    let (&index, parent) = self.path.split_last().unwrap();
                    if matches!(
                        self.list(parent).get(index),
                        Some(MenuItem::Submenu { enabled: true, .. })
                    ) {
                        return self.choose();
                    }
                    if matches!(self.mode, Some(Mode::Bar)) {
                        self.root_move(1);
                    }
                }
                KeyCode::Left => {
                    if self.path.len()
                        > if matches!(self.mode, Some(Mode::Bar)) {
                            2
                        } else {
                            1
                        }
                    {
                        self.path.pop();
                        self.dirty = true;
                    } else if matches!(self.mode, Some(Mode::Bar)) {
                        self.root_move(-1);
                    }
                }
                KeyCode::Enter | KeyCode::Char(' ') => return self.choose(),
                KeyCode::Char(c)
                    if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
                {
                    let now = Instant::now();
                    if self
                        .typed
                        .is_none_or(|t| now.duration_since(t) > Duration::from_millis(700))
                    {
                        self.prefix.clear();
                    }
                    if self.prefix.len() < 256 {
                        self.prefix.extend(c.to_lowercase());
                    }
                    self.typed = Some(now);
                    let parent = &self.path[..self.path.len() - 1];
                    let at = self.list(parent).iter().position(|i| {
                        enabled(i) && label(i).to_lowercase().starts_with(&self.prefix)
                    });
                    if let Some(at) = at {
                        *self.path.last_mut().unwrap() = at;
                        self.dirty = true;
                    }
                }
                _ => {}
            }
            return MenuInput::Handled;
        }
        if let Event::Mouse(mouse) = event {
            let released = if mouse.kind == MouseEventKind::Up(MouseButton::Left) {
                self.pressed.take()
            } else {
                None
            };
            if mouse.kind == MouseEventKind::Down(MouseButton::Right) && !self.context.is_empty() {
                self.close();
                self.mode = Some(Mode::Context(mouse.column, mouse.row));
                if let Some(at) = self.first(&[]) {
                    self.path.push(at);
                } else {
                    self.close();
                }
                self.dirty = true;
                return MenuInput::Handled;
            }
            let hit = self
                .hits
                .borrow()
                .iter()
                .rev()
                .find(|(r, _)| r.contains((mouse.column, mouse.row).into()))
                .map(|(_, p)| p.clone());
            if let Some(path) = hit {
                if matches!(self.mode, None | Some(Mode::Bar)) && path.len() == 1 {
                    if matches!(
                        mouse.kind,
                        MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Moved
                    ) && (self.is_open()
                        || mouse.kind == MouseEventKind::Down(MouseButton::Left))
                    {
                        self.open_bar(path[0]);
                        return MenuInput::Handled;
                    }
                } else if self.is_open() {
                    let (&at, parent) = path.split_last().unwrap();
                    if !self.list(parent).get(at).is_some_and(enabled) {
                        return MenuInput::Handled;
                    }
                    self.path = path.clone();
                    self.dirty = true;
                    if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                        self.pressed = Some(path.clone());
                    }
                    if mouse.kind == MouseEventKind::Up(MouseButton::Left)
                        && released.as_ref() == Some(&path)
                    {
                        return self.choose();
                    }
                    if mouse.kind == MouseEventKind::Moved {
                        let (&at, parent) = self.path.split_last().unwrap();
                        if matches!(
                            self.list(parent).get(at),
                            Some(MenuItem::Submenu { enabled: true, .. })
                        ) {
                            return self.choose();
                        }
                    }
                    return MenuInput::Handled;
                }
            } else if self.is_open() && matches!(mouse.kind, MouseEventKind::Down(_)) {
                self.close();
                return MenuInput::Handled;
            }
            if self.is_open() {
                return MenuInput::Handled;
            }
        }
        if self.is_open() {
            MenuInput::Handled
        } else {
            MenuInput::Pass
        }
    }
    /// Match the content color policy without losing selection/disabled cues.
    pub fn set_colors(&mut self, colors: crate::CellColors) {
        if self.colors != colors {
            self.colors = colors;
            self.dirty = true;
        }
    }
    pub fn render(&mut self, area: Rect, buffer: &mut Buffer) {
        let area = area.intersection(buffer.area);
        self.hits.borrow_mut().clear();
        self.dirty = false;
        if area.is_empty() {
            return;
        }
        let mono = self.colors == crate::CellColors::Monochrome;
        let style = if mono {
            Style::default().fg(Color::Reset).bg(Color::Reset)
        } else {
            Style::default().fg(Color::White).bg(Color::DarkGray)
        };
        if !self.bar.is_empty() {
            for x in area.x..area.right() {
                buffer[(x, area.y)].set_symbol(" ").set_style(style);
            }
        }
        let mut bar_positions = Vec::new();
        let mut x = area.x;
        for (index, item) in self.bar.iter().enumerate() {
            let width =
                (Line::from(label(item)).width() + 2).min(usize::from(area.right() - x)) as u16;
            let rect = Rect::new(x, area.y, width, 1);
            bar_positions.push(rect);
            let selected =
                matches!(self.mode, Some(Mode::Bar)) && self.path.first() == Some(&index);
            buffer.set_stringn(
                x,
                area.y,
                format!(" {} ", label(item)),
                width as usize,
                if selected {
                    style.add_modifier(Modifier::REVERSED)
                } else {
                    if enabled(item) {
                        style
                    } else {
                        style.add_modifier(Modifier::DIM)
                    }
                },
            );
            if enabled(item) && !matches!(self.mode, Some(Mode::Context(..))) {
                self.hits.borrow_mut().push((rect, vec![index]));
            }
            x = x.saturating_add(width);
            if x >= area.right() {
                break;
            }
        }
        if !self.is_open() || self.path.is_empty() {
            return;
        }
        let first = usize::from(matches!(self.mode, Some(Mode::Bar)));
        let (mut px, mut py) = match self.mode.unwrap() {
            Mode::Bar => (
                bar_positions.get(self.path[0]).map_or(area.x, |r| r.x),
                area.y.saturating_add(1),
            ),
            Mode::Context(x, y) => (x, y),
        };
        for depth in first..self.path.len() {
            let parent = &self.path[..depth];
            let items = self.list(parent);
            let selected = self.path[depth];
            let width = items
                .iter()
                .map(|i| {
                    Line::from(label(i)).width()
                        + match i {
                            MenuItem::Command {
                                state:
                                    jadix_core::menu::MenuState::Checked(_)
                                    | jadix_core::menu::MenuState::Radio { .. },
                                ..
                            } => 4,
                            _ => 0,
                        }
                        + match i {
                            MenuItem::Command {
                                accelerator: Some(a),
                                ..
                            } => a.len() + 3,
                            MenuItem::Submenu { .. } => 3,
                            _ => 0,
                        }
                })
                .max()
                .unwrap_or(1)
                .saturating_add(2)
                .min(area.width as usize) as u16;
            let height = (items.len() + 2).min(area.height as usize) as u16;
            let rect = Rect::new(
                px.clamp(area.x, area.right() - width),
                py.clamp(area.y, area.bottom() - height),
                width,
                height,
            );
            if width < 2 || height < 3 {
                break;
            }
            for y in rect.y..rect.bottom() {
                for x in rect.x..rect.right() {
                    buffer[(x, y)].set_symbol(" ").set_style(style);
                }
            }
            let visible = usize::from(height - 2);
            let start = selected.saturating_add(1).saturating_sub(visible);
            for (row, (index, item)) in items
                .iter()
                .enumerate()
                .skip(start)
                .take(visible)
                .enumerate()
            {
                let row_rect = Rect::new(rect.x + 1, rect.y + 1 + row as u16, width - 2, 1);
                let mut paint = style;
                if !enabled(item) {
                    paint = if mono {
                        paint.add_modifier(Modifier::DIM)
                    } else {
                        paint.fg(Color::Gray)
                    };
                }
                if selected == index {
                    paint = paint.add_modifier(Modifier::REVERSED);
                }
                let text = match item {
                    MenuItem::Separator => "─".repeat(width.saturating_sub(2) as usize),
                    MenuItem::Command {
                        label,
                        accelerator,
                        state,
                        ..
                    } => format!(
                        "{}{label}{}",
                        match state {
                            jadix_core::menu::MenuState::Plain
                            | jadix_core::menu::MenuState::Role(_)
                            | jadix_core::menu::MenuState::LocalizedRole(_) => "",
                            jadix_core::menu::MenuState::Checked(true) => "[x] ",
                            jadix_core::menu::MenuState::Checked(false) => "[ ] ",
                            jadix_core::menu::MenuState::Radio { selected: true, .. } => "(●) ",
                            jadix_core::menu::MenuState::Radio {
                                selected: false, ..
                            } => "( ) ",
                        },
                        accelerator
                            .as_ref()
                            .map_or(String::new(), |a| format!("  {a}"))
                    ),
                    MenuItem::Submenu { label, .. } => format!("{label} ›"),
                };
                buffer.set_stringn(row_rect.x, row_rect.y, text, row_rect.width as usize, paint);
                let mut path = parent.to_vec();
                path.push(index);
                self.hits.borrow_mut().push((row_rect, path));
            }
            px = rect.right().saturating_sub(1);
            py = rect.y + 1 + (selected - start).min(visible - 1) as u16;
        }
    }
}
