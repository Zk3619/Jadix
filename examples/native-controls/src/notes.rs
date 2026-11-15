use jadix_components::{authoring::native::ElementProjection, prelude::*};
use jadix_core::reactivity::{create_root, OwnerHandle};
use jadix_native::{
    host::{NativeRoot, PlatformHost},
    presentation::EventPayload,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub const MAX_DOCUMENT_BYTES: usize = 512 * 1024;
pub const MAX_NOTES: usize = 128;
pub const MAX_BODY_BYTES: usize = 32 * 1024;
const MAX_TITLE_BYTES: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    pub title: String,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notebook {
    next_id: u64,
    pub notes: BTreeMap<u64, Note>,
}

impl Default for Notebook {
    fn default() -> Self {
        Self {
            next_id: 1,
            notes: BTreeMap::new(),
        }
    }
}

fn valid_note(title: &str, body: &str) -> Result<(), String> {
    if title.trim().is_empty() {
        return Err("A title is required".into());
    }
    if title.len() > MAX_TITLE_BYTES || body.len() > MAX_BODY_BYTES {
        return Err("Title or note exceeds the UTF-8 size limit".into());
    }
    Ok(())
}

impl Notebook {
    pub fn encode(&self) -> String {
        json!({"schema": 1, "next_id": self.next_id.to_string(),
            "notes": self.notes.iter().map(|(id,note)| json!({"id":id.to_string(),"title":note.title,"body":note.body})).collect::<Vec<_>>()}).to_string()
    }

    pub fn decode(source: &str) -> Result<Self, String> {
        if source.len() > MAX_DOCUMENT_BYTES {
            return Err("Notebook exceeds 512 KiB".into());
        }
        let value: Value = serde_json::from_str(source).map_err(|_| "Invalid notebook JSON")?;
        if value["schema"].as_u64() != Some(1) {
            return Err("Unsupported notebook schema".into());
        }
        let parse_id = |value: &Value| -> Result<u64, String> {
            let text = value.as_str().ok_or("Invalid note identity")?;
            let id = text.parse::<u64>().map_err(|_| "Invalid note identity")?;
            if id == 0 || id.to_string() != text {
                return Err("Invalid note identity".into());
            }
            Ok(id)
        };
        let next_id = parse_id(&value["next_id"])?;
        let entries = value["notes"].as_array().ok_or("Missing notes")?;
        if entries.len() > MAX_NOTES {
            return Err("Notebook is full".into());
        }
        let mut notes = BTreeMap::new();
        for entry in entries {
            let id = parse_id(&entry["id"])?;
            let title = entry["title"].as_str().ok_or("Invalid title")?.to_string();
            let body = entry["body"].as_str().ok_or("Invalid body")?.to_string();
            valid_note(&title, &body)?;
            if id >= next_id || notes.insert(id, Note { title, body }).is_some() {
                return Err("Duplicate or out-of-range note identity".into());
            }
        }
        let notebook = Self { next_id, notes };
        if notebook.encode().len() > MAX_DOCUMENT_BYTES {
            return Err("Notebook exceeds 512 KiB".into());
        }
        Ok(notebook)
    }

    pub fn save(
        &mut self,
        selected: Option<u64>,
        title: String,
        body: String,
    ) -> Result<u64, String> {
        valid_note(&title, &body)?;
        let mut candidate = self.clone();
        let id = if let Some(id) = selected {
            if !candidate.notes.contains_key(&id) {
                return Err("Note no longer exists".into());
            }
            id
        } else {
            if candidate.notes.len() >= MAX_NOTES {
                return Err("Notebook is full".into());
            }
            let id = candidate.next_id;
            candidate.next_id = id.checked_add(1).ok_or("Note identities exhausted")?;
            id
        };
        candidate.notes.insert(id, Note { title, body });
        if candidate.encode().len() > MAX_DOCUMENT_BYTES {
            return Err("Notebook exceeds 512 KiB".into());
        }
        *self = candidate;
        Ok(id)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Action {
    New,
    Open(u64),
    Save,
    Revert,
    Delete,
    Export,
    Import,
}

pub struct NotesApp<Host: PlatformHost> {
    pub root: NativeRoot<Host>,
    projection: ElementProjection,
    element: Element,
    notebook: Notebook,
    selected: Option<u64>,
    title: Signal<String>,
    body: Signal<String>,
    query: Signal<String>,
    exchange: Signal<String>,
    status: Signal<String>,
    action: Signal<Option<Action>>,
    replacement_confirmed: bool,
    storage_pending: bool,
    owner: Option<OwnerHandle>,
    viewport: [f32; 2],
    environment: u64,
}

fn label(value: &str) -> Element {
    text(value).h(28.0).flex_shrink(0.0)
}

fn command(id: &str, caption: &str, action: Signal<Option<Action>>, next: Action) -> Element {
    let element: Element = button(id)
        .on_click(move || action.set(Some(next)))
        .child(text(caption))
        .into();
    element.h(44.0).flex_shrink(0.0)
}

fn confirmation(
    id: &str,
    caption: &str,
    warning: &str,
    action: Signal<Option<Action>>,
    next: Action,
) -> Element {
    let caption = caption.to_string();
    let warning = warning.to_string();
    dialog(id).child(move || {
        let trigger: Element = button("confirm-trigger").child(text(&caption)).into();
        let trigger = DialogTrigger(DialogTriggerProps {
            children: Some(trigger.h(44.0).flex_shrink(0.0)),
            ..Default::default()
        });
        let warning = warning.clone();
        let mut panel = dialog_content(DialogContentOpts::default(), move || {
            let accept: Element = button("confirm-action")
                .on_click(move || {
                    action.set(Some(next));
                    dialog_close();
                })
                .child(text("Confirm"))
                .into();
            let cancel: Element = button("cancel-action")
                .on_click(dialog_close)
                .child(text("Cancel"))
                .into();
            vstack()
                .gap(12.0)
                .child(label(&warning).h(76.0))
                .child(accept.h(44.0))
                .child(cancel.h(44.0))
        });
        panel.descriptor.accessible_name = Some(caption.clone());
        vstack().flex_shrink(0.0).child(trigger).child(panel)
    })
}

impl<Host: PlatformHost> NotesApp<Host> {
    pub fn new(host: Host, viewport: [f32; 2], _: usize, _: String) -> Result<Self, String> {
        let root = NativeRoot::new(host);
        let projection = ElementProjection::with_lifetime(root.model().lifetime());
        let (owner, (element, title, body, query, exchange, status, action)) = create_root(|| {
            let title = create_signal(String::new());
            let body = create_signal(String::new());
            let query = create_signal(String::new());
            let exchange = create_signal(String::new());
            let status = create_signal("No notes yet. Create your first note.".to_string());
            let action = create_signal(None);
            let mut title_input: Element = input("note-title")
                .value_signal(title)
                .placeholder("Title / 标题")
                .into();
            title_input.descriptor.accessible_name = Some("Note title".into());
            let mut body_input: Element = textarea("note-body")
                .value_signal(body)
                .placeholder("Write a note / 正文")
                .into();
            body_input.descriptor.accessible_name = Some("Note body".into());
            let search: Element = input("note-search")
                .value_signal(query)
                .placeholder("Search notes / 搜索")
                .into();
            let transfer: Element = textarea("note-transfer")
                .value_signal(exchange)
                .placeholder("Notebook JSON for export or import")
                .into();
            let element = scroll_area("notebook-page").child(
                vstack()
                    .p(12.0)
                    .gap(8.0)
                    .flex_shrink(0.0)
                    .child(label("Jadix Notes"))
                    .child(
                        jadix_components::app::reactive::bind_text("note-status", move || {
                            status.get()
                        })
                        .h(96.0)
                        .flex_shrink(0.0),
                    )
                    .child(title_input.h(44.0).flex_shrink(0.0))
                    .child(body_input.h(156.0).flex_shrink(0.0))
                    .child(command("note-save", "Save note", action, Action::Save))
                    .child(command("note-new", "New note", action, Action::New))
                    .child(command(
                        "note-revert",
                        "Revert draft",
                        action,
                        Action::Revert,
                    ))
                    .child(confirmation(
                        "note-delete",
                        "Delete note",
                        "Delete the selected note and its draft?",
                        action,
                        Action::Delete,
                    ))
                    .child(search.h(44.0).flex_shrink(0.0))
                    .child(vstack().id("notes-list").gap(6.0).flex_shrink(0.0))
                    .child(command(
                        "note-export",
                        "Export notebook",
                        action,
                        Action::Export,
                    ))
                    .child(transfer.h(120.0).flex_shrink(0.0))
                    .child(confirmation(
                        "note-import",
                        "Import notebook",
                        "Replace all notes and discard the current draft?",
                        action,
                        Action::Import,
                    )),
            );
            (element, title, body, query, exchange, status, action)
        });
        let mut app = Self {
            root,
            projection,
            element,
            notebook: Notebook::default(),
            selected: None,
            title,
            body,
            query,
            exchange,
            status,
            action,
            replacement_confirmed: false,
            storage_pending: false,
            owner: Some(owner),
            viewport,
            environment: 1,
        };
        app.render()?;
        Ok(app)
    }

    fn dirty(&self) -> bool {
        let title = self.title.get();
        let body = self.body.get();
        match self.selected.and_then(|id| self.notebook.notes.get(&id)) {
            Some(note) => note.title != title || note.body != body,
            None => !title.is_empty() || !body.is_empty(),
        }
    }

    fn select(&mut self, id: Option<u64>) {
        self.selected = id;
        let note = id.and_then(|id| self.notebook.notes.get(&id));
        self.title
            .set(note.map_or_else(String::new, |note| note.title.clone()));
        self.body
            .set(note.map_or_else(String::new, |note| note.body.clone()));
    }

    fn apply(&mut self, action: Action) -> Result<(), String> {
        match action {
            Action::New | Action::Open(_) if self.dirty() => {
                return Err("Save or revert changes before opening another note".into());
            }
            Action::New => {
                self.select(None);
                self.status.set("New draft".into());
            }
            Action::Open(id) => {
                if !self.notebook.notes.contains_key(&id) {
                    return Err("Note no longer exists".into());
                }
                self.select(Some(id));
                self.status.set("Note opened".into());
            }
            Action::Save => {
                self.selected = Some(self.notebook.save(
                    self.selected,
                    self.title.get(),
                    self.body.get(),
                )?);
                self.status.set("Save pending device storage".into());
                self.storage_pending = true;
            }
            Action::Revert => {
                self.select(self.selected);
                self.status.set("Draft reverted".into());
            }
            Action::Delete => {
                let id = self.selected.ok_or("Select a saved note first")?;
                self.notebook.notes.remove(&id);
                self.select(None);
                self.status.set("Deletion pending device storage".into());
                self.storage_pending = true;
            }
            Action::Export => {
                self.exchange.set(self.notebook.encode());
                self.status
                    .set("Notebook exported below; copy it to keep a backup".into());
            }
            Action::Import => {
                let notebook = Notebook::decode(&self.exchange.get())?;
                self.notebook = notebook;
                self.select(None);
                self.replacement_confirmed = true;
                self.storage_pending = true;
                self.status.set("Import pending device storage".into());
            }
        }
        Ok(())
    }

    pub fn render(&mut self) -> Result<(), String> {
        let list = self.element.children[0]
            .children
            .iter_mut()
            .find(|child| child.template_id.as_deref() == Some("notes-list"))
            .ok_or("Missing notes list")?;
        let mut retained: BTreeMap<_, _> = list
            .children
            .drain(..)
            .filter_map(|child| child.template_id.clone().map(|key| (key, child)))
            .collect();
        let query = self.query.get().to_lowercase();
        for (id, note) in &self.notebook.notes {
            if !note.title.to_lowercase().contains(&query)
                && !note.body.to_lowercase().contains(&query)
            {
                continue;
            }
            let key = format!("open-note-{id}");
            let mut child = retained
                .remove(&key)
                .unwrap_or_else(|| command(&key, "", self.action, Action::Open(*id)));
            child.children = vec![text(format!("Open: {}", note.title))];
            list.children.push(child);
        }
        if list.children.is_empty() {
            let mut empty = retained
                .remove("notes-empty")
                .unwrap_or_else(|| label("").id("notes-empty"));
            empty.descriptor.content = jadix_native::presentation::NodeContent::Text(
                if self.notebook.notes.is_empty() {
                    "No saved notes"
                } else {
                    "No matching notes"
                }
                .into(),
            );
            list.children.push(empty);
        }
        let mut nodes = vec![self
            .projection
            .snapshot(&mut self.element)
            .map_err(|error| format!("{error:?}"))?];
        self.root
            .present(&mut nodes, self.viewport, self.environment)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    pub fn event(&mut self, source: &str) -> Result<(), String> {
        let event = jadix_native::wire::decode_event(source, self.root.model())
            .map_err(|error| error.to_string())?;
        self.root.event(&event).map_err(|error| error.to_string())?;
        if matches!(
            event.payload,
            EventPayload::Activate
                | EventPayload::Edit { .. }
                | EventPayload::Toggle { .. }
                | EventPayload::Dismiss { .. }
        ) {
            let previous = (
                self.title.get(),
                self.body.get(),
                self.query.get(),
                self.exchange.get(),
            );
            self.projection
                .dispatch(&mut self.element, &event)
                .map_err(|error| format!("{error:?}"))?;
            if self.title.get().len() > MAX_TITLE_BYTES
                || self.body.get().len() > MAX_BODY_BYTES
                || self.query.get().len() > 4096
                || self.exchange.get().len() > MAX_DOCUMENT_BYTES
            {
                self.title.set(previous.0);
                self.body.set(previous.1);
                self.query.set(previous.2);
                self.exchange.set(previous.3);
                self.status
                    .set("Edit rejected: UTF-8 size limit exceeded".into());
            }
            if let Some(action) = self.action.get() {
                self.action.set(None);
                if let Err(error) = self.apply(action) {
                    self.status.set(error);
                }
            }
            self.render()?;
        }
        Ok(())
    }

    pub fn resize(&mut self, viewport: [f32; 2], environment: u64) -> Result<(), String> {
        self.viewport = viewport;
        self.environment = environment;
        self.render()
    }

    pub fn state(&self) -> String {
        json!({"application":"notes","archive":serde_json::from_str::<Value>(&self.notebook.encode()).expect("encoded notebook"),
            "selected":self.selected.map(|id|id.to_string()),"draft":{"title":self.title.get(),"body":self.body.get()},
            "query":self.query.get(),"dirty":self.dirty(),"storage_pending":self.storage_pending,"status":self.status.get(),"replacement_confirmed":self.replacement_confirmed}).to_string()
    }

    #[cfg(any(target_os = "android", test))]
    pub fn storage_notification(&self) -> Option<String> {
        self.storage_pending.then(|| self.state())
    }

    pub fn restore(&mut self, source: &str) -> Result<(), String> {
        if self.root.model().lifetime().is_disposed() {
            return Err("Disposed notes root".into());
        }
        if source.len() > 2 * MAX_DOCUMENT_BYTES {
            return Err("Saved state too large".into());
        }
        let state: Value = serde_json::from_str(source).map_err(|_| "Invalid saved state")?;
        let full = state.get("archive").is_some();
        let notebook = Notebook::decode(&if full {
            state["archive"].to_string()
        } else {
            source.to_string()
        })?;
        let (selected, title, body, query) = if full {
            if state["application"].as_str() != Some("notes") {
                return Err("Invalid application state".into());
            }
            let selected = if state["selected"].is_null() {
                None
            } else {
                Some(
                    state["selected"]
                        .as_str()
                        .and_then(|value| value.parse::<u64>().ok())
                        .filter(|id| notebook.notes.contains_key(id))
                        .ok_or("Invalid selected note")?,
                )
            };
            let title = state["draft"]["title"]
                .as_str()
                .ok_or("Invalid draft")?
                .to_string();
            let body = state["draft"]["body"]
                .as_str()
                .ok_or("Invalid draft")?
                .to_string();
            let query = state["query"].as_str().ok_or("Invalid query")?.to_string();
            if title.len() > MAX_TITLE_BYTES || body.len() > MAX_BODY_BYTES || query.len() > 4096 {
                return Err("Saved draft too large".into());
            }
            (selected, title, body, query)
        } else {
            (None, String::new(), String::new(), String::new())
        };
        self.notebook = notebook;
        self.selected = selected;
        self.title.set(title);
        self.body.set(body);
        self.query.set(query);
        self.replacement_confirmed = false;
        self.storage_pending = false;
        self.status.set("Notebook restored".into());
        self.render()
    }

    pub fn storage_result(&mut self, error: Option<String>) -> Result<(), String> {
        self.storage_pending = error.is_some();
        if error.is_none() {
            self.replacement_confirmed = false;
        }
        self.status.set(error.map_or_else(
            || "Saved to device storage".into(),
            |error| format!("Storage failed: {error}. Export a backup or retry Save."),
        ));
        self.render()
    }
}

impl<Host: PlatformHost> Drop for NotesApp<Host> {
    fn drop(&mut self) {
        self.root.dispose();
        self.projection.dispose();
        self.owner.take();
    }
}

#[cfg(test)]
#[path = "tests/notes.rs"]
mod tests;
