//! The command palette.
//!
//! One place to reach everything: commands, and the objects in the database.
//! Typing filters by subsequence rather than by prefix, so `cusord` finds
//! `customer_orders`, which is what makes a palette faster than a menu.
//!
//! The scorer is a few lines of our own rather than a dependency. It needs to be
//! predictable more than it needs to be clever: a user who types the initials of
//! a name expects that name first, every time.

use crate::app::message::Action;

/// What choosing an entry does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteCommand {
    /// Perform an action, exactly as a key binding would.
    Run(Action),
    /// Open the read-only connection and authentication details surface.
    ConnectionDetails,
    /// Insert text at the cursor, for example a qualified object name.
    Insert(String),
    /// Open a saved query by name, replacing the buffer.
    Open(String),
}

/// One entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteEntry {
    /// What the user reads.
    pub label: String,
    /// Secondary text: a key binding, a type, a schema.
    pub detail: String,
    /// A short word saying which group this belongs to.
    pub group: &'static str,
    /// What happens when it is chosen.
    pub command: PaletteCommand,
}

/// The palette's state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Palette {
    /// What has been typed.
    pub query: String,
    /// Everything that could be chosen.
    pub entries: Vec<PaletteEntry>,
    /// Selected position within the current matches.
    pub selected: usize,
    /// Whether what it lists is still being read.
    ///
    /// The definition panel opens while it waits rather than after; so does
    /// this, for the same reason: a key that appears to do nothing is a key
    /// people press again.
    pub loading: bool,
    /// What this palette is for, shown in its title.
    ///
    /// The same widget searches commands, objects and past statements. Saying
    /// which is open is the difference between one overlay and three.
    pub purpose: Purpose,
}

/// What a palette is searching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Purpose {
    /// Commands and database objects.
    #[default]
    GoTo,
    /// Statements that have run.
    History,
    /// What an object depends on and what depends on it.
    Dependencies,
    /// Queries saved as files.
    SavedQueries,
}

impl Purpose {
    /// The title of the overlay, including how to leave it.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::GoTo => " Go to  Enter to choose, Esc to cancel ",
            Self::History => " History  Enter puts it in the editor, Esc to cancel ",
            Self::Dependencies => {
                " Dependencies  Enter puts the name in the editor, Esc to cancel "
            }
            Self::SavedQueries => " Saved queries  Enter opens it, Esc to cancel ",
        }
    }

    /// What is said when nothing matches.
    #[must_use]
    pub const fn empty_message(self) -> &'static str {
        match self {
            Self::GoTo => " Nothing matches that.",
            Self::History => " No statement matches that.",
            Self::Dependencies => " Nothing here depends on it, and it depends on nothing here.",
            Self::SavedQueries => " Nothing is saved yet. Ctrl+K w saves what is in the editor.",
        }
    }

    /// A rule worth stating whenever this palette is open.
    ///
    /// The history's is here rather than in a message after each statement: this
    /// is where someone goes looking for one that is not there.
    #[must_use]
    pub const fn standing_note(self) -> Option<&'static str> {
        match self {
            Self::GoTo => None,
            Self::History => Some(" Statements that mention a credential are never recorded."),
            // The limit is stated where the answer is read. A dependency list
            // people trust has to say what it cannot see.
            Self::Dependencies => Some(
                " Views and foreign keys only. What a function body reads is not recorded by PostgreSQL.",
            ),
            Self::SavedQueries => None,
        }
    }
}

impl Palette {
    /// Opens a palette over a set of entries.
    #[must_use]
    pub fn new(entries: Vec<PaletteEntry>) -> Self {
        Self {
            query: String::new(),
            entries,
            selected: 0,
            loading: false,
            purpose: Purpose::GoTo,
        }
    }

    /// Opens a palette over past statements.
    #[must_use]
    pub fn over_history(entries: Vec<PaletteEntry>) -> Self {
        Self {
            purpose: Purpose::History,
            ..Self::new(entries)
        }
    }

    /// Opens a palette over an object's dependencies.
    #[must_use]
    pub fn over_dependencies(entries: Vec<PaletteEntry>) -> Self {
        Self {
            purpose: Purpose::Dependencies,
            ..Self::new(entries)
        }
    }

    /// Opens a palette over the saved queries.
    #[must_use]
    pub fn over_saved_queries(entries: Vec<PaletteEntry>) -> Self {
        Self {
            purpose: Purpose::SavedQueries,
            ..Self::new(entries)
        }
    }

    /// Opens the dependency palette before the answer has arrived.
    #[must_use]
    pub fn awaiting_dependencies() -> Self {
        Self {
            loading: true,
            ..Self::over_dependencies(Vec::new())
        }
    }

    /// Entries matching the query, best first.
    ///
    /// With no query the original order is preserved, so an empty palette is a
    /// browsable list rather than an arbitrary one.
    #[must_use]
    pub fn matches(&self) -> Vec<&PaletteEntry> {
        if self.query.trim().is_empty() {
            return self.entries.iter().collect();
        }
        let mut scored: Vec<(i32, usize, &PaletteEntry)> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                fuzzy_score(&self.query, &entry.label)
                    .or_else(|| fuzzy_score(&self.query, &entry.detail).map(|s| s - 40))
                    .map(|score| (score, index, entry))
            })
            .collect();
        // Sort by score, then by original position, so results never reshuffle
        // between identical queries.
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        scored.into_iter().map(|(_, _, entry)| entry).collect()
    }

    /// The entry that would be chosen right now.
    #[must_use]
    pub fn selected_entry(&self) -> Option<PaletteEntry> {
        let matches = self.matches();
        matches
            .get(self.selected.min(matches.len().saturating_sub(1)))
            .map(|entry| (*entry).clone())
    }

    /// Types a character.
    pub fn push(&mut self, ch: char) {
        self.query.push(ch);
        self.selected = 0;
    }

    /// Deletes the last character.
    pub fn backspace(&mut self) {
        self.query.pop();
        self.selected = 0;
    }

    /// Moves the selection, clamped to the matches.
    pub fn move_selection(&mut self, delta: isize) {
        let count = self.matches().len();
        if count == 0 {
            self.selected = 0;
            return;
        }
        let current = isize::try_from(self.selected).unwrap_or(0);
        let next = (current + delta).clamp(0, isize::try_from(count - 1).unwrap_or(0));
        self.selected = usize::try_from(next).unwrap_or(0);
    }
}

/// Scores a subsequence match, or returns `None` when the needle does not fit.
///
/// Higher is better. The rules, in order of weight:
///
/// - every matched character scores
/// - consecutive matches score more, so a contiguous run beats a scattered one
/// - a match at a word boundary scores more, so initials find what you mean
/// - a shorter haystack breaks ties, so `orders` beats `orders_archive_2019`
///
/// The search considers every alignment rather than taking the first one that
/// fits. A greedy scan matches `cot` against the `o` in `customer` and never
/// discovers the far better `c`-`o`-`t` at the start of each word in
/// `customer_order_totals`, which is exactly the case a palette exists for.
#[must_use]
pub fn fuzzy_score(needle: &str, haystack: &str) -> Option<i32> {
    const MATCH: i32 = 10;
    // Consecutive outweighs a word boundary on purpose. Without that, a name
    // like `o_r_d_e_r` beats `orders` for the query `order`, because every one
    // of its characters follows a separator. Contiguity is the stronger signal
    // of intent, so it is scored that way.
    const CONSECUTIVE: i32 = 18;
    const BOUNDARY: i32 = 14;
    const UNMATCHED: i32 = i32::MIN / 4;

    if needle.is_empty() {
        return Some(0);
    }
    let needle: Vec<char> = needle.to_lowercase().chars().collect();
    let folded: Vec<char> = haystack.to_lowercase().chars().collect();
    let raw: Vec<char> = haystack.chars().collect();
    if needle.len() > folded.len() {
        return None;
    }

    // The value of matching a needle character at this position, before any
    // bonus for following on from the previous match.
    let position_score = |index: usize| -> i32 {
        let boundary = index == 0
            || matches!(raw.get(index - 1), Some('_' | '.' | ' ' | '-' | '/'))
            || (raw[index].is_uppercase()
                && raw.get(index - 1).is_some_and(char::is_ascii_lowercase));
        MATCH + if boundary { BOUNDARY } else { 0 }
    };

    // best[j] is the best score for the needle prefix matched so far, ending
    // with a match at haystack position j. `reachable` is the running maximum
    // over every earlier ending position, which is what lets a later alignment
    // win without an inner loop.
    let mut best = vec![UNMATCHED; folded.len()];
    for (i, needle_char) in needle.iter().enumerate() {
        let previous = best.clone();
        let mut reachable = UNMATCHED;
        for j in 0..folded.len() {
            // Anything ending before j - 1 is a non-consecutive predecessor.
            if j >= 2 {
                reachable = reachable.max(previous[j - 2]);
            }
            best[j] = if folded[j] != *needle_char {
                UNMATCHED
            } else if i == 0 {
                position_score(j)
            } else {
                let consecutive = if j > 0 { previous[j - 1] } else { UNMATCHED };
                let from_consecutive = if consecutive > UNMATCHED {
                    consecutive + CONSECUTIVE
                } else {
                    UNMATCHED
                };
                let from_gap = reachable;
                let predecessor = from_consecutive.max(from_gap);
                if predecessor <= UNMATCHED {
                    UNMATCHED
                } else {
                    predecessor + position_score(j)
                }
            };
        }
    }

    let score = best.into_iter().max().unwrap_or(UNMATCHED);
    if score <= UNMATCHED {
        return None;
    }
    // Prefer the shorter of two otherwise equal matches.
    Some(score - i32::try_from(folded.len()).unwrap_or(0) / 4)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(label: &str) -> PaletteEntry {
        PaletteEntry {
            label: label.to_owned(),
            detail: String::new(),
            group: "Test",
            command: PaletteCommand::Insert(label.to_owned()),
        }
    }

    #[test]
    fn a_subsequence_matches_and_a_missing_character_does_not() {
        assert!(fuzzy_score("cusord", "customer_orders").is_some());
        assert!(fuzzy_score("orders", "orders").is_some());
        assert!(fuzzy_score("", "anything").is_some());
        assert!(fuzzy_score("zzz", "orders").is_none());
        assert!(
            fuzzy_score("ordersx", "orders").is_none(),
            "every character of the query must appear"
        );
    }

    #[test]
    fn matching_ignores_case_in_both_directions() {
        assert!(fuzzy_score("ORD", "orders").is_some());
        assert!(fuzzy_score("ord", "ORDERS").is_some());
    }

    #[test]
    fn a_contiguous_match_beats_a_scattered_one() {
        let contiguous = fuzzy_score("order", "orders").expect("match");
        let scattered = fuzzy_score("order", "o_r_d_e_r").expect("match");
        assert!(
            contiguous > scattered,
            "contiguous {contiguous} should beat scattered {scattered}"
        );
    }

    #[test]
    fn initials_find_what_you_meant() {
        // Typing the initials of a snake_case name should rank it first.
        let mut palette = Palette::new(vec![
            entry("customer_order_totals"),
            entry("cost"),
            entry("accounts"),
        ]);
        palette.query = "cot".into();
        let matches = palette.matches();
        assert_eq!(matches[0].label, "customer_order_totals", "{:?}", matches);
    }

    #[test]
    fn a_shorter_name_wins_a_tie() {
        let short = fuzzy_score("orders", "orders").expect("match");
        let long = fuzzy_score("orders", "orders_archive_2019").expect("match");
        assert!(short > long, "short {short} should beat long {long}");
    }

    #[test]
    fn an_empty_query_lists_everything_in_its_original_order() {
        let palette = Palette::new(vec![entry("b"), entry("a"), entry("c")]);
        let labels: Vec<&str> = palette.matches().iter().map(|e| e.label.as_str()).collect();
        assert_eq!(labels, vec!["b", "a", "c"], "browsing order is preserved");
    }

    #[test]
    fn results_do_not_reshuffle_between_identical_queries() {
        let mut palette = Palette::new(vec![entry("alpha"), entry("alpen"), entry("alpaca")]);
        palette.query = "alp".into();
        let first: Vec<String> = palette.matches().iter().map(|e| e.label.clone()).collect();
        let second: Vec<String> = palette.matches().iter().map(|e| e.label.clone()).collect();
        assert_eq!(first, second);
    }

    #[test]
    fn the_detail_field_is_searched_but_ranks_below_the_label() {
        let mut palette = Palette::new(vec![
            PaletteEntry {
                label: "unrelated".into(),
                detail: "public.orders".into(),
                group: "Objects",
                command: PaletteCommand::Insert("x".into()),
            },
            entry("orders"),
        ]);
        palette.query = "orders".into();
        let matches = palette.matches();
        assert_eq!(matches.len(), 2, "both are found");
        assert_eq!(matches[0].label, "orders", "the label match ranks first");
    }

    #[test]
    fn typing_resets_the_selection_so_enter_never_picks_a_stale_row() {
        let mut palette = Palette::new(vec![entry("alpha"), entry("beta"), entry("gamma")]);
        palette.move_selection(2);
        assert_eq!(palette.selected, 2);
        palette.push('a');
        assert_eq!(palette.selected, 0, "a new query starts at the top");

        palette.move_selection(1);
        palette.backspace();
        assert_eq!(palette.selected, 0);
    }

    #[test]
    fn selection_is_clamped_to_the_matches() {
        let mut palette = Palette::new(vec![entry("alpha"), entry("beta")]);
        for _ in 0..10 {
            palette.move_selection(1);
        }
        assert_eq!(palette.selected, 1);
        assert_eq!(palette.selected_entry().expect("entry").label, "beta");

        // Narrowing the matches must not leave the selection pointing past them.
        palette.query = "alpha".into();
        assert_eq!(
            palette.selected_entry().expect("entry").label,
            "alpha",
            "the selection follows the matches"
        );
    }

    #[test]
    fn a_query_matching_nothing_selects_nothing() {
        let mut palette = Palette::new(vec![entry("alpha")]);
        palette.query = "zzzz".into();
        assert!(palette.matches().is_empty());
        assert!(palette.selected_entry().is_none());
    }

    #[test]
    fn multibyte_queries_do_not_panic() {
        let mut palette = Palette::new(vec![entry("日本語のテーブル"), entry("orders")]);
        palette.query = "日本".into();
        assert_eq!(palette.matches().len(), 1);
        palette.push('語');
        palette.backspace();
        assert!(palette.selected_entry().is_some());
    }
}
