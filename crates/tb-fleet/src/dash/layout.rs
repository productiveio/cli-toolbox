//! Pure responsive layout planning for the dashboard.
//!
//! No ratatui widgets and no `Rect`s: [`plan`] takes the frame's dimensions in
//! columns and rows and answers "what fits". The reason this is its own module is
//! that the interesting sizes — a ~40-column iPhone terminal, a 16-row split pane
//! — are exactly the ones nobody renders while developing.

/// How much room the sessions block has, in three coarse regimes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Density {
    /// One line per session, columns to spare.
    Wide,
    /// Two lines per session: name + status on top, context underneath.
    Narrow,
    /// Barely a terminal. Name and age, nothing else.
    Tiny,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub density: Density,
    pub two_row: bool,
    /// Usable content width inside the sessions block — borders already excluded.
    /// Rows are built to exactly this, never wider.
    pub width: usize,
    /// Cap on the label's own width in columns before it gets ellipsised. It's a
    /// cap, not a promise: the row builder gives the label less when the
    /// right-hand `status age` column would otherwise not fit.
    pub name_width: usize,
    pub show_cwd: bool,
    /// `false` = the status dot carries the state on its own.
    pub show_status_word: bool,
    /// Terminal rows for the events pane; `0` = hidden.
    pub events_height: u16,
    pub borders: bool,
    /// Show the `1`-`9` index gutter that makes digit-jumping discoverable.
    pub gutter: bool,
}

impl Plan {
    /// Terminal rows one session occupies.
    pub fn item_height(&self) -> u16 {
        if self.two_row { 2 } else { 1 }
    }
}

/// Width below which we stop drawing borders and take the two columns back.
const BORDER_FLOOR: usize = 40;

/// Plan a frame. `width`/`height` are the whole terminal; `rows` is how many
/// sessions there are; `longest_label` the widest session name in display
/// columns; `force_two_row` the `--rows`/`z` override (`None` = auto).
pub fn plan(
    width: u16,
    height: u16,
    rows: usize,
    longest_label: usize,
    force_two_row: Option<bool>,
) -> Plan {
    // The sessions block spans the frame, so its inner width depends on whether
    // we can afford borders — and below BORDER_FLOOR usable columns we can't.
    let bordered_inner = width.saturating_sub(2) as usize;
    let borders = bordered_inner >= BORDER_FLOOR;
    let inner = if borders {
        bordered_inner
    } else {
        width as usize
    };

    let (density, mut two_row, mut name_width, mut show_cwd, mut show_status_word) = if inner >= 100
    {
        (
            Density::Wide,
            false,
            longest_label.clamp(12, 32),
            true,
            true,
        )
    } else if inner >= 70 {
        // Room for a title but not a path; the dot replaces the status word.
        (
            Density::Wide,
            false,
            longest_label.clamp(12, 26),
            false,
            false,
        )
    } else if inner >= 40 {
        (Density::Narrow, true, inner - 12, true, true)
    } else if inner >= 28 {
        (Density::Narrow, true, inner - 12, false, true)
    } else {
        // Tiny sizes the name from the row buffer's remaining room, so there is
        // no cap to compute here.
        (Density::Tiny, false, 0, false, false)
    };

    // Tiny has nowhere to put a second line, so the override doesn't apply there.
    if density != Density::Tiny
        && let Some(forced) = force_two_row
    {
        two_row = forced;
    }
    if two_row {
        // 12 columns are reserved on the right of line 1 for `status age`.
        name_width = inner.saturating_sub(12).max(12);
        show_status_word = true;
        show_cwd = inner >= BORDER_FLOOR;
    } else if density == Density::Narrow {
        // Forced back to one line in a pane that can't hold the columns. Size the
        // name to the data instead of reserving 12 columns for a status word we
        // are about to drop — the leftovers are better spent on the terminal name.
        // `.max(12)` on the ceiling: `clamp` panics when min > max, and a pane
        // narrower than 24 columns would otherwise get there.
        name_width = longest_label.clamp(12, inner.saturating_sub(12).max(12));
        show_cwd = false;
        show_status_word = false;
    }

    let two_row = two_row && density != Density::Tiny;
    let item_height = if two_row { 2 } else { 1 };

    // Height policy. The old fixed `Length(10)` ate two thirds of a 16-row pane.
    let base_events: u16 = match height {
        h if h >= 30 => 10,
        h if h >= 24 => 8,
        h if h >= 18 => 5,
        h if h >= 12 => 3,
        _ => 0,
    };
    // Never let the events pane squeeze the sessions list below three items.
    let chrome = if borders { 2 } else { 1 };
    let need = (rows.min(3) as u16) * item_height + chrome;
    let events_height = base_events.min(height.saturating_sub(1).saturating_sub(need));

    Plan {
        density,
        two_row,
        width: inner,
        name_width,
        show_cwd,
        show_status_word,
        events_height,
        borders,
        gutter: two_row,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every size worth caring about, from a full-screen desktop terminal down to
    /// a split pane on a phone.
    const SIZES: [(u16, u16); 7] = [
        (200, 60),
        (120, 40),
        (80, 24),
        (50, 20),
        (40, 16),
        (30, 10),
        (20, 5),
    ];

    #[test]
    fn density_and_row_mode_per_size() {
        let got: Vec<_> = SIZES
            .iter()
            .map(|&(w, h)| {
                let p = plan(w, h, 3, 22, None);
                (p.density, p.two_row)
            })
            .collect();
        assert_eq!(
            got,
            vec![
                (Density::Wide, false),
                (Density::Wide, false),
                (Density::Wide, false),
                (Density::Narrow, true),
                (Density::Narrow, true),
                (Density::Narrow, true),
                (Density::Tiny, false),
            ]
        );
    }

    #[test]
    fn events_pane_shrinks_with_height_and_disappears_on_a_phone() {
        let got: Vec<u16> = SIZES
            .iter()
            .map(|&(w, h)| plan(w, h, 3, 22, None).events_height)
            .collect();
        assert_eq!(got, vec![10, 10, 8, 5, 3, 0, 0]);
    }

    #[test]
    fn borders_are_dropped_when_the_columns_are_needed_elsewhere() {
        let got: Vec<bool> = SIZES
            .iter()
            .map(|&(w, h)| plan(w, h, 3, 22, None).borders)
            .collect();
        assert_eq!(got, vec![true, true, true, true, false, false, false]);
        // Dropping the border hands the two columns to the content.
        assert_eq!(plan(40, 16, 3, 22, None).width, 40);
        assert_eq!(plan(50, 20, 3, 22, None).width, 48);
    }

    // The invariant that makes names readable at all: nothing narrows the name
    // column past 12 columns while there are 28 to work with.
    #[test]
    fn name_width_never_collapses_above_28_columns() {
        for w in 28..=220u16 {
            for h in [5u16, 10, 16, 20, 24, 40, 60] {
                for rows in [0usize, 1, 15] {
                    for longest in [3usize, 12, 22, 64] {
                        for forced in [None, Some(true), Some(false)] {
                            let p = plan(w, h, rows, longest, forced);
                            if p.width >= 28 {
                                assert!(
                                    p.name_width >= 12,
                                    "{w}x{h} rows={rows} longest={longest} forced={forced:?} -> {}",
                                    p.name_width
                                );
                            }
                            assert!(p.name_width <= p.width, "name wider than the pane");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn wide_panes_size_the_name_column_to_the_data() {
        assert_eq!(plan(200, 60, 3, 5, None).name_width, 12);
        assert_eq!(plan(200, 60, 3, 22, None).name_width, 22);
        assert_eq!(plan(200, 60, 3, 99, None).name_width, 32);
        // The 70..100 band is stingier, and trades the status word for the dot.
        let p = plan(80, 24, 3, 99, None);
        assert_eq!(p.name_width, 26);
        assert!(!p.show_status_word);
        assert!(!p.show_cwd);
    }

    #[test]
    fn narrow_panes_reserve_twelve_columns_for_status_and_age() {
        let p = plan(50, 20, 3, 60, None);
        assert_eq!(p.width, 48);
        assert_eq!(p.name_width, 36);
        assert!(p.show_status_word);
        assert!(p.show_cwd);
        assert!(p.gutter);
        // Below 40 the path goes; the name and the title are what's left.
        let p = plan(30, 10, 3, 60, None);
        assert_eq!(p.name_width, 18);
        assert!(!p.show_cwd);
        assert!(p.gutter);
    }

    #[test]
    fn the_row_override_wins_except_on_a_tiny_pane() {
        assert!(plan(200, 60, 3, 22, Some(true)).two_row);
        assert!(plan(200, 60, 3, 22, Some(true)).gutter);
        assert!(!plan(50, 20, 3, 22, Some(false)).two_row);
        assert!(!plan(50, 20, 3, 22, Some(false)).gutter);
        // Forced 1-row sizes the name to the data, not to the reserved column.
        assert_eq!(plan(50, 20, 3, 22, Some(false)).name_width, 22);
        // Tiny has no second line to give.
        assert!(!plan(20, 5, 3, 22, Some(true)).two_row);
    }

    #[test]
    fn item_height_follows_the_row_mode() {
        assert_eq!(plan(120, 40, 3, 22, None).item_height(), 1);
        assert_eq!(plan(50, 20, 3, 22, None).item_height(), 2);
    }

    #[test]
    fn events_never_squeeze_the_list_below_three_sessions() {
        // 12 rows: 1 header + 3 two-row items + 2 borders = 9, so events get 3
        // at most — and here exactly that.
        let p = plan(50, 12, 15, 22, None);
        assert!(p.events_height <= 12 - 1 - (3 * 2 + 2));
    }

    // `clamp(12, inner - 12)` panics when the ceiling drops below the floor.
    // Nothing reaches that today — the Narrow floor is 28 — so the guard is what
    // keeps a future floor change from being a crash instead of a layout tweak.
    #[test]
    fn no_width_panics_in_any_row_mode() {
        for w in 0..=220u16 {
            for forced in [None, Some(true), Some(false)] {
                let p = plan(w, 24, 3, 60, forced);
                assert!(p.name_width <= p.width.max(1));
            }
        }
    }

    #[test]
    fn degenerate_sizes_do_not_panic() {
        for (w, h) in [(0u16, 0u16), (1, 1), (2, 3), (5, 2), (300, 1)] {
            let p = plan(w, h, 0, 0, None);
            assert!(p.width <= w.max(1) as usize);
            assert_eq!(p.events_height, 0);
        }
    }
}
