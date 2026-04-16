use crate::core::model::{Column, RottingBucket, SizeBucket};

pub fn size_bucket(additions: u64, deletions: u64) -> SizeBucket {
    let total = additions + deletions;
    if total < 20 {
        SizeBucket::Xs
    } else if total < 100 {
        SizeBucket::S
    } else if total < 300 {
        SizeBucket::M
    } else if total < 800 {
        SizeBucket::L
    } else {
        SizeBucket::Xl
    }
}

/// Rotting thresholds in hours per column: `[fresh, warming, stale, rotting]`.
/// age < fresh → Fresh; age < warming → Warming; … age ≥ rotting → Critical.
fn thresholds_hours(column: Column) -> [u32; 4] {
    match column {
        Column::DraftMine => [3 * 24, 7 * 24, 14 * 24, 30 * 24],
        Column::ReviewMine | Column::ReadyToMergeMine => [24, 3 * 24, 7 * 24, 14 * 24],
        Column::WaitingOnMe => [4, 24, 2 * 24, 4 * 24],
        Column::WaitingOnAuthor => [2 * 24, 5 * 24, 10 * 24, 14 * 24],
        // Mentions use the same aggressive thresholds as Waiting-on-me —
        // somebody is blocked on me responding.
        Column::Mentions => [4, 24, 2 * 24, 4 * 24],
    }
}

pub fn rotting_bucket(column: Column, age_hours: f64) -> RottingBucket {
    let t = thresholds_hours(column);
    if age_hours < t[0] as f64 {
        RottingBucket::Fresh
    } else if age_hours < t[1] as f64 {
        RottingBucket::Warming
    } else if age_hours < t[2] as f64 {
        RottingBucket::Stale
    } else if age_hours < t[3] as f64 {
        RottingBucket::Rotting
    } else {
        RottingBucket::Critical
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_buckets_cover_edges() {
        assert_eq!(size_bucket(0, 0), SizeBucket::Xs);
        assert_eq!(size_bucket(10, 9), SizeBucket::Xs);
        assert_eq!(size_bucket(10, 10), SizeBucket::S);
        assert_eq!(size_bucket(99, 0), SizeBucket::S);
        assert_eq!(size_bucket(50, 50), SizeBucket::M);
        assert_eq!(size_bucket(299, 0), SizeBucket::M);
        assert_eq!(size_bucket(150, 150), SizeBucket::L);
        assert_eq!(size_bucket(400, 400), SizeBucket::Xl);
    }

    #[test]
    fn rotting_waiting_on_me_is_aggressive() {
        // <4h fresh, <1d warming, <2d stale, <4d rotting, ≥4d critical
        assert_eq!(
            rotting_bucket(Column::WaitingOnMe, 1.0),
            RottingBucket::Fresh
        );
        assert_eq!(
            rotting_bucket(Column::WaitingOnMe, 5.0),
            RottingBucket::Warming
        );
        assert_eq!(
            rotting_bucket(Column::WaitingOnMe, 36.0),
            RottingBucket::Stale
        );
        assert_eq!(
            rotting_bucket(Column::WaitingOnMe, 60.0),
            RottingBucket::Rotting
        );
        assert_eq!(
            rotting_bucket(Column::WaitingOnMe, 120.0),
            RottingBucket::Critical
        );
    }

    #[test]
    fn rotting_draft_mine_is_lenient() {
        // <3d fresh, <7d warming, <14d stale, <30d rotting, ≥30d critical
        assert_eq!(
            rotting_bucket(Column::DraftMine, 48.0),
            RottingBucket::Fresh
        );
        assert_eq!(
            rotting_bucket(Column::DraftMine, 24.0 * 29.0),
            RottingBucket::Rotting
        );
        assert_eq!(
            rotting_bucket(Column::DraftMine, 24.0 * 40.0),
            RottingBucket::Critical
        );
    }
}
