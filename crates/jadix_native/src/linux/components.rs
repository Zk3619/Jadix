//! Root-local factories for native GTK extensions.
use super::*;

pub trait NativeComponent {
    fn widget(&self) -> gtk::Widget;
    fn update(&mut self, properties: &serde_json::Value) -> Result<(), String>;
}
#[derive(Clone)]
pub struct ComponentContext {
    control: std::rc::Weak<Control>,
    events: std::rc::Weak<Events>,
    revision: Rc<Cell<u64>>,
}
impl ComponentContext {
    pub fn emit(&self, event: &str, data: serde_json::Value) {
        if let (Some(control), Some(events)) = (self.control.upgrade(), self.events.upgrade()) {
            hosted::emit(&events, &control, event, data);
        }
    }
    /// Publish intrinsic size without bypassing the shared layout engine.
    pub fn set_size(&self, size: [f64; 2]) -> Result<(), String> {
        if size
            .iter()
            .any(|value| !value.is_finite() || !(0.0..=16384.0).contains(value))
        {
            return Err("invalid component size".into());
        }
        let control = self
            .control
            .upgrade()
            .filter(|control| control.alive.get())
            .ok_or("component disposed")?;
        let events = self
            .events
            .upgrade()
            .filter(|events| !events.disposed.get())
            .ok_or("root disposed")?;
        if control.intrinsic.get() != Some(size) {
            let revision = self
                .revision
                .get()
                .checked_add(1)
                .ok_or("measurement revision exhausted")?;
            control.intrinsic.set(Some(size));
            self.revision.set(revision);
            events.wake.wake();
        }
        Ok(())
    }
}
type Factory = Rc<dyn Fn(ComponentContext) -> Result<Box<dyn NativeComponent>, String>>;
#[derive(Clone, Default)]
pub(super) struct Components {
    factories: BTreeMap<String, Factory>,
    pub revision: Rc<Cell<u64>>,
}
impl Components {
    pub fn register(&mut self, kind: String, factory: Factory) -> Result<(), String> {
        jadix_core::hosted::HostedContent::Custom {
            kind: kind.clone(),
            properties: serde_json::json!({}),
        }
        .validate()
        .map_err(str::to_owned)?;
        if self.factories.len() >= 128 || self.factories.contains_key(&kind) {
            return Err("component registry full or kind already registered".into());
        }
        self.factories.insert(kind, factory);
        Ok(())
    }
    pub fn child(&self) -> Self {
        Self {
            factories: self.factories.clone(),
            ..Default::default()
        }
    }
    pub fn create(
        &self,
        kind: &str,
        events: &Rc<Events>,
        control: &Rc<Control>,
    ) -> Result<Box<dyn NativeComponent>, String> {
        let factory = self
            .factories
            .get(kind)
            .ok_or_else(|| format!("native component {kind:?} is not registered"))?;
        factory(ComponentContext {
            control: Rc::downgrade(control),
            events: Rc::downgrade(events),
            revision: self.revision.clone(),
        })
    }
}
