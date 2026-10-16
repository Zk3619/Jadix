//! Signal / Store / reactive registry diagnostics (Wave P P.2.3).

use crate::app::keyed_list::keyed_list_item_owner_count;
use crate::app::reactive::{
    component_children_count, component_props_count, registered_anchor_count,
    registered_binding_count,
};
use crate::core::reactivity::{reactivity_debug_snapshot, ReactivityDebugSnapshot};
use crate::core::tree_stats::process_private_mb;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryStats {
    pub anchors: usize,
    pub bindings: usize,
    pub component_props: usize,
    pub component_children: usize,
    pub keyed_list_owners: usize,
    pub reactivity: ReactivityDebugSnapshot,
}

impl RegistryStats {
    pub fn collect() -> Self {
        Self {
            anchors: registered_anchor_count(),
            bindings: registered_binding_count(),
            component_props: component_props_count(),
            component_children: component_children_count(),
            keyed_list_owners: keyed_list_item_owner_count(),
            reactivity: reactivity_debug_snapshot(),
        }
    }

    pub fn format_line(&self) -> String {
        format!(
            "anchors={} bindings={} props={} children={} keyed={} | {}",
            self.anchors,
            self.bindings,
            self.component_props,
            self.component_children,
            self.keyed_list_owners,
            self.reactivity.dump(),
        )
    }
}

pub fn registry_stats_enabled() -> bool {
    matches!(
        std::env::var("JADIX_REGISTRY_STATS").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE")
    )
}

pub fn emit_registry_stats() {
    if !registry_stats_enabled() {
        return;
    }
    let stats = RegistryStats::collect();
    log::info!(
        target: "jadix::registry_stats",
        "private={:.1} MiB {}",
        process_private_mb(),
        stats.format_line()
    );
}
