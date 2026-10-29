//! Node-owned typed values. Cloned nodes share values until mutation.
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Clone, Default)]
pub struct State(Option<Rc<HashMap<TypeId, Rc<dyn Any>>>>);

impl State {
    pub fn get<Value: 'static>(&self) -> Option<&Value> {
        self.0.as_ref()?.get(&TypeId::of::<Value>())?.downcast_ref()
    }
    pub fn get_mut<Value: Clone + 'static>(&mut self) -> Option<&mut Value> {
        let values = Rc::make_mut(self.0.as_mut()?);
        let value = values.get_mut(&TypeId::of::<Value>())?;
        if Rc::get_mut(value).is_none() {
            *value = Rc::new(value.downcast_ref::<Value>()?.clone());
        }
        Rc::get_mut(value)?.downcast_mut()
    }
    pub fn set<Value: 'static>(&mut self, value: Value) {
        Rc::make_mut(self.0.get_or_insert_with(Default::default))
            .insert(TypeId::of::<Value>(), Rc::new(value));
    }
    pub fn remove<Value: 'static>(&mut self) {
        if let Some(values) = &mut self.0 {
            Rc::make_mut(values).remove(&TypeId::of::<Value>());
        }
    }
    pub fn is_empty(&self) -> bool {
        self.0.as_ref().is_none_or(|values| values.is_empty())
    }
}

#[cfg(test)]
#[path = "tests/state.rs"]
mod tests;
