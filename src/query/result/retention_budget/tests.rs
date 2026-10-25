use super::*;
use std::cell::Cell as Counter;

fn description_cost(names: &[&str]) -> usize {
    size_of::<ResultSet>()
        + names.len() * (size_of::<String>() + size_of::<Option<String>>())
        + names.iter().map(|name| name.len()).sum::<usize>()
}

fn row_cost(cells: &[Option<&str>]) -> usize {
    size_of::<Vec<Cell>>()
        + cells.len() * size_of::<Cell>()
        + cells
            .iter()
            .flatten()
            .map(|value| value.len())
            .sum::<usize>()
}

struct UnvisitedNames;
impl IntoIterator for UnvisitedNames {
    type Item = &'static str;
    type IntoIter = std::iter::Empty<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        panic!("a closed admission must not visit discarded names")
    }
}
struct UnvisitedCells;
impl IntoIterator for UnvisitedCells {
    type Item = Option<&'static str>;
    type IntoIter = std::iter::Empty<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        panic!("a closed/capped admission must not visit discarded values")
    }
}

#[test]
#[allow(clippy::print_stdout)]
fn actual_native_layout_and_default_contract_are_recorded() {
    let budget = RetentionBudget::new();
    assert_eq!(RETAINED_RESULT_DATA_BYTES, 33_554_432);
    assert_eq!(budget.charged_bytes(), 0);
    assert_eq!(budget.remaining_bytes(), 33_554_432);
    assert!(!budget.data_is_closed());
    println!(
        "native layout: ResultSet={} String={} Option<String>={} Vec<Cell>={} Cell={} pointer_bits={}",
        size_of::<ResultSet>(),
        size_of::<String>(),
        size_of::<Option<String>>(),
        size_of::<Vec<Cell>>(),
        size_of::<Cell>(),
        usize::BITS
    );
}

#[test]
fn exact_description_charges_native_slots_empty_names_and_utf8_bytes() {
    let names = ["", "界é"];
    let cost = description_cost(&names);
    let mut budget = RetentionBudget::with_limit(cost);
    assert_eq!(
        budget.admit_description(names),
        DescriptionAdmission::Admitted {
            charged_bytes: cost
        }
    );
    assert_eq!(budget.remaining_bytes(), 0);
    assert!(!budget.data_is_closed(), "exact use is not a refusal");
}

#[test]
fn one_over_description_is_atomic_and_closes_all_later_data() {
    let names = ["a", "界"];
    let cost = description_cost(&names);
    let mut budget = RetentionBudget::with_limit(cost - 1);
    assert_eq!(
        budget.admit_description(names),
        DescriptionAdmission::Unavailable
    );
    assert_eq!(budget.charged_bytes(), 0, "no per-column partial debit");
    assert!(budget.data_is_closed());
    assert_eq!(
        budget.admit_description(UnvisitedNames),
        DescriptionAdmission::Unavailable
    );
    assert_eq!(budget.admit_row(UnvisitedCells), RowAdmission::DataLimit);
}

#[test]
fn complete_row_exactly_charges_null_empty_utf8_and_vector_slots() {
    let names = ["n", "empty", "text"];
    let cells = [None, Some(""), Some("界é")];
    let header = description_cost(&names);
    let row = row_cost(&cells);
    let mut budget = RetentionBudget::with_limit(header + row);
    assert!(matches!(
        budget.admit_description(names),
        DescriptionAdmission::Admitted { .. }
    ));
    assert_eq!(
        budget.admit_row(cells),
        RowAdmission::Admitted { charged_bytes: row }
    );
    assert_eq!(budget.charged_bytes(), header + row);
    assert_eq!(budget.remaining_bytes(), 0);
}

#[test]
fn one_over_whole_row_does_not_spend_a_partial_prefix_or_take_later_small_rows() {
    let names = ["a", "b"];
    let cells = [Some("first"), Some("界")];
    let header = description_cost(&names);
    let mut budget = RetentionBudget::with_limit(header + row_cost(&cells) - 1);
    assert!(matches!(
        budget.admit_description(names),
        DescriptionAdmission::Admitted { .. }
    ));
    assert_eq!(budget.admit_row(cells), RowAdmission::DataLimit);
    assert_eq!(budget.charged_bytes(), header);
    assert_eq!(budget.admit_row(UnvisitedCells), RowAdmission::DataLimit);
}

#[test]
fn zero_column_results_and_empty_rows_still_have_native_container_cost() {
    let mut budget = RetentionBudget::with_limit(size_of::<ResultSet>() + size_of::<Vec<Cell>>());
    assert_eq!(
        budget.admit_description([]),
        DescriptionAdmission::Admitted {
            charged_bytes: size_of::<ResultSet>()
        }
    );
    assert_eq!(
        budget.admit_row([]),
        RowAdmission::Admitted {
            charged_bytes: size_of::<Vec<Cell>>()
        }
    );
    assert_eq!(budget.remaining_bytes(), 0);
}

#[test]
fn null_and_empty_text_each_consume_one_cell_slot() {
    let mut nulls = RetentionBudget::new();
    let mut empty = RetentionBudget::new();
    let expected = size_of::<Vec<Cell>>() + size_of::<Cell>();
    assert_eq!(
        nulls.admit_row([None]),
        RowAdmission::Admitted {
            charged_bytes: expected
        }
    );
    assert_eq!(
        empty.admit_row([Some("")]),
        RowAdmission::Admitted {
            charged_bytes: expected
        }
    );
}

#[test]
fn wide_empty_descriptions_and_null_rows_charge_slots_without_payload_allocations() {
    let columns = 1024;
    let header =
        size_of::<ResultSet>() + columns * (size_of::<String>() + size_of::<Option<String>>());
    let row = size_of::<Vec<Cell>>() + columns * size_of::<Cell>();
    let mut budget = RetentionBudget::with_limit(header + row);
    assert_eq!(
        budget.admit_description(std::iter::repeat_n("", columns)),
        DescriptionAdmission::Admitted {
            charged_bytes: header
        }
    );
    assert_eq!(
        budget.admit_row(std::iter::repeat_n(None, columns)),
        RowAdmission::Admitted { charged_bytes: row }
    );
    assert_eq!(budget.remaining_bytes(), 0);
}

#[test]
fn charges_accumulate_across_result_sets_and_completed_statement_ownership() {
    let first_names = ["first"];
    let first_row = [Some("v")];
    let second_names = ["second"];
    let total =
        description_cost(&first_names) + row_cost(&first_row) + description_cost(&second_names);
    let mut budget = RetentionBudget::with_limit(total);
    assert!(matches!(
        budget.admit_description(first_names),
        DescriptionAdmission::Admitted { .. }
    ));
    assert!(matches!(
        budget.admit_row(first_row),
        RowAdmission::Admitted { .. }
    ));
    let after_first = budget.charged_bytes();
    // Ownership transfer is caller work; this budget has no release/reset API.
    assert!(matches!(
        budget.admit_description(second_names),
        DescriptionAdmission::Admitted { .. }
    ));
    assert!(budget.charged_bytes() > after_first);
    assert_eq!(budget.charged_bytes(), total);
    assert_eq!(budget.admit_row([None]), RowAdmission::DataLimit);
    assert_eq!(
        budget.admit_description(UnvisitedNames),
        DescriptionAdmission::Unavailable
    );
}

#[test]
fn byte_row_refusal_latches_before_a_later_small_result_description() {
    let names = ["a"];
    let header = description_cost(&names);
    let mut budget = RetentionBudget::with_limit(header + size_of::<Vec<Cell>>() - 1);
    assert!(matches!(
        budget.admit_description(names),
        DescriptionAdmission::Admitted { .. }
    ));
    assert_eq!(budget.admit_row([None]), RowAdmission::DataLimit);
    assert_eq!(
        budget.admit_description([]),
        DescriptionAdmission::Unavailable
    );
    assert_eq!(budget.charged_bytes(), header);
}

// Integration-usage example only: production Session row-cap gating is outside
// this candidate and is not proved by a standalone pure test.
fn caller_row_cap_gate<'a>(
    budget: &mut RetentionBudget,
    retained: usize,
    cap: usize,
    source: impl IntoIterator<Item = Option<&'a str>>,
) -> Option<RowAdmission> {
    if retained >= cap {
        None
    } else {
        Some(budget.admit_row(source))
    }
}

#[test]
fn caller_row_cap_zero_and_reached_gate_skip_values_without_closing_later_data() {
    let first = ["first"];
    let second = ["second"];
    let mut budget =
        RetentionBudget::with_limit(description_cost(&first) + description_cost(&second));
    assert!(matches!(
        budget.admit_description(first),
        DescriptionAdmission::Admitted { .. }
    ));
    let charged = budget.charged_bytes();
    assert_eq!(caller_row_cap_gate(&mut budget, 0, 0, UnvisitedCells), None);
    assert_eq!(caller_row_cap_gate(&mut budget, 1, 1, UnvisitedCells), None);
    assert_eq!(budget.charged_bytes(), charged);
    assert!(!budget.data_is_closed());
    assert!(matches!(
        budget.admit_description(second),
        DescriptionAdmission::Admitted { .. }
    ));
}

#[test]
fn optional_types_charge_only_available_text_not_the_already_charged_slots() {
    let names = ["a", "b", "c"];
    let header = description_cost(&names);
    let mut budget = RetentionBudget::with_limit(header + 7);
    assert!(matches!(
        budget.admit_description(names),
        DescriptionAdmission::Admitted { .. }
    ));
    assert_eq!(
        budget.admit_type_names(3, [Some("int4"), None, Some("界")]),
        TypeAdmission::Admitted { charged_bytes: 7 }
    );
    assert_eq!(budget.charged_bytes(), header + 7);
}

#[test]
fn declined_optional_type_payload_is_atomic_nonfatal_and_does_not_close_rows() {
    let names = ["a"];
    let header = description_cost(&names);
    let row = row_cost(&[None]);
    let type_name = "t".repeat(row + 1); // at most a few native slots, not a large payload
    let mut budget = RetentionBudget::with_limit(header + row);
    assert!(matches!(
        budget.admit_description(names),
        DescriptionAdmission::Admitted { .. }
    ));
    assert_eq!(
        budget.admit_type_names(1, [Some(type_name.as_str())]),
        TypeAdmission::Unavailable
    );
    assert_eq!(budget.charged_bytes(), header);
    assert!(!budget.data_is_closed());
    assert_eq!(
        budget.admit_row([None]),
        RowAdmission::Admitted { charged_bytes: row }
    );
}

#[test]
fn misaligned_optional_type_payload_is_unavailable_without_any_debit_or_closure() {
    let mut budget = RetentionBudget::new();
    assert_eq!(
        budget.admit_type_names(2, [Some("int4")]),
        TypeAdmission::Unavailable
    );
    assert_eq!(
        budget.admit_type_names(1, [Some("int4"), None]),
        TypeAdmission::Unavailable
    );
    assert_eq!(budget.charged_bytes(), 0);
    assert!(!budget.data_is_closed());
}

#[test]
fn closed_data_skips_optional_type_input_and_keeps_its_previous_charge() {
    let mut budget = RetentionBudget::with_limit(0);
    assert_eq!(
        budget.admit_description([]),
        DescriptionAdmission::Unavailable
    );
    assert_eq!(
        budget.admit_type_names(0, UnvisitedCells),
        TypeAdmission::Unavailable
    );
    assert_eq!(budget.charged_bytes(), 0);
}

#[test]
fn empty_or_all_unavailable_optional_labels_need_no_further_payload_budget() {
    let mut budget = RetentionBudget::with_limit(0);
    assert_eq!(
        budget.admit_type_names(3, [None, Some(""), None]),
        TypeAdmission::Admitted { charged_bytes: 0 }
    );
    assert_eq!(budget.charged_bytes(), 0);
    assert!(!budget.data_is_closed());
}

#[test]
fn checked_header_row_math_refuses_slot_and_text_overflow_without_allocations() {
    assert_eq!(description_charge(Some(usize::MAX), Some(0)), None);
    assert_eq!(description_charge(Some(1), Some(usize::MAX)), None);
    assert_eq!(description_charge(None, Some(0)), None);
    assert_eq!(row_charge(Some(usize::MAX), Some(0)), None);
    assert_eq!(row_charge(Some(1), Some(usize::MAX)), None);
    assert_eq!(row_charge(Some(0), None), None);
}

#[test]
fn data_arithmetic_failure_latches_without_a_partial_debit() {
    let mut budget = RetentionBudget::with_limit(usize::MAX);
    assert_eq!(budget.admit_data(Some(1)), Some(1));
    assert_eq!(budget.admit_data(Some(usize::MAX)), None);
    assert_eq!(budget.charged_bytes(), 1);
    assert!(budget.data_is_closed());
    assert_eq!(budget.admit_data(Some(0)), None);
}

#[test]
fn unknown_data_cost_also_latches_and_never_becomes_zero_cost() {
    let mut budget = RetentionBudget::new();
    assert_eq!(budget.admit_data(None), None);
    assert_eq!(budget.charged_bytes(), 0);
    assert!(budget.data_is_closed());
}

#[test]
fn optional_arithmetic_failure_is_nonfatal_and_never_pretends_admission() {
    let mut budget = RetentionBudget::with_limit(usize::MAX);
    assert_eq!(budget.admit_data(Some(1)), Some(1));
    assert_eq!(
        budget.admit_optional(Some(usize::MAX)),
        TypeAdmission::Unavailable
    );
    assert_eq!(budget.admit_optional(None), TypeAdmission::Unavailable);
    assert_eq!(budget.charged_bytes(), 1);
    assert!(!budget.data_is_closed());
    assert_eq!(
        budget.admit_optional(Some(0)),
        TypeAdmission::Admitted { charged_bytes: 0 }
    );
}

#[test]
fn oversized_candidate_precedes_insufficient_budget_and_numeric_sum_overflow() {
    let mut budget = RetentionBudget::with_limit(0);
    assert_eq!(
        budget.admit_row_lengths([Some(MAX_RETAINED_VALUE_BYTES + 1)]),
        RowAdmission::OversizedCell {
            bytes: MAX_RETAINED_VALUE_BYTES + 1
        }
    );
    assert!(
        !budget.data_is_closed(),
        "caller must preserve the existing execution error path"
    );
    assert_eq!(
        budget.admit_row_lengths([Some(usize::MAX), Some(1)]),
        RowAdmission::OversizedCell { bytes: usize::MAX }
    );
    assert_eq!(budget.charged_bytes(), 0);
    assert!(!budget.data_is_closed());
}

#[test]
fn per_cell_exact_limit_passes_that_check_then_obeys_aggregate_admission() {
    let native_row = size_of::<Vec<Cell>>() + size_of::<Cell>();
    let mut exact = RetentionBudget::with_limit(native_row + MAX_RETAINED_VALUE_BYTES);
    assert_eq!(
        exact.admit_row_lengths([Some(MAX_RETAINED_VALUE_BYTES)]),
        RowAdmission::Admitted {
            charged_bytes: native_row + MAX_RETAINED_VALUE_BYTES
        }
    );
    let mut short = RetentionBudget::with_limit(native_row + MAX_RETAINED_VALUE_BYTES - 1);
    assert_eq!(
        short.admit_row_lengths([Some(MAX_RETAINED_VALUE_BYTES)]),
        RowAdmission::DataLimit
    );
    assert!(short.data_is_closed());
}

#[test]
fn all_borrowed_lengths_are_preflighted_before_a_oversized_candidate_is_returned() {
    let visits = Counter::new(0);
    let mut budget = RetentionBudget::new();
    let lengths = [Some(MAX_RETAINED_VALUE_BYTES + 1), None, Some(0)]
        .into_iter()
        .inspect(|_| visits.set(visits.get() + 1));
    assert!(matches!(
        budget.admit_row_lengths(lengths),
        RowAdmission::OversizedCell { .. }
    ));
    assert_eq!(visits.get(), 3);
    assert_eq!(budget.charged_bytes(), 0);
}

#[test]
fn after_aggregate_closure_no_new_oversized_error_is_invented() {
    let mut budget = RetentionBudget::with_limit(0);
    assert_eq!(budget.admit_row_lengths([None]), RowAdmission::DataLimit);
    let visits = Counter::new(0);
    let lengths = [Some(usize::MAX)]
        .into_iter()
        .inspect(|_| visits.set(visits.get() + 1));
    assert_eq!(budget.admit_row_lengths(lengths), RowAdmission::DataLimit);
    assert_eq!(visits.get(), 0);
}
