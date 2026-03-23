use serde_json::{json, Value};

/// Print a JSON error to stdout and exit with code 1.
/// Used by resource commands where output is always JSON.
pub fn exit_with_error(code: &str, message: &str) -> ! {
    exit_with_error_details(code, message, None)
}

/// Print a JSON error with optional details to stdout and exit with code 1.
pub fn exit_with_error_details(code: &str, message: &str, details: Option<Value>) -> ! {
    let mut err = json!({
        "error": message,
        "code": code,
    });
    if let Some(d) = details {
        err["details"] = d;
    }
    println!("{}", serde_json::to_string_pretty(&err).unwrap());
    std::process::exit(1)
}

/// Convert a TbProdError into a JSON error and exit.
pub fn exit_with_tb_error(err: &crate::error::TbProdError) -> ! {
    match err {
        crate::error::TbProdError::Api { status, message } => {
            exit_with_error_details(
                "api_error",
                &format!("API error ({})", status),
                Some(json!({ "status": status, "body": message })),
            )
        }
        crate::error::TbProdError::Config(msg) => {
            exit_with_error("config_error", msg)
        }
        other => {
            exit_with_error("internal_error", &other.to_string())
        }
    }
}
