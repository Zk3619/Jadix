//! Commands shared by menu, shortcut and programmatic invocation. The current
//! enabled state is checked at dispatch, including callbacks from stale menus.
use crate::{
    menu::{MenuItem, MenuState},
    presentation::RootLifetime,
    schema::{Schema, Value},
};
use std::{cell::{Cell, RefCell}, collections::BTreeMap, rc::Rc};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandState {
    pub enabled: bool,
    pub menu: MenuState,
}
/// A live command snapshot for palettes, toolbars and other command surfaces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandEntry {
    pub id: String,
    pub label: String,
    pub state: CommandState,
    pub accepts_null: bool,
}
impl Default for CommandState {
    fn default() -> Self {
        Self {
            enabled: true,
            menu: MenuState::Plain,
        }
    }
}
struct Command {
    label: String,
    schema: Schema,
    state: Rc<dyn Fn() -> CommandState>,
    run: Rc<dyn Fn(Value) -> Result<(), String>>,
}
#[derive(Clone)]
pub struct CommandRegistry {
    lifetime: RootLifetime,
    commands: Rc<RefCell<BTreeMap<String, Command>>>,
    revision: Rc<Cell<u64>>,
}
impl CommandRegistry {
    pub fn new(lifetime: RootLifetime) -> Self {
        Self {
            lifetime,
            commands: Default::default(),
            revision: Default::default(),
        }
    }
    pub fn register(
        &self,
        id: &str,
        label: &str,
        schema: Schema,
        state: impl Fn() -> CommandState + 'static,
        run: impl Fn(Value) -> Result<(), String> + 'static,
    ) -> Result<(), String> {
        let valid =
            |s: &str| !s.trim().is_empty() && s.len() <= 256 && !s.chars().any(char::is_control);
        let mut commands = self.commands.borrow_mut();
        if self.lifetime.is_disposed()
            || !valid(id)
            || !valid(label)
            || !schema.is_valid()
            || commands.len() >= 1024
            || commands.contains_key(id)
        {
            return Err("invalid, duplicate or disposed command".into());
        }
        commands.insert(
            id.into(),
            Command {
                label: label.into(),
                schema,
                state: Rc::new(state),
                run: Rc::new(run),
            },
        );
        self.revision.set(self.revision.get().wrapping_add(1));
        Ok(())
    }
    pub fn unregister(&self, id: &str) -> bool {
        let removed = self.commands.borrow_mut().remove(id).is_some();
        if removed { self.revision.set(self.revision.get().wrapping_add(1)); }
        removed
    }
    pub fn revision(&self) -> u64 { self.revision.get() }
    /// Case-insensitive token search over IDs and captions. State callbacks run
    /// without a registry borrow, so they may update or remove commands safely.
    pub fn search(&self, query: &str) -> Vec<CommandEntry> {
        if self.lifetime.is_disposed() || query.len() > 4096 { return Vec::new(); }
        let tokens: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
        let candidates: Vec<_> = self.commands.borrow().iter().filter_map(|(id, command)| {
            let text = format!("{id} {}", command.label).to_lowercase();
            tokens.iter().all(|token| text.contains(token)).then(|| (
                id.clone(), command.label.clone(), command.schema.accepts(&Value::Null), command.state.clone(),
            ))
        }).collect();
        candidates.into_iter().filter_map(|(id, label, accepts_null, state)| {
            let snapshot = state();
            (self.contains(&id) && self.commands.borrow().get(&id).is_some_and(|command| Rc::ptr_eq(&command.state, &state)))
                .then_some(CommandEntry { id, label, state: snapshot, accepts_null })
        }).collect()
    }
    pub fn contains(&self, id: &str) -> bool {
        !self.lifetime.is_disposed() && self.commands.borrow().contains_key(id)
    }
    pub fn invoke(&self, id: &str, args: Value) -> Result<bool, String> {
        if self.lifetime.is_disposed() {
            return Err("disposed command registry".into());
        }
        let (schema, state, run) = {
            let commands = self.commands.borrow();
            let command = commands.get(id).ok_or("unknown command")?;
            (
                command.schema.clone(),
                command.state.clone(),
                command.run.clone(),
            )
        };
        if !schema.accepts(&args) {
            return Err("invalid command arguments".into());
        }
        if !(state)().enabled {
            return Ok(false);
        }
        if self.lifetime.is_disposed() || !self.commands.borrow().get(id).is_some_and(|command| Rc::ptr_eq(&command.run, &run)) {
            return Ok(false);
        }
        // Drop registry borrows before user code (handlers may unregister).
        run(args)?;
        Ok(true)
    }
    pub fn menu(&self, id: &str) -> Result<MenuItem, String> {
        if self.lifetime.is_disposed() {
            return Err("disposed command registry".into());
        }
        let (label, state, run) = {
            let commands = self.commands.borrow();
            let c = commands.get(id).ok_or("unknown command")?;
            if !c.schema.accepts(&Value::Null) {
                return Err("menu command requires null arguments".into());
            }
            (c.label.clone(), c.state.clone(), c.run.clone())
        };
        let state = state();
        if !self.contains(id) || !self.commands.borrow().get(id).is_some_and(|command| Rc::ptr_eq(&command.run, &run)) {
            return Err("command changed while reading menu state".into());
        }
        let registry = self.clone();
        let key = id.to_string();
        let mut item = MenuItem::command(id, label, move || {
            let current = registry.commands.borrow().get(&key).is_some_and(|command| Rc::ptr_eq(&command.run, &run));
            if current { let _ = registry.invoke(&key, Value::Null); }
        })
        .enabled(state.enabled);
        if let MenuItem::Command { state: menu, .. } = &mut item {
            *menu = state.menu;
        }
        Ok(item)
    }
}
