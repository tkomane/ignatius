//! Conservative source analysis and planning for a result-cell `UPDATE`.
//!
//! This is intentionally not a general SQL parser. A client-generated write is
//! safe only when the client can explain exactly which relation, column and
//! primary-key row it is using. The accepted shape is therefore one direct
//! projection from one relation. Anything more expressive is refused here,
//! before live metadata or a write effect is requested.

use crate::query::identifiers::quote_identifier;
use crate::query::parameters::{ParameterBindings, ParameterError};
use crate::query::value::Cell;
use secrecy::SecretString;
use std::fmt;

/// Maximum size of a generated update after literal binding.
pub const MAX_UPDATE_SQL_BYTES: usize = 16 * 1024;

const NEW_VALUE_PARAMETER: &str = "__ignatius_new_value";

/// A relation named by the source SQL, before an unqualified name is resolved
/// by PostgreSQL's search path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationReference {
    /// Optional explicit schema.
    pub schema: Option<String>,
    /// Relation name after SQL identifier folding or unquoting.
    pub relation: String,
}

/// The value-free mapping from result columns back to one base relation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateSource {
    /// Relation named by the source query.
    pub relation: RelationReference,
    /// Source column for every result position, in result order.
    pub projected_columns: Vec<String>,
    /// Source column selected for editing.
    pub target_column: String,
    /// Zero-based result position selected by the user.
    pub selected_result_column: usize,
}

/// The small metadata shape the planner needs from PostgreSQL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateColumn {
    /// Exact PostgreSQL column name.
    pub name: String,
    /// Whether the column belongs to the primary key.
    pub primary_key: bool,
}

/// A generated update template and its secret values.
///
/// `sql_template` contains only quoted identifiers and private parameter names.
/// The values are deliberately held separately so history and tracing can keep
/// the template while the PostgreSQL adapter binds the values at the last safe
/// boundary.
#[derive(Clone, PartialEq, Eq)]
pub struct UpdatePlan {
    /// The source column being changed.
    pub target_column: String,
    /// Primary-key columns used in the identity predicate, in order.
    pub key_columns: Vec<String>,
    /// SQL with private named placeholders and no user values.
    pub sql_template: String,
    /// Replacement and key values, hidden by Debug.
    pub parameters: ParameterBindings,
}

impl fmt::Debug for UpdatePlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UpdatePlan")
            .field("target_column", &self.target_column)
            .field("key_columns", &self.key_columns)
            .field("sql_template", &self.sql_template)
            .field("parameters", &"<hidden>")
            .finish()
    }
}

impl UpdatePlan {
    /// Derives the literal-bound statement for the review surface.
    ///
    /// The caller must never put this return value into history or tracing. It
    /// exists so the user can inspect the exact SQL that confirmation would
    /// submit; execution continues to use [`Self::sql_template`] and
    /// [`Self::parameters`].
    pub fn bound_sql(&self) -> Result<String, ParameterError> {
        crate::query::parameters::discover(&self.sql_template)
            .and_then(|template| template.bind(&self.sql_template, &self.parameters))
    }
}

/// Why a source or candidate was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateRefusal {
    /// The source is not one direct, single-table projection.
    UnsupportedSource(&'static str),
    /// The result has no matching source column at the selected position.
    SelectedColumnOutOfBounds,
    /// Result labels do not match the direct projection.
    ProjectionMismatch,
    /// The same source or output name occurs more than once.
    DuplicateColumn(String),
    /// The relation has no primary key.
    NoPrimaryKey,
    /// A primary-key column was not returned by the source query.
    PrimaryKeyNotProjected(String),
    /// A key value in the selected row is SQL NULL.
    NullPrimaryKey(String),
    /// The selected target is itself part of the primary key.
    PrimaryKeyTarget(String),
    /// The target column is not part of the live relation metadata.
    TargetColumnUnavailable(String),
    /// A value cannot cross the existing text-query boundary.
    NulValue,
    /// The generated statement exceeded the client bound.
    TooLarge { bytes: usize, limit: usize },
    /// The planner's own generated template failed its parameter contract.
    InvalidTemplate,
}

impl fmt::Display for UpdateRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSource(reason) => write!(f, "cell update is unavailable: {reason}"),
            Self::SelectedColumnOutOfBounds => {
                f.write_str("cell update is unavailable: the selected result column is stale")
            }
            Self::ProjectionMismatch => f.write_str(
                "cell update is unavailable: PostgreSQL's result columns do not match the direct projection",
            ),
            Self::DuplicateColumn(name) => write!(
                f,
                "cell update is unavailable: column {name:?} appears more than once"
            ),
            Self::NoPrimaryKey => {
                f.write_str("cell update is unavailable: the relation has no primary key")
            }
            Self::PrimaryKeyNotProjected(name) => write!(
                f,
                "cell update is unavailable: primary-key column {name:?} was not returned"
            ),
            Self::NullPrimaryKey(name) => write!(
                f,
                "cell update is unavailable: primary-key column {name:?} is NULL in this row"
            ),
            Self::PrimaryKeyTarget(name) => write!(
                f,
                "cell update is unavailable: primary-key column {name:?} cannot be edited in this slice"
            ),
            Self::TargetColumnUnavailable(name) => write!(
                f,
                "cell update is unavailable: selected column {name:?} is not writable metadata"
            ),
            Self::NulValue => f.write_str(
                "cell update is unavailable: the replacement or key value contains NUL",
            ),
            Self::TooLarge { bytes, limit } => write!(
                f,
                "cell update is unavailable: the generated statement is {bytes} bytes, over the {limit}-byte limit"
            ),
            Self::InvalidTemplate => {
                f.write_str("cell update is unavailable: the generated parameter template is invalid")
            }
        }
    }
}

/// Parses the source of a retained result and maps one selected result column
/// back to a direct source column.
pub fn parse_source(
    sql: &str,
    result_columns: &[String],
    selected_result_column: usize,
) -> Result<UpdateSource, UpdateRefusal> {
    let mut tokens = lex(sql);
    if let Some(index) = tokens
        .iter()
        .position(|token| matches!(token, Token::Symbol(';')))
    {
        if index + 1 != tokens.len() {
            return Err(UpdateRefusal::UnsupportedSource(
                "multiple statements are not editable through a result cell",
            ));
        }
        tokens.pop();
    }
    if !word_at(&tokens, 0).is_some_and(|word| word.eq_ignore_ascii_case("select")) {
        return Err(UpdateRefusal::UnsupportedSource(
            "the retained statement is not a SELECT",
        ));
    }

    let from = top_level_word(&tokens, "from").ok_or(UpdateRefusal::UnsupportedSource(
        "the SELECT has no direct FROM relation",
    ))?;
    if ["union", "intersect", "except"]
        .iter()
        .any(|word| top_level_word(&tokens, word).is_some())
    {
        return Err(UpdateRefusal::UnsupportedSource(
            "set operations are not editable through a result cell",
        ));
    }
    let parsed_relation = parse_relation(&tokens, from)?;
    let projections = parse_projections(&tokens[1..from], &parsed_relation)?;

    let projected_columns = if projections.len() == 1 && projections[0].is_star {
        if result_columns.is_empty() {
            return Err(UpdateRefusal::UnsupportedSource(
                "the result has no columns to edit",
            ));
        }
        result_columns.to_vec()
    } else {
        if projections.len() != result_columns.len() {
            return Err(UpdateRefusal::ProjectionMismatch);
        }
        for (projection, result) in projections.iter().zip(result_columns) {
            if projection.output.as_deref() != Some(result.as_str()) {
                return Err(UpdateRefusal::ProjectionMismatch);
            }
        }
        projections
            .iter()
            .map(|projection| projection.source.clone().expect("non-star projection"))
            .collect()
    };

    if selected_result_column >= projected_columns.len() {
        return Err(UpdateRefusal::SelectedColumnOutOfBounds);
    }
    if let Some(duplicate) = duplicate_name(result_columns) {
        return Err(UpdateRefusal::DuplicateColumn(duplicate.to_owned()));
    }
    if let Some(duplicate) = duplicate_name(&projected_columns) {
        return Err(UpdateRefusal::DuplicateColumn(duplicate.to_owned()));
    }

    Ok(UpdateSource {
        relation: parsed_relation.reference,
        target_column: projected_columns[selected_result_column].clone(),
        projected_columns,
        selected_result_column,
    })
}

/// Plans one update after live relation metadata has been read.
pub fn plan_update(
    source: &UpdateSource,
    resolved_schema: &str,
    resolved_relation: &str,
    result_columns: &[String],
    selected_row: &[Cell],
    relation_columns: &[UpdateColumn],
    replacement: &str,
) -> Result<UpdatePlan, UpdateRefusal> {
    if source.selected_result_column >= result_columns.len()
        || source.selected_result_column >= selected_row.len()
    {
        return Err(UpdateRefusal::SelectedColumnOutOfBounds);
    }
    if source.projected_columns.len() != result_columns.len() {
        return Err(UpdateRefusal::ProjectionMismatch);
    }

    let keys: Vec<&UpdateColumn> = relation_columns
        .iter()
        .filter(|column| column.primary_key)
        .collect();
    if keys.is_empty() {
        return Err(UpdateRefusal::NoPrimaryKey);
    }

    let target = relation_columns
        .iter()
        .find(|column| column.name == source.target_column)
        .ok_or_else(|| UpdateRefusal::TargetColumnUnavailable(source.target_column.clone()))?;
    if target.primary_key {
        return Err(UpdateRefusal::PrimaryKeyTarget(target.name.clone()));
    }

    let mut key_values = Vec::with_capacity(keys.len());
    for key in keys {
        let result_index = source
            .projected_columns
            .iter()
            .position(|column| column == &key.name)
            .ok_or_else(|| UpdateRefusal::PrimaryKeyNotProjected(key.name.clone()))?;
        let cell = selected_row
            .get(result_index)
            .ok_or(UpdateRefusal::SelectedColumnOutOfBounds)?;
        let value = cell
            .raw()
            .ok_or_else(|| UpdateRefusal::NullPrimaryKey(key.name.clone()))?;
        key_values.push((key.name.clone(), value.to_owned()));
    }

    let mut parameter_names = vec![NEW_VALUE_PARAMETER.to_owned()];
    let mut parameter_values = vec![SecretString::from(replacement.to_owned())];
    let mut predicate = Vec::with_capacity(key_values.len());
    for (index, (name, value)) in key_values.iter().enumerate() {
        let parameter = format!("__ignatius_key_{index}");
        parameter_names.push(parameter.clone());
        parameter_values.push(SecretString::from(value.clone()));
        predicate.push(format!("{} = :{parameter}", quote_identifier(name)));
    }

    let sql_template = format!(
        "UPDATE {}.{} SET {} = :{} WHERE {}",
        quote_identifier(resolved_schema),
        quote_identifier(resolved_relation),
        quote_identifier(&target.name),
        NEW_VALUE_PARAMETER,
        predicate.join(" AND ")
    );
    let parameters = ParameterBindings::from_secrets(parameter_names, parameter_values)
        .map_err(|_| UpdateRefusal::InvalidTemplate)?;
    let bound = crate::query::parameters::discover(&sql_template)
        .and_then(|template| template.bind(&sql_template, &parameters))
        .map_err(|error| match error {
            ParameterError::NulValue { .. } => UpdateRefusal::NulValue,
            _ => UpdateRefusal::InvalidTemplate,
        })?;
    if bound.len() > MAX_UPDATE_SQL_BYTES {
        return Err(UpdateRefusal::TooLarge {
            bytes: bound.len(),
            limit: MAX_UPDATE_SQL_BYTES,
        });
    }

    Ok(UpdatePlan {
        target_column: target.name.clone(),
        key_columns: key_values.into_iter().map(|(name, _)| name).collect(),
        sql_template,
        parameters,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Projection {
    source: Option<String>,
    output: Option<String>,
    is_star: bool,
}

struct ParsedRelation {
    reference: RelationReference,
    qualifier: String,
}

fn parse_relation(tokens: &[Token], from: usize) -> Result<ParsedRelation, UpdateRefusal> {
    let mut index = from + 1;
    if word_at(tokens, index).is_some_and(|word| word.eq_ignore_ascii_case("only")) {
        return Err(UpdateRefusal::UnsupportedSource(
            "FROM ONLY is not editable through a result cell because an update can reach inherited rows",
        ));
    }
    let Some(first) = identifier_at(tokens, index) else {
        return Err(UpdateRefusal::UnsupportedSource(
            "FROM must name one base relation",
        ));
    };
    index += 1;
    let (schema, relation) = if symbol_at(tokens, index) == Some('.') {
        let Some(second) = identifier_at(tokens, index + 1) else {
            return Err(UpdateRefusal::UnsupportedSource(
                "the schema-qualified relation is incomplete",
            ));
        };
        index += 2;
        (Some(first), second)
    } else {
        (None, first)
    };

    // An alias is useful for a direct projection, but it may not hide another
    // source. The clause check below deliberately names joins and commas.
    let qualifier = if word_at(tokens, index).is_some_and(|word| word.eq_ignore_ascii_case("as")) {
        let Some(alias) = identifier_at(tokens, index + 1) else {
            return Err(UpdateRefusal::UnsupportedSource(
                "the relation alias is incomplete",
            ));
        };
        index += 2;
        alias
    } else if identifier_at(tokens, index).is_some_and(|name| !is_clause_word(&name)) {
        let alias = identifier_at(tokens, index).expect("checked above");
        index += 1;
        alias
    } else {
        relation.clone()
    };

    if symbol_at(tokens, index) == Some(',') {
        return Err(UpdateRefusal::UnsupportedSource(
            "multiple FROM relations are not editable",
        ));
    }
    if word_at(tokens, index).is_some_and(|word| {
        matches!(
            word.to_ascii_lowercase().as_str(),
            "join" | "left" | "right" | "full" | "inner" | "cross" | "lateral"
        )
    }) {
        return Err(UpdateRefusal::UnsupportedSource(
            "joins are not editable through a result cell",
        ));
    }
    if index < tokens.len() && !is_clause_token(tokens, index) {
        return Err(UpdateRefusal::UnsupportedSource(
            "the FROM clause contains more than one source",
        ));
    }

    Ok(ParsedRelation {
        reference: RelationReference { schema, relation },
        qualifier,
    })
}

fn parse_projections(
    tokens: &[Token],
    relation: &ParsedRelation,
) -> Result<Vec<Projection>, UpdateRefusal> {
    let parts = split_top_level(tokens)?;
    if parts.is_empty() {
        return Err(UpdateRefusal::UnsupportedSource(
            "the SELECT has no direct projection",
        ));
    }
    let alias = relation.qualifier.as_str();
    let mut projections = Vec::with_capacity(parts.len());
    for part in parts {
        if part.len() == 1 && symbol_at(part, 0) == Some('*') {
            projections.push(Projection {
                source: None,
                output: None,
                is_star: true,
            });
            continue;
        }
        if part.len() == 3
            && identifier_at(part, 0).is_some()
            && symbol_at(part, 1) == Some('.')
            && symbol_at(part, 2) == Some('*')
        {
            let qualifier = identifier_at(part, 0).expect("checked above");
            if qualifier != alias {
                return Err(UpdateRefusal::UnsupportedSource(
                    "the star projection names another relation",
                ));
            }
            projections.push(Projection {
                source: None,
                output: None,
                is_star: true,
            });
            continue;
        }

        let Some(first) = identifier_at(part, 0) else {
            return Err(UpdateRefusal::UnsupportedSource(
                "computed expressions are not editable",
            ));
        };
        let (qualifier, source, mut index) = if symbol_at(part, 1) == Some('.') {
            let Some(second) = identifier_at(part, 2) else {
                return Err(UpdateRefusal::UnsupportedSource(
                    "a qualified source column is incomplete",
                ));
            };
            (Some(first), second, 3)
        } else {
            (None, first, 1)
        };
        if qualifier.is_some_and(|value| value != alias) {
            return Err(UpdateRefusal::UnsupportedSource(
                "the projection names another relation",
            ));
        }

        let output = if word_at(part, index).is_some_and(|word| word.eq_ignore_ascii_case("as")) {
            let Some(output) = identifier_at(part, index + 1) else {
                return Err(UpdateRefusal::UnsupportedSource(
                    "a column alias is incomplete",
                ));
            };
            index += 2;
            output
        } else if let Some(candidate) = identifier_at(part, index)
            && !is_clause_word(&candidate)
        {
            index += 1;
            candidate
        } else {
            source.clone()
        };
        if index != part.len() {
            return Err(UpdateRefusal::UnsupportedSource(
                "computed expressions are not editable",
            ));
        }
        projections.push(Projection {
            source: Some(source),
            output: Some(output),
            is_star: false,
        });
    }
    if projections
        .iter()
        .filter(|projection| projection.is_star)
        .count()
        > 0
        && projections.len() != 1
    {
        return Err(UpdateRefusal::UnsupportedSource(
            "star must be the only projection",
        ));
    }
    Ok(projections)
}

fn duplicate_name(names: &[String]) -> Option<&str> {
    names.iter().enumerate().find_map(|(index, name)| {
        names[..index]
            .iter()
            .any(|candidate| candidate == name)
            .then_some(name.as_str())
    })
}

fn top_level_word(tokens: &[Token], wanted: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate() {
        match token {
            Token::Symbol('(') => depth += 1,
            Token::Symbol(')') => depth = depth.saturating_sub(1),
            Token::Word(word) if depth == 0 && word.eq_ignore_ascii_case(wanted) => {
                return Some(index);
            }
            _ => {}
        }
    }
    None
}

fn split_top_level(tokens: &[Token]) -> Result<Vec<&[Token]>, UpdateRefusal> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate() {
        match token {
            Token::Symbol('(') => depth += 1,
            Token::Symbol(')') => {
                if depth == 0 {
                    return Err(UpdateRefusal::UnsupportedSource(
                        "the source projection has unbalanced parentheses",
                    ));
                }
                depth -= 1;
            }
            Token::Symbol(',') if depth == 0 => {
                if start == index {
                    return Err(UpdateRefusal::UnsupportedSource(
                        "the source projection contains an empty item",
                    ));
                }
                parts.push(&tokens[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err(UpdateRefusal::UnsupportedSource(
            "the source projection has unbalanced parentheses",
        ));
    }
    if start == tokens.len() {
        return Err(UpdateRefusal::UnsupportedSource(
            "the source projection contains an empty item",
        ));
    }
    parts.push(&tokens[start..]);
    Ok(parts)
}

fn identifier_at(tokens: &[Token], index: usize) -> Option<String> {
    match tokens.get(index)? {
        Token::Ident(name) => Some(name.clone()),
        Token::Word(word) if !is_clause_word(word) => Some(word.to_ascii_lowercase()),
        _ => None,
    }
}

fn word_at(tokens: &[Token], index: usize) -> Option<&str> {
    match tokens.get(index)? {
        Token::Word(word) => Some(word),
        _ => None,
    }
}

fn symbol_at(tokens: &[Token], index: usize) -> Option<char> {
    match tokens.get(index)? {
        Token::Symbol(symbol) => Some(*symbol),
        _ => None,
    }
}

fn is_clause_token(tokens: &[Token], index: usize) -> bool {
    word_at(tokens, index).is_some_and(is_clause_word)
}

fn is_clause_word(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "where"
            | "order"
            | "group"
            | "having"
            | "limit"
            | "offset"
            | "fetch"
            | "for"
            | "window"
            | "union"
            | "intersect"
            | "except"
            | "join"
            | "left"
            | "right"
            | "full"
            | "inner"
            | "outer"
            | "cross"
            | "lateral"
            | "as"
            | "distinct"
            | "all"
            | "into"
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Word(String),
    Ident(String),
    Symbol(char),
    Literal,
}

fn lex(sql: &str) -> Vec<Token> {
    let bytes = sql.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            byte if byte.is_ascii_whitespace() => index += 1,
            b'\'' => {
                index = crate::query::statements::skip_single_quoted(sql, index);
                tokens.push(Token::Literal);
            }
            b'"' => {
                let end = crate::query::statements::skip_double_quoted(sql, index);
                let raw = &sql[index + 1..end.saturating_sub(1).min(sql.len())];
                tokens.push(Token::Ident(raw.replace("\"\"", "\"")));
                index = end;
            }
            b'$' => {
                let end = crate::query::statements::skip_dollar_quoted(sql, index);
                if end > index {
                    tokens.push(Token::Literal);
                    index = end;
                } else {
                    tokens.push(Token::Symbol('$'));
                    index += 1;
                }
            }
            b'-' if bytes.get(index + 1) == Some(&b'-') => {
                index = crate::query::statements::skip_line_comment(sql, index);
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index = crate::query::statements::skip_block_comment(sql, index);
            }
            byte if byte.is_ascii_alphabetic() || byte == b'_' || byte >= 0x80 => {
                let start = index;
                index += 1;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric()
                        || bytes[index] == b'_'
                        || bytes[index] >= 0x80)
                {
                    index += 1;
                }
                tokens.push(Token::Word(sql[start..index].to_owned()));
            }
            byte if byte.is_ascii_punctuation() => {
                tokens.push(Token::Symbol(byte as char));
                index += 1;
            }
            _ => {
                let end = sql[index..]
                    .char_indices()
                    .nth(1)
                    .map_or(sql.len(), |(offset, _)| index + offset);
                tokens.push(Token::Symbol('?'));
                index = end;
            }
        }
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn columns() -> Vec<UpdateColumn> {
        vec![
            UpdateColumn {
                name: "order_id".into(),
                primary_key: true,
            },
            UpdateColumn {
                name: "note".into(),
                primary_key: false,
            },
            UpdateColumn {
                name: "total".into(),
                primary_key: false,
            },
        ]
    }

    fn source(sql: &str, result_columns: &[&str], selected: usize) -> UpdateSource {
        parse_source(
            sql,
            &result_columns
                .iter()
                .map(|column| (*column).to_owned())
                .collect::<Vec<_>>(),
            selected,
        )
        .expect("source")
    }

    fn plan(replacement: &str) -> UpdatePlan {
        let result_columns = vec!["order_id".to_owned(), "note".to_owned()];
        let selected = source(
            "SELECT order_id, note FROM public.orders WHERE order_id = 1",
            &["order_id", "note"],
            1,
        );
        plan_update(
            &selected,
            "public",
            "orders",
            &result_columns,
            &[Cell::Text("1".into()), Cell::Text("old".into())],
            &columns(),
            replacement,
        )
        .expect("plan")
    }

    #[test]
    fn parses_a_direct_schema_qualified_projection_and_alias() {
        let source = source(
            "SELECT o.order_id, o.note AS customer_note FROM public.orders o WHERE o.order_id = 1 ORDER BY o.order_id",
            &["order_id", "customer_note"],
            1,
        );
        assert_eq!(source.relation.schema.as_deref(), Some("public"));
        assert_eq!(source.relation.relation, "orders");
        assert_eq!(source.projected_columns, ["order_id", "note"]);
        assert_eq!(source.target_column, "note");
    }

    #[test]
    fn star_uses_the_server_result_shape_and_bare_names_remain_unresolved() {
        let source = source("SELECT * FROM orders", &["order_id", "note"], 1);
        assert_eq!(source.relation.schema, None);
        assert_eq!(source.projected_columns, ["order_id", "note"]);
    }

    #[test]
    fn comments_and_quoted_identifiers_do_not_change_the_source_shape() {
        let source = source(
            "/* read */ SELECT \"order_id\", \"note\" FROM \"public\".\"orders\"",
            &["order_id", "note"],
            0,
        );
        assert_eq!(source.relation.schema.as_deref(), Some("public"));
        assert_eq!(source.relation.relation, "orders");
        assert_eq!(source.target_column, "order_id");
    }

    #[test]
    fn from_only_is_refused_without_breaking_a_quoted_only_relation() {
        for sql in [
            "SELECT id, note FROM ONLY orders",
            "SELECT id, note FROM ONLY public.orders",
            "SELECT id, note FROM ONLY orders AS o",
            "SELECT id, note FROM ONLY orders o",
            "SELECT id, note FROM ONLY (public.orders)",
        ] {
            assert_eq!(
                parse_source(sql, &["id".into(), "note".into()], 0),
                Err(UpdateRefusal::UnsupportedSource(
                    "FROM ONLY is not editable through a result cell because an update can reach inherited rows"
                )),
                "{sql} must be refused as FROM ONLY"
            );
        }

        let quoted = source("SELECT id, note FROM \"only\"", &["id", "note"], 0);
        assert_eq!(quoted.relation.schema, None);
        assert_eq!(quoted.relation.relation, "only");

        for (sql, relation) in [
            ("SELECT id, note FROM onlys", "onlys"),
            ("SELECT id, note FROM only_t", "only_t"),
        ] {
            let accepted = source(sql, &["id", "note"], 0);
            assert_eq!(accepted.relation.relation, relation, "{sql} is not ONLY");
        }
    }

    #[test]
    fn joins_expressions_ctes_and_set_operations_are_refused() {
        for sql in [
            "SELECT o.id, o.note FROM orders o JOIN notes n ON n.id = o.id",
            "SELECT id, lower(note) FROM orders",
            "WITH rows AS (SELECT id, note FROM orders) SELECT id, note FROM rows",
            "SELECT id, note FROM orders UNION SELECT id, note FROM archived_orders",
            "SELECT id, note FROM orders WHERE id = 1; DELETE FROM orders",
        ] {
            assert!(
                parse_source(sql, &["id".into(), "note".into()], 1).is_err(),
                "{sql} must be refused"
            );
        }
        assert!(
            parse_source(
                "SELECT id, note FROM orders;",
                &["id".into(), "note".into()],
                1
            )
            .is_ok()
        );
    }

    #[test]
    fn duplicate_and_mismatched_result_columns_are_refused() {
        assert!(matches!(
            parse_source("SELECT id, id FROM orders", &["id".into(), "id".into()], 0),
            Err(UpdateRefusal::DuplicateColumn(_))
        ));
        assert_eq!(
            parse_source(
                "SELECT id, note FROM orders",
                &["id".into(), "renamed".into()],
                0
            ),
            Err(UpdateRefusal::ProjectionMismatch)
        );
    }

    #[test]
    fn missing_and_null_primary_keys_are_refused() {
        let selected = source("SELECT note FROM orders", &["note"], 0);
        assert_eq!(
            plan_update(
                &selected,
                "public",
                "orders",
                &["note".into()],
                &[Cell::Text("old".into())],
                &columns(),
                "new",
            ),
            Err(UpdateRefusal::PrimaryKeyNotProjected("order_id".into()))
        );

        let source = source(
            "SELECT order_id, note FROM orders",
            &["order_id", "note"],
            1,
        );
        assert_eq!(
            plan_update(
                &source,
                "public",
                "orders",
                &["order_id".into(), "note".into()],
                &[Cell::Null, Cell::Text("old".into())],
                &columns(),
                "new",
            ),
            Err(UpdateRefusal::NullPrimaryKey("order_id".into()))
        );
    }

    #[test]
    fn primary_key_cell_is_not_an_editable_target() {
        let source = source(
            "SELECT order_id, note FROM orders",
            &["order_id", "note"],
            0,
        );
        assert_eq!(
            plan_update(
                &source,
                "public",
                "orders",
                &["order_id".into(), "note".into()],
                &[Cell::Text("1".into()), Cell::Text("old".into())],
                &columns(),
                "2",
            ),
            Err(UpdateRefusal::PrimaryKeyTarget("order_id".into()))
        );
    }

    #[test]
    fn plan_quotes_identifiers_binds_literal_values_and_keeps_debug_safe() {
        let value = "new ' note \\\\ line\n ; --";
        let plan = plan(value);
        assert_eq!(
            plan.sql_template,
            "UPDATE \"public\".\"orders\" SET \"note\" = :__ignatius_new_value WHERE \"order_id\" = :__ignatius_key_0"
        );
        let bound = plan.bound_sql().expect("bound SQL");
        assert!(bound.contains("new \\\' note"), "{bound}");
        assert!(bound.contains("\\\\ line\\n"), "{bound}");
        let debug = format!("{plan:?}");
        assert!(debug.contains("sql_template"));
        assert!(!debug.contains("new ' note"));
        assert!(!debug.contains("old"));
    }

    #[test]
    fn empty_values_are_valid_but_nul_and_oversized_values_are_refused() {
        let empty = plan("");
        assert!(
            empty
                .bound_sql()
                .expect("empty literal")
                .contains("\"note\" = E''")
        );
        // The planner intentionally does not expose the value in the refusal.
        let refusal = plan_update(
            &source(
                "SELECT order_id, note FROM orders",
                &["order_id", "note"],
                1,
            ),
            "public",
            "orders",
            &["order_id".into(), "note".into()],
            &[Cell::Text("1".into()), Cell::Text("old".into())],
            &columns(),
            "safe\0marker",
        )
        .expect_err("NUL");
        assert_eq!(refusal, UpdateRefusal::NulValue);
        assert!(!format!("{refusal:?}").contains("safe"));
        let refusal = plan_update(
            &source(
                "SELECT order_id, note FROM orders",
                &["order_id", "note"],
                1,
            ),
            "public",
            "orders",
            &["order_id".into(), "note".into()],
            &[Cell::Text("1".into()), Cell::Text("old".into())],
            &columns(),
            &"x".repeat(MAX_UPDATE_SQL_BYTES),
        )
        .expect_err("bound");
        assert!(matches!(refusal, UpdateRefusal::TooLarge { .. }));
    }
}
