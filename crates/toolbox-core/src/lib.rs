pub mod config;
pub mod error;
pub mod output;
pub mod skill;

// Note: consumer crates need `thiserror` as a direct dependency for the
// define_error! macro's #[derive(thiserror::Error)] to resolve.

#[cfg(feature = "cache")]
pub mod cache;

#[cfg(feature = "prompt")]
pub mod prompt;

#[cfg(feature = "version-check")]
pub mod version_check;
