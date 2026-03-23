use std::collections::HashMap;

use serde_json::{json, Value};

use crate::api::{ProductiveClient, Query, Resource};

use super::ExtensionResult;

pub async fn dispatch(
    client: &ProductiveClient,
    _id: &str,
    action_name: &str,
    data: Option<&Value>,
) -> Option<Result<ExtensionResult, String>> {
    match action_name {
        "find_conflicts" => Some(find_conflicts(client, data).await),
        "capacity_availability" | "get_capacity_availability" => {
            Some(capacity_availability(client, data).await)
        }
        _ => None,
    }
}

// --- Find booking conflicts ---

async fn find_conflicts(
    client: &ProductiveClient,
    data: Option<&Value>,
) -> Result<ExtensionResult, String> {
    let started_on = data
        .and_then(|d| d.get("startedOn").or(d.get("started_on")))
        .and_then(|v| v.as_str())
        .ok_or("Missing 'startedOn' (YYYY-MM-DD) in action data.")?;
    let ended_on = data
        .and_then(|d| d.get("endedOn").or(d.get("ended_on")))
        .and_then(|v| v.as_str())
        .ok_or("Missing 'endedOn' (YYYY-MM-DD) in action data.")?;
    let person_id = data
        .and_then(|d| d.get("personId").or(d.get("person_id")))
        .and_then(|v| v.as_str());

    let mut query = Query::new()
        .filter_indexed(0, "started_on", "lt_eq", ended_on)
        .filter_indexed(1, "ended_on", "gt_eq", started_on)
        .filter_op("and")
        .include("person,service,event");

    if let Some(pid) = person_id {
        query = query.filter("person_id", pid);
    }

    let resp = client
        .get_all("/bookings", &query, 10)
        .await
        .map_err(|e| e.to_string())?;

    // Group by person
    let mut by_person: HashMap<String, Vec<&Resource>> = HashMap::new();
    for booking in &resp.data {
        if let Some(pid) = booking.relationship_id("person") {
            by_person.entry(pid.to_string()).or_default().push(booking);
        }
    }

    // Find name for each person from included
    let person_names: HashMap<&str, String> = resp
        .included
        .iter()
        .filter(|r| r.resource_type == "people")
        .map(|r| {
            let name = format!(
                "{} {}",
                r.attr_str("first_name"),
                r.attr_str("last_name")
            )
            .trim()
            .to_string();
            (r.id.as_str(), name)
        })
        .collect();

    let mut conflicts = Vec::new();

    for (person_id, bookings) in &by_person {
        let work_bookings: Vec<&&Resource> = bookings
            .iter()
            .filter(|b| b.relationship_id("service").is_some())
            .collect();
        let absence_bookings: Vec<&&Resource> = bookings
            .iter()
            .filter(|b| b.relationship_id("event").is_some())
            .collect();

        if work_bookings.is_empty() || absence_bookings.is_empty() {
            continue;
        }

        let mut person_conflicts = Vec::new();

        for wb in &work_bookings {
            let ws = wb.attr_str("started_on");
            let we = wb.attr_str("ended_on");
            if ws.is_empty() || we.is_empty() {
                continue;
            }

            for ab in &absence_bookings {
                let as_ = ab.attr_str("started_on");
                let ae = ab.attr_str("ended_on");
                if as_.is_empty() || ae.is_empty() {
                    continue;
                }

                // Check overlap
                if ws <= ae && as_ <= we {
                    let overlap_start = if ws > as_ { ws } else { as_ };
                    let overlap_end = if we < ae { we } else { ae };

                    person_conflicts.push(json!({
                        "workBookingId": wb.id,
                        "absenceBookingId": ab.id,
                        "overlapStart": overlap_start,
                        "overlapEnd": overlap_end,
                    }));
                }
            }
        }

        if !person_conflicts.is_empty() {
            let name = person_names
                .get(person_id.as_str())
                .cloned()
                .unwrap_or_else(|| "Unknown".to_string());
            conflicts.push(json!({
                "personId": person_id,
                "personName": name,
                "conflictCount": person_conflicts.len(),
                "conflicts": person_conflicts,
            }));
        }
    }

    let total_conflicts: usize = conflicts
        .iter()
        .filter_map(|c| c.get("conflictCount").and_then(|v| v.as_u64()))
        .sum::<u64>() as usize;

    let output = json!({
        "totalPeopleWithConflicts": conflicts.len(),
        "totalConflicts": total_conflicts,
        "summary": conflicts,
    });

    Ok(ExtensionResult::Json(output))
}

// --- Capacity & Availability ---

async fn capacity_availability(
    client: &ProductiveClient,
    data: Option<&Value>,
) -> Result<ExtensionResult, String> {
    let started_on = data
        .and_then(|d| d.get("startedOn").or(d.get("started_on")))
        .and_then(|v| v.as_str())
        .ok_or("Missing 'startedOn' (YYYY-MM-DD) in action data.")?;
    let ended_on = data
        .and_then(|d| d.get("endedOn").or(d.get("ended_on")))
        .and_then(|v| v.as_str())
        .ok_or("Missing 'endedOn' (YYYY-MM-DD) in action data.")?;
    let person_id = data
        .and_then(|d| d.get("personId").or(d.get("person_id")))
        .and_then(|v| v.as_str());
    let people_ids: Option<Vec<&str>> = data
        .and_then(|d| d.get("peopleIds").or(d.get("people_ids")))
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect());
    let include_non_working = data
        .and_then(|d| d.get("includeNonWorkingDays"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Fetch people
    let mut people_query = Query::new();
    if let Some(pid) = person_id {
        people_query = people_query.filter("id", pid);
    } else if let Some(pids) = &people_ids {
        for pid in pids {
            people_query = people_query.filter_array("id", pid);
        }
    }

    let people_resp = client
        .get_all("/people", &people_query, 5)
        .await
        .map_err(|e| e.to_string())?;

    if people_resp.data.is_empty() {
        return Ok(ExtensionResult::Json(json!({
            "error": "No people found matching the specified filters."
        })));
    }

    let person_ids: Vec<&str> = people_resp.data.iter().map(|p| p.id.as_str()).collect();

    // Fetch salaries for these people
    let mut salary_query = Query::new();
    for pid in &person_ids {
        salary_query = salary_query.filter_array("person_id", pid);
    }
    let salary_resp = client
        .get_all("/salaries", &salary_query, 5)
        .await
        .map_err(|e| e.to_string())?;

    // Fetch bookings overlapping the date range
    let mut booking_query = Query::new()
        .filter_indexed(0, "started_on", "lt_eq", ended_on)
        .filter_indexed(1, "ended_on", "gt_eq", started_on)
        .filter_op("and")
        .include("person,service,service.deal,service.deal.project,event");
    for pid in &person_ids {
        booking_query = booking_query.filter_array("person_id", pid);
    }
    let booking_resp = client
        .get_all("/bookings", &booking_query, 10)
        .await
        .map_err(|e| e.to_string())?;

    // Group salaries and bookings by person
    let mut salaries_by_person: HashMap<String, Vec<&Resource>> = HashMap::new();
    for s in &salary_resp.data {
        if let Some(pid) = s.relationship_id("person") {
            salaries_by_person
                .entry(pid.to_string())
                .or_default()
                .push(s);
        }
    }

    let mut bookings_by_person: HashMap<String, Vec<&Resource>> = HashMap::new();
    for b in &booking_resp.data {
        if let Some(pid) = b.relationship_id("person") {
            bookings_by_person
                .entry(pid.to_string())
                .or_default()
                .push(b);
        }
    }

    // Calculate per-person per-day
    let mut result: HashMap<String, Vec<Value>> = HashMap::new();

    for person in &people_resp.data {
        let pid = person.id.as_str();
        let person_name = format!(
            "{} {}",
            person.attr_str("first_name"),
            person.attr_str("last_name")
        )
        .trim()
        .to_string();

        let person_salaries = salaries_by_person.get(pid).map(|v| v.as_slice()).unwrap_or(&[]);
        let person_bookings = bookings_by_person.get(pid).map(|v| v.as_slice()).unwrap_or(&[]);

        let dates = generate_date_range(started_on, ended_on);
        let mut days = Vec::new();

        for date in &dates {
            let capacity = calculate_capacity_for_date(date, person_salaries);

            if !include_non_working && capacity == 0.0 {
                continue;
            }

            let booked = calculate_booked_hours(date, person_bookings, capacity, person_salaries);
            let availability = capacity - booked;

            let booking_details = extract_booking_details(date, person_bookings, capacity, person_salaries, &booking_resp.included);

            days.push(json!({
                "date": date,
                "capacity": round2(capacity),
                "availability": round2(availability),
                "bookings": booking_details,
            }));
        }

        let key = if person_name.is_empty() { pid.to_string() } else { person_name };
        result.insert(key, days);
    }

    let total_people = result.len();
    let total_days: usize = result.values().map(|d| d.len()).sum();
    let overbooked: usize = result
        .values()
        .flat_map(|d| d.iter())
        .filter(|d| d.get("availability").and_then(|v| v.as_f64()).unwrap_or(0.0) < 0.0)
        .count();

    let output = json!({
        "summary": format!(
            "Calculated capacity and availability for {} {} across {} day entries.{}",
            total_people,
            if total_people == 1 { "person" } else { "people" },
            total_days,
            if overbooked > 0 { format!(" Found {} overbooked day(s).", overbooked) } else { String::new() }
        ),
        "data": result,
    });

    Ok(ExtensionResult::Json(output))
}

// --- Capacity calculation helpers ---

fn calculate_capacity_for_date(date: &str, salaries: &[&Resource]) -> f64 {
    let salary = find_salary_for_date(salaries, date);
    let salary = match salary {
        Some(s) => s,
        None => return 0.0,
    };

    let working_hours = match salary.attributes.get("working_hours").and_then(|v| v.as_array()) {
        Some(wh) if !wh.is_empty() => wh,
        _ => return 0.0,
    };

    // Day of week: workingHours is indexed from Monday (0=Mon, 6=Sun)
    let day_of_week = day_of_week_from_monday(date);

    let alternating = salary.attr_bool("alternating_hours");
    let week_index = if alternating {
        let started_on = salary.attr_str("started_on");
        if started_on.is_empty() {
            0
        } else {
            get_week_index(date, started_on)
        }
    } else {
        0
    };

    let hour_index = week_index * 7 + day_of_week;
    working_hours
        .get(hour_index)
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
}

fn find_salary_for_date<'a>(salaries: &[&'a Resource], date: &str) -> Option<&'a Resource> {
    let mut matching: Vec<&Resource> = salaries
        .iter()
        .filter(|s| {
            let started = s.attr_str("started_on");
            let ended = s.attr_str("ended_on");
            if started.is_empty() || started > date {
                return false;
            }
            if !ended.is_empty() && ended < date {
                return false;
            }
            true
        })
        .copied()
        .collect();

    matching.sort_by(|a, b| b.attr_str("started_on").cmp(a.attr_str("started_on")));
    matching.first().copied()
}

fn calculate_booked_hours(date: &str, bookings: &[&Resource], capacity: f64, salaries: &[&Resource]) -> f64 {
    let mut total = 0.0;
    for b in bookings {
        let bs = b.attr_str("started_on");
        let be = b.attr_str("ended_on");
        if bs.is_empty() || be.is_empty() || bs > date || be < date {
            continue;
        }
        total += booking_time_for_date(b, date, capacity, salaries);
    }
    total
}

fn booking_time_for_date(booking: &Resource, date: &str, capacity: f64, salaries: &[&Resource]) -> f64 {
    // booking_method_id: 1=per_day, 2=percentage, 3=total_hours
    let method_id = booking.attributes.get("booking_method_id")
        .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
        .unwrap_or(0);

    match method_id {
        1 => {
            let time = booking.attributes.get("time").and_then(|v| v.as_f64()).unwrap_or(0.0);
            time / 60.0
        }
        2 => {
            let pct = booking.attributes.get("percentage").and_then(|v| v.as_f64()).unwrap_or(0.0);
            capacity * (pct / 100.0)
        }
        3 => {
            distribute_total_hours(booking, date, salaries)
        }
        _ => {
            let time = booking.attributes.get("time").and_then(|v| v.as_f64()).unwrap_or(0.0);
            time / 60.0
        }
    }
}

fn distribute_total_hours(booking: &Resource, target_date: &str, salaries: &[&Resource]) -> f64 {
    let bs = booking.attr_str("started_on");
    let be = booking.attr_str("ended_on");
    if bs.is_empty() || be.is_empty() {
        return 0.0;
    }

    let dates = generate_date_range(bs, be);
    let working_days: Vec<&String> = dates
        .iter()
        .filter(|d| calculate_capacity_for_date(d, salaries) > 0.0)
        .collect();

    let target_index = working_days.iter().position(|d| d.as_str() == target_date);
    let target_index = match target_index {
        Some(i) => i,
        None => return 0.0,
    };

    let total_minutes = booking.attributes.get("time").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let mut remaining = total_minutes;

    for i in 0..=target_index {
        let remaining_days = (working_days.len() - i) as f64;
        let minutes_for_day = (remaining / remaining_days).floor();
        if i == target_index {
            return minutes_for_day / 60.0;
        }
        remaining -= minutes_for_day;
    }

    0.0
}

fn extract_booking_details(
    date: &str,
    bookings: &[&Resource],
    capacity: f64,
    salaries: &[&Resource],
    included: &[Resource],
) -> Vec<Value> {
    let mut details = Vec::new();

    for b in bookings {
        let bs = b.attr_str("started_on");
        let be = b.attr_str("ended_on");
        if bs.is_empty() || be.is_empty() || bs > date || be < date {
            continue;
        }

        let hours = booking_time_for_date(b, date, capacity, salaries);
        let pct = b.attributes.get("percentage").and_then(|v| v.as_f64());

        if b.relationship_id("service").is_some() {
            // Work booking — find service name from included
            let service_name = b
                .relationship_id("service")
                .and_then(|sid| {
                    included
                        .iter()
                        .find(|r| r.resource_type == "services" && r.id == sid)
                })
                .map(|s| s.attr_str("name").to_string())
                .unwrap_or_else(|| "Unknown Service".to_string());

            details.push(json!({
                "type": "work",
                "name": service_name,
                "hours": round2(hours),
                "percentage": pct,
            }));
        } else if b.relationship_id("event").is_some() {
            let event_name = b
                .relationship_id("event")
                .and_then(|eid| {
                    included
                        .iter()
                        .find(|r| r.resource_type == "events" && r.id == eid)
                })
                .map(|e| e.attr_str("name").to_string())
                .unwrap_or_else(|| "Unknown Absence".to_string());

            details.push(json!({
                "type": "absence",
                "name": event_name,
                "hours": round2(hours),
                "percentage": pct,
            }));
        }
    }

    details
}

// --- Date/time helpers ---

fn generate_date_range(start: &str, end: &str) -> Vec<String> {
    let mut dates = Vec::new();
    let mut current = start.to_string();
    while current.as_str() <= end {
        dates.push(current.clone());
        let next = next_date(&current);
        if next == current {
            break; // malformed date — prevent infinite loop
        }
        current = next;
    }
    dates
}

fn next_date(date: &str) -> String {
    // Parse YYYY-MM-DD and add one day
    let parts: Vec<u32> = date.split('-').filter_map(|p| p.parse().ok()).collect();
    if parts.len() != 3 {
        return date.to_string();
    }
    let (y, m, d) = (parts[0] as i32, parts[1], parts[2]);

    let days_in_month = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 31,
    };

    if d < days_in_month {
        format!("{:04}-{:02}-{:02}", y, m, d + 1)
    } else if m < 12 {
        format!("{:04}-{:02}-01", y, m + 1)
    } else {
        format!("{:04}-01-01", y + 1)
    }
}

/// Day of week indexed from Monday (0=Mon, 6=Sun)
fn day_of_week_from_monday(date: &str) -> usize {
    // Zeller-like calculation for day of week
    let parts: Vec<i32> = date.split('-').filter_map(|p| p.parse().ok()).collect();
    if parts.len() != 3 {
        return 0;
    }
    let (mut y, mut m, d) = (parts[0], parts[1], parts[2]);
    if m < 3 {
        m += 12;
        y -= 1;
    }
    let dow = (d + (13 * (m + 1)) / 5 + y + y / 4 - y / 100 + y / 400) % 7;
    // Zeller: 0=Sat, 1=Sun, 2=Mon, ..., 6=Fri
    // Convert to 0=Mon, ..., 6=Sun
    ((dow + 5) % 7) as usize
}

fn get_week_index(date: &str, salary_started: &str) -> usize {
    let days = days_between(salary_started, date);
    let weeks = days / 7;
    (weeks % 2) as usize
}

fn days_between(start: &str, end: &str) -> i64 {
    // Simple days-between for YYYY-MM-DD strings
    let start_days = date_to_days(start);
    let end_days = date_to_days(end);
    (end_days - start_days).abs()
}

fn date_to_days(date: &str) -> i64 {
    let parts: Vec<i64> = date.split('-').filter_map(|p| p.parse().ok()).collect();
    if parts.len() != 3 {
        return 0;
    }
    let (mut y, mut m, d) = (parts[0], parts[1], parts[2]);
    // Normalize: shift Jan/Feb to months 13/14 of previous year for leap day handling
    if m <= 2 {
        y -= 1;
        m += 12;
    }
    // Days since a fixed epoch using the Gaussian civil calendar formula
    365 * y + y / 4 - y / 100 + y / 400 + (153 * (m - 3) + 2) / 5 + d
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}
