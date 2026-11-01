use super::*;

fn bindings(names: &[&str], values: &[&str]) -> ParameterBindings {
    ParameterBindings::from_secrets(
        names.iter().map(|name| (*name).to_owned()).collect(),
        values
            .iter()
            .map(|value| SecretString::from((*value).to_owned()))
            .collect(),
    )
    .expect("valid bindings")
}

#[test]
fn discovers_only_executable_names_in_first_use_order() {
    let sql = r#"SELECT ':ignored', ":also_ignored", $$:body$$,
        :first, value::text, :first, :second
        /* :comment /* :nested */ */ -- :line
        WHERE note = E'escaped \' :literal'"#;
    let template = discover(sql).expect("template");
    assert_eq!(template.names(), ["first", "second"]);
    assert_eq!(template.occurrences().len(), 3);
    assert_eq!(template.occurrences()[0].name, "first");
}

#[test]
fn punctuation_after_a_colon_is_left_for_postgresql() {
    let template = discover("SELECT :ok, :=, :9, ::text, :_").expect("template");
    assert_eq!(template.names(), ["ok", "_"]);
}

#[test]
fn the_distinct_name_limit_is_enforced_without_partial_output() {
    let sql = (0..=MAX_DISTINCT_PARAMETERS)
        .map(|index| format!(":p{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    assert_eq!(
        discover(&sql),
        Err(ParameterError::TooManyNames {
            limit: MAX_DISTINCT_PARAMETERS
        })
    );
}

#[test]
fn binding_repeats_one_literal_and_escapes_adversarial_text() {
    let sql = "SELECT :value, :value, :empty";
    let template = discover(sql).expect("template");
    let value = "quote ' slash \\\\ newline\n tab\t unicode ✓ ; -- /* */";
    let bound = template
        .bind(sql, &bindings(&["value", "empty"], &[value, ""]))
        .expect("bound");
    assert_eq!(
        bound,
        "SELECT E'quote \\\' slash \\\\\\\\ newline\\n tab\\t unicode ✓ ; -- /* */', E'quote \\\' slash \\\\\\\\ newline\\n tab\\t unicode ✓ ; -- /* */', E''"
    );
}

#[test]
fn nul_is_refused_without_including_the_value_in_the_error() {
    let sql = "SELECT :secret";
    let template = discover(sql).expect("template");
    let secret = "safe-marker\0never-log";
    let error = template
        .bind(sql, &bindings(&["secret"], &[secret]))
        .expect_err("NUL must be refused");
    assert_eq!(
        error,
        ParameterError::NulValue {
            name: "secret".to_owned()
        }
    );
    assert!(!error.to_string().contains("never-log"));
    assert!(!format!("{error:?}").contains("never-log"));
}

#[test]
fn bindings_debug_contains_names_but_not_values() {
    let bindings = bindings(&["token"], &["synthetic-secret"]);
    let debug = format!("{bindings:?}");
    assert!(debug.contains("token"));
    assert!(debug.contains("<hidden>"));
    assert!(!debug.contains("synthetic-secret"));
}

#[test]
fn w03_adversarial_parameter_lexer_ignores_casts_comments_quotes_and_dollar_quotes() {
    let sql = r#"
        -- :not_in_line_comment
        /* :not_in_block_comment */
        SELECT
            ':not_a_param' AS lit,
            ":not_a_col" AS col,
            $$ :not_in_dollar_quote $$ AS body,
            value::text AS casted
        FROM orders
        WHERE tenant = :tenant_id
    "#;
    let template = discover(sql).expect("valid template");
    assert_eq!(template.names(), ["tenant_id"]);
    assert_eq!(template.occurrences().len(), 1);
    assert_eq!(template.occurrences()[0].name, "tenant_id");

    let bound = template
        .bind(sql, &bindings(&["tenant_id"], &["acme-corp"]))
        .expect("binds successfully");
    assert!(bound.contains("':not_a_param'"));
    assert!(bound.contains("\":not_a_col\""));
    assert!(bound.contains("$$ :not_in_dollar_quote $$"));
    assert!(bound.contains("-- :not_in_line_comment"));
    assert!(bound.contains("/* :not_in_block_comment */"));
    assert!(bound.contains("value::text"));
    assert!(bound.contains("WHERE tenant = E'acme-corp'"));
    assert!(!bound.contains(":tenant_id"));
}
