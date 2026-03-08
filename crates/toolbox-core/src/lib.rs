pub mod config;
pub mod error;
pub mod output;

// Note: consumer crates need `thiserror` as a direct dependency for the
// define_error! macro's #[derive(thiserror::Error)] to resolve.

#[cfg(feature = "cache")]
pub mod cache;
