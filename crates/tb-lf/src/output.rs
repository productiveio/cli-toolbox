// Re-export shared output functions from toolbox-core
pub use toolbox_core::output::{
    empty_hint, fmt_cost, pagination_hint, relative_time, render_json, truncate,
};

use colored::Colorize;

/// Score coloring: green >= 0.8, yellow >= 0.5, red < 0.5 (tool-specific).
pub fn score_color(value: f64) -> String {
    let text = format!("{:.2}", value);
    if value >= 0.8 {
        text.green().to_string()
    } else if value >= 0.5 {
        text.yellow().to_string()
    } else {
        text.red().to_string()
    }
}
