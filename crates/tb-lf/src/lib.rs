pub mod api;
pub mod cache;
pub mod cli;
pub mod config;
pub mod error;
pub mod output;
pub mod share;
pub mod share_alias;
pub mod types;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
