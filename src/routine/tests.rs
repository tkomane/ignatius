//! Unit tests for PL/pgSQL routine runner and profiler domain engine.

use super::*;

fn sample_function() -> RoutineSignature {
    RoutineSignature {
        schema: "public".to_string(),
        name: "calculate_tax".to_string(),
        kind: RoutineKind::Function,
        language: "plpgsql".to_string(),
        return_type: "numeric".to_string(),
        is_set_returning: false,
        params: vec![
            RoutineParam {
                name: "subtotal".to_string(),
                data_type: "numeric".to_string(),
                mode: ParamMode::In,
                default_value: None,
                has_default: false,
            },
            RoutineParam {
                name: "tax_rate".to_string(),
                data_type: "numeric".to_string(),
                mode: ParamMode::In,
                default_value: Some("0.0825".to_string()),
                has_default: true,
            },
        ],
        source: "BEGIN\n  RETURN subtotal * tax_rate;\nEND;".to_string(),
        volatility: "immutable".to_string(),
        security_definer: false,
    }
}

fn sample_procedure() -> RoutineSignature {
    RoutineSignature {
        schema: "billing".to_string(),
        name: "archive_invoices".to_string(),
        kind: RoutineKind::Procedure,
        language: "plpgsql".to_string(),
        return_type: "void".to_string(),
        is_set_returning: false,
        params: vec![
            RoutineParam {
                name: "before_date".to_string(),
                data_type: "date".to_string(),
                mode: ParamMode::In,
                default_value: None,
                has_default: false,
            },
            RoutineParam {
                name: "archived_count".to_string(),
                data_type: "integer".to_string(),
                mode: ParamMode::Out,
                default_value: None,
                has_default: false,
            },
        ],
        source: "BEGIN\n  DELETE FROM billing.invoices;\nEND;".to_string(),
        volatility: "volatile".to_string(),
        security_definer: true,
    }
}

#[test]
fn routine_kind_labels_and_keywords() {
    assert_eq!(RoutineKind::Function.label(), "Function");
    assert_eq!(RoutineKind::Procedure.label(), "Procedure");
    assert_eq!(RoutineKind::Function.sql_keyword(), "FUNCTION");
    assert_eq!(RoutineKind::Procedure.sql_keyword(), "PROCEDURE");
}

#[test]
fn param_mode_from_catalog_and_keywords() {
    assert_eq!(ParamMode::from_catalog_char('i'), ParamMode::In);
    assert_eq!(ParamMode::from_catalog_char('o'), ParamMode::Out);
    assert_eq!(ParamMode::from_catalog_char('b'), ParamMode::InOut);
    assert_eq!(ParamMode::from_catalog_char('v'), ParamMode::Variadic);
    assert_eq!(ParamMode::from_catalog_char('t'), ParamMode::Table);
    assert_eq!(ParamMode::In.sql_keyword(), "IN");
    assert_eq!(ParamMode::Out.sql_keyword(), "OUT");
}

#[test]
fn display_signature_formats_cleanly() {
    let func = sample_function();
    let sig = func.display_signature();
    assert_eq!(
        sig,
        "public.calculate_tax(subtotal numeric, tax_rate numeric DEFAULT 0.0825) -> numeric"
    );

    let proc = sample_procedure();
    let proc_sig = proc.display_signature();
    assert_eq!(
        proc_sig,
        "billing.archive_invoices(before_date date, OUT archived_count integer) -> void"
    );
}

#[test]
fn build_routine_call_sql_for_function_and_procedure() {
    let func = sample_function();
    let mut args = BTreeMap::new();
    args.insert("subtotal".to_string(), "100.00".to_string());
    args.insert("tax_rate".to_string(), "0.05".to_string());

    let sql = build_routine_call_sql(&func, &args);
    assert_eq!(
        sql,
        "SELECT \"public\".\"calculate_tax\"(100.00::numeric, 0.05::numeric);"
    );

    // Procedure CALL
    let proc = sample_procedure();
    let mut proc_args = BTreeMap::new();
    proc_args.insert("before_date".to_string(), "2026-01-01".to_string());
    let proc_sql = build_routine_call_sql(&proc, &proc_args);
    assert_eq!(
        proc_sql,
        "CALL \"billing\".\"archive_invoices\"('2026-01-01'::date);"
    );
}

#[test]
fn build_test_harness_sql_wraps_in_savepoint_rollback() {
    let func = sample_function();
    let mut args = BTreeMap::new();
    args.insert("subtotal".to_string(), "50.0".to_string());

    let rollback_sql = build_test_harness_sql(&func, &args, true);
    assert!(rollback_sql.contains("BEGIN;"));
    assert!(rollback_sql.contains("SAVEPOINT ignatius_routine_test;"));
    assert!(rollback_sql.contains("ROLLBACK TO SAVEPOINT ignatius_routine_test;"));
    assert!(rollback_sql.contains("ROLLBACK;"));
    assert!(
        rollback_sql
            .contains("SELECT \"public\".\"calculate_tax\"(50.0::numeric, 0.0825::numeric);")
    );

    let direct_sql = build_test_harness_sql(&func, &args, false);
    assert!(!direct_sql.contains("SAVEPOINT"));
    assert_eq!(
        direct_sql,
        "SELECT \"public\".\"calculate_tax\"(50.0::numeric, 0.0825::numeric);"
    );
}

#[test]
fn static_inspect_routine_source_catches_unbounded_mutation() {
    let proc = sample_procedure();
    let findings = static_inspect_routine_source(&proc);
    assert!(!findings.is_empty());
    assert!(findings.iter().any(|f| f.code == "unbounded_mutation"));
}

#[test]
fn parse_routine_arguments_extracts_names_modes_and_defaults() {
    let raw = "IN subtotal numeric, OUT tax numeric, rate numeric DEFAULT 0.0825, flags text[]";
    let params = parse_routine_arguments(raw);
    assert_eq!(params.len(), 4);

    assert_eq!(params[0].name, "subtotal");
    assert_eq!(params[0].data_type, "numeric");
    assert_eq!(params[0].mode, ParamMode::In);
    assert!(!params[0].has_default);

    assert_eq!(params[1].name, "tax");
    assert_eq!(params[1].data_type, "numeric");
    assert_eq!(params[1].mode, ParamMode::Out);
    assert!(!params[1].has_default);

    assert_eq!(params[2].name, "rate");
    assert_eq!(params[2].data_type, "numeric");
    assert_eq!(params[2].mode, ParamMode::In);
    assert!(params[2].has_default);
    assert_eq!(params[2].default_value.as_deref(), Some("0.0825"));

    assert_eq!(params[3].name, "flags");
    assert_eq!(params[3].data_type, "text[]");
    assert_eq!(params[3].mode, ParamMode::In);
}

#[test]
fn render_routine_result_table_and_json() {
    let result = RoutineExecutionResult {
        routine_name: "public.calculate_tax".to_string(),
        success: true,
        columns: vec!["calculate_tax".to_string()],
        rows: vec![vec!["8.25".to_string()]],
        notices: vec![RoutineNotice {
            severity: "NOTICE".to_string(),
            message: "Tax calculation completed".to_string(),
            detail: None,
            hint: None,
            context: None,
        }],
        elapsed_ms: 1.42,
        rolled_back: true,
        executed_sql: "SELECT calculate_tax(100.0, 0.0825);".to_string(),
    };

    let table = render_routine_result_table(&result);
    assert!(table.contains("Status: SUCCESS"));
    assert!(table.contains("Tax calculation completed"));
    assert!(table.contains("8.25"));

    let json = render_routine_result_json(&result);
    assert!(json.contains("\"success\": true"));
    assert!(json.contains("calculate_tax"));
}
