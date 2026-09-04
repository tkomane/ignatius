//! Pure SQL completion semantics.
//!
//! This module deliberately knows nothing about a terminal, a database client,
//! or a clock. PostgreSQL metadata is reduced to [`CompletionCatalog`] by the
//! adapter, and this module turns that snapshot plus an editor buffer into
//! candidates. The same operation is therefore safe to use in the TUI and in
//! plain mode.

use crate::query::highlight::{self, TokenKind};
use crate::query::identifiers::quote_identifier;
use std::cmp::Ordering;
use std::ops::Range;

/// The maximum number of candidates drawn or printed at once.
pub const MAX_VISIBLE_CANDIDATES: usize = 12;

/// A PostgreSQL object represented in a catalogue snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CatalogObjectKind {
    /// An ordinary table.
    Table,
    /// A view.
    View,
    /// A materialized view.
    MaterializedView,
    /// A sequence.
    Sequence,
    /// A foreign table.
    ForeignTable,
    /// A partitioned table.
    PartitionedTable,
    /// A function or procedure.
    Function,
}

impl CatalogObjectKind {
    /// A stable word for the UI and plain output.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::View => "view",
            Self::MaterializedView => "materialized view",
            Self::Sequence => "sequence",
            Self::ForeignTable => "foreign table",
            Self::PartitionedTable => "partitioned table",
            Self::Function => "function",
        }
    }

    const fn rank(self) -> u8 {
        match self {
            Self::Table => 0,
            Self::PartitionedTable => 1,
            Self::View => 2,
            Self::MaterializedView => 3,
            Self::ForeignTable => 4,
            Self::Sequence => 5,
            Self::Function => 6,
        }
    }

    /// Builds an object kind from PostgreSQL's `pg_class.relkind` value.
    #[must_use]
    pub fn from_relkind(value: &str) -> Option<Self> {
        match value {
            "r" => Some(Self::Table),
            "v" => Some(Self::View),
            "m" => Some(Self::MaterializedView),
            "S" => Some(Self::Sequence),
            "f" => Some(Self::ForeignTable),
            "p" => Some(Self::PartitionedTable),
            _ => None,
        }
    }
}

/// One object in a completion catalogue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogObject {
    /// What it is.
    pub kind: CatalogObjectKind,
    /// Schema containing the object.
    pub schema: String,
    /// Exact PostgreSQL object name.
    pub name: String,
    /// Whether the current role may read or execute it.
    pub readable: bool,
    /// Function return type or another short detail.
    pub detail: Option<String>,
}

/// One column in a relation held by the snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogColumn {
    /// Exact PostgreSQL column name.
    pub name: String,
    /// PostgreSQL's formatted data type.
    pub data_type: String,
}

/// One relation and its columns in a completion catalogue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogRelation {
    /// Schema containing the relation.
    pub schema: String,
    /// Exact PostgreSQL relation name.
    pub name: String,
    /// Columns in attribute order.
    pub columns: Vec<CatalogColumn>,
}

/// A read-only snapshot used for local completion.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompletionCatalog {
    /// Schema-qualified objects, including functions.
    pub objects: Vec<CatalogObject>,
    /// Relations and their columns.
    pub relations: Vec<CatalogRelation>,
}

impl CompletionCatalog {
    /// Whether a schema exists in this snapshot.
    #[must_use]
    pub fn has_schema(&self, name: &str) -> bool {
        self.objects
            .iter()
            .any(|object| same_identifier(&object.schema, name))
            || self
                .relations
                .iter()
                .any(|relation| same_identifier(&relation.schema, name))
    }

    /// Finds a relation by schema and name using SQL's bare-name comparison.
    #[must_use]
    pub fn relation(&self, schema: Option<&str>, name: &str) -> Option<&CatalogRelation> {
        self.relations.iter().find(|relation| {
            schema.is_none_or(|schema| same_identifier(&relation.schema, schema))
                && same_identifier(&relation.name, name)
        })
    }

    /// Returns all relations matching a relation name when no schema was given.
    #[must_use]
    pub fn relations_named(&self, name: &str) -> Vec<&CatalogRelation> {
        self.relations
            .iter()
            .filter(|relation| same_identifier(&relation.name, name))
            .collect()
    }
}

/// The semantic group a completion menu is answering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionScope {
    /// SQL words such as SELECT and WHERE.
    Keywords,
    /// Relations and other objects usable after FROM or JOIN.
    Relations,
    /// Columns available in the current expression scope.
    Columns,
    /// Objects restricted to one schema.
    Schema(String),
    /// Columns restricted to one relation or alias.
    QualifiedColumns(String),
    /// Both useful SQL words and context objects are relevant.
    Mixed,
}

impl CompletionScope {
    /// Human-readable context for a menu heading or plain output.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Keywords => "SQL keywords".to_owned(),
            Self::Relations => "tables, views, and functions".to_owned(),
            Self::Columns => "columns in scope".to_owned(),
            Self::Schema(schema) => format!("objects in schema {schema}"),
            Self::QualifiedColumns(name) => format!("columns from {name}"),
            Self::Mixed => "SQL keywords and objects".to_owned(),
        }
    }
}

/// The kind of one completion candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CandidateKind {
    /// A SQL keyword.
    Keyword,
    /// A common table expression declared in this statement.
    Cte,
    /// A relation column.
    Column,
    /// A catalogue object kind.
    Object(CatalogObjectKind),
}

impl CandidateKind {
    /// A stable word for the UI and plain output.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Keyword => "keyword",
            Self::Cte => "CTE",
            Self::Column => "column",
            Self::Object(kind) => kind.label(),
        }
    }

    const fn rank(self) -> u8 {
        match self {
            Self::Cte => 0,
            Self::Column => 1,
            Self::Object(kind) => 2 + kind.rank(),
            Self::Keyword => 20,
        }
    }
}

/// One insertion candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// Exact stored name or uppercase keyword shown to the user.
    pub label: String,
    /// Text inserted on acceptance.
    pub insert_text: String,
    /// What kind of thing it is.
    pub kind: CandidateKind,
    /// Source schema, when there is one.
    pub schema: Option<String>,
    /// Relation source for a column, when there is one.
    pub source: Option<String>,
    /// Type, return value, or permission detail.
    pub detail: Option<String>,
}

impl Candidate {
    /// Whether this candidate is an exact name match for a plain-mode choice.
    #[must_use]
    pub fn exact_name(&self, name: &str) -> bool {
        self.label == name || self.label.eq_ignore_ascii_case(name)
    }

    /// A stable, all-words detail line for plain mode.
    #[must_use]
    pub fn plain_detail(&self) -> String {
        let mut parts = vec![self.kind.label().to_owned()];
        if let Some(schema) = &self.schema {
            parts.push(format!("schema {schema}"));
        }
        if let Some(source) = &self.source {
            parts.push(format!("from {source}"));
        }
        if let Some(detail) = &self.detail {
            parts.push(detail.clone());
        }
        parts.join(", ")
    }
}

/// The pure answer to a completion request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionResult {
    /// The exact range to replace on acceptance.
    pub replacement: Range<usize>,
    /// The word the user has typed so far.
    pub prefix: String,
    /// The semantic context of the list.
    pub scope: CompletionScope,
    /// Visible candidates, bounded by [`MAX_VISIBLE_CANDIDATES`].
    pub candidates: Vec<Candidate>,
    /// Candidates matching the prefix before the visible bound.
    pub matching_count: usize,
    /// Candidates available in the context before prefix filtering.
    pub total_count: usize,
}

/// Computes completion from the current SQL buffer and cursor.
///
/// `catalog` is optional because keywords remain useful while the metadata
/// snapshot is loading or unavailable. The caller supplies the honest status
/// wording for those cases; this function never turns a missing catalogue into
/// a claim that the database has no objects.
#[must_use]
pub fn complete(sql: &str, cursor: usize, catalog: Option<&CompletionCatalog>) -> CompletionResult {
    complete_with_prefix(sql, cursor, catalog, None)
}

/// Computes completion with an optional explicit prefix override.
///
/// Plain mode has no cursor editing, so `\\complete ord` can ask for the same
/// context with a supplied prefix while the TUI always uses the bytes before
/// its cursor. The replacement range remains the analyzer's range either way.
#[must_use]
pub fn complete_with_prefix(
    sql: &str,
    cursor: usize,
    catalog: Option<&CompletionCatalog>,
    prefix_override: Option<&str>,
) -> CompletionResult {
    let context = context(sql, cursor);
    if context.in_literal_or_comment {
        return CompletionResult {
            replacement: context.replacement,
            prefix: context.prefix,
            scope: CompletionScope::Keywords,
            candidates: Vec::new(),
            matching_count: 0,
            total_count: 0,
        };
    }

    let lexemes = lexemes(sql);
    let before_all = lexemes_before(&lexemes, context.cursor);
    let before_start = before_all
        .iter()
        .rposition(|lexeme| lexeme.kind == LexemeKind::Symbol && lexeme.text == ";")
        .map_or(0, |index| index + 1);
    let before = before_all[before_start..].to_vec();
    let refs = relation_refs(&before);
    let ctes = cte_names(&before);
    let (scope, mut candidates) =
        candidates_for_context(sql, &before, &context, &refs, &ctes, catalog);

    let display_prefix = prefix_override.map_or_else(|| context.prefix.clone(), ToOwned::to_owned);
    let prefix = display_prefix.to_ascii_lowercase();
    let total_count = candidates.len();
    candidates.retain(|candidate| candidate.label.to_ascii_lowercase().starts_with(&prefix));
    candidates.sort_by(|left, right| compare_candidates(left, right, &prefix));
    let matching_count = candidates.len();
    candidates.truncate(MAX_VISIBLE_CANDIDATES);

    CompletionResult {
        replacement: context.replacement,
        prefix: display_prefix,
        scope,
        candidates,
        matching_count,
        total_count,
    }
}

/// Applies a UTF-8-safe replacement to a string.
///
/// Editor and plain mode both call this operation so acceptance has one range
/// validation rule. A false result means the range was invalid or already held
/// the replacement and the caller must not create an undo step or claim success.
pub fn replace_range(text: &mut String, range: Range<usize>, replacement: &str) -> bool {
    if range.start > range.end
        || range.end > text.len()
        || !text.is_char_boundary(range.start)
        || !text.is_char_boundary(range.end)
        || &text[range.clone()] == replacement
    {
        return false;
    }
    text.replace_range(range, replacement);
    true
}

/// The lexical/context portion of a completion request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionContext {
    /// Cursor at a valid UTF-8 boundary.
    pub cursor: usize,
    /// Exact word or partial quoted identifier range.
    pub replacement: Range<usize>,
    /// User-entered prefix without the opening quote.
    pub prefix: String,
    /// True when the cursor is in a protected SQL region.
    pub in_literal_or_comment: bool,
}

/// Finds the current replacement range and rejects strings/comments.
#[must_use]
pub fn context(sql: &str, cursor: usize) -> CompletionContext {
    let cursor = clamp_cursor(sql, cursor);
    let syntax = highlight::tokens(sql);
    let protected = syntax.iter().any(|token| {
        matches!(token.kind, TokenKind::Literal | TokenKind::Comment)
            && ((cursor > token.start && cursor <= token.end)
                || (cursor == token.start && token.start < sql.len()))
    });

    let lexemes = lexemes_from_tokens(sql, &syntax);
    let current = lexemes.iter().find(|lexeme| {
        matches!(lexeme.kind, LexemeKind::Word | LexemeKind::Quoted)
            && ((lexeme.start < cursor && cursor < lexeme.end) || lexeme.end == cursor)
    });

    let (replacement, prefix) = match current {
        Some(lexeme) if lexeme.kind == LexemeKind::Quoted => {
            let raw = &sql[lexeme.start..cursor];
            (lexeme.start..cursor, quoted_prefix(raw))
        }
        Some(lexeme) => (lexeme.start..cursor, sql[lexeme.start..cursor].to_owned()),
        None => (cursor..cursor, String::new()),
    };

    CompletionContext {
        cursor,
        replacement,
        prefix,
        in_literal_or_comment: protected,
    }
}

// ---------------------------------------------------------------------------
// Context extraction

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LexemeKind {
    Word,
    Quoted,
    Symbol,
    Protected,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Lexeme {
    start: usize,
    end: usize,
    kind: LexemeKind,
    text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RelationRef {
    schema: Option<String>,
    name: String,
    alias: Option<String>,
    subquery: bool,
}

fn lexemes(sql: &str) -> Vec<Lexeme> {
    let syntax = highlight::tokens(sql);
    lexemes_from_tokens(sql, &syntax)
}

fn lexemes_from_tokens(sql: &str, syntax: &[highlight::Token]) -> Vec<Lexeme> {
    let bytes = sql.as_bytes();
    let mut result = Vec::new();
    let mut token_index = 0usize;
    let mut i = 0usize;

    while i < bytes.len() {
        while token_index < syntax.len() && syntax[token_index].end <= i {
            token_index += 1;
        }
        if let Some(token) = syntax.get(token_index).filter(|token| token.start == i) {
            let kind = match token.kind {
                TokenKind::Literal | TokenKind::Comment => LexemeKind::Protected,
                TokenKind::Identifier if !sql[token.start..token.end].starts_with('$') => {
                    LexemeKind::Quoted
                }
                TokenKind::Keyword => LexemeKind::Word,
                _ => LexemeKind::Other,
            };
            result.push(Lexeme {
                start: token.start,
                end: token.end,
                kind,
                text: sql[token.start..token.end].to_owned(),
            });
            i = token.end.max(i + 1);
            continue;
        }

        let byte = bytes[i];
        if byte.is_ascii_whitespace() {
            i += 1;
        } else if highlight::is_word_start(byte) {
            let mut end = i + 1;
            while end < bytes.len() && highlight::is_word_byte(bytes[end]) {
                end += 1;
            }
            result.push(Lexeme {
                start: i,
                end,
                kind: LexemeKind::Word,
                text: sql[i..end].to_owned(),
            });
            i = end;
        } else if byte.is_ascii_punctuation() || byte == b'\x7f' {
            result.push(Lexeme {
                start: i,
                end: i + 1,
                kind: LexemeKind::Symbol,
                text: sql[i..i + 1].to_owned(),
            });
            i += 1;
        } else {
            let end = sql[i..]
                .char_indices()
                .nth(1)
                .map_or(sql.len(), |(offset, _)| i + offset);
            result.push(Lexeme {
                start: i,
                end,
                kind: LexemeKind::Other,
                text: sql[i..end].to_owned(),
            });
            i = end;
        }
    }
    result
}

fn lexemes_before(lexemes: &[Lexeme], cursor: usize) -> Vec<&Lexeme> {
    lexemes
        .iter()
        .filter(|lexeme| lexeme.start < cursor || lexeme.end <= cursor)
        .collect()
}

fn relation_refs(lexemes: &[&Lexeme]) -> Vec<RelationRef> {
    let mut refs = Vec::new();
    for (index, lexeme) in lexemes.iter().enumerate() {
        if lexeme.kind != LexemeKind::Word {
            continue;
        }
        let keyword = lexeme_text(lexeme).to_ascii_lowercase();
        if !matches!(
            keyword.as_str(),
            "from" | "join" | "update" | "into" | "references"
        ) {
            continue;
        }
        let mut cursor = index + 1;
        if word_at(lexemes, cursor).is_some_and(|word| word.eq_ignore_ascii_case("lateral")) {
            cursor += 1;
        }
        if symbol_at(lexemes, cursor) == Some("(") {
            if let Some(close) = matching_paren(lexemes, cursor) {
                let alias = alias_after(lexemes, close + 1);
                refs.push(RelationRef {
                    schema: None,
                    name: String::new(),
                    alias,
                    subquery: true,
                });
            }
            continue;
        }
        let Some(first) = name_at(lexemes, cursor) else {
            continue;
        };
        cursor = first.1;
        let (schema, name) = if symbol_at(lexemes, cursor) == Some(".") {
            if let Some(second) = name_at(lexemes, cursor + 1) {
                (Some(first.0), second.0)
            } else {
                (None, first.0)
            }
        } else {
            (None, first.0)
        };
        if schema.is_some() {
            cursor += 2;
        }
        let alias = alias_after(lexemes, cursor);
        refs.push(RelationRef {
            schema,
            name,
            alias,
            subquery: false,
        });
    }
    refs
}

fn alias_after(lexemes: &[&Lexeme], mut index: usize) -> Option<String> {
    if word_at(lexemes, index).is_some_and(|word| word.eq_ignore_ascii_case("as")) {
        index += 1;
    }
    let candidate = name_at(lexemes, index)?;
    let text = candidate.0;
    if !highlight::is_keyword(&text) {
        Some(text)
    } else {
        None
    }
}

fn cte_names(lexemes: &[&Lexeme]) -> Vec<String> {
    let Some(with) = lexemes.iter().position(|lexeme| {
        lexeme.kind == LexemeKind::Word && lexeme_text(lexeme).eq_ignore_ascii_case("with")
    }) else {
        return Vec::new();
    };
    let mut index = with + 1;
    if word_at(lexemes, index).is_some_and(|word| word.eq_ignore_ascii_case("recursive")) {
        index += 1;
    }
    let mut names = Vec::new();
    while let Some((name, after_name)) = name_at(lexemes, index) {
        if !word_at(lexemes, after_name).is_some_and(|word| word.eq_ignore_ascii_case("as")) {
            break;
        }
        names.push(name);
        let Some(open) = (after_name + 1..lexemes.len())
            .find(|candidate| symbol_at(lexemes, *candidate) == Some("("))
        else {
            break;
        };
        let Some(close) = matching_paren(lexemes, open) else {
            break;
        };
        let Some(comma) = (close + 1..lexemes.len())
            .find(|candidate| symbol_at(lexemes, *candidate) == Some(","))
        else {
            break;
        };
        index = comma + 1;
    }
    names
}

fn candidates_for_context(
    sql: &str,
    before: &[&Lexeme],
    context: &CompletionContext,
    refs: &[RelationRef],
    ctes: &[String],
    catalog: Option<&CompletionCatalog>,
) -> (CompletionScope, Vec<Candidate>) {
    let qualifier = qualifier_before(sql, before, context);
    if let Some(qualifier) = qualifier {
        if let Some(catalog) = catalog {
            if catalog.has_schema(&qualifier) {
                return (
                    CompletionScope::Schema(qualifier.clone()),
                    objects_in_schema(catalog, &qualifier),
                );
            }
            if let Some(relation) = relation_for_qualifier(catalog, refs, &qualifier) {
                return (
                    CompletionScope::QualifiedColumns(qualifier.clone()),
                    columns_for_relation(relation, &qualifier),
                );
            }
        }
        return (CompletionScope::QualifiedColumns(qualifier), Vec::new());
    }

    let relation_position = last_context_keyword(before).is_some_and(|keyword| {
        matches!(
            keyword.as_str(),
            "from" | "join" | "update" | "into" | "references"
        )
    });

    if relation_position {
        let mut candidates = catalog.map_or_else(Vec::new, relation_objects);
        candidates.extend(ctes.iter().map(|name| Candidate {
            label: name.clone(),
            insert_text: quote_identifier(name),
            kind: CandidateKind::Cte,
            schema: None,
            source: None,
            detail: Some("defined in this statement".to_owned()),
        }));
        // Relation positions benefit from the same discoverability as every
        // other position, but keywords sort after actual objects.
        candidates.extend(keyword_candidates());
        return (CompletionScope::Relations, candidates);
    }

    let mut candidates = Vec::new();
    if let Some(catalog) = catalog {
        for relation_ref in refs {
            if relation_ref.subquery || relation_ref.name.is_empty() {
                continue;
            }
            let relation = catalog
                .relation(relation_ref.schema.as_deref(), &relation_ref.name)
                .or_else(|| {
                    relation_ref
                        .schema
                        .is_none()
                        .then(|| catalog.relations_named(&relation_ref.name))
                        .and_then(|relations| relations.into_iter().next())
                });
            if let Some(relation) = relation {
                let source = relation_ref
                    .alias
                    .as_deref()
                    .unwrap_or(&relation_ref.name)
                    .to_owned();
                candidates.extend(relation.columns.iter().map(|column| Candidate {
                    label: column.name.clone(),
                    insert_text: quote_identifier(&column.name),
                    kind: CandidateKind::Column,
                    schema: Some(relation.schema.clone()),
                    source: Some(source.clone()),
                    detail: Some(column.data_type.clone()),
                }));
            }
        }
    }
    candidates.extend(keyword_candidates());
    let scope = if candidates
        .iter()
        .any(|candidate| candidate.kind == CandidateKind::Column)
    {
        CompletionScope::Columns
    } else {
        CompletionScope::Mixed
    };
    (scope, candidates)
}

fn qualifier_before(sql: &str, before: &[&Lexeme], context: &CompletionContext) -> Option<String> {
    let mut index = before.len();
    while index > 0 {
        index -= 1;
        let lexeme = before[index];
        if lexeme.end <= context.replacement.start {
            if lexeme.kind == LexemeKind::Symbol && &sql[lexeme.start..lexeme.end] == "." {
                let previous = index.checked_sub(1)?;
                return name_at(before, previous).map(|(name, _)| name);
            }
            if lexeme.end < context.replacement.start {
                break;
            }
        }
    }
    None
}

fn relation_for_qualifier<'a>(
    catalog: &'a CompletionCatalog,
    refs: &[RelationRef],
    qualifier: &str,
) -> Option<&'a CatalogRelation> {
    refs.iter()
        .find(|relation| {
            relation
                .alias
                .as_deref()
                .is_some_and(|alias| same_identifier(alias, qualifier))
                || same_identifier(&relation.name, qualifier)
        })
        .and_then(|relation| {
            if relation.subquery {
                None
            } else {
                catalog.relation(relation.schema.as_deref(), &relation.name)
            }
        })
}

fn objects_in_schema(catalog: &CompletionCatalog, schema: &str) -> Vec<Candidate> {
    catalog
        .objects
        .iter()
        .filter(|object| same_identifier(&object.schema, schema))
        .map(object_candidate)
        .collect()
}

fn relation_objects(catalog: &CompletionCatalog) -> Vec<Candidate> {
    catalog
        .objects
        .iter()
        .filter(|object| object.kind != CatalogObjectKind::Function || object.readable)
        .map(object_candidate)
        .collect()
}

fn object_candidate(object: &CatalogObject) -> Candidate {
    Candidate {
        label: object.name.clone(),
        insert_text: quote_identifier(&object.name),
        kind: CandidateKind::Object(object.kind),
        schema: Some(object.schema.clone()),
        source: None,
        detail: if object.readable {
            object.detail.clone()
        } else {
            Some("not readable by this role".to_owned())
        },
    }
}

fn columns_for_relation(relation: &CatalogRelation, source: &str) -> Vec<Candidate> {
    relation
        .columns
        .iter()
        .map(|column| Candidate {
            label: column.name.clone(),
            insert_text: quote_identifier(&column.name),
            kind: CandidateKind::Column,
            schema: Some(relation.schema.clone()),
            source: Some(source.to_owned()),
            detail: Some(column.data_type.clone()),
        })
        .collect()
}

fn keyword_candidates() -> Vec<Candidate> {
    highlight::KEYWORDS
        .iter()
        .map(|word| Candidate {
            label: word.to_ascii_uppercase(),
            insert_text: word.to_ascii_uppercase(),
            kind: CandidateKind::Keyword,
            schema: None,
            source: None,
            detail: None,
        })
        .collect()
}

fn compare_candidates(left: &Candidate, right: &Candidate, prefix: &str) -> Ordering {
    let left_exact = usize::from(!left.label.eq_ignore_ascii_case(prefix));
    let right_exact = usize::from(!right.label.eq_ignore_ascii_case(prefix));
    left_exact
        .cmp(&right_exact)
        .then_with(|| left.kind.rank().cmp(&right.kind.rank()))
        .then_with(|| {
            left.label
                .to_ascii_lowercase()
                .cmp(&right.label.to_ascii_lowercase())
        })
        .then_with(|| left.schema.cmp(&right.schema))
        .then_with(|| left.source.cmp(&right.source))
}

fn last_context_keyword(before: &[&Lexeme]) -> Option<String> {
    before.iter().rev().find_map(|lexeme| {
        if lexeme.kind != LexemeKind::Word {
            return None;
        }
        let text = lexeme_text(lexeme).to_ascii_lowercase();
        (highlight::is_keyword(&text)
            || matches!(
                text.as_str(),
                "from" | "join" | "update" | "into" | "references"
            ))
        .then_some(text)
    })
}

fn matching_paren(lexemes: &[&Lexeme], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for index in open..lexemes.len() {
        match symbol_at(lexemes, index) {
            Some("(") => depth += 1,
            Some(")") => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn name_at(lexemes: &[&Lexeme], index: usize) -> Option<(String, usize)> {
    let lexeme = *lexemes.get(index)?;
    matches!(lexeme.kind, LexemeKind::Word | LexemeKind::Quoted).then(|| {
        let text = lexeme_text(lexeme);
        (unquote_identifier(&text), index + 1)
    })
}

fn word_at(lexemes: &[&Lexeme], index: usize) -> Option<String> {
    let lexeme = *lexemes.get(index)?;
    (lexeme.kind == LexemeKind::Word).then(|| lexeme_text(lexeme))
}

fn symbol_at<'a>(lexemes: &[&'a Lexeme], index: usize) -> Option<&'a str> {
    let lexeme = *lexemes.get(index)?;
    (lexeme.kind == LexemeKind::Symbol).then_some(lexeme.text.as_str())
}

fn lexeme_text(lexeme: &Lexeme) -> String {
    lexeme.text.clone()
}

fn unquote_identifier(value: &str) -> String {
    if value.starts_with('"') {
        value
            .strip_prefix('"')
            .unwrap_or(value)
            .strip_suffix('"')
            .unwrap_or(value)
            .replace("\"\"", "\"")
    } else {
        value.to_owned()
    }
}

fn quoted_prefix(value: &str) -> String {
    unquote_identifier(value)
}

fn same_identifier(left: &str, right: &str) -> bool {
    left == right
        || (is_bare_identifier(left)
            && is_bare_identifier(right)
            && left.eq_ignore_ascii_case(right))
}

fn is_bare_identifier(value: &str) -> bool {
    value
        .as_bytes()
        .first()
        .is_some_and(|byte| highlight::is_word_start(*byte))
        && value
            .as_bytes()
            .iter()
            .all(|byte| highlight::is_word_byte(*byte))
}

fn clamp_cursor(sql: &str, cursor: usize) -> usize {
    let mut cursor = cursor.min(sql.len());
    while cursor > 0 && !sql.is_char_boundary(cursor) {
        cursor -= 1;
    }
    cursor
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> CompletionCatalog {
        CompletionCatalog {
            objects: vec![
                CatalogObject {
                    kind: CatalogObjectKind::Table,
                    schema: "public".into(),
                    name: "orders".into(),
                    readable: true,
                    detail: None,
                },
                CatalogObject {
                    kind: CatalogObjectKind::Table,
                    schema: "public".into(),
                    name: "order_items".into(),
                    readable: true,
                    detail: None,
                },
                CatalogObject {
                    kind: CatalogObjectKind::Table,
                    schema: "sales".into(),
                    name: "orders".into(),
                    readable: false,
                    detail: None,
                },
                CatalogObject {
                    kind: CatalogObjectKind::Table,
                    schema: "public".into(),
                    name: "we\"ird name".into(),
                    readable: true,
                    detail: None,
                },
            ],
            relations: vec![CatalogRelation {
                schema: "public".into(),
                name: "orders".into(),
                columns: vec![
                    CatalogColumn {
                        name: "order_id".into(),
                        data_type: "bigint".into(),
                    },
                    CatalogColumn {
                        name: "Total Value".into(),
                        data_type: "numeric(12,2)".into(),
                    },
                ],
            }],
        }
    }

    #[test]
    fn relation_completion_is_prefix_ordered_and_bounded() {
        let sql = "SELECT * FROM ord";
        let result = complete(sql, sql.len(), Some(&catalog()));
        assert_eq!(result.scope, CompletionScope::Relations);
        assert_eq!(result.prefix, "ord");
        assert_eq!(result.candidates[0].label, "order_items");
        assert_eq!(result.candidates[1].label, "orders");
        assert!(result.candidates[0].kind != CandidateKind::Keyword);
        assert_eq!(
            result
                .candidates
                .last()
                .map(|candidate| candidate.label.as_str()),
            Some("ORDER")
        );
    }

    #[test]
    fn alias_completion_offers_only_the_alias_relation_columns() {
        let sql = "SELECT * FROM orders o WHERE o.";
        let result = complete(sql, sql.len(), Some(&catalog()));
        assert_eq!(result.scope, CompletionScope::QualifiedColumns("o".into()));
        assert_eq!(
            result
                .candidates
                .iter()
                .map(|candidate| candidate.label.as_str())
                .collect::<Vec<_>>(),
            vec!["order_id", "Total Value"]
        );
        assert_eq!(result.candidates[1].insert_text, "\"Total Value\"");
    }

    #[test]
    fn schema_completion_restricts_objects_to_that_schema() {
        let sql = "SELECT * FROM public.or";
        let result = complete(sql, sql.len(), Some(&catalog()));
        assert_eq!(result.scope, CompletionScope::Schema("public".into()));
        assert!(
            result
                .candidates
                .iter()
                .all(|candidate| candidate.schema.as_deref() == Some("public"))
        );
    }

    #[test]
    fn strings_and_comments_are_hard_boundaries() {
        let string = "SELECT 'ord";
        let comment = "SELECT 1 -- ord";
        let block = "SELECT /* ord";
        assert!(
            complete(string, string.len(), Some(&catalog()))
                .candidates
                .is_empty()
        );
        assert!(
            complete(comment, comment.len(), Some(&catalog()))
                .candidates
                .is_empty()
        );
        assert!(
            complete(block, block.len(), Some(&catalog()))
                .candidates
                .is_empty()
        );
    }

    #[test]
    fn ctes_are_offered_even_when_the_catalogue_has_no_cte_row() {
        let sql = "WITH recent_orders AS (SELECT 1) SELECT * FROM rec";
        let result = complete(sql, sql.len(), Some(&catalog()));
        assert!(result.candidates.iter().any(|candidate| {
            candidate.label == "recent_orders" && candidate.kind == CandidateKind::Cte
        }));
    }

    #[test]
    fn hostile_names_are_quoted_for_acceptance() {
        let sql = "SELECT * FROM public.we";
        let result = complete(sql, sql.len(), Some(&catalog()));
        let candidate = result
            .candidates
            .iter()
            .find(|candidate| candidate.label == "we\"ird name")
            .expect("hostile name should be offered");
        assert_eq!(candidate.insert_text, "\"we\"\"ird name\"");
    }

    #[test]
    fn no_catalogue_still_offers_keywords_without_claiming_objects() {
        let result = complete("SEL", 3, None);
        assert_eq!(result.scope, CompletionScope::Mixed);
        assert!(
            result
                .candidates
                .iter()
                .any(|candidate| candidate.label == "SELECT")
        );
        assert_eq!(result.total_count, highlight::KEYWORDS.len());
    }

    #[test]
    fn a_prefix_longer_than_the_visible_limit_reports_the_true_match_count() {
        let mut catalog = catalog();
        for index in 0..20 {
            catalog.objects.push(CatalogObject {
                kind: CatalogObjectKind::Table,
                schema: "public".into(),
                name: format!("thing_{index:02}"),
                readable: true,
                detail: None,
            });
        }
        let sql = "SELECT * FROM thing_";
        let result = complete(sql, sql.len(), Some(&catalog));
        assert_eq!(result.matching_count, 20);
        assert_eq!(result.candidates.len(), MAX_VISIBLE_CANDIDATES);
    }

    #[test]
    fn an_incomplete_previous_statement_does_not_make_completion_panic() {
        let sql = "SELECT ( FROM orders; SELECT * FROM ord";
        let result = complete(sql, sql.len(), Some(&catalog()));
        assert_eq!(result.scope, CompletionScope::Relations);
        assert!(
            result
                .candidates
                .iter()
                .any(|candidate| candidate.label == "orders")
        );
    }

    #[test]
    fn context_ranges_are_utf8_boundaries() {
        let sql = "SELECT café";
        let result = complete(sql, sql.len(), None);
        assert!(sql.is_char_boundary(result.replacement.start));
        assert!(sql.is_char_boundary(result.replacement.end));
        assert_eq!(result.prefix, "café");
    }
}
