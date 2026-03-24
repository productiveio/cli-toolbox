use std::io::{self, IsTerminal, Read};

/// Read JSON input from either a CLI flag value or stdin.
///
/// If `flag_value` is Some, parse it as JSON.
/// If `flag_value` is None and stdin is a pipe (not a terminal), read stdin and parse.
/// If neither, return None.
pub fn read_json_input(flag_value: Option<&str>) -> Option<serde_json::Value> {
    if let Some(val) = flag_value {
        let parsed = serde_json::from_str(val).unwrap_or_else(|e| {
            crate::json_error::exit_with_error("invalid_json", &format!("Invalid JSON input: {e}"));
        });
        return Some(parsed);
    }

    let stdin = io::stdin();
    if stdin.is_terminal() {
        return None;
    }

    let mut buf = String::new();
    stdin.lock().read_to_string(&mut buf).unwrap_or_else(|e| {
        crate::json_error::exit_with_error(
            "stdin_read_error",
            &format!("Failed to read stdin: {e}"),
        );
    });

    if buf.trim().is_empty() {
        return None;
    }

    let parsed = serde_json::from_str(&buf).unwrap_or_else(|e| {
        crate::json_error::exit_with_error(
            "invalid_json",
            &format!("Invalid JSON from stdin: {e}"),
        );
    });
    Some(parsed)
}

/// Read JSON input, exiting with an error if no input is provided.
pub fn require_json_input(flag_value: Option<&str>, context: &str) -> serde_json::Value {
    read_json_input(flag_value).unwrap_or_else(|| {
        crate::json_error::exit_with_error(
            "missing_input",
            &format!(
                "No JSON input provided for {context}. Use --data '<json>' or pipe via stdin."
            ),
        );
    })
}
