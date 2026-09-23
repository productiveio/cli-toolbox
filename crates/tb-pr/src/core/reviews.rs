use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Review {
    pub user: ReviewUser,
    pub state: String,
    #[serde(default)]
    pub submitted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ReviewUser {
    pub login: String,
}

/// Summary of a PR's review state, two views per reviewer:
///
/// - `latest_by_user`: the most recent submitted review of any state
///   (PENDING/DISMISSED excluded). Answers "what did I do last" for the
///   waiting-on-author filter, where COMMENTED matters.
/// - `decision_by_user`: the reviewer's standing verdict, i.e. their latest
///   APPROVED or CHANGES_REQUESTED. Mirrors GitHub: a COMMENTED review (a
///   thread reply, say) never clears a verdict; DISMISSED does.
pub struct ReviewSummary {
    latest_by_user: HashMap<String, Review>,
    decision_by_user: HashMap<String, Review>,
}

impl ReviewSummary {
    pub fn from_reviews(reviews: &[Review]) -> Self {
        // Walk in submission order so "later wins" and "DISMISSED clears"
        // hold regardless of how the API ordered the list.
        let mut ordered: Vec<&Review> = reviews
            .iter()
            .filter(|r| r.submitted_at.is_some())
            .collect();
        ordered.sort_by_key(|r| r.submitted_at);

        let mut latest_by_user: HashMap<String, Review> = HashMap::new();
        let mut decision_by_user: HashMap<String, Review> = HashMap::new();
        for r in ordered {
            let login = r.user.login.clone();
            match r.state.to_ascii_uppercase().as_str() {
                "DISMISSED" => {
                    decision_by_user.remove(&login);
                }
                "APPROVED" | "CHANGES_REQUESTED" => {
                    decision_by_user.insert(login.clone(), r.clone());
                    latest_by_user.insert(login, r.clone());
                }
                _ => {
                    latest_by_user.insert(login, r.clone());
                }
            }
        }
        Self {
            latest_by_user,
            decision_by_user,
        }
    }

    /// At least one reviewer's standing verdict is APPROVED.
    pub fn has_approval(&self) -> bool {
        self.decision_by_user
            .values()
            .any(|r| r.state.eq_ignore_ascii_case("APPROVED"))
    }

    /// Any reviewer's standing verdict is CHANGES_REQUESTED.
    pub fn has_pending_changes_requested(&self) -> bool {
        self.decision_by_user
            .values()
            .any(|r| r.state.eq_ignore_ascii_case("CHANGES_REQUESTED"))
    }

    /// Logins whose standing verdict is CHANGES_REQUESTED, sorted for stable output.
    pub fn changes_requested_by(&self) -> Vec<String> {
        let mut logins: Vec<String> = self
            .decision_by_user
            .values()
            .filter(|r| r.state.eq_ignore_ascii_case("CHANGES_REQUESTED"))
            .map(|r| r.user.login.clone())
            .collect();
        logins.sort();
        logins
    }

    /// Approved by at least one reviewer AND no reviewer is blocking.
    pub fn is_ready_to_merge(&self) -> bool {
        self.has_approval() && !self.has_pending_changes_requested()
    }

    /// The viewer's latest review (if any).
    pub fn my_latest_review(&self, my_login: &str) -> Option<&Review> {
        self.latest_by_user.get(my_login)
    }

    /// One review per reviewer: their standing verdict when they have one,
    /// otherwise their latest review. What `show` renders, so a blocking
    /// reviewer reads as CHANGES_REQUESTED even after a later thread reply.
    /// Callers that want the full raw history should iterate the original
    /// `&[Review]` instead.
    pub fn iter_effective(&self) -> impl Iterator<Item = &Review> {
        self.latest_by_user
            .iter()
            .map(|(login, latest)| self.decision_by_user.get(login).unwrap_or(latest))
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
    fn changes_requested_by_lists_blocking_reviewers_sorted() {
        let s = ReviewSummary::from_reviews(&[
            review("zed", "CHANGES_REQUESTED", Some("2026-04-10T10:00:00Z")),
            review("amy", "CHANGES_REQUESTED", Some("2026-04-11T10:00:00Z")),
            review("bob", "APPROVED", Some("2026-04-11T10:00:00Z")),
            // Superseded by a later approval — must not be listed.
            review("cal", "CHANGES_REQUESTED", Some("2026-04-10T10:00:00Z")),
            review("cal", "APPROVED", Some("2026-04-12T10:00:00Z")),
        ]);
        assert_eq!(s.changes_requested_by(), vec!["amy", "zed"]);
        assert!(
            ReviewSummary::from_reviews(&[review("bob", "APPROVED", Some("2026-04-11T10:00:00Z"))])
                .changes_requested_by()
                .is_empty()
        );
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
    fn comment_after_changes_requested_keeps_it_blocking() {
        // GitHub keeps a CHANGES_REQUESTED verdict until the reviewer approves
        // or it's dismissed; a thread reply lands as a COMMENTED review and
        // must not clear it. Fed in reverse order to prove we sort first.
        let s = ReviewSummary::from_reviews(&[
            review("bob", "COMMENTED", Some("2026-04-12T10:00:00Z")),
            review("bob", "APPROVED", Some("2026-04-11T10:00:00Z")),
            review("zed", "COMMENTED", Some("2026-04-11T10:00:05Z")),
            review("zed", "CHANGES_REQUESTED", Some("2026-04-11T10:00:00Z")),
        ]);
        assert_eq!(s.changes_requested_by(), vec!["zed"]);
        assert!(s.has_pending_changes_requested());
        assert!(
            s.has_approval(),
            "bob's approval survives his later comment"
        );
        assert!(!s.is_ready_to_merge());
        // The plain-latest view still sees the comment (waiting-on-author needs it).
        assert_eq!(s.my_latest_review("zed").unwrap().state, "COMMENTED");
    }

    #[test]
    fn dismissed_review_clears_the_decision() {
        let s = ReviewSummary::from_reviews(&[
            review("alice", "CHANGES_REQUESTED", Some("2026-04-10T10:00:00Z")),
            review("alice", "DISMISSED", Some("2026-04-11T10:00:00Z")),
        ]);
        assert!(!s.has_pending_changes_requested());
        assert!(s.changes_requested_by().is_empty());
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
