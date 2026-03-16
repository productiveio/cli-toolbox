use crate::cache::Cache;
use crate::error::Result;
use crate::output;

use serde::Serialize;

#[derive(Debug, Serialize)]
struct PersonRow {
    id: String,
    name: String,
    email: String,
}

pub fn run(cache: &Cache, query: Option<&str>, json: bool) -> Result<()> {
    let people = cache.people()?;

    let rows: Vec<PersonRow> = people
        .iter()
        .filter(|p| {
            let Some(q) = query else { return true };
            let needle = q.to_lowercase();
            let full = format!("{} {}", p.first_name, p.last_name).to_lowercase();
            full.contains(&needle) || p.email.to_lowercase().contains(&needle)
        })
        .map(|p| PersonRow {
            id: p.id.clone(),
            name: p.display_name(),
            email: p.email.clone(),
        })
        .collect();

    if json {
        println!("{}", output::render_json(&rows));
        return Ok(());
    }

    if rows.is_empty() {
        eprintln!("No people found.");
        return Ok(());
    }

    // Column widths
    let id_w = rows.iter().map(|r| r.id.len()).max().unwrap_or(2).max(2);
    let name_w = rows.iter().map(|r| r.name.len()).max().unwrap_or(4).max(4);

    println!(
        "{:<id_w$}  {:<name_w$}  Email",
        "ID",
        "Name",
        id_w = id_w,
        name_w = name_w
    );
    for r in &rows {
        println!(
            "{:<id_w$}  {:<name_w$}  {}",
            r.id,
            r.name,
            r.email,
            id_w = id_w,
            name_w = name_w
        );
    }
    eprintln!("\n{} people", rows.len());

    Ok(())
}
