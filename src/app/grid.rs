//! View-only state and deterministic ordering for the interactive result grid.
//!
//! The server-owned [`crate::query::result::ResultSet`] remains in the order and
//! shape PostgreSQL returned. This module owns only the small, disposable view
//! around it: which source columns are visible, how they are sized, which
//! retained rows are ordered, and how a wide result is positioned on screen.
//! Keeping those facts separate is what makes reset, export, inspection, and a
//! new execution unsurprising.

use crate::query::result::ResultSet;
use crate::query::value::{Cell, display_width, sanitize_for_display};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

/// The narrowest width that still leaves a column distinguishable.
pub const MIN_COLUMN_WIDTH: usize = 4;
/// The widest explicit width a local view can request.
pub const MAX_COLUMN_WIDTH: usize = 80;
/// The amount a width control changes at a time.
pub const COLUMN_WIDTH_STEP: usize = 2;

/// Formats the viewport position line for the rows actually on screen.
///
/// `first` and `last` are one-based and inclusive. The returned shape is pinned
/// in `contracts/grid.md`: a plain result names the retained total, a truncated
/// one adds what the server returned, and a filtered one names the match count.
/// `None` means there is nothing to state, so an empty result keeps its own
/// wording rather than showing `rows 0-0`.
#[must_use]
pub fn viewport_position_label(
    first: usize,
    last: usize,
    matching: usize,
    retained: usize,
    rows_seen: u64,
    truncated: bool,
    filtered: bool,
) -> Option<String> {
    if first == 0 || last < first {
        return None;
    }
    let range = format!(
        "rows {}-{}",
        thousands(first as u64),
        thousands(last as u64)
    );
    let body = if filtered {
        format!(
            "{range} matching {} of {} retained",
            thousands(matching as u64),
            thousands(retained as u64)
        )
    } else {
        format!("{range} of {} retained", thousands(retained as u64))
    };
    Some(if truncated {
        format!("{body}, of {} returned", thousands(rows_seen))
    } else {
        body
    })
}

/// Groups a count with commas, matching the pinned wording shapes.
fn thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// Calculates an automatic readable width for one source column.
#[must_use]
pub fn automatic_column_width(set: &ResultSet, index: usize) -> usize {
    let has_any_type = set.column_types.iter().any(Option::is_some);
    let header = set
        .columns
        .get(index)
        .map_or(0, |name| display_width(&sanitize_for_display(name)));
    let type_name = set.column_type(index).map_or_else(
        || {
            if has_any_type {
                display_width("unavailable")
            } else {
                0
            }
        },
        |name| display_width(&sanitize_for_display(name)),
    );
    let values = set
        .rows
        .iter()
        .filter_map(|row| row.get(index))
        .map(|cell| display_width(&cell.display()))
        .max()
        .unwrap_or(0);
    header
        .max(type_name)
        .max(values)
        .clamp(MIN_COLUMN_WIDTH, MAX_COLUMN_WIDTH)
}

/// The local direction applied to one source column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    /// Smaller values appear first.
    Ascending,
    /// Larger values appear first.
    Descending,
}

impl SortDirection {
    /// Words suitable for a title or palette detail.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ascending => "ascending",
            Self::Descending => "descending",
        }
    }
}

/// A local sort over one source-column position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SortSpec {
    /// Zero-based source-column position.
    pub column: usize,
    /// Direction of the local comparison.
    pub direction: SortDirection,
}

/// A command offered by the result-grid controls surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridCommand {
    /// Cycle the selected column through ascending, descending, and original order.
    SortSelected,
    /// Open the searchable source-column chooser.
    ChooseColumns,
    /// Increase the selected column's local width.
    WidenSelected,
    /// Decrease the selected column's local width.
    NarrowSelected,
    /// Show or hide the available server type labels.
    ToggleTypes,
    /// Pin the first visible source column while moving across the rest.
    ToggleFreezeFirst,
    /// Restore automatic widths, visibility, order, and horizontal position.
    Reset,
}

/// The disposable presentation state for the current retained result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultGridState {
    /// The current local sort, if any.
    pub sort: Option<SortSpec>,
    /// Source-column positions omitted from the grid.
    pub hidden_columns: BTreeSet<usize>,
    /// Explicit display widths by source-column position.
    pub widths: BTreeMap<usize, usize>,
    /// Whether available server type labels should be drawn.
    pub show_types: bool,
    /// Whether the first visible source column should stay pinned.
    pub freeze_first: bool,
    /// Start position in the visible source-column sequence.
    pub horizontal_start: usize,
    /// Header labels belonging to the result this state was created for.
    pub columns_signature: Vec<String>,
    /// A short limitation or recovery note for the current result view.
    pub note: Option<String>,
}

impl Default for ResultGridState {
    fn default() -> Self {
        Self {
            sort: None,
            hidden_columns: BTreeSet::new(),
            widths: BTreeMap::new(),
            show_types: true,
            freeze_first: false,
            horizontal_start: 0,
            columns_signature: Vec::new(),
            note: None,
        }
    }
}

impl ResultGridState {
    /// Creates the default reading preferences.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears state tied to the previous result while retaining session reading
    /// preferences for type labels and the frozen identity column.
    pub fn reset_for_result(&mut self, columns: &[String]) {
        let show_types = self.show_types;
        let freeze_first = self.freeze_first;
        *self = Self {
            show_types,
            freeze_first,
            columns_signature: columns.to_vec(),
            ..Self::default()
        };
    }

    /// Restores the current result to its automatic view.
    pub fn reset_view(&mut self) {
        self.sort = None;
        self.hidden_columns.clear();
        self.widths.clear();
        self.horizontal_start = 0;
        self.note = None;
    }

    /// Returns source columns in their original order, excluding hidden ones.
    #[must_use]
    pub fn visible_columns(&self, column_count: usize) -> Vec<usize> {
        let visible: Vec<usize> = (0..column_count)
            .filter(|index| !self.hidden_columns.contains(index))
            .collect();
        // All public transitions prevent this state. The fallback keeps a model
        // assembled by a test or a future deserialiser safe at render time.
        if visible.is_empty() && column_count > 0 {
            vec![0]
        } else {
            visible
        }
    }

    /// Shows or hides one source column, returning whether the state changed.
    pub fn toggle_column(&mut self, index: usize, column_count: usize) -> bool {
        if index >= column_count {
            self.note = Some("That result column is no longer available.".to_owned());
            return false;
        }
        if self.hidden_columns.remove(&index) {
            self.note = None;
            return true;
        }
        if self.visible_columns(column_count).len() <= 1 {
            self.note = Some("At least one result column must remain shown.".to_owned());
            return false;
        }
        self.hidden_columns.insert(index);
        self.note = None;
        true
    }

    /// Returns an explicit width, clamped to the supported range, or the
    /// automatic width supplied by the renderer.
    #[must_use]
    pub fn width_for(&self, index: usize, automatic: usize) -> usize {
        self.widths
            .get(&index)
            .copied()
            .unwrap_or(automatic)
            .clamp(MIN_COLUMN_WIDTH, MAX_COLUMN_WIDTH)
    }

    /// Applies a bounded width step to one source column.
    #[must_use]
    pub fn adjust_width(&mut self, index: usize, automatic: usize, delta: isize) -> usize {
        let current = self.width_for(index, automatic);
        let width = if delta.is_negative() {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current.saturating_add(delta as usize)
        }
        .clamp(MIN_COLUMN_WIDTH, MAX_COLUMN_WIDTH);
        self.widths.insert(index, width);
        self.note = None;
        width
    }

    /// Cycles the local sort for a source column.
    pub fn toggle_sort(&mut self, index: usize) {
        self.sort = match self.sort {
            Some(SortSpec {
                column,
                direction: SortDirection::Ascending,
            }) if column == index => Some(SortSpec {
                column,
                direction: SortDirection::Descending,
            }),
            Some(SortSpec {
                column,
                direction: SortDirection::Descending,
            }) if column == index => None,
            _ => Some(SortSpec {
                column: index,
                direction: SortDirection::Ascending,
            }),
        };
        self.note = None;
    }

    /// The pinned wording for the active sort, or `None` in server order.
    ///
    /// `contracts/grid.md` requires `sorted by {column} ascending` or
    /// `sorted by {column} descending`; the caller states server order with the
    /// existing wording when this returns `None`.
    #[must_use]
    pub fn sort_label(&self, columns: &[String]) -> Option<String> {
        let sort = self.sort?;
        let name = columns.get(sort.column).map_or_else(
            || format!("column {}", sort.column + 1),
            |name| sanitize_for_display(name),
        );
        Some(format!("sorted by {name} {}", sort.direction.label()))
    }

    /// Orders source-row indices without changing the server-owned result.
    #[must_use]
    pub fn sort_rows(&self, rows: &[usize], set: &ResultSet) -> Vec<usize> {
        let mut ordered = rows.to_vec();
        let Some(sort) = self.sort else {
            return ordered;
        };
        ordered
            .sort_by(|left, right| compare_rows(*left, *right, set, sort.column, sort.direction));
        ordered
    }
}

/// Chooses the source columns that fit in a horizontal result viewport.
///
/// The returned order is render order. When freezing is enabled, the first item
/// is the first visible source column and the remaining items are the movable
/// window. The selected column is always included, even when the terminal is too
/// narrow for two ordinary columns to fit.
#[must_use]
pub fn column_window(
    visible_columns: &[usize],
    selected_column: usize,
    widths: &[usize],
    available: usize,
    freeze_first: bool,
    horizontal_start: usize,
) -> Vec<usize> {
    if visible_columns.is_empty() {
        return Vec::new();
    }
    let selected_position = visible_columns
        .iter()
        .position(|index| *index == selected_column)
        .unwrap_or(0);

    if !freeze_first {
        let mut start = horizontal_start.min(visible_columns.len() - 1);
        let mut window = fit_window(visible_columns, start, widths, available, true);
        if !window.contains(&selected_column) {
            start = selected_position;
            window = fit_window(visible_columns, start, widths, available, true);
        }
        while start > 0 {
            let candidate = fit_window(visible_columns, start - 1, widths, available, true);
            if candidate.contains(&selected_column) {
                start -= 1;
                window = candidate;
            } else {
                break;
            }
        }
        return window;
    }

    let frozen = visible_columns[0];
    if selected_position == 0 {
        return vec![frozen];
    }

    let frozen_cost = column_cost(width_at(widths, frozen), true);
    let movable_budget = available.saturating_sub(frozen_cost);
    let movable = &visible_columns[1..];
    let movable_selected = selected_position - 1;
    let mut start = horizontal_start
        .saturating_sub(1)
        .min(movable.len().saturating_sub(1));
    let mut window = fit_window(movable, start, widths, movable_budget, false);
    if !window.contains(&selected_column) {
        start = movable_selected;
        window = fit_window(movable, start, widths, movable_budget, false);
    }
    while start > 0 {
        let candidate = fit_window(movable, start - 1, widths, movable_budget, false);
        if candidate.contains(&selected_column) {
            start -= 1;
            window = candidate;
        } else {
            break;
        }
    }

    let mut result = Vec::with_capacity(window.len() + 1);
    result.push(frozen);
    result.extend(window);
    result
}

fn fit_window(
    columns: &[usize],
    start: usize,
    widths: &[usize],
    available: usize,
    first_in_grid: bool,
) -> Vec<usize> {
    let mut used = 0usize;
    let mut fitted = Vec::new();
    for &index in columns.iter().skip(start) {
        let cost = column_cost(width_at(widths, index), first_in_grid && fitted.is_empty());
        if fitted.is_empty() || used.saturating_add(cost) <= available {
            fitted.push(index);
            used = used.saturating_add(cost);
        } else {
            break;
        }
    }
    fitted
}

fn width_at(widths: &[usize], index: usize) -> usize {
    widths
        .get(index)
        .copied()
        .unwrap_or(MIN_COLUMN_WIDTH)
        .clamp(MIN_COLUMN_WIDTH, MAX_COLUMN_WIDTH)
}

fn column_cost(width: usize, first: bool) -> usize {
    width.saturating_add(if first { 1 } else { 3 })
}

fn compare_rows(
    left_index: usize,
    right_index: usize,
    set: &ResultSet,
    column: usize,
    direction: SortDirection,
) -> Ordering {
    let left = set.rows.get(left_index).and_then(|row| row.get(column));
    let right = set.rows.get(right_index).and_then(|row| row.get(column));
    let left_null = left.is_none_or(Cell::is_null);
    let right_null = right.is_none_or(Cell::is_null);
    let value_order = compare_cells(left, right);
    let directed = if left_null != right_null {
        // NULL is last in both directions. Only comparable non-NULL values
        // reverse when descending.
        value_order
    } else {
        match direction {
            SortDirection::Ascending => value_order,
            SortDirection::Descending => value_order.reverse(),
        }
    };
    directed.then(left_index.cmp(&right_index))
}

fn compare_cells(left: Option<&Cell>, right: Option<&Cell>) -> Ordering {
    let left_null = left.is_none_or(Cell::is_null);
    let right_null = right.is_none_or(Cell::is_null);
    match (left_null, right_null) {
        (true, true) => return Ordering::Equal,
        // NULL is intentionally last in both directions. A direction changes
        // values, not the meaning of absence.
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        (false, false) => {}
    }

    let left_text = left.and_then(Cell::raw).unwrap_or_default();
    let right_text = right.and_then(Cell::raw).unwrap_or_default();
    match (DecimalKey::parse(left_text), DecimalKey::parse(right_text)) {
        // Numerically equal spellings, such as `2` and `2.00`, remain equal so
        // the row comparator can preserve their source order.
        (Some(left), Some(right)) => left.cmp(&right),
        _ => match (left, right) {
            (Some(left), Some(right)) => {
                compare_text(left, right).then_with(|| left.display().cmp(&right.display()))
            }
            // The NULL cases returned above. This fallback is still total for
            // malformed or future Cell variants and keeps sorting a renderer
            // safety boundary rather than a panic boundary.
            _ => Ordering::Equal,
        },
    }
}

fn compare_text(left: &Cell, right: &Cell) -> Ordering {
    let left_display = left.display();
    let right_display = right.display();
    left_display
        .to_lowercase()
        .cmp(&right_display.to_lowercase())
}

/// A decimal spelling represented without floating-point conversion.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DecimalKey {
    negative: bool,
    digits: String,
    scale: i64,
}

impl DecimalKey {
    fn parse(input: &str) -> Option<Self> {
        let input = input.trim();
        if input.is_empty() {
            return None;
        }

        let (mantissa, exponent) = match input.find(['e', 'E']) {
            Some(index) => {
                if input[index + 1..].contains(['e', 'E']) {
                    return None;
                }
                let exponent = input[index + 1..].parse::<i64>().ok()?;
                (&input[..index], exponent)
            }
            None => (input, 0),
        };

        let (negative, mantissa) = match mantissa.as_bytes().first().copied() {
            Some(b'-') => (true, &mantissa[1..]),
            Some(b'+') => (false, &mantissa[1..]),
            _ => (false, mantissa),
        };
        if mantissa.is_empty() {
            return None;
        }

        let mut seen_dot = false;
        let mut digits = String::new();
        let mut fraction = 0i64;
        let mut has_digit = false;
        for byte in mantissa.bytes() {
            match byte {
                b'0'..=b'9' => {
                    has_digit = true;
                    digits.push(char::from(byte));
                    if seen_dot {
                        fraction = fraction.checked_add(1)?;
                    }
                }
                b'.' if !seen_dot => seen_dot = true,
                _ => return None,
            }
        }
        if !has_digit {
            return None;
        }

        let first_nonzero = digits
            .bytes()
            .position(|byte| byte != b'0')
            .unwrap_or(digits.len());
        if first_nonzero == digits.len() {
            return Some(Self {
                negative: false,
                digits: "0".to_owned(),
                scale: 0,
            });
        }
        if first_nonzero > 0 {
            digits.drain(..first_nonzero);
        }

        Some(Self {
            negative,
            digits,
            scale: fraction.checked_sub(exponent)?,
        })
    }

    fn magnitude_cmp(&self, other: &Self) -> Ordering {
        let self_places = self.digits.len() as i64 - self.scale;
        let other_places = other.digits.len() as i64 - other.scale;
        self_places.cmp(&other_places).then_with(|| {
            let length = self.digits.len().max(other.digits.len());
            for index in 0..length {
                let left = self.digits.as_bytes().get(index).copied().unwrap_or(b'0');
                let right = other.digits.as_bytes().get(index).copied().unwrap_or(b'0');
                match left.cmp(&right) {
                    Ordering::Equal => {}
                    difference => return difference,
                }
            }
            Ordering::Equal
        })
    }
}

impl Ord for DecimalKey {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.negative, other.negative) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (false, false) => self.magnitude_cmp(other),
            (true, true) => self.magnitude_cmp(other).reverse(),
        }
    }
}

impl PartialOrd for DecimalKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests;
