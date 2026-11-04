use crate::{StartupController, StartupPhase, StartupSnapshot};
use jadix::core::tree::Element;

pub struct SplashBuilder {
    title: String,
    description: String,
    state: Option<StartupSnapshot>,
    controller: Option<StartupController>,
    background: String,
    backdrop: Option<Element>,
    children: Vec<Element>,
}

pub fn splash() -> SplashBuilder {
    SplashBuilder {
        title: String::new(),
        description: String::new(),
        state: None,
        controller: None,
        background: "background".into(),
        backdrop: None,
        children: Vec::new(),
    }
}

impl SplashBuilder {
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }
    pub fn state(mut self, state: StartupSnapshot) -> Self {
        self.state = Some(state);
        self
    }
    pub fn controller(mut self, controller: StartupController) -> Self {
        self.controller = Some(controller);
        self
    }
    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }
    pub fn background(mut self, background: impl Into<String>) -> Self {
        self.background = background.into();
        self
    }
    pub fn backdrop(mut self, backdrop: impl Into<Element>) -> Self {
        self.backdrop = Some(backdrop.into());
        self
    }
}

fn render(builder: SplashBuilder) -> Element {
    let text = jadix::primitives::text::text;
    let mut content = Element::new()
        .w("min(24rem, 100%)")
        .flex_col()
        .gap("1rem")
        .child(text(builder.title).size("2rem"))
        .child(text(builder.description));
    if let Some(snapshot) = builder.state {
        let message = match &snapshot.phase {
            StartupPhase::Failed(error) => error.clone(),
            StartupPhase::Ready => jadix::tr_default_zh("splash.ready", "Ready", "准备就绪"),
            StartupPhase::Cancelled => {
                jadix::tr_default_zh("splash.cancelled", "Startup cancelled", "启动已取消")
            }
            _ => snapshot.stage,
        };
        content = content.child(text(message));
        if let Some((completed, total)) = snapshot.progress.filter(|(_, total)| *total > 0) {
            let percent = (u128::from(completed.min(total)) * 100 / u128::from(total)) as u64;
            let mut progress = Element::new()
                .w("100%")
                .h("0.5rem")
                .bg(jadix::core::tree::parse_color("muted"))
                .child(
                    Element::new()
                        .w(format!("{percent}%"))
                        .h("100%")
                        .bg(jadix::core::tree::parse_color("primary")),
                );
            progress.set_attr("role", "progressbar");
            progress.set_attr("accesskit_role", "progressbar");
            progress.set_attr("aria-valuemin", "0");
            progress.set_attr("aria-valuemax", "100");
            progress.set_attr("aria-valuenow", percent.to_string());
            progress.set_attr(
                "aria-label",
                jadix::tr_default_zh("splash.progress", "Startup progress", "启动进度"),
            );
            content = content.child(progress).child(text(format!("{percent}%")));
        }
        if let Some(controller) = builder.controller {
            match snapshot.phase {
                StartupPhase::Preparing => {
                    content = content.child(
                        jadix_components::button("startup-cancel")
                            .text(jadix::tr_default_zh("splash.cancel", "Cancel", "取消"))
                            .on_click(move || controller.cancel()),
                    );
                }
                StartupPhase::Failed(_) | StartupPhase::Cancelled => {
                    content = content.child(
                        jadix_components::button("startup-retry")
                            .text(jadix::tr_default_zh("splash.retry", "Retry", "重试"))
                            .on_click(move || controller.retry()),
                    );
                }
                StartupPhase::Ready => {}
            }
        }
    }
    for child in builder.children {
        content = content.child(child);
    }
    content = content.relative();
    content.set_attr("data-slot", "splash-content");
    let mut element = Element::new()
        .w("100%")
        .h("100%")
        .relative()
        .flex_col()
        .items_center()
        .justify_center()
        .bg(jadix::core::tree::parse_color(&builder.background));
    if let Some(backdrop) = builder.backdrop {
        let mut layer = Element::new()
            .absolute()
            .inset(0.0)
            .pointer_events_none()
            .child(backdrop);
        layer.set_attr("data-slot", "splash-backdrop");
        layer.set_attr("aria-hidden", "true");
        element = element.child(layer);
    }
    element = element.child(content);
    element.set_attr("data-slot", "splash");
    element.set_attr("role", "status");
    element.set_attr("aria-live", "polite");
    element
}

impl From<SplashBuilder> for Element {
    fn from(builder: SplashBuilder) -> Self {
        render(builder)
    }
}

impl jadix::core::tree::IntoElementList for SplashBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

jadix::__component_contract! {
SplashProps [];
#[allow(non_snake_case)]
pub fn Splash(
plain [] title: Option<String>,
plain [] description: Option<String>,
plain [] state: Option<StartupSnapshot>,
plain [] controller: Option<StartupController>,
plain [] background: Option<String>,
plain [] backdrop: Option<Element>,
plain [] children: Option<Element>,
) -> Element {
    render(SplashBuilder {
        title: title.unwrap_or_default(), description: description.unwrap_or_default(),
        state, controller, background: background.unwrap_or_else(|| "background".into()),
        backdrop, children: children.into_iter().collect(),
    })
}
}
