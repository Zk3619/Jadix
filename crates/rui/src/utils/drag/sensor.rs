//! Custom drag sensors (solid-dnd `addSensor` / `activators`).
//!
//! Pointer drag remains the default path (`assemble_draggable` /
//! `window_events`). Additional sensors call the same [`super::BeginDrag`] entry
//! when their activator returns true.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::session::BeginDrag;

/// Inputs for a custom sensor activator.
pub struct DragSensorCtx {
    pub scope_id: String,
    /// Call to start a drag (same as pointer `pointerdown`).
    pub begin: Rc<dyn Fn(BeginDrag)>,
}

pub type DragSensorFn = Rc<dyn Fn(&DragSensorCtx) -> bool>;

/// Named sensor stage (solid-dnd `Sensor` + `activators`).
#[derive(Clone)]
pub struct DragSensor {
    pub id: String,
    pub activator: DragSensorFn,
}

impl DragSensor {
    pub fn new(
        id: impl Into<String>,
        activator: impl Fn(&DragSensorCtx) -> bool + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            activator: Rc::new(activator),
        }
    }
}

thread_local! {
    static EXTRA_SENSORS: RefCell<HashMap<String, Vec<DragSensor>>> = RefCell::new(HashMap::new());
}

pub(super) fn merge_sensors(scope_id: &str, builder: &[DragSensor]) -> Vec<DragSensor> {
    let mut out = builder.to_vec();
    EXTRA_SENSORS.with(|m| {
        if let Some(extra) = m.borrow().get(scope_id) {
            for s in extra {
                out.retain(|x| x.id != s.id);
                out.push(s.clone());
            }
        }
    });
    out
}

/// Register a runtime sensor (survives provider rebuild). Returns false if the
/// scope is unknown.
pub fn add_sensor(scope_id: &str, sensor: DragSensor) -> bool {
    if !super::session::scope_exists(scope_id) {
        return false;
    }
    EXTRA_SENSORS.with(|m| {
        let mut map = m.borrow_mut();
        let list = map.entry(scope_id.to_string()).or_default();
        list.retain(|s| s.id != sensor.id);
        list.push(sensor);
    });
    true
}

/// Remove a runtime sensor by id.
pub fn remove_sensor(scope_id: &str, id: &str) -> bool {
    EXTRA_SENSORS.with(|m| {
        let mut map = m.borrow_mut();
        let Some(list) = map.get_mut(scope_id) else {
            return false;
        };
        let before = list.len();
        list.retain(|s| s.id != id);
        let changed = list.len() != before;
        if list.is_empty() {
            map.remove(scope_id);
        }
        changed
    })
}

/// Run registered sensors for one scope (keyboard / programmatic). Returns true if
/// an activator consumed the event (typically after calling `begin`).
pub fn run_scope_sensors(scope_id: &str) -> bool {
    if !super::session::scope_exists(scope_id) {
        return false;
    }
    let sensors = super::session::scope_sensors(scope_id);
    let begin: Rc<dyn Fn(BeginDrag)> = Rc::new(super::session::begin);
    let ctx = DragSensorCtx {
        scope_id: scope_id.to_string(),
        begin,
    };
    try_sensors(scope_id, &sensors, &ctx)
}

/// Start a drag from the registered idle hit table (sensor / keyboard helpers).
pub fn begin_draggable(scope_id: &str, draggable_id: &str, begin: &Rc<dyn Fn(BeginDrag)>) -> bool {
    let hits = super::registry::registered_hits_in_scope(scope_id);
    let Some(hit) = hits
        .iter()
        .find(|h| h.is_draggable && h.id == draggable_id && !h.disabled)
    else {
        return false;
    };
    let r = hit.rect;
    if !r.is_usable() {
        return false;
    }
    let pointer = [r.x + r.width * 0.5, r.y + r.height * 0.5];
    let bounds = [r.x, r.y, r.width, r.height];
    let (grab_offset, start_bounds) =
        super::session::start_geometry(scope_id, draggable_id, pointer, bounds);
    begin(BeginDrag {
        scope_id: scope_id.to_string(),
        draggable_id: draggable_id.to_string(),
        pointer,
        grab_offset,
        start_bounds,
        from_list: hit.list_id.clone(),
        from_index: hit.index,
        item_type: hit.item_type.clone(),
        group: hit.group.clone(),
        capture_id: draggable_id.to_string(),
    });
    true
}

/// Run sensors in order; first `true` wins.
pub fn try_sensors(scope_id: &str, builder: &[DragSensor], ctx: &DragSensorCtx) -> bool {
    for sensor in merge_sensors(scope_id, builder) {
        if (sensor.activator)(ctx) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn begin_draggable_missing_hit_returns_false() {
        let begin: Rc<dyn Fn(BeginDrag)> = Rc::new(|_| {});
        assert!(!begin_draggable("missing", "x", &begin));
    }

    #[test]
    fn add_sensor_requires_scope() {
        assert!(!add_sensor("missing", DragSensor::new("kbd", |_| false)));
    }
}
