use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Review {
    pub user: ReviewUser,
    pub state: String,
    #[serde(default)]
    pub submitted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ReviewUser {
    pub login: String,
}

/// Summary of a PR's review state: each reviewer's *latest* review.
///
/// GitHub returns the full history; for filter decisions what matters is the
/// most recent submitted review per reviewer (excluding `PENDING`/`DISMISSED`).
pub struct ReviewSummary {
    latest_by_user: HashMap<String, Review>,
}

impl ReviewSummary {
    pub fn from_reviews(reviews: &[Review]) -> Self {
        let mut latest_by_user: HashMap<String, Review> = HashMap::new();
        for r in reviews {
            // Ignore reviews without a submitted_at (drafts/pending) and
            // explicitly DISMISSED ones.
            if r.submitted_at.is_none() {
                continue;
            }
            if r.state.eq_ignore_ascii_case("DISMISSED") {
                continue;
            }
            let entry = latest_by_user.entry(r.user.login.clone());
            match entry {
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert(r.clone());
                }
                std::collections::hash_map::Entry::Occupied(mut e) => {
                    let existing_ts = e.get().submitted_at;
                    if r.submitted_at > existing_ts {
                        e.insert(r.clone());
                    }
                }
            }
        }
        Self { latest_by_user }
    }

    /// At least one reviewer's latest review is APPROVED.
    pub fn has_approval(&self) -> bool {
        self.latest_by_user
            .values()
            .any(|r| r.state.eq_ignore_ascii_case("APPROVED"))
    }

    /// Any reviewer's latest review is CHANGES_REQUESTED.
    pub fn has_pending_changes_requested(&self) -> bool {
        self.latest_by_user
            .values()
            .any(|r| r.state.eq_ignore_ascii_case("CHANGES_REQUESTED"))
    }

    /// Approved by at least one reviewer AND no reviewer is blocking.
    pub fn is_ready_to_merge(&self) -> bool {
        self.has_approval() && !self.has_pending_changes_requested()
    }

    /// The viewer's latest review (if any).
    pub fn my_latest_review(&self, my_login: &str) -> Option<&Review> {
        self.latest_by_user.get(my_login)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn review(user: &str, state: &str, ts: Option<&str>) -> Review {
        Review {
            user: ReviewUser {
                login: user.to_string(),
            },
            state: state.to_string(),
            submitted_at: ts.map(|s| {
                chrono::DateTime::parse_from_rfc3339(s)
                    .unwrap()
                    .with_timezone(&Utc)
            }),
        }
    }

    #[test]
    fn ready_to_merge_needs_approval_and_no_changes_requested() {
        let s = ReviewSummary::from_reviews(&[
            review("alice", "APPROVED", Some("2026-04-10T10:00:00Z")),
            review("bob", "COMMENTED", Some("2026-04-11T10:00:00Z")),
        ]);
        assert!(s.is_ready_to_merge());

        let s = ReviewSummary::from_reviews(&[
            review("alice", "APPROVED", Some("2026-04-10T10:00:00Z")),
            review("bob", "CHANGES_REQUESTED", Some("2026-04-11T10:00:00Z")),
        ]);
        assert!(!s.is_ready_to_merge());

        let s = ReviewSummary::from_reviews(&[review(
            "alice",
            "COMMENTED",
            Some("2026-04-10T10:00:00Z"),
        )]);
        assert!(!s.is_ready_to_merge());
    }

    #[test]
    fn superseded_changes_requested_is_cleared() {
        // Bob first requested changes, then approved — latest wins.
        let s = ReviewSummary::from_reviews(&[
            review("bob", "CHANGES_REQUESTED", Some("2026-04-10T10:00:00Z")),
            review("bob", "APPROVED", Some("2026-04-11T10:00:00Z")),
        ]);
        assert!(s.is_ready_to_merge());
    }

    #[test]
    fn dismissed_reviews_ignored() {
        let s = ReviewSummary::from_reviews(&[
            review("alice", "CHANGES_REQUESTED", Some("2026-04-10T10:00:00Z")),
            review("alice", "DISMISSED", Some("2026-04-11T10:00:00Z")),
        ]);
        // Dismissed is filtered out, so alice's CHANGES_REQUESTED is latest
        // kept — pretty conservative but matches reality where the dismissed
        // review was re-classified.
        assert!(s.has_pending_changes_requested());
    }

    #[test]
    fn my_latest_review_returns_user_specific() {
        let s = ReviewSummary::from_reviews(&[
            review("ilucin", "COMMENTED", Some("2026-04-10T10:00:00Z")),
            review("ilucin", "CHANGES_REQUESTED", Some("2026-04-12T10:00:00Z")),
            review("other", "APPROVED", Some("2026-04-11T10:00:00Z")),
        ]);
        let mine = s.my_latest_review("ilucin").unwrap();
        assert_eq!(mine.state, "CHANGES_REQUESTED");
        assert_eq!(
            mine.submitted_at.unwrap(),
            Utc.with_ymd_and_hms(2026, 4, 12, 10, 0, 0).unwrap()
        );
    }
}
