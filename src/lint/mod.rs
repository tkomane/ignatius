//! Deterministic, offline SQL linting.
//!
//! The linter is lexical on purpose: it can run in CI with no server, and it
//! says only what the text supports. String literals, quoted identifiers,
//! dollar bodies and comments are masked before matching, so their contents
//! can never trigger a rule.

use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crate::query::highlight::{TokenKind, tokens};
use crate::query::statements::{Statement, split};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Largest file accepted by the linter.
pub const MAX_FILE_BYTES: usize = 1024 * 1024;

/// Largest number of files accepted in one directory walk.
pub const MAX_FILES: usize = 1000;

/// Longest excerpt retained with a finding.
pub const MAX_EXCERPT_CHARS: usize = 120;

/// How serious a finding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LintSeverity {
    /// Style or advice.
    Info,
    /// A real risk that may be intentional.
    Warning,
    /// A statement that can destroy data or block a deployment.
    Error,
}

impl LintSeverity {
    /// Lowercase label used in reports.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }

    /// Rank used for threshold comparison.
    #[must_use]
    pub const fn rank(self) -> u8 {
        match self {
            Self::Info => 0,
            Self::Warning => 1,
            Self::Error => 2,
        }
    }

    /// Parses a CLI threshold.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "info" => Some(Self::Info),
            "warning" => Some(Self::Warning),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

/// Stable rule identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LintCode {
    /// DELETE without a WHERE clause.
    DeleteWithoutWhere,
    /// UPDATE without a WHERE clause.
    UpdateWithoutWhere,
    /// TRUNCATE removes every row.
    TruncateTable,
    /// DROP removes an object.
    DropObject,
    /// GRANT ALL is broader than most changes need.
    GrantAll,
    /// CREATE INDEX without CONCURRENTLY blocks writes.
    CreateIndexWithoutConcurrently,
    /// REINDEX without CONCURRENTLY blocks writes.
    ReindexWithoutConcurrently,
    /// VACUUM FULL takes an exclusive lock.
    VacuumFull,
    /// SELECT * makes the result shape implicit.
    SelectStar,
    /// A leading wildcard cannot use a B-tree index.
    LeadingWildcardLike,
    /// A function or cast around a column prevents a plain index scan.
    NonSargablePredicate,
    /// A comma in FROM is an implicit cross join.
    ImplicitCrossJoin,
    /// DDL appears without an explicit transaction.
    MissingTransaction,
}

impl LintCode {
    /// Every rule, in display order.
    pub const ALL: [Self; 13] = [
        Self::DeleteWithoutWhere,
        Self::UpdateWithoutWhere,
        Self::TruncateTable,
        Self::DropObject,
        Self::GrantAll,
        Self::CreateIndexWithoutConcurrently,
        Self::ReindexWithoutConcurrently,
        Self::VacuumFull,
        Self::SelectStar,
        Self::LeadingWildcardLike,
        Self::NonSargablePredicate,
        Self::ImplicitCrossJoin,
        Self::MissingTransaction,
    ];

    /// Stable kebab-case code used in every format.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DeleteWithoutWhere => "delete-without-where",
            Self::UpdateWithoutWhere => "update-without-where",
            Self::TruncateTable => "truncate-table",
            Self::DropObject => "drop-object",
            Self::GrantAll => "grant-all",
            Self::CreateIndexWithoutConcurrently => "create-index-without-concurrently",
            Self::ReindexWithoutConcurrently => "reindex-without-concurrently",
            Self::VacuumFull => "vacuum-full",
            Self::SelectStar => "select-star",
            Self::LeadingWildcardLike => "leading-wildcard-like",
            Self::NonSargablePredicate => "non-sargable-predicate",
            Self::ImplicitCrossJoin => "implicit-cross-join",
            Self::MissingTransaction => "missing-transaction",
        }
    }

    /// Severity this rule always carries.
    #[must_use]
    pub const fn severity(self) -> LintSeverity {
        match self {
            Self::DeleteWithoutWhere | Self::UpdateWithoutWhere | Self::TruncateTable => {
                LintSeverity::Error
            }
            Self::DropObject
            | Self::GrantAll
            | Self::CreateIndexWithoutConcurrently
            | Self::ReindexWithoutConcurrently
            | Self::VacuumFull
            | Self::LeadingWildcardLike
            | Self::NonSargablePredicate
            | Self::ImplicitCrossJoin => LintSeverity::Warning,
            Self::SelectStar | Self::MissingTransaction => LintSeverity::Info,
        }
    }

    /// One-line explanation shown with the finding.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::DeleteWithoutWhere => "DELETE has no WHERE clause and would remove every row",
            Self::UpdateWithoutWhere => "UPDATE has no WHERE clause and would rewrite every row",
            Self::TruncateTable => "TRUNCATE removes every row and cannot be filtered",
            Self::DropObject => "DROP removes an object permanently",
            Self::GrantAll => "GRANT ALL gives every privilege this object supports",
            Self::CreateIndexWithoutConcurrently => {
                "CREATE INDEX without CONCURRENTLY blocks writes while it builds"
            }
            Self::ReindexWithoutConcurrently => {
                "REINDEX without CONCURRENTLY blocks writes while it rebuilds"
            }
            Self::VacuumFull => "VACUUM FULL takes an exclusive lock and rewrites the table",
            Self::SelectStar => "SELECT * makes the result shape depend on the table",
            Self::LeadingWildcardLike => {
                "a leading wildcard prevents a B-tree index from serving the LIKE"
            }
            Self::NonSargablePredicate => {
                "a function or cast around the column prevents a plain index scan"
            }
            Self::ImplicitCrossJoin => {
                "a comma in FROM is an implicit cross join; use an explicit JOIN"
            }
            Self::MissingTransaction => "DDL appears without an explicit transaction in this file",
        }
    }
}

/// One finding in one statement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    /// Optional file label. `None` means the editor buffer or standard input.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Stable rule code.
    pub code: String,
    /// Rule severity.
    pub severity: LintSeverity,
    /// One-line explanation.
    pub message: String,
    /// Zero-based statement index within the source.
    pub statement_index: usize,
    /// One-based line.
    pub line: usize,
    /// One-based column, counted in characters.
    pub column: usize,
    /// Bounded source line containing the finding.
    pub excerpt: String,
}

/// Findings for one or more sources.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LintReport {
    /// Findings in deterministic order.
    pub findings: Vec<Finding>,
    /// Number of sources examined.
    pub files: usize,
    /// Number of statements examined.
    pub statements: usize,
}

impl LintReport {
    /// Appends another report, preserving order and counts.
    pub fn extend(&mut self, other: Self) {
        self.findings.extend(other.findings);
        self.files += other.files;
        self.statements += other.statements;
    }

    /// Sorts findings by file, line, column and code.
    pub fn sort(&mut self) {
        self.findings.sort_by(|a, b| {
            a.file
                .cmp(&b.file)
                .then(a.line.cmp(&b.line))
                .then(a.column.cmp(&b.column))
                .then(a.code.cmp(&b.code))
        });
    }

    /// Whether any finding meets or exceeds the threshold.
    #[must_use]
    pub fn meets(&self, threshold: LintSeverity) -> bool {
        self.findings
            .iter()
            .any(|finding| finding.severity.rank() >= threshold.rank())
    }

    /// Number of findings at one severity.
    #[must_use]
    pub fn count(&self, severity: LintSeverity) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.severity == severity)
            .count()
    }

    /// Drops findings below the display threshold.
    pub fn retain_at_or_above(&mut self, threshold: LintSeverity) {
        self.findings
            .retain(|finding| finding.severity.rank() >= threshold.rank());
    }
}

/// Lints one source with an optional file label.
#[must_use]
pub fn lint_source(label: Option<&str>, sql: &str) -> LintReport {
    let statements = split(sql);
    let masked = mask_protected(sql);
    let mut findings = Vec::new();

    for (index, statement) in statements.iter().enumerate() {
        let local = &masked[statement.start..statement.end];
        let upper = local.to_ascii_uppercase();
        let mut push = |code: LintCode, offset: usize| {
            findings.push(finding_for(label, sql, statement, index, code, offset));
        };

        if starts_with_word(&upper, "DELETE") && !has_word(&upper, "WHERE") {
            push(LintCode::DeleteWithoutWhere, first_word(&upper, "DELETE"));
        }
        if starts_with_word(&upper, "UPDATE") && !has_word(&upper, "WHERE") {
            push(LintCode::UpdateWithoutWhere, first_word(&upper, "UPDATE"));
        }
        if has_word(&upper, "TRUNCATE") {
            push(LintCode::TruncateTable, first_word(&upper, "TRUNCATE"));
        }
        if has_word(&upper, "DROP") {
            push(LintCode::DropObject, first_word(&upper, "DROP"));
        }
        if has_word(&upper, "GRANT") && has_word(&upper, "ALL") {
            push(LintCode::GrantAll, first_word(&upper, "GRANT"));
        }
        if is_create_index(&upper) && !has_word(&upper, "CONCURRENTLY") {
            push(
                LintCode::CreateIndexWithoutConcurrently,
                first_word(&upper, "CREATE"),
            );
        }
        if has_word(&upper, "REINDEX") && !has_word(&upper, "CONCURRENTLY") {
            push(
                LintCode::ReindexWithoutConcurrently,
                first_word(&upper, "REINDEX"),
            );
        }
        if is_vacuum_full(&upper) {
            push(LintCode::VacuumFull, first_word(&upper, "VACUUM"));
        }
        if let Some(position) = select_star(&upper) {
            push(LintCode::SelectStar, position);
        }
        if let Some(position) = implicit_cross_join(&upper) {
            push(LintCode::ImplicitCrossJoin, position);
        }
        if let Some(position) = non_sargable(&upper) {
            push(LintCode::NonSargablePredicate, position);
        }
    }

    if let Some(first_ddl) = first_ddl_without_transaction(&masked, &statements) {
        let statement = &statements[first_ddl];
        findings.push(finding_for(
            label,
            sql,
            statement,
            first_ddl,
            LintCode::MissingTransaction,
            first_word(
                &masked[statement.start..statement.end].to_ascii_uppercase(),
                "CREATE",
            ),
        ));
    }

    findings.extend(leading_wildcard_findings(label, sql, &statements));

    let mut report = LintReport {
        findings,
        files: 1,
        statements: statements.len(),
    };
    report.sort();
    report
}

fn finding_for(
    label: Option<&str>,
    sql: &str,
    statement: &Statement,
    statement_index: usize,
    code: LintCode,
    offset_in_statement: usize,
) -> Finding {
    let absolute = statement.start + offset_in_statement.min(statement.end - statement.start);
    let (line, column) = position(sql, absolute);
    Finding {
        file: label.map(str::to_owned),
        code: code.as_str().to_owned(),
        severity: code.severity(),
        message: code.message().to_owned(),
        statement_index,
        line,
        column,
        excerpt: excerpt(sql, absolute),
    }
}

/// Lints every file or directory in order.
///
/// # Errors
///
/// Returns a usage diagnostic for an unreadable file, a symlink, an oversized
/// file or more than [`MAX_FILES`] entries.
pub fn lint_paths(paths: &[PathBuf]) -> Result<LintReport, Diagnostic> {
    let files = collect_files(paths)?;
    let mut report = LintReport::default();
    for path in files {
        let metadata =
            std::fs::symlink_metadata(&path).map_err(|error| file_diagnostic(&path, &error))?;
        if metadata.file_type().is_symlink() {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!("refusing to lint the symbolic link {}", path.display()),
                "linting SQL files",
            )
            .next_action("pass the real file or directory instead"));
        }
        if metadata.len() as usize > MAX_FILE_BYTES {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!(
                    "{} is {} bytes, over the {MAX_FILE_BYTES}-byte lint bound",
                    path.display(),
                    metadata.len()
                ),
                "linting SQL files",
            )
            .next_action("split the file or lint a smaller one"));
        }
        let sql = std::fs::read_to_string(&path).map_err(|error| file_diagnostic(&path, &error))?;
        let label = path.display().to_string();
        report.extend(lint_source(Some(&label), &sql));
    }
    report.sort();
    Ok(report)
}

/// Reads one file and lints it.
///
/// # Errors
///
/// Returns a usage diagnostic when the file cannot be read or exceeds the bound.
pub fn lint_file(path: &Path) -> Result<LintReport, Diagnostic> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|error| file_diagnostic(path, &error))?;
    if metadata.file_type().is_symlink() {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            format!("refusing to lint the symbolic link {}", path.display()),
            "linting a SQL file",
        )
        .next_action("pass the real file instead"));
    }
    if metadata.len() as usize > MAX_FILE_BYTES {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            format!(
                "{} is {} bytes, over the {MAX_FILE_BYTES}-byte lint bound",
                path.display(),
                metadata.len()
            ),
            "linting a SQL file",
        ));
    }
    let sql = std::fs::read_to_string(path).map_err(|error| file_diagnostic(path, &error))?;
    Ok(lint_source(Some(&path.display().to_string()), &sql))
}

fn collect_files(paths: &[PathBuf]) -> Result<Vec<PathBuf>, Diagnostic> {
    let mut files = Vec::new();
    for path in paths {
        let metadata =
            std::fs::symlink_metadata(path).map_err(|error| file_diagnostic(path, &error))?;
        if metadata.file_type().is_symlink() {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!("refusing to lint the symbolic link {}", path.display()),
                "collecting SQL files",
            ));
        }
        if metadata.is_file() {
            files.push(path.clone());
        } else if metadata.is_dir() {
            collect_directory(path, &mut files)?;
        } else {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!("{} is not a regular file or directory", path.display()),
                "collecting SQL files",
            ));
        }
        if files.len() > MAX_FILES {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!("more than {MAX_FILES} SQL files were selected"),
                "collecting SQL files",
            )
            .next_action("lint a narrower directory"));
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn collect_directory(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), Diagnostic> {
    let entries =
        std::fs::read_dir(directory).map_err(|error| file_diagnostic(directory, &error))?;
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    paths.sort();
    for path in paths {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if name == "target" || name == ".git" || name == "node_modules" {
            continue;
        }
        let metadata =
            std::fs::symlink_metadata(&path).map_err(|error| file_diagnostic(&path, &error))?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_directory(&path, files)?;
        } else if metadata.is_file()
            && path.extension().and_then(|extension| extension.to_str()) == Some("sql")
        {
            files.push(path);
            if files.len() > MAX_FILES {
                return Err(Diagnostic::new(
                    DiagnosticKind::Usage,
                    format!("more than {MAX_FILES} SQL files were selected"),
                    "collecting SQL files",
                )
                .next_action("lint a narrower directory"));
            }
        }
    }
    Ok(())
}

fn file_diagnostic(path: &Path, error: &std::io::Error) -> Diagnostic {
    Diagnostic::new(
        DiagnosticKind::Usage,
        format!("failed to read {}: {error}", path.display()),
        "linting SQL files",
    )
    .likely_cause("the path does not exist or cannot be read")
}

fn mask_protected(sql: &str) -> String {
    let mut bytes = sql.as_bytes().to_vec();
    for token in tokens(sql) {
        if matches!(
            token.kind,
            TokenKind::Literal | TokenKind::Comment | TokenKind::Identifier
        ) {
            for byte in &mut bytes[token.start..token.end] {
                if *byte != b'\n' && *byte != b'\r' {
                    *byte = b' ';
                }
            }
        }
    }
    for byte in &mut bytes {
        if !byte.is_ascii() {
            *byte = b' ';
        }
    }
    String::from_utf8(bytes).unwrap_or_else(|_| sql.to_owned())
}

fn position(sql: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(sql.len());
    let before = &sql[..offset];
    let line = before.matches('\n').count() + 1;
    let column = before
        .rsplit_once('\n')
        .map_or(before.chars().count(), |(_, last)| last.chars().count())
        + 1;
    (line, column)
}

fn excerpt(sql: &str, offset: usize) -> String {
    let offset = offset.min(sql.len());
    let line_start = sql[..offset].rfind('\n').map_or(0, |index| index + 1);
    let line_end = sql[offset..]
        .find('\n')
        .map_or(sql.len(), |index| offset + index);
    let line = &sql[line_start..line_end];
    let mut cleaned: String = line
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect();
    cleaned = cleaned.trim().to_owned();
    if cleaned.chars().count() > MAX_EXCERPT_CHARS {
        cleaned = cleaned.chars().take(MAX_EXCERPT_CHARS).collect();
        cleaned.push_str("...");
    }
    cleaned
}

fn has_word(haystack: &str, word: &str) -> bool {
    first_word(haystack, word) != usize::MAX
}

fn first_word(haystack: &str, word: &str) -> usize {
    let bytes = haystack.as_bytes();
    let needle = word.as_bytes();
    let mut start = 0;
    while let Some(index) = haystack[start..].find(word) {
        let position = start + index;
        let end = position + needle.len();
        let before_ok = position == 0 || !word_byte(bytes[position - 1]);
        let after_ok = end == bytes.len() || !word_byte(bytes[end]);
        if before_ok && after_ok {
            return position;
        }
        start = position + needle.len();
    }
    usize::MAX
}

fn starts_with_word(haystack: &str, word: &str) -> bool {
    let trimmed = haystack.trim_start();
    if !trimmed.starts_with(word) {
        return false;
    }
    trimmed.get(word.len()..).is_none_or(|rest| {
        rest.chars()
            .next()
            .is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_' && c != '$')
    })
}

const fn word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}

fn is_create_index(haystack: &str) -> bool {
    let Some(create) = haystack.find("CREATE") else {
        return false;
    };
    let rest = &haystack[create + "CREATE".len()..];
    let rest = rest.trim_start();
    let rest = rest.strip_prefix("UNIQUE").map_or(rest, str::trim_start);
    rest.starts_with("INDEX") && rest["INDEX".len()..].starts_with(|c: char| !word_byte(c as u8))
}

fn is_vacuum_full(haystack: &str) -> bool {
    let vacuum = first_word(haystack, "VACUUM");
    if vacuum == usize::MAX {
        return false;
    }
    let rest = haystack[vacuum + "VACUUM".len()..].trim_start();
    if let Some(options) = rest.strip_prefix('(') {
        // The parenthesised form, `VACUUM (FULL, ANALYZE) t`, is the same
        // operation and must not slip past the rule.
        return options
            .split_once(')')
            .is_some_and(|(inside, _)| has_word(&inside.to_ascii_uppercase(), "FULL"));
    }
    rest.starts_with("FULL")
}

fn select_star(haystack: &str) -> Option<usize> {
    let mut search = 0;
    while let Some(found) = haystack[search..].find("SELECT") {
        let position = search + found;
        let end = position + "SELECT".len();
        if position > 0 && word_byte(haystack.as_bytes()[position - 1]) {
            search = end;
            continue;
        }
        let mut rest = haystack[end..].trim_start();
        for modifier in ["ALL", "DISTINCT"] {
            if let Some(after) = rest.strip_prefix(modifier) {
                rest = after.trim_start();
            }
        }
        if rest.starts_with('*') {
            return Some(position);
        }
        search = end;
    }
    None
}

fn implicit_cross_join(haystack: &str) -> Option<usize> {
    let bytes = haystack.as_bytes();
    let mut depth = 0i32;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ if depth == 0
                && haystack[index..].starts_with("FROM")
                && (index == 0 || !word_byte(bytes[index - 1]))
                && bytes.get(index + 4).is_none_or(|byte| !word_byte(*byte)) =>
            {
                let mut cursor = index + 4;
                let mut inner_depth = 0i32;
                while cursor < bytes.len() {
                    match bytes[cursor] {
                        b'(' => inner_depth += 1,
                        b')' => inner_depth -= 1,
                        b',' if inner_depth == 0 => return Some(index),
                        _ if inner_depth == 0 && is_clause_at(haystack, cursor) => break,
                        _ => {}
                    }
                    cursor += 1;
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

fn is_clause_at(haystack: &str, index: usize) -> bool {
    [
        "WHERE",
        "GROUP",
        "ORDER",
        "HAVING",
        "LIMIT",
        "UNION",
        "EXCEPT",
        "RETURNING",
    ]
    .iter()
    .any(|clause| {
        haystack[index..].starts_with(clause)
            && (index == 0 || !word_byte(haystack.as_bytes()[index - 1]))
            && haystack
                .as_bytes()
                .get(index + clause.len())
                .is_none_or(|byte| !word_byte(*byte))
    })
}

fn non_sargable(haystack: &str) -> Option<usize> {
    for function in ["LOWER", "UPPER", "DATE", "CAST", "COALESCE"] {
        let mut search = 0;
        while let Some(found) = haystack[search..].find(function) {
            let position = search + found;
            let end = position + function.len();
            let before_ok = position == 0 || !word_byte(haystack.as_bytes()[position - 1]);
            let after = haystack[end..].trim_start();
            if before_ok
                && after.starts_with('(')
                && let Some(close) = after.find(')')
                && after[close + 1..].trim_start().starts_with('=')
            {
                return Some(position);
            }
            search = end;
        }
    }
    haystack
        .find("::TEXT =")
        .or_else(|| haystack.find("::VARCHAR ="))
}

fn leading_wildcard_findings(
    label: Option<&str>,
    sql: &str,
    statements: &[Statement],
) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (index, statement) in statements.iter().enumerate() {
        let local = &sql[statement.start..statement.end];
        let masked = mask_protected(local);
        let upper = masked.to_ascii_uppercase();
        if !has_word(&upper, "LIKE") && !has_word(&upper, "ILIKE") {
            continue;
        }
        for token in tokens(local) {
            if token.kind != TokenKind::Literal {
                continue;
            }
            let literal = &local[token.start..token.end];
            let content = literal
                .strip_prefix('E')
                .or_else(|| literal.strip_prefix('e'))
                .unwrap_or(literal);
            let content = content.trim_matches('\'');
            if content.starts_with('%') || content.starts_with('_') {
                let before = &upper[..token.start.min(upper.len())];
                if has_word(before, "LIKE") || has_word(before, "ILIKE") {
                    findings.push(finding_for(
                        label,
                        sql,
                        statement,
                        index,
                        LintCode::LeadingWildcardLike,
                        token.start,
                    ));
                    break;
                }
            }
        }
    }
    findings
}

fn first_ddl_without_transaction(masked: &str, statements: &[Statement]) -> Option<usize> {
    let has_transaction = statements.iter().any(|statement| {
        let upper = masked[statement.start..statement.end]
            .trim_start()
            .to_ascii_uppercase();
        upper.starts_with("BEGIN") || upper.starts_with("START TRANSACTION")
    });
    if has_transaction {
        return None;
    }
    statements.iter().position(|statement| {
        let upper = masked[statement.start..statement.end]
            .trim_start()
            .to_ascii_uppercase();
        ["CREATE", "ALTER", "DROP", "TRUNCATE", "GRANT", "REVOKE"]
            .iter()
            .any(|keyword| upper.starts_with(keyword))
    })
}

/// Renders findings as `PATH:LINE:COL SEVERITY CODE MESSAGE`.
#[must_use]
pub fn render_table(report: &LintReport) -> String {
    if report.findings.is_empty() {
        return "No findings.\n".to_owned();
    }
    let mut output = String::new();
    for finding in &report.findings {
        let file = finding.file.as_deref().unwrap_or("<buffer>");
        output.push_str(&format!(
            "{}:{}:{} {} {} {}\n",
            file,
            finding.line,
            finding.column,
            finding.severity.label(),
            finding.code,
            finding.message
        ));
    }
    output
}

/// Renders findings as stable JSON.
///
/// # Errors
///
/// Returns the serializer error, which for this model means a defect.
pub fn render_json(report: &LintReport) -> Result<String, serde_json::Error> {
    #[derive(Serialize)]
    struct JsonReport<'a> {
        version: u32,
        files: usize,
        statements: usize,
        findings: &'a [Finding],
    }
    serde_json::to_string_pretty(&JsonReport {
        version: 1,
        files: report.files,
        statements: report.statements,
        findings: &report.findings,
    })
}

/// Renders findings as SARIF 2.1.0.
///
/// # Errors
///
/// Returns the serializer error, which for this model means a defect.
pub fn render_sarif(report: &LintReport) -> Result<String, serde_json::Error> {
    let rules: Vec<serde_json::Value> = LintCode::ALL
        .iter()
        .map(|code| {
            serde_json::json!({
                "id": code.as_str(),
                "shortDescription": { "text": code.message() },
                "properties": { "severity": code.severity().label() },
            })
        })
        .collect();
    let results: Vec<serde_json::Value> = report
        .findings
        .iter()
        .map(|finding| {
            let level = match finding.severity {
                LintSeverity::Error => "error",
                LintSeverity::Warning => "warning",
                LintSeverity::Info => "note",
            };
            serde_json::json!({
                "ruleId": finding.code,
                "level": level,
                "message": { "text": finding.message },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": { "uri": finding.file.clone().unwrap_or_default() },
                        "region": {
                            "startLine": finding.line,
                            "startColumn": finding.column,
                            "snippet": { "text": finding.excerpt },
                        }
                    }
                }]
            })
        })
        .collect();
    let document = serde_json::json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": { "driver": {
                "name": "ignatius",
                "informationUri": "https://github.com/tkomane/ignatius",
                "rules": rules,
            }},
            "results": results,
        }]
    });
    serde_json::to_string_pretty(&document)
}

#[cfg(test)]
mod tests;
