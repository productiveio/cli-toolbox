/// Generate a standard CLI toolbox error enum with common variants.
///
/// Usage:
/// ```ignore
/// toolbox_core::define_error!(MyError);
/// ```
///
/// Generates `MyError` enum with Api, Config, Http, Io, Json,
/// TomlDeserialize, TomlSerialize, Other variants + `Result<T>` alias.
#[macro_export]
macro_rules! define_error {
    ($name:ident) => {
        #[derive(Debug, thiserror::Error)]
        pub enum $name {
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

        pub type Result<T> = std::result::Result<T, $name>;
    };
}

/// Wrap an async `run()` function with consistent error display.
///
/// Usage:
/// ```ignore
/// toolbox_core::run_main!(run());
/// ```
#[macro_export]
macro_rules! run_main {
    ($run:expr) => {
        #[tokio::main]
        async fn main() {
            if let Err(e) = $run.await {
                use colored::Colorize;
                eprintln!("{} {e}", "Error:".red().bold());
                std::process::exit(1);
            }
        }
    };
}

#[cfg(test)]
#[allow(dead_code)]
mod tests {
    define_error!(TestError);

    #[test]
    fn api_error_display() {
        let e = TestError::Api {
            status: 404,
            message: "not found".into(),
        };
        assert_eq!(e.to_string(), "API error (404): not found");
    }

    #[test]
    fn from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
        let e: TestError = io_err.into();
        assert!(matches!(e, TestError::Io(_)));
    }

    #[test]
    fn result_alias() {
        fn example() -> Result<()> {
            Ok(())
        }
        assert!(example().is_ok());
    }
}
