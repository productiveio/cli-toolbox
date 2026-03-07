use thiserror::Error;

#[derive(Debug, Error)]
pub enum TbSemError {
    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String },

    #[error("Config error: {0}")]
    Config(String),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("TOML deserialize error: {0}")]
    TomlDeserialize(#[from] toml::de::Error),

    #[error("TOML serialize error: {0}")]
    TomlSerialize(#[from] toml::ser::Error),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, TbSemError>;
