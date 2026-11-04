pub use jadix_core::schema::*;

#[derive(Clone, Debug, PartialEq)]
pub struct MethodContract {
    pub permission: String,
    pub arguments: Schema,
    pub result: Schema,
}
