//! Pure, allocation-free admission arithmetic for one buffered Execution.
//!
//! Admission uses the actual production layouts. It owns neither
//! result values nor SQL/output actions and does not increment received rows.
//! The integration caller must preserve real server outcomes, source identity,
//! row shape and row-cap gating. Call `admit_row` only while retained < row_cap;
//! after RowCap, count/drop the row without visiting its cell values. A byte-
//! closed budget also skips cells itself. Successful admissions precede every
//! owned header/type/cell copy; no admission may be replayed after the copy.

use crate::query::result::ResultSet;
use crate::query::value::{Cell, MAX_RETAINED_VALUE_BYTES};
use std::mem::size_of;

/// One named logical occupied-data allowance, not an RSS/allocator ceiling.
pub const RETAINED_RESULT_DATA_BYTES: usize = 32 * 1024 * 1024;

/// Whole essential-description admission. Refusal means a typed unavailable
/// result in the caller, never a fabricated zero-row result or query failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DescriptionAdmission {
    Admitted { charged_bytes: usize },
    Unavailable,
}

/// Whole candidate-row admission. The caller's existing per-cell diagnostic
/// handles `OversizedCell`; no value or new redactor crosses this boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowAdmission {
    Admitted { charged_bytes: usize },
    DataLimit,
    OversizedCell { bytes: usize },
}

/// Optional aligned type-name payload admission; refusal is nonfatal and does
/// not close data admission. The already charged type slots remain None.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeAdmission {
    Admitted { charged_bytes: usize },
    Unavailable,
}

/// Single owner, reused across every successful statement/result set.
/// It deliberately has no Clone implementation or release/replenish API.
#[derive(Debug)]
pub struct RetentionBudget {
    limit: usize,
    charged: usize,
    data_closed: bool,
}

impl Default for RetentionBudget {
    fn default() -> Self {
        Self::new()
    }
}

impl RetentionBudget {
    #[must_use]
    pub const fn new() -> Self {
        Self::with_limit(RETAINED_RESULT_DATA_BYTES)
    }

    // Small-budget tests exercise real boundaries without large allocations.
    // This is not a public runtime/config/CLI override.
    const fn with_limit(limit: usize) -> Self {
        Self {
            limit,
            charged: 0,
            data_closed: false,
        }
    }

    #[cfg(test)]
    pub(crate) const fn for_tests(limit: usize) -> Self {
        Self::with_limit(limit)
    }

    #[cfg(test)]
    #[must_use]
    pub const fn charged_bytes(&self) -> usize {
        self.charged
    }

    #[cfg(test)]
    #[must_use]
    pub const fn remaining_bytes(&self) -> usize {
        self.limit - self.charged
    }

    #[must_use]
    pub const fn data_is_closed(&self) -> bool {
        self.data_closed
    }

    /// Borrow all names, charge the entire ordered description, or retain none.
    /// This includes aligned name/type slots, including empty/unknown fields.
    pub fn admit_description<'a>(
        &mut self,
        names: impl IntoIterator<Item = &'a str>,
    ) -> DescriptionAdmission {
        if self.data_closed {
            return DescriptionAdmission::Unavailable;
        }
        let mut columns = Some(0_usize);
        let mut text = Some(0_usize);
        for name in names {
            columns = columns.and_then(|n| n.checked_add(1));
            text = text.and_then(|n| n.checked_add(name.len()));
        }
        match self.admit_data(description_charge(columns, text)) {
            Some(charged_bytes) => DescriptionAdmission::Admitted { charged_bytes },
            None => DescriptionAdmission::Unavailable,
        }
    }

    /// The caller has already applied its per-set row-cap gate. Inspect only
    /// borrowed cells. Detect an oversized candidate before aggregate refusal,
    /// even if its row's aggregate arithmetic would overflow. No partial debit.
    pub fn admit_row<'a>(
        &mut self,
        cells: impl IntoIterator<Item = Option<&'a str>>,
    ) -> RowAdmission {
        if self.data_closed {
            return RowAdmission::DataLimit;
        }
        self.admit_row_lengths(cells.into_iter().map(|cell| cell.map(str::len)))
    }

    fn admit_row_lengths(
        &mut self,
        lengths: impl IntoIterator<Item = Option<usize>>,
    ) -> RowAdmission {
        if self.data_closed {
            return RowAdmission::DataLimit;
        }
        let mut cells = Some(0_usize);
        let mut text = Some(0_usize);
        let mut first_oversized = None;
        for length in lengths {
            cells = cells.and_then(|n| n.checked_add(1));
            if let Some(length) = length {
                if length > MAX_RETAINED_VALUE_BYTES && first_oversized.is_none() {
                    first_oversized = Some(length);
                }
                text = text.and_then(|n| n.checked_add(length));
            }
        }
        if let Some(bytes) = first_oversized {
            return RowAdmission::OversizedCell { bytes };
        }
        match self.admit_data(row_charge(cells, text)) {
            Some(charged_bytes) => RowAdmission::Admitted { charged_bytes },
            None => RowAdmission::DataLimit,
        }
    }

    /// Invoke only for an available description. Caller validates names/source
    /// shape first; this additionally refuses misaligned label count. Name/type
    /// slots were charged by `admit_description`; only available UTF-8 payload
    /// is added here. No vector, String or partial label payload is allocated.
    pub fn admit_type_names<'a>(
        &mut self,
        expected_columns: usize,
        names: impl IntoIterator<Item = Option<&'a str>>,
    ) -> TypeAdmission {
        if self.data_closed {
            return TypeAdmission::Unavailable;
        }
        let mut columns = Some(0_usize);
        let mut text = Some(0_usize);
        for name in names {
            columns = columns.and_then(|n| n.checked_add(1));
            text = text.and_then(|n| n.checked_add(name.map_or(0, str::len)));
        }
        if columns != Some(expected_columns) {
            return TypeAdmission::Unavailable;
        }
        self.admit_optional(text)
    }

    fn admit_data(&mut self, charge: Option<usize>) -> Option<usize> {
        if self.data_closed {
            return None;
        }
        let accepted = charge
            .and_then(|n| self.charged.checked_add(n).map(|total| (n, total)))
            .filter(|(_, total)| *total <= self.limit);
        if let Some((bytes, total)) = accepted {
            self.charged = total;
            Some(bytes)
        } else {
            self.data_closed = true;
            None
        }
    }

    fn admit_optional(&mut self, charge: Option<usize>) -> TypeAdmission {
        if self.data_closed {
            return TypeAdmission::Unavailable;
        }
        let accepted = charge
            .and_then(|n| self.charged.checked_add(n).map(|total| (n, total)))
            .filter(|(_, total)| *total <= self.limit);
        match accepted {
            Some((charged_bytes, total)) => {
                self.charged = total;
                TypeAdmission::Admitted { charged_bytes }
            }
            None => TypeAdmission::Unavailable,
        }
    }
}

fn description_charge(columns: Option<usize>, text: Option<usize>) -> Option<usize> {
    let column_slots = size_of::<String>().checked_add(size_of::<Option<String>>())?;
    columns?
        .checked_mul(column_slots)?
        .checked_add(size_of::<ResultSet>())?
        .checked_add(text?)
}

fn row_charge(cells: Option<usize>, text: Option<usize>) -> Option<usize> {
    cells?
        .checked_mul(size_of::<Cell>())?
        .checked_add(size_of::<Vec<Cell>>())?
        .checked_add(text?)
}

#[cfg(test)]
mod tests;
