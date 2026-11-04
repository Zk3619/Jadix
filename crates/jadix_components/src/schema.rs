pub use jadix::presentation::component::ComponentSchema;

pub const PORTABLE_SCHEMA: &str = include_str!("../schema/portable-v1.json");
pub const EXTENDED_SCHEMA: &str = include_str!("../schema/extended-v1.json");

pub fn portable_components() -> Result<Vec<ComponentSchema>, serde_json::Error> {
    serde_json::from_str(PORTABLE_SCHEMA)
}

pub fn extended_components() -> Result<Vec<ComponentSchema>, serde_json::Error> {
    serde_json::from_str(EXTENDED_SCHEMA)
}
