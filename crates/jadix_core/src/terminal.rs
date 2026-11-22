//! Backend-independent terminal stream contract for native and TUI hosts.
use serde::{Deserialize, Serialize};
#[cfg(feature = "terminal")]
mod screen;
#[cfg(feature = "terminal")]
pub use screen::{TerminalScreen, TerminalFrame, CellStyle};
mod input;
pub use input::special_key;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminalSpec { pub columns: u16, pub rows: u16, #[serde(default="default_scrollback")] pub scrollback: usize }
fn default_scrollback()->usize{10_000}
impl TerminalSpec { pub fn validate(&self)->Result<(), &'static str>{ if !(1..=512).contains(&self.columns)||!(1..=256).contains(&self.rows)||self.scrollback.saturating_add(usize::from(self.rows)).saturating_mul(usize::from(self.columns))>1_000_000 {Err("invalid terminal dimensions")}else{Ok(())} } }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag="type", rename_all="snake_case", deny_unknown_fields)]
pub enum TerminalEvent { Input { bytes: String }, Output { data: Vec<u8> }, Resize { columns:u16, rows:u16 }, Exit { code:i32 } }
impl TerminalEvent { pub fn validate(&self)->Result<(), &'static str>{ match self { Self::Input{bytes} if bytes.len()<=64*1024=>Ok(()), Self::Output{data} if data.len()<=64*1024=>Ok(()), Self::Resize{columns,rows} if (1..=512).contains(columns)&&(1..=256).contains(rows)=>Ok(()), Self::Exit{..}=>Ok(()), _=>Err("invalid terminal event") } } }
