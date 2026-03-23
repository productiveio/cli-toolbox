pub mod api;
pub mod cache;
pub mod commands;
pub mod config;
pub mod error;
pub mod input;
pub mod json_error;
pub mod output;
pub mod schema;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
