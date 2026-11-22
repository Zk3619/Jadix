//! A shared native form workflow with explicit draft, review, cancellation and recovery.
use jadix_components::{authoring::native::ElementProjection, prelude::*};
use jadix_core::OwnerHandle;
use jadix_native::{
    host::{NativeRoot, PlatformHost},
    presentation::*,
};
use serde::{Deserialize, Serialize};

const MAX_STATE_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldDraft {
    pub name: String,
    pub details: String,
    pub priority: String,
    pub consent: bool,
}

impl FieldDraft {
    fn bounded(&self) -> bool {
        self.name.len() <= 256
            && self.details.len() <= 8192
            && !self.name.chars().any(char::is_control)
            && !self
                .details
                .chars()
                .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
            && ["", "normal", "urgent"].contains(&self.priority.as_str())
    }
    pub fn validate(&self) -> Result<(), String> {
        if !self.bounded() {
            return Err("Field exceeds its size limit, contains unsupported controls or has an unknown priority".into());
        }
        if self.name.trim().is_empty() {
            return Err("A contact name is required".into());
        }
        if self.details.trim().is_empty() {
            return Err("Describe the work to be done".into());
        }
        if self.priority.is_empty() {
            return Err("Choose a priority".into());
        }
        if !self.consent {
            return Err("Confirm the details before review".into());
        }
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Checkpoint {
    schema: u32,
    application: String,
    navigation: NavigationHistory,
    saved: Option<FieldDraft>,
    draft: FieldDraft,
    revision: u64,
}

#[derive(Clone, Copy)]
struct Fields {
    name: Signal<String>,
    details: Signal<String>,
    priority: Signal<String>,
    consent: Signal<bool>,
    history: Signal<NavigationHistory>,
    saved: Signal<Option<FieldDraft>>,
    revision: Signal<u64>,
    status: Signal<String>,
    pending: Signal<bool>,
}

impl Fields {
    fn draft(self) -> FieldDraft {
        FieldDraft {
            name: self.name.get(),
            details: self.details.get(),
            priority: self.priority.get(),
            consent: self.consent.get(),
        }
    }
    fn set_draft(self, draft: FieldDraft) {
        batch(|| {
            self.name.set(draft.name);
            self.details.set(draft.details);
            self.priority.set(draft.priority);
            self.consent.set(draft.consent);
        });
    }
    fn route(self, route: &str, reset: bool) {
        let mut history = self.history.get();
        let result = if reset {
            history.reset(route)
        } else {
            history.push(route).map(|_| ())
        };
        match result {
            Ok(()) => self.history.set(history),
            Err(error) => self.status.set(error),
        }
    }
    fn review(self) {
        match self.draft().validate() {
            Ok(()) => {
                self.status.set("Review the request before saving".into());
                self.route("review", false);
            }
            Err(error) => self.status.set(error),
        }
    }
    fn save(self) {
        let draft = self.draft();
        if let Err(error) = draft.validate() {
            self.status.set(error);
            return;
        }
        let Some(revision) = self.revision.get().checked_add(1) else {
            self.status.set("Request revision exhausted".into());
            return;
        };
        self.saved.set(Some(draft));
        self.revision.set(revision);
        self.pending.set(true);
        self.status
            .set("Saved in memory; waiting for storage acknowledgement".into());
        self.route("overview", true);
    }
    fn cancel(self) {
        self.set_draft(self.saved.get().unwrap_or_default());
        self.status
            .set("Draft cancelled; saved request preserved".into());
        self.route("overview", true);
    }
}

pub struct FieldApp<Host: PlatformHost> {
    pub root: NativeRoot<Host>,
    element: Element,
    projection: ElementProjection,
    fields: Fields,
    owner: Option<OwnerHandle>,
    viewport: [f32; 2],
    environment: u64,
    route: String,
}

fn named(mut element: Element, name: &str) -> Element {
    element.descriptor.accessible_name = Some(name.into());
    element
}
fn command(id: &str, caption: &str, callback: impl FnMut() + 'static) -> Element {
    button(id).text(caption).on_click(callback).into()
}

impl<Host: PlatformHost> FieldApp<Host> {
    #[cfg(target_os = "android")]
    pub(crate) fn new_with_content(host: Host, viewport: [f32; 2], content: jadix_core::window::WindowContent) -> Result<Self, String> {
        if content.factory != "field" { return Err("unknown window content factory".into()); }
        let mut app = Self::new(host, viewport, 0, String::new())?;
        if let Some(state) = content.argument { app.restore(&state)?; }
        Ok(app)
    }

    pub fn new(host: Host, viewport: [f32; 2], _: usize, _: String) -> Result<Self, String> {
        let mut root = NativeRoot::new(host);
        root.refresh_environment().map_err(|error| error.to_string())?;
        let projection = ElementProjection::with_lifetime(root.model().lifetime());
        #[cfg(any(feature = "appkit", feature = "uikit", target_os = "android"))]
        let projection = projection.with_locale_context(root.locale_context());
        let declarations = root.host().window_declarations();
        let build = || create_root(|| {
            if let Some(declarations) = declarations { jadix_core::window::provide_window_declarations(declarations); }
            let fields = Fields {
                name: create_signal(String::new()),
                details: create_signal(String::new()),
                priority: create_signal(String::new()),
                consent: create_signal(false),
                history: create_signal(
                    NavigationHistory::new(["overview", "edit", "review"], "overview").unwrap(),
                ),
                saved: create_signal(None),
                revision: create_signal(0),
                pending: create_signal(false),
                status: create_signal("Create a request to begin".to_string()),
            };
            let overview = vstack()
                .gap(8.0)
                .child(command("field-edit", "Edit request", move || {
                    fields.route("edit", false)
                }))
                .child(jadix_components::app::reactive::bind_text(
                    "field-summary",
                    move || {
                        fields.saved.get().map_or_else(
                            || "No saved request".to_owned(),
                            |saved| {
                                format!("{} / {}\n{}", saved.name, saved.priority, saved.details)
                            },
                        )
                    },
                ));
            let name: Element = input("field-name")
                .value_signal(fields.name)
                .placeholder("Contact name")
                .into();
            let details: Element = textarea("field-details")
                .value_signal(fields.details)
                .placeholder("Work details")
                .into();
            let priority: Element = select("field-priority")
                .value_signal(fields.priority)
                .options(vec![
                    ("normal".into(), "Normal".into()),
                    ("urgent".into(), "Urgent".into()),
                ])
                .placeholder("Choose priority")
                .into();
            let consent: Element = checkbox("field-consent")
                .checked_signal(fields.consent)
                .child(text("Confirm details"))
                .into();
            let edit = vstack()
                .gap(8.0)
                .child(named(name.h(40.0), "Contact name"))
                .child(named(details.h(100.0), "Work details"))
                .child(named(priority.h(40.0), "Request priority"))
                .child(consent.h(40.0))
                .child(command("field-review", "Review request", move || fields.review()).h(40.0))
                .child(command("field-cancel", "Cancel draft", move || fields.cancel()).h(40.0));
            let review = vstack()
                .gap(8.0)
                .child(jadix_components::app::reactive::bind_text(
                    "field-review-summary",
                    move || {
                        let draft = fields.draft();
                        format!("{} / {}\n{}", draft.name, draft.priority, draft.details)
                    },
                ))
                .child(command("field-save", "Save request", move || fields.save()).h(40.0))
                .child(navigation_back("field-back", fields.history, "Back to editor").h(40.0))
                .child(
                    command("field-review-cancel", "Discard draft", move || {
                        fields.cancel()
                    })
                    .h(40.0),
                );
            let content = vstack()
                .p(12.0)
                .gap(10.0)
                .child(text("Native Field Workflow").h(28.0))
                .child(jadix_components::app::reactive::bind_text(
                    "field-route",
                    move || format!("Step: {}", fields.history.get().current()),
                ))
                .child(jadix_components::app::reactive::bind_text(
                    "field-status",
                    move || fields.status.get(),
                ))
                .child(navigation_page(
                    "field-overview-page",
                    fields.history,
                    "overview",
                    overview,
                ))
                .child(navigation_page(
                    "field-edit-page",
                    fields.history,
                    "edit",
                    edit,
                ))
                .child(navigation_page(
                    "field-review-page",
                    fields.history,
                    "review",
                    review,
                ));
            let element = scroll_area("field-scroll").child(content);
            (fields, element)
        });
        #[cfg(any(feature = "appkit", feature = "uikit", target_os = "android"))]
        let (owner, (fields, element)) = root.with_locale(build);
        #[cfg(not(any(feature = "appkit", feature = "uikit", target_os = "android")))]
        let (owner, (fields, element)) = build();
        let mut app = Self {
            root,
            element,
            projection,
            fields,
            owner: Some(owner),
            viewport,
            environment: 1,
            route: String::new(),
        };
        app.render()?;
        Ok(app)
    }

    pub fn render(&mut self) -> Result<(), String> {
        self.root.refresh_environment().map_err(|error| error.to_string())?;
        let mut tree = self
            .projection
            .snapshot_with_focus(&mut self.element, self.root.model().focused())
            .map_err(|error| format!("{error:?}"))?;
        self.root
            .present(
                std::slice::from_mut(&mut tree),
                self.viewport,
                self.environment,
            )
            .map_err(|e| e.to_string())?;
        let route = self.fields.history.get().current().to_owned();
        if route != self.route {
            let target = match route.as_str() {
                "edit" => "Contact name",
                "review" => "Save request",
                _ => "Edit request",
            };
            let mut pending = vec![&tree];
            while let Some(node) = pending.pop() {
                if node.descriptor.accessible_name.as_deref() == Some(target)
                    || matches!(&node.descriptor.content, NodeContent::Button { label, .. } if label == target)
                {
                    self.root.focus(Some(node.id)).map_err(|e| e.to_string())?;
                    break;
                }
                pending.extend(&node.children);
            }
            self.route = route;
        }
        Ok(())
    }

    fn dispatch(&mut self, event: &PlatformEvent) -> Result<(), String> {
        if matches!(
            event.payload,
            EventPayload::Focus | EventPayload::Scroll { .. }
        ) {
            return Ok(());
        }
        let previous = self.fields.draft();
        self.projection
            .dispatch(&mut self.element, event)
            .map_err(|error| format!("{error:?}"))?;
        if !self.fields.draft().bounded() {
            self.fields.set_draft(previous);
            self.fields
                .status
                .set("Edit rejected: UTF-8 field limit exceeded".into());
        }
        self.render()
    }
    pub fn event(&mut self, source: &str) -> Result<(), String> {
        let event = jadix_native::wire::decode_event(source, self.root.model())
            .map_err(|e| e.to_string())?;
        self.root.event(&event).map_err(|e| e.to_string())?;
        self.dispatch(&event)
    }
    pub fn poll_events(&mut self) -> Result<usize, String> {
        // Apply and publish each command before validating the next queued event:
        // navigation may retire the remaining source controls in this batch.
        let events = self.root.host_mut().take_events()?;
        for event in &events {
            if event.node.root == self.root.model().id()
                && self.root.model().node(event.node).is_none()
            {
                continue;
            }
            self.root.event(event).map_err(|e| e.to_string())?;
            self.dispatch(event)?;
        }
        Ok(events.len())
    }
    pub fn resize(&mut self, viewport: [f32; 2], environment: u64) -> Result<(), String> {
        self.viewport = viewport;
        self.environment = environment;
        self.render()
    }
    pub fn state(&self) -> String {
        serde_json::to_string(&Checkpoint {
            schema: 1,
            application: "field".into(),
            navigation: self.fields.history.get(),
            saved: self.fields.saved.get(),
            draft: self.fields.draft(),
            revision: self.fields.revision.get(),
        })
        .expect("field checkpoint serializes")
    }
    pub fn restore(&mut self, source: &str) -> Result<(), String> {
        if self.root.model().lifetime().is_disposed() {
            return Err("Disposed field root".into());
        }
        if source.len() > MAX_STATE_BYTES {
            return Err("Field checkpoint exceeds 64 KiB".into());
        }
        let candidate: Checkpoint =
            serde_json::from_str(source).map_err(|_| "Malformed field checkpoint")?;
        let mut navigation = self.fields.history.get();
        navigation.restore(&candidate.navigation.encode())?;
        if candidate.schema != 1
            || candidate.application != "field"
            || !candidate.draft.bounded()
            || candidate
                .saved
                .as_ref()
                .is_some_and(|saved| saved.validate().is_err())
            || (candidate.revision == 0) != candidate.saved.is_none()
            || navigation.current() == "review" && candidate.draft.validate().is_err()
        {
            return Err("Invalid field checkpoint".into());
        }
        self.fields.set_draft(candidate.draft);
        self.fields.saved.set(candidate.saved);
        self.fields.revision.set(candidate.revision);
        self.fields.history.set(navigation);
        self.fields.pending.set(false);
        self.fields
            .status
            .set("Recovered saved request and draft".into());
        self.render()
    }
    pub fn storage_notification(&self) -> Option<String> {
        self.fields.pending.get().then(|| self.state())
    }
    pub fn storage_result(&mut self, error: Option<String>) -> Result<(), String> {
        self.fields.pending.set(error.is_some());
        self.fields.status.set(error.map_or_else(
            || {
                if self.fields.history.get().current() != "overview" {
                    "Draft stored; review before saving the request".into()
                } else if self.fields.saved.get().is_some() {
                    "Request stored".into()
                } else {
                    "Draft stored; no request submitted".into()
                }
            },
            |message| {
                format!(
                    "Storage failed: {}",
                    message.chars().take(256).collect::<String>()
                )
            },
        ));
        self.render()
    }
}

impl<Host: PlatformHost> Drop for FieldApp<Host> {
    fn drop(&mut self) {
        self.root.dispose();
        self.projection.dispose();
        self.owner.take();
    }
}

#[cfg(test)]
#[path = "tests/field.rs"]
mod tests;
