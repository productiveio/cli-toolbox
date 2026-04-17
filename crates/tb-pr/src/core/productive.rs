use regex::Regex;

/// Extract a Productive task ID from a PR body, matching the URL
/// pattern `https://app.productive.io/{org_slug}/tasks/(\w+)`.
pub fn extract_task_id(body: &str, org_slug: &str) -> Option<String> {
    let pattern = format!(
        r"https://app\.productive\.io/{}/tasks/(\w+)",
        regex::escape(org_slug)
    );
    let re = Regex::new(&pattern).ok()?;
    re.captures(body)?.get(1).map(|m| m.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_standard_task_url() {
        let body = "Fixes https://app.productive.io/109-productive/tasks/abc123xyz in scope.";
        assert_eq!(
            extract_task_id(body, "109-productive"),
            Some("abc123xyz".to_string())
        );
    }

    #[test]
    fn ignores_url_for_other_org() {
        let body = "Task: https://app.productive.io/42-other/tasks/zzz";
        assert_eq!(extract_task_id(body, "109-productive"), None);
    }

    #[test]
    fn returns_none_when_missing() {
        assert_eq!(extract_task_id("no link here", "109-productive"), None);
    }

    #[test]
    fn picks_first_match_when_multiple() {
        let body = "See https://app.productive.io/109-productive/tasks/one and \
                    https://app.productive.io/109-productive/tasks/two";
        assert_eq!(
            extract_task_id(body, "109-productive"),
            Some("one".to_string())
        );
    }
}
