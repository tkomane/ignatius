//! Conservative, local SQL formatting.
//!
//! Formatting is a presentation aid, not a parser and not a query rewrite. The
//! scanner knows the PostgreSQL lexical boundaries already owned by
//! [`crate::query::statements`], copies protected regions exactly, and changes
//! only whitespace and layout outside them. Ambiguous input is refused rather
//! than guessed.

use crate::query::highlight;
use crate::query::statements;
use std::fmt;

/// The largest source buffer the formatter will derive a second string from.
pub const MAX_FORMAT_BYTES: usize = 1024 * 1024;

/// A deterministic formatted buffer and the cursor position that belongs in it.
#[derive(PartialEq, Eq)]
pub struct FormattedSql {
    /// The formatted SQL text.
    pub text: String,
    /// A UTF-8-safe byte offset mapped from the source cursor.
    pub cursor: usize,
}

impl fmt::Debug for FormattedSql {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FormattedSql")
            .field("text", &"<hidden>")
            .field("cursor", &self.cursor)
            .finish()
    }
}

/// A protected lexical region that could not be closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtectedRegion {
    /// A single-quoted string literal.
    StringLiteral,
    /// A double-quoted identifier.
    QuotedIdentifier,
    /// A dollar-quoted body.
    DollarQuotedBody,
    /// A nested block comment.
    BlockComment,
}

impl ProtectedRegion {
    /// Plain-language label for refusal and diagnostic text.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::StringLiteral => "string literal",
            Self::QuotedIdentifier => "quoted identifier",
            Self::DollarQuotedBody => "dollar-quoted body",
            Self::BlockComment => "block comment",
        }
    }
}

/// Why formatting was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatError {
    /// The source is over the bounded formatter input size.
    TooLarge {
        /// Raw UTF-8 byte count.
        bytes: usize,
    },
    /// A protected region has no closing delimiter.
    Unterminated {
        /// The protected region kind.
        kind: ProtectedRegion,
        /// Source byte where the region began.
        position: usize,
    },
}

impl FormatError {
    /// Safe wording suitable for a visible notice. It never includes source
    /// text, only a kind, count, and source location.
    #[must_use]
    pub fn message(self, source: &str) -> String {
        match self {
            Self::TooLarge { bytes } => format!(
                "Format skipped: the buffer is {bytes} bytes, over the 1 MiB limit. Save the query as a file, then format a smaller buffer."
            ),
            Self::Unterminated { kind, position } => {
                let (line, column) = source_position(source, position);
                format!(
                    "Format skipped: the {} at line {line}, column {column} is not closed. Close it, then format again.",
                    kind.label()
                )
            }
        }
    }
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge { bytes } => {
                write!(f, "format input is {bytes} bytes, over the 1 MiB limit")
            }
            Self::Unterminated { kind, .. } => {
                write!(f, "unterminated {}", kind.label())
            }
        }
    }
}

impl std::error::Error for FormatError {}

/// Formats a complete SQL buffer and maps a source cursor into the result.
///
/// The cursor is clamped to a character boundary if a caller supplies an
/// invalid byte offset. The application editor already guarantees a valid
/// offset; clamping keeps this pure boundary safe for other callers too.
pub fn format_sql(source: &str, cursor: usize) -> Result<FormattedSql, FormatError> {
    if source.len() > MAX_FORMAT_BYTES {
        return Err(FormatError::TooLarge {
            bytes: source.len(),
        });
    }

    let cursor = clamp_cursor(source, cursor);
    let tokens = scan(source)?;
    if tokens.is_empty() || tokens.iter().all(|token| token.is_comment()) {
        return Ok(FormattedSql {
            text: source.to_owned(),
            cursor,
        });
    }

    let mut renderer = Renderer::new(source, &tokens, cursor);
    renderer.render();
    let (text, mapped_cursor) = renderer.finish();
    Ok(FormattedSql {
        text,
        cursor: mapped_cursor,
    })
}

/// Returns whether a source buffer contains only complete comments and
/// whitespace. An unterminated protected region is not comment-only, because
/// callers need to report the refusal rather than silently treating it as an
/// empty buffer.
pub(crate) fn is_comment_only(source: &str) -> bool {
    scan(source).is_ok_and(|tokens| tokens.iter().all(|token| token.is_comment()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    Word,
    Number,
    StringLiteral,
    QuotedIdentifier,
    DollarQuoted,
    Parameter,
    LineComment,
    BlockComment,
    Symbol,
}

impl TokenKind {
    const fn is_atom(self) -> bool {
        matches!(
            self,
            Self::Word
                | Self::Number
                | Self::StringLiteral
                | Self::QuotedIdentifier
                | Self::DollarQuoted
                | Self::Parameter
        )
    }

    const fn is_comment(self) -> bool {
        matches!(self, Self::LineComment | Self::BlockComment)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Token {
    start: usize,
    end: usize,
    kind: TokenKind,
    line_break_before: bool,
}

impl Token {
    fn text(self, source: &str) -> &str {
        &source[self.start..self.end]
    }

    const fn is_comment(self) -> bool {
        self.kind.is_comment()
    }
}

/// The style's maximum visible indentation. Deeply nested SQL stays bounded
/// instead of consuming a whole narrow terminal with spaces.
const MAX_INDENT: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Clause {
    None,
    With,
    Select,
    From,
    Where,
    Join,
    On,
    Group,
    Having,
    Order,
    Values,
    Set,
    Returning,
    Other,
}

impl Clause {
    const fn accepts_top_level_list(self) -> bool {
        matches!(
            self,
            Self::With | Self::Select | Self::From | Self::Values | Self::Set | Self::Returning
        )
    }

    const fn boolean_predicate(self) -> bool {
        matches!(self, Self::Where | Self::On | Self::Having)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PreviousToken {
    kind: TokenKind,
    start: usize,
    end: usize,
    unary_operator: bool,
}

impl PreviousToken {
    fn text(self, source: &str) -> &str {
        &source[self.start..self.end]
    }
}

struct Renderer<'a> {
    source: &'a str,
    tokens: &'a [Token],
    output: String,
    source_cursor: usize,
    mapped_cursor: Option<usize>,
    previous_source_end: usize,
    line_started: bool,
    parens: Vec<bool>,
    clause_stack: Vec<Clause>,
    clause: Clause,
    between_pending: bool,
    previous: Option<PreviousToken>,
    pending_indent: Option<usize>,
    trailing_newline: bool,
}

impl<'a> Renderer<'a> {
    fn new(source: &'a str, tokens: &'a [Token], source_cursor: usize) -> Self {
        Self {
            source,
            tokens,
            output: String::with_capacity(source.len().saturating_add(64)),
            source_cursor,
            mapped_cursor: None,
            previous_source_end: 0,
            line_started: false,
            parens: Vec::new(),
            clause_stack: Vec::new(),
            clause: Clause::None,
            between_pending: false,
            previous: None,
            pending_indent: None,
            trailing_newline: source.ends_with(['\n', '\r']),
        }
    }

    fn render(&mut self) {
        for (index, token) in self.tokens.iter().copied().enumerate() {
            self.map_gap(token.start);
            match token.kind {
                TokenKind::LineComment => self.render_line_comment(token),
                TokenKind::BlockComment => self.render_block_comment(token),
                TokenKind::StringLiteral
                | TokenKind::QuotedIdentifier
                | TokenKind::DollarQuoted
                | TokenKind::Parameter
                | TokenKind::Number
                | TokenKind::Word => self.render_atom(token),
                TokenKind::Symbol => {
                    self.render_symbol(token, index);
                }
            }
            self.previous_source_end = token.end;
        }
    }

    fn finish(mut self) -> (String, usize) {
        self.trim_trailing_spaces();
        if self.line_started {
            if self.trailing_newline {
                self.output.push('\n');
            }
        } else if !self.trailing_newline && self.output.ends_with('\n') {
            self.output.pop();
        }
        if self.mapped_cursor.is_none() {
            self.mapped_cursor = Some(self.output.len());
        }
        let cursor = self
            .mapped_cursor
            .unwrap_or_default()
            .min(self.output.len());
        let cursor = clamp_cursor(&self.output, cursor);
        (self.output, cursor)
    }

    fn render_line_comment(&mut self, token: Token) {
        if token.line_break_before && self.line_started {
            self.newline();
        } else if self.line_started {
            self.space();
        }
        self.write_raw(token, self.indent());
        self.map_inside_or_after(token);
        self.newline();
        self.previous = Some(PreviousToken {
            kind: token.kind,
            start: token.start,
            end: token.end,
            unary_operator: false,
        });
    }

    fn render_block_comment(&mut self, token: Token) {
        if token.line_break_before && self.line_started {
            self.newline();
        } else if self.line_started {
            self.space();
        }
        self.write_raw(token, self.indent());
        self.map_inside_or_after(token);
        self.previous = Some(PreviousToken {
            kind: token.kind,
            start: token.start,
            end: token.end,
            unary_operator: false,
        });
    }

    fn render_atom(&mut self, token: Token) {
        let text = token.text(self.source);
        let lower = text.to_ascii_lowercase();
        let is_major = major_clause(&lower).is_some();
        let in_query = self.in_query_context();

        if is_major && in_query && self.should_break_before_major(&lower) {
            self.newline_to(self.query_indent());
        } else if is_boolean_word(&lower)
            && self.clause.boolean_predicate()
            && !self.between_pending
            && self.line_started
        {
            self.newline_to(self.continuation_indent());
        }

        let space_before = self.needs_space_before(token.kind, text);
        self.write_spaced(token, self.indent(), space_before);
        self.map_inside_or_after(token);

        if lower == "between" {
            self.between_pending = true;
        } else if (lower == "and" && self.between_pending) || is_major || lower == "or" {
            self.between_pending = false;
        }
        if let Some(clause) = major_clause(&lower)
            && (!is_major || in_query)
        {
            self.clause = clause;
        }
        self.previous = Some(PreviousToken {
            kind: token.kind,
            start: token.start,
            end: token.end,
            unary_operator: false,
        });
    }

    fn render_symbol(&mut self, token: Token, index: usize) {
        let text = token.text(self.source);
        match text {
            "(" => self.render_open_paren(token, index),
            ")" => self.render_close_paren(token),
            "," => self.render_comma(token),
            ";" => self.render_semicolon(token),
            _ => self.render_operator(token),
        }
    }

    fn render_open_paren(&mut self, token: Token, index: usize) {
        let text = token.text(self.source);
        let query = self
            .tokens
            .get(index + 1)
            .map(|next| next.text(self.source).to_ascii_lowercase())
            .is_some_and(|next| {
                matches!(
                    next.as_str(),
                    "select" | "with" | "values" | "insert" | "update" | "delete"
                )
            });
        let space_before = self.needs_space_before_paren();
        self.write_spaced(token, self.indent(), space_before);
        self.map_inside_or_after(token);
        self.parens.push(query);
        if query {
            self.clause_stack.push(self.clause);
            self.clause = Clause::None;
        }
        self.previous = Some(PreviousToken {
            kind: TokenKind::Symbol,
            start: token.start,
            end: token.end,
            unary_operator: false,
        });
        let _ = text;
    }

    fn render_close_paren(&mut self, token: Token) {
        let query = self.parens.pop().unwrap_or(false);
        if query && self.line_started && !self.previous_is_open_paren() {
            self.newline_to(self.query_indent());
        }
        self.trim_trailing_spaces();
        self.write_spaced(token, self.indent(), false);
        self.map_inside_or_after(token);
        if query {
            self.clause = self.clause_stack.pop().unwrap_or(Clause::None);
        }
        self.previous = Some(PreviousToken {
            kind: TokenKind::Symbol,
            start: token.start,
            end: token.end,
            unary_operator: false,
        });
    }

    fn render_comma(&mut self, token: Token) {
        self.trim_trailing_spaces();
        self.write_raw(token, self.indent());
        self.map_inside_or_after(token);
        let list =
            self.parens.last().is_none_or(|query| *query) && self.clause.accepts_top_level_list();
        if list {
            self.newline_to(self.continuation_indent());
        } else {
            self.space();
        }
        self.previous = Some(PreviousToken {
            kind: TokenKind::Symbol,
            start: token.start,
            end: token.end,
            unary_operator: false,
        });
    }

    fn render_semicolon(&mut self, token: Token) {
        self.trim_trailing_spaces();
        self.write_raw(token, self.indent());
        self.map_inside_or_after(token);
        self.newline();
        self.clause = Clause::None;
        self.between_pending = false;
        self.previous = Some(PreviousToken {
            kind: TokenKind::Symbol,
            start: token.start,
            end: token.end,
            unary_operator: false,
        });
    }

    fn render_operator(&mut self, token: Token) {
        let text = token.text(self.source);
        let unary = is_unary_operator(
            text,
            self.previous.map(|previous| previous.text(self.source)),
        );
        let tight = is_tight_operator(text);
        let space_before = (!tight
            && !unary
            && self.line_started
            && !self.previous_is_open_paren()
            && !self.previous_is_comma())
            || (unary
                && self.line_started
                && self
                    .previous
                    .is_some_and(|previous| previous.kind.is_atom()));
        self.write_spaced(token, self.indent(), space_before);
        self.map_inside_or_after(token);
        if !tight && !unary {
            self.space();
        }
        self.previous = Some(PreviousToken {
            kind: TokenKind::Symbol,
            start: token.start,
            end: token.end,
            unary_operator: unary,
        });
    }

    fn should_break_before_major(&self, word: &str) -> bool {
        if !self.line_started {
            return false;
        }
        if word == "join"
            && self
                .previous
                .is_some_and(|previous| is_join_modifier(previous.text(self.source)))
        {
            return false;
        }
        true
    }

    fn needs_space_before(&self, kind: TokenKind, text: &str) -> bool {
        let Some(previous) = self.previous else {
            return false;
        };
        let previous_text = previous.text(self.source);
        if previous.kind.is_comment() {
            return self.line_started;
        }
        if kind.is_atom() && previous.kind.is_atom() {
            return true;
        }
        if kind.is_atom() && previous_text == ")" {
            return true;
        }
        if kind == TokenKind::Word && is_operator_text(previous_text) {
            return true;
        }
        if previous.unary_operator {
            return false;
        }
        if kind == TokenKind::Symbol && text == "[" {
            return false;
        }
        if kind == TokenKind::Symbol && is_tight_operator(text) {
            return false;
        }
        if previous.kind == TokenKind::Symbol && is_tight_operator(previous_text) {
            return false;
        }
        false
    }

    fn needs_space_before_paren(&self) -> bool {
        let Some(previous) = self.previous else {
            return false;
        };
        let text = previous.text(self.source);
        if text == ")" {
            return true;
        }
        if previous.kind != TokenKind::Word {
            return false;
        }
        matches!(
            text.to_ascii_lowercase().as_str(),
            "in" | "exists"
                | "from"
                | "where"
                | "values"
                | "returning"
                | "select"
                | "not"
                | "join"
                | "left"
                | "right"
                | "full"
                | "inner"
                | "cross"
        )
    }

    fn write_raw(&mut self, token: Token, indent: usize) {
        self.ensure_line(indent);
        self.output.push_str(token.text(self.source));
        self.line_started = !self.output.ends_with('\n');
    }

    fn write_spaced(&mut self, token: Token, indent: usize, space_before: bool) {
        self.ensure_line(indent);
        if space_before {
            self.space();
        }
        self.output.push_str(token.text(self.source));
        self.line_started = !self.output.ends_with('\n');
    }

    fn ensure_line(&mut self, indent: usize) {
        if !self.line_started {
            let indent = self.pending_indent.take().unwrap_or(indent).min(MAX_INDENT);
            self.output.push_str(&" ".repeat(indent));
            self.line_started = true;
        }
    }

    fn newline(&mut self) {
        self.trim_trailing_spaces();
        if self.line_started {
            self.output.push('\n');
        }
        self.line_started = false;
        self.pending_indent = None;
    }

    fn newline_to(&mut self, indent: usize) {
        self.newline();
        self.pending_indent = Some(indent.min(MAX_INDENT));
    }

    fn space(&mut self) {
        if self.line_started && !self.output.ends_with([' ', '\n']) {
            self.output.push(' ');
        }
    }

    fn trim_trailing_spaces(&mut self) {
        while self.output.ends_with(' ') {
            self.output.pop();
        }
    }

    fn map_gap(&mut self, next_start: usize) {
        if self.mapped_cursor.is_none()
            && self.source_cursor >= self.previous_source_end
            && self.source_cursor < next_start
        {
            self.mapped_cursor = Some(self.output.len());
        }
    }

    fn map_inside_or_after(&mut self, token: Token) {
        if self.mapped_cursor.is_some() {
            return;
        }
        if self.source_cursor < token.start || self.source_cursor > token.end {
            return;
        }
        let offset = self.source_cursor.saturating_sub(token.start);
        self.mapped_cursor =
            Some(self.output.len().saturating_sub(token.end - token.start) + offset);
    }

    fn indent(&self) -> usize {
        self.query_indent()
    }

    fn query_indent(&self) -> usize {
        self.parens.iter().filter(|query| **query).count() * 2
    }

    fn continuation_indent(&self) -> usize {
        self.query_indent() + 2
    }

    fn in_query_context(&self) -> bool {
        self.parens.is_empty() || self.parens.iter().any(|query| *query)
    }

    fn previous_is_open_paren(&self) -> bool {
        self.previous
            .is_some_and(|previous| previous.text(self.source) == "(")
    }

    fn previous_is_comma(&self) -> bool {
        self.previous
            .is_some_and(|previous| previous.text(self.source) == ",")
    }
}

fn scan(source: &str) -> Result<Vec<Token>, FormatError> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0usize;
    let mut line_break_before = false;

    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            if matches!(bytes[index], b'\n' | b'\r') {
                line_break_before = true;
            }
            index += 1;
            continue;
        }

        let start = index;
        match bytes[index] {
            b'\'' => {
                let end = statements::skip_single_quoted(source, index);
                if !is_closed_single(source, end) {
                    return Err(FormatError::Unterminated {
                        kind: ProtectedRegion::StringLiteral,
                        position: start,
                    });
                }
                tokens.push(Token {
                    start,
                    end,
                    kind: TokenKind::StringLiteral,
                    line_break_before,
                });
                index = end;
                line_break_before = false;
            }
            b'"' => {
                let end = statements::skip_double_quoted(source, index);
                if !is_closed_double(source, end) {
                    return Err(FormatError::Unterminated {
                        kind: ProtectedRegion::QuotedIdentifier,
                        position: start,
                    });
                }
                tokens.push(Token {
                    start,
                    end,
                    kind: TokenKind::QuotedIdentifier,
                    line_break_before,
                });
                index = end;
                line_break_before = false;
            }
            b'$' => {
                if let Some(end) = dollar_quote_end(source, index) {
                    tokens.push(Token {
                        start,
                        end,
                        kind: TokenKind::DollarQuoted,
                        line_break_before,
                    });
                    index = end;
                    line_break_before = false;
                } else if has_dollar_quote_opener(source, index) {
                    return Err(FormatError::Unterminated {
                        kind: ProtectedRegion::DollarQuotedBody,
                        position: start,
                    });
                } else {
                    let end = symbol_end(source, index);
                    tokens.push(Token {
                        start,
                        end,
                        kind: if end > index + 1 && bytes[index + 1].is_ascii_digit() {
                            TokenKind::Parameter
                        } else {
                            TokenKind::Symbol
                        },
                        line_break_before,
                    });
                    index = end;
                    line_break_before = false;
                }
            }
            b'-' if bytes.get(index + 1) == Some(&b'-') => {
                let raw_end = statements::skip_line_comment(source, index);
                let mut end = raw_end;
                while end > index && matches!(bytes[end - 1], b'\n' | b'\r') {
                    end -= 1;
                }
                tokens.push(Token {
                    start,
                    end,
                    kind: TokenKind::LineComment,
                    line_break_before,
                });
                index = raw_end;
                line_break_before = true;
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                let end = statements::skip_block_comment(source, index);
                if closed_block_comment_end(source, index).is_none() {
                    return Err(FormatError::Unterminated {
                        kind: ProtectedRegion::BlockComment,
                        position: start,
                    });
                }
                tokens.push(Token {
                    start,
                    end,
                    kind: TokenKind::BlockComment,
                    line_break_before,
                });
                index = end;
                line_break_before = false;
            }
            byte if is_prefixed_string(source, index, byte) => {
                let end = statements::skip_single_quoted(source, index + 1);
                if !is_closed_single(source, end) {
                    return Err(FormatError::Unterminated {
                        kind: ProtectedRegion::StringLiteral,
                        position: start,
                    });
                }
                tokens.push(Token {
                    start,
                    end,
                    kind: TokenKind::StringLiteral,
                    line_break_before,
                });
                index = end;
                line_break_before = false;
            }
            byte if highlight::is_word_start(byte) || !byte.is_ascii() => {
                let end = word_end(source, index);
                tokens.push(Token {
                    start,
                    end,
                    kind: TokenKind::Word,
                    line_break_before,
                });
                index = end;
                line_break_before = false;
            }
            b'0'..=b'9' => {
                let end = number_end(source, index);
                tokens.push(Token {
                    start,
                    end,
                    kind: TokenKind::Number,
                    line_break_before,
                });
                index = end;
                line_break_before = false;
            }
            _ => {
                let end = symbol_end(source, index);
                tokens.push(Token {
                    start,
                    end,
                    kind: TokenKind::Symbol,
                    line_break_before,
                });
                index = end;
                line_break_before = false;
            }
        }
    }
    Ok(tokens)
}

fn is_closed_single(source: &str, end: usize) -> bool {
    end > 0 && source.as_bytes().get(end - 1) == Some(&b'\'')
}

fn is_closed_double(source: &str, end: usize) -> bool {
    end > 0 && source.as_bytes().get(end - 1) == Some(&b'"')
}

fn is_prefixed_string(source: &str, index: usize, byte: u8) -> bool {
    matches!(byte, b'e' | b'E' | b'b' | b'B' | b'x' | b'X')
        && source.as_bytes().get(index + 1) == Some(&b'\'')
        && (index == 0 || !highlight::is_word_byte(source.as_bytes()[index - 1]))
}

fn word_end(source: &str, start: usize) -> usize {
    let bytes = source.as_bytes();
    let mut end = start;
    while end < bytes.len() {
        if highlight::is_word_byte(bytes[end]) {
            end += 1;
        } else if bytes[end] == b'$' {
            // PostgreSQL permits dollar signs after the first character of an
            // unquoted identifier. Treating `foo$bar` as separate tokens would
            // turn one identifier into an operator expression.
            end += 1;
        } else if !bytes[end].is_ascii() {
            end += 1;
            while end < bytes.len() && !bytes[end].is_ascii() {
                end += 1;
            }
        } else {
            break;
        }
    }
    end
}

fn closed_block_comment_end(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0usize;
    let mut index = start;
    while index + 1 < bytes.len() {
        if bytes[index] == b'/' && bytes[index + 1] == b'*' {
            depth += 1;
            index += 2;
        } else if bytes[index] == b'*' && bytes[index + 1] == b'/' {
            depth = depth.saturating_sub(1);
            index += 2;
            if depth == 0 {
                return Some(index);
            }
        } else {
            index += 1;
        }
    }
    None
}

fn number_end(source: &str, start: usize) -> usize {
    let bytes = source.as_bytes();
    let mut end = start;
    while end < bytes.len() && (bytes[end].is_ascii_digit() || matches!(bytes[end], b'.' | b'_')) {
        end += 1;
    }
    let exponent = end;
    if bytes
        .get(end)
        .is_some_and(|byte| matches!(byte, b'e' | b'E'))
    {
        end += 1;
        if bytes
            .get(end)
            .is_some_and(|byte| matches!(byte, b'+' | b'-'))
        {
            end += 1;
        }
        let exponent_digits = end;
        while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'_') {
            end += 1;
        }
        if end == exponent_digits {
            end = exponent;
        }
    }
    end
}

fn symbol_end(source: &str, start: usize) -> usize {
    let bytes = source.as_bytes();
    if bytes[start] == b'$' && bytes.get(start + 1).is_some_and(u8::is_ascii_digit) {
        let mut end = start + 2;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        return end;
    }
    const OPERATORS: &[&str] = &[
        "!~~*", "!~~", "~~*", "!~*", "->>", "#>>", "?&", "?|", "@>", "<@", "#-", "#>", "->", "::",
        ":=", "=>", "||", "&&", "<=", ">=", "<>", "!=", "!~", "~*", "<<", ">>", "..", "^@",
    ];
    for operator in OPERATORS {
        if source[start..].starts_with(operator) {
            return start + operator.len();
        }
    }
    if is_operator_byte(bytes[start]) {
        let mut end = start + 1;
        while end < bytes.len() && is_operator_byte(bytes[end]) {
            end += 1;
        }
        return end;
    }
    let mut end = start + 1;
    while end < bytes.len() && !bytes[end].is_ascii() {
        end += 1;
    }
    end
}

fn is_operator_byte(byte: u8) -> bool {
    matches!(
        byte,
        b'!' | b'%'
            | b'&'
            | b'*'
            | b'+'
            | b'-'
            | b'/'
            | b'<'
            | b'='
            | b'>'
            | b'?'
            | b'@'
            | b'^'
            | b'|'
            | b'~'
    )
}

fn dollar_quote_end(source: &str, start: usize) -> Option<usize> {
    let tag_end = dollar_tag_end(source, start)?;
    let tag = &source[start..tag_end];
    source[tag_end..]
        .find(tag)
        .map(|relative| tag_end + relative + tag.len())
}

fn has_dollar_quote_opener(source: &str, start: usize) -> bool {
    dollar_tag_end(source, start).is_some()
}

fn dollar_tag_end(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    if bytes.get(start) != Some(&b'$') {
        return None;
    }
    let mut index = start + 1;
    if bytes.get(index).is_some_and(u8::is_ascii_digit) {
        return None;
    }
    while index < bytes.len() && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_') {
        index += 1;
    }
    (bytes.get(index) == Some(&b'$')).then_some(index + 1)
}

fn major_clause(word: &str) -> Option<Clause> {
    Some(match word {
        "with" => Clause::With,
        "select" => Clause::Select,
        "from" => Clause::From,
        "where" => Clause::Where,
        "join" | "left" | "right" | "full" | "inner" | "cross" => Clause::Join,
        "on" => Clause::On,
        "group" => Clause::Group,
        "having" => Clause::Having,
        "order" => Clause::Order,
        "values" => Clause::Values,
        "set" => Clause::Set,
        "returning" => Clause::Returning,
        "insert" | "update" | "delete" | "merge" | "union" | "except" | "intersect" | "limit"
        | "offset" | "fetch" | "for" => Clause::Other,
        _ => return None,
    })
}

fn is_boolean_word(word: &str) -> bool {
    matches!(word, "and" | "or")
}

fn is_join_modifier(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "left" | "right" | "full" | "inner" | "cross"
    )
}

fn is_operator_text(text: &str) -> bool {
    matches!(
        text,
        "=" | "<"
            | ">"
            | "<="
            | ">="
            | "<>"
            | "!="
            | "+"
            | "-"
            | "*"
            | "/"
            | "%"
            | "||"
            | "&&"
            | "->"
            | "->>"
            | "#>"
            | "#>>"
            | "@>"
            | "<@"
            | "?&"
            | "?|"
            | "~"
            | "~*"
            | "!~"
            | "!~*"
            | "^@"
    )
}

fn is_tight_operator(text: &str) -> bool {
    matches!(
        text,
        "." | "::" | "->" | "->>" | "#>" | "#>>" | "#-" | "[" | "]"
    )
}

fn is_unary_operator(text: &str, previous: Option<&str>) -> bool {
    matches!(text, "+" | "-" | "~")
        && previous.is_none_or(|previous| {
            let previous = previous.to_ascii_lowercase();
            matches!(
                previous.as_str(),
                "(" | "["
                    | ","
                    | "="
                    | "+"
                    | "-"
                    | "*"
                    | "/"
                    | "%"
                    | "and"
                    | "or"
                    | "not"
                    | "select"
                    | "where"
                    | "having"
                    | "on"
                    | "when"
                    | "then"
                    | "else"
                    | "returning"
                    | "values"
                    | "set"
            )
        })
}

fn source_position(source: &str, position: usize) -> (usize, usize) {
    let position = clamp_cursor(source, position);
    let before = &source[..position];
    let line = before.matches('\n').count() + 1;
    let column = before
        .rsplit_once('\n')
        .map_or(before.chars().count(), |(_, last)| last.chars().count())
        + 1;
    (line, column)
}

fn clamp_cursor(source: &str, cursor: usize) -> usize {
    let mut cursor = cursor.min(source.len());
    while cursor > 0 && !source.is_char_boundary(cursor) {
        cursor -= 1;
    }
    cursor
}

#[cfg(test)]
mod tests {
    use super::*;

    fn formatted(source: &str) -> String {
        format_sql(source, source.len())
            .expect("format should succeed")
            .text
    }

    fn protected(source: &str) -> Vec<&str> {
        scan(source)
            .expect("fixture should scan")
            .into_iter()
            .filter(|token| {
                matches!(
                    token.kind,
                    TokenKind::StringLiteral
                        | TokenKind::QuotedIdentifier
                        | TokenKind::DollarQuoted
                        | TokenKind::LineComment
                        | TokenKind::BlockComment
                )
            })
            .map(|token| token.text(source))
            .collect()
    }

    #[test]
    fn major_clauses_and_lists_get_a_stable_readable_layout() {
        let source = "select o.id,o.total from orders o where o.total>0 and o.status='ready';";
        assert_eq!(
            formatted(source),
            "select o.id,\n  o.total\nfrom orders o\nwhere o.total > 0\n  and o.status = 'ready';"
        );
    }

    #[test]
    fn joins_and_nested_queries_are_indented_without_a_parser() {
        let source = "SELECT a.id FROM accounts a LEFT JOIN (SELECT id FROM orders WHERE total>0) o ON o.id=a.id;";
        assert_eq!(
            formatted(source),
            "SELECT a.id\nFROM accounts a\nLEFT JOIN (\n  SELECT id\n  FROM orders\n  WHERE total > 0\n) o\nON o.id = a.id;"
        );
    }

    #[test]
    fn operators_casts_arrays_and_parameters_remain_lexically_separated() {
        assert_eq!(
            formatted("SELECT -$1::int + data[1] FROM t WHERE data->>'name'='x';"),
            "SELECT -$1::int + data[1]\nFROM t\nWHERE data->>'name' = 'x';"
        );
        assert_eq!(
            formatted("SELECT count(*) FROM t;"),
            "SELECT count(*)\nFROM t;"
        );
        assert_eq!(
            formatted("SELECT foo$bar, 1e2, .5e-1 FROM t;"),
            "SELECT foo$bar,\n  1e2,\n  .5e-1\nFROM t;"
        );
        assert_eq!(
            formatted("SELECT embedding<->query_vector FROM items;"),
            "SELECT embedding <-> query_vector\nFROM items;"
        );
    }

    #[test]
    fn multiple_statements_keep_order_and_semicolons() {
        assert_eq!(formatted("select 1;select 2"), "select 1;\nselect 2");
    }

    #[test]
    fn formatting_is_idempotent() {
        let once = formatted(
            "WITH recent AS (SELECT id,total FROM orders WHERE total>0) SELECT id,total FROM recent;",
        );
        let twice = formatted(&once);
        assert_eq!(once, twice);
    }

    #[test]
    fn protected_regions_are_copied_byte_for_byte() {
        let source = "SELECT 'a;  b', E'a\\'b', E'\u{1b}]52;c;SGVsbG8=', \"odd; name\", $$SELECT  1;\nDROP TABLE x;$$ /* outer /* inner; */ end */ -- keep;\nFROM t;";
        let result = formatted(source);
        for value in protected(source) {
            assert!(
                result.contains(value),
                "protected region changed: {value:?}"
            );
        }
        assert!(result.contains("SELECT  1;\nDROP TABLE x;"));
        assert!(result.contains("-- keep;"));
        assert!(result.contains('\u{1b}'), "protected control byte changed");
    }

    #[test]
    fn comments_keep_their_content_and_line_role() {
        let source = "SELECT 1\n-- a line comment\nFROM t /* block\n comment */ WHERE a=1;";
        let result = formatted(source);
        assert!(result.contains("-- a line comment\nFROM t"), "{result}");
        assert!(result.contains("/* block\n comment */"), "{result}");
    }

    #[test]
    fn empty_and_comment_only_buffers_are_unchanged() {
        for source in ["", "  \n", "-- only this\n", "/* only this */"] {
            let result = format_sql(source, source.len()).expect("no-op should succeed");
            assert_eq!(result.text, source);
            assert_eq!(result.cursor, source.len());
        }
    }

    #[test]
    fn unterminated_protected_regions_fail_closed() {
        for (source, kind) in [
            ("SELECT 'unfinished", ProtectedRegion::StringLiteral),
            ("SELECT \"unfinished", ProtectedRegion::QuotedIdentifier),
            ("SELECT $tag$ unfinished", ProtectedRegion::DollarQuotedBody),
            ("SELECT /* unfinished", ProtectedRegion::BlockComment),
        ] {
            let error = format_sql(source, source.len()).expect_err("must refuse");
            assert_eq!(
                error,
                FormatError::Unterminated {
                    kind,
                    position: source
                        .find(if kind == ProtectedRegion::StringLiteral {
                            '\''
                        } else if kind == ProtectedRegion::QuotedIdentifier {
                            '"'
                        } else if kind == ProtectedRegion::BlockComment {
                            '/'
                        } else {
                            '$'
                        })
                        .expect("opening delimiter")
                }
            );
            let message = error.message(source);
            assert!(!message.contains(source), "raw SQL leaked: {message}");
            assert!(message.contains("Format skipped"));
        }
        let nested = format_sql("SELECT /* outer /* inner */", 8).expect_err("must refuse");
        assert_eq!(
            nested,
            FormatError::Unterminated {
                kind: ProtectedRegion::BlockComment,
                position: 7
            }
        );
    }

    #[test]
    fn the_one_mib_limit_is_inclusive_and_refuses_before_formatting() {
        let exact = "x".repeat(MAX_FORMAT_BYTES);
        assert!(format_sql(&exact, exact.len()).is_ok());
        let over = "x".repeat(MAX_FORMAT_BYTES + 1);
        assert_eq!(
            format_sql(&over, over.len()),
            Err(FormatError::TooLarge {
                bytes: MAX_FORMAT_BYTES + 1
            })
        );
    }

    #[test]
    fn cursor_follows_the_same_unicode_token() {
        let source = "select   café,total from t";
        let source_start = source.find("café").expect("source token");
        let cursor = source_start + "ca".len();
        let result = format_sql(source, cursor).expect("format should succeed");
        let output_start = result.text.find("café").expect("output token");
        assert_eq!(result.cursor, output_start + "ca".len());
        assert!(result.text.is_char_boundary(result.cursor));
    }

    #[test]
    fn unicode_identifiers_do_not_merge_with_following_words() {
        assert_eq!(formatted("SELECT café   FROM t;"), "SELECT café\nFROM t;");
    }

    #[test]
    fn cursor_in_collapsed_whitespace_maps_to_an_output_boundary() {
        let source = "select    total from t";
        let cursor = source.find("   total").expect("gap") + 2;
        let result = format_sql(source, cursor).expect("format should succeed");
        assert!(result.text.is_char_boundary(result.cursor));
        assert!(result.cursor <= result.text.find("total").expect("total"));
    }

    #[test]
    fn debug_does_not_print_formatted_sql() {
        let result = format_sql("SELECT secret_value FROM private_table", 4).expect("format");
        let debug = format!("{result:?}");
        assert!(!debug.contains("secret_value"));
        assert!(!debug.contains("private_table"));
        assert!(debug.contains("<hidden>"));
    }
}
