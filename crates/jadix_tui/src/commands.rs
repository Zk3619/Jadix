use crate::{
    crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers},
    menu::chord,
};
use jadix_core::{commands::CommandRegistry, schema::Value};
use std::{
    io,
    time::{Duration, Instant},
};
#[derive(Clone, Debug)]
pub struct CommandBinding {
    pub command: String,
    pub chord: String,
    pub scope: Option<String>,
    pub in_editors: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandInput {
    Pass,
    Pending,
    Invoked { command: String, executed: bool },
    Cancelled,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandContext {
    pub allowed: bool,
    pub in_editor: bool,
    pub scope: Option<String>,
    pub focus: Option<jadix_core::presentation::NodeId>,
}
impl Default for CommandContext {
    fn default() -> Self {
        Self {
            allowed: true,
            in_editor: false,
            scope: None,
            focus: None,
        }
    }
}
struct Binding {
    command: String,
    keys: Vec<(KeyCode, KeyModifiers)>,
    scope: Option<String>,
    in_editors: bool,
}
pub struct CommandRouter {
    registry: CommandRegistry,
    bindings: Vec<Binding>,
    prefix: Vec<(KeyCode, KeyModifiers)>,
    deadline: Option<Instant>,
    context: Option<CommandContext>,
}
impl CommandRouter {
    pub fn new(registry: CommandRegistry) -> Self {
        Self {
            registry,
            bindings: vec![],
            prefix: vec![],
            deadline: None,
            context: None,
        }
    }
    pub fn set_bindings(&mut self, bindings: Vec<CommandBinding>) -> io::Result<()> {
        let invalid = || {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid, ambiguous or unknown command chord",
            )
        };
        if bindings.len() > 128 {
            return Err(invalid());
        }
        let mut parsed: Vec<Binding> = vec![];
        for binding in bindings {
            let keys = binding
                .chord
                .split_whitespace()
                .map(chord)
                .collect::<io::Result<Vec<_>>>()?;
            if !(1..=4).contains(&keys.len())
                || binding.chord.len() > 256
                || !self.registry.contains(&binding.command)
                || binding
                    .scope
                    .as_ref()
                    .is_some_and(|s| s.is_empty() || s.len() > 256)
                || parsed.iter().any(|p| {
                    (p.scope.is_none() || binding.scope.is_none() || p.scope == binding.scope)
                        && (p.keys.starts_with(&keys) || keys.starts_with(&p.keys))
                })
            {
                return Err(invalid());
            }
            parsed.push(Binding {
                command: binding.command,
                keys,
                scope: binding.scope,
                in_editors: binding.in_editors,
            });
        }
        self.bindings = parsed;
        self.cancel();
        Ok(())
    }
    pub fn cancel(&mut self) {
        self.prefix.clear();
        self.deadline = None;
    }
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }
    pub fn handle(&mut self, event: &Event, context: CommandContext) -> io::Result<CommandInput> {
        self.handle_at(event, context, Instant::now())
    }
    /// Host clock seam permits deterministic timeout/focus/modal verification.
    pub fn handle_at(
        &mut self,
        event: &Event,
        context: CommandContext,
        now: Instant,
    ) -> io::Result<CommandInput> {
        if self.context.as_ref() != Some(&context) || self.deadline.is_some_and(|end| now >= end) {
            self.cancel();
        }
        self.context = Some(context.clone());
        if !context.allowed {
            self.cancel();
            return Ok(CommandInput::Pass);
        }
        let Event::Key(key) = event else {
            if matches!(event, Event::FocusLost | Event::Mouse(_)) {
                self.cancel();
            }
            return Ok(CommandInput::Pass);
        };
        if key.kind != KeyEventKind::Press {
            return Ok(CommandInput::Pass);
        }
        if key.code == KeyCode::Esc && !self.prefix.is_empty() {
            self.cancel();
            return Ok(CommandInput::Cancelled);
        }
        let code = match key.code {
            KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
            code => code,
        };
        self.prefix.push((code, key.modifiers));
        let eligible = |b: &Binding| {
            (!context.in_editor || b.in_editors)
                && b.scope
                    .as_ref()
                    .is_none_or(|scope| Some(scope) == context.scope.as_ref())
        };
        if !self
            .bindings
            .iter()
            .any(|b| eligible(b) && b.keys.starts_with(&self.prefix))
        {
            self.prefix.clear();
            self.prefix.push((code, key.modifiers));
        }
        if let Some(command) = self
            .bindings
            .iter()
            .find(|b| eligible(b) && b.keys == self.prefix)
            .map(|b| b.command.clone())
        {
            self.cancel();
            let executed = self
                .registry
                .invoke(&command, Value::Null)
                .map_err(io::Error::other)?;
            return Ok(CommandInput::Invoked { command, executed });
        }
        if self
            .bindings
            .iter()
            .any(|b| eligible(b) && b.keys.starts_with(&self.prefix))
        {
            self.deadline = Some(now + Duration::from_secs(1));
            Ok(CommandInput::Pending)
        } else {
            self.cancel();
            Ok(CommandInput::Pass)
        }
    }
}
