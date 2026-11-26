//! Saved queries: ordinary `.sql` files in a directory the user owns.
//!
//! `config paths` has advertised this directory since the first release and
//! nothing wrote to it. It does now, and deliberately as plain files: a query
//! saved here can be opened in any editor, kept in a repository, sent to a
//! colleague, or read by `psql`. Nothing about it is this product's format.
//!
//! Queries may live directly in the root directory (`monthly-revenue`) or inside
//! a single-level collection folder (`billing/monthly-revenue`). Optional header
//! comments (`-- @tag:`, `-- @tags:`, `-- @description:`) supply metadata while
//! keeping every file 100% valid PostgreSQL SQL.
//!
//! A name is not a path. Every segment is checked before it reaches the
//! filesystem: at most one `/`, no `\`, no `..`, no control characters, no
//! Windows-reserved device names, and non-symlink directory/file checks on disk.
//! That check is a security boundary and is tested as one.

use crate::diagnostics::{Diagnostic, DiagnosticKind};
use std::io::Read;
use std::path::{Path, PathBuf};

/// Names Windows will not give to a file, whatever extension follows them.
const RESERVED_ON_WINDOWS: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// The extension every saved query has.
pub const EXTENSION: &str = "sql";

/// Maximum `.sql` file size read by [`Library::load`] and [`Library::list`] (1 MiB).
pub const MAX_QUERY_FILE_BYTES: u64 = 1_048_576;

/// Maximum number of lines retained in [`SavedQuery::preview`].
pub const MAX_PREVIEW_LINES: usize = 24;

/// Maximum byte budget retained in [`SavedQuery::preview`].
pub const MAX_PREVIEW_BYTES: usize = 2048;

/// One saved query.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SavedQuery {
    /// The name, without the extension (for example `monthly-revenue` or `billing/monthly-revenue`).
    pub name: String,
    /// Optional single-level collection directory name (for example `billing`).
    pub collection: Option<String>,
    /// Normalized tags parsed from `-- @tag:` or `-- @tags:` header comments.
    pub tags: Vec<String>,
    /// Optional description parsed from `-- @description:` header comments.
    pub description: Option<String>,
    /// Where it is.
    pub path: PathBuf,
    /// The first line of it, for the list.
    pub summary: String,
    /// Bounded, display-safe multi-line SQL preview.
    pub preview: String,
}

/// The directory saved queries live in.
#[derive(Debug, Clone)]
pub struct Library {
    directory: PathBuf,
}

impl Library {
    /// Opens the library at the configured location.
    #[must_use]
    pub fn new(directory: &Path) -> Self {
        Self {
            directory: directory.to_path_buf(),
        }
    }

    /// Where the files are.
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// The file a name refers to, refusing anything that is not a flat name or
    /// single-level `collection/query-name`.
    ///
    /// This is the lexical name boundary: at most one `/`, non-empty segments
    /// before and after `/`, no `\` separators, no `.` or `..`, no `:`, no
    /// control characters, and no Windows-reserved device names in either segment.
    /// Everything else in this module goes through here. Reads and writes also
    /// refuse linked/non-regular entries via `symlink_metadata`.
    pub fn path_for(&self, name: &str) -> Result<PathBuf, Diagnostic> {
        self.resolve_target(name).map(|(_, path)| path)
    }

    /// Resolves a query name into its optional collection name and safe path.
    fn resolve_target(&self, name: &str) -> Result<(Option<String>, PathBuf), Diagnostic> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return refuse_name(name, "a name cannot be empty");
        }
        if trimmed.chars().any(char::is_control) {
            return refuse_name(name, "a name cannot contain control characters");
        }
        if trimmed.contains('\\') {
            return refuse_name(name, "a name cannot contain a '\\' path separator");
        }
        if trimmed.contains(':') {
            // A Windows drive or stream specifier is a path, not a name.
            return refuse_name(name, "a name cannot contain a colon");
        }
        if trimmed.starts_with('/') || trimmed.ends_with('/') {
            return refuse_name(name, "a name cannot start or end with a '/' path separator");
        }

        if let Some((raw_col, raw_query)) = trimmed.split_once('/') {
            if raw_query.contains('/') {
                return refuse_name(
                    name,
                    "a name cannot contain more than one '/' path separator",
                );
            }
            let collection = validate_collection_segment(name, raw_col)?;
            let file = validate_query_segment(name, raw_query)?;
            Ok((
                Some(collection.clone()),
                self.directory.join(collection).join(file),
            ))
        } else {
            let file = validate_query_segment(name, trimmed)?;
            Ok((None, self.directory.join(file)))
        }
    }

    /// Saves a query, replacing one of the same name.
    ///
    /// Replacing is the expected behaviour of saving under a name you chose;
    /// the editor is where the previous text was, and it is still there.
    pub fn save(&self, name: &str, sql: &str) -> Result<PathBuf, Diagnostic> {
        let (collection, path) = self.resolve_target(name)?;
        std::fs::create_dir_all(&self.directory).map_err(|err| self.failure("creating", &err))?;
        if let Some(col) = collection {
            let col_dir = self.directory.join(col);
            self.check_collection_dir(&col_dir)?;
            std::fs::create_dir_all(&col_dir).map_err(|err| self.failure("creating", &err))?;
            self.check_collection_dir(&col_dir)?;
        }
        self.check_entry(&path)?;
        let mut text = sql.to_owned();
        if !text.ends_with('\n') {
            text.push('\n');
        }
        crate::platform::write_atomic(&path, text.as_bytes())
            .map_err(|err| self.failure("writing to", &err))?;
        Ok(path)
    }

    /// Reads a saved query, refusing symlinks, non-regular entries, and files over 1 MiB.
    pub fn load(&self, name: &str) -> Result<String, Diagnostic> {
        let (collection, path) = self.resolve_target(name)?;
        if let Some(col) = collection {
            let col_dir = self.directory.join(col);
            self.check_collection_dir(&col_dir)?;
        }
        self.check_entry(&path)?;
        read_bounded_query_file(&path)
    }

    /// Every saved query across the root directory and 1-level real collection
    /// subdirectories, sorted deterministically by name.
    ///
    /// A file that cannot be read as UTF-8 is listed with `"could not be read"`
    /// rather than failing the listing: a directory of `.sql` files is a place
    /// people put things by hand, and one unreadable file must not hide the rest.
    /// Symlinks, nested subdirectories beyond depth 1, and files larger than
    /// 1 MiB are skipped without following or loading oversized payloads.
    pub fn list(&self) -> Result<Vec<SavedQuery>, Diagnostic> {
        let entries = match std::fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(self.failure("reading", &err)),
        };

        let mut found = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.file_type().is_file() {
                if let Some(item) = self.inspect_query_file(None, &path, &meta) {
                    found.push(item);
                }
            } else if meta.file_type().is_dir() {
                let Some(col_name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if validate_collection_name(col_name).is_err() {
                    continue;
                }
                let Ok(sub_entries) = std::fs::read_dir(&path) else {
                    continue;
                };
                for sub_entry in sub_entries.flatten() {
                    let child_path = sub_entry.path();
                    let Ok(child_meta) = std::fs::symlink_metadata(&child_path) else {
                        continue;
                    };
                    if child_meta.file_type().is_symlink() || !child_meta.file_type().is_file() {
                        continue;
                    }
                    if let Some(item) =
                        self.inspect_query_file(Some(col_name), &child_path, &child_meta)
                    {
                        found.push(item);
                    }
                }
            }
        }
        found.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(found)
    }

    /// Lists saved queries filtered optionally by collection name and/or `#tag`.
    pub fn list_filtered(
        &self,
        collection: Option<&str>,
        tag: Option<&str>,
    ) -> Result<Vec<SavedQuery>, Diagnostic> {
        let collection_filter = match collection {
            Some(col) => Some(validate_collection_name(col)?),
            None => None,
        };
        let tag_filter = match tag {
            Some(raw) => {
                let normalized = normalize_tag(raw);
                if normalized.is_empty() {
                    return Err(Diagnostic::new(
                        DiagnosticKind::Usage,
                        "tag filter cannot be empty",
                        "filtering saved queries",
                    )
                    .next_action("pass a non-empty tag name, for example --tag finance"));
                }
                Some(normalized)
            }
            None => None,
        };

        let mut items = self.list()?;
        items.retain(|query| {
            let col_ok = collection_filter
                .as_ref()
                .is_none_or(|wanted| query.collection.as_deref() == Some(wanted.as_str()));
            let tag_ok = tag_filter
                .as_ref()
                .is_none_or(|wanted| query.tags.iter().any(|t| t.eq_ignore_ascii_case(wanted)));
            col_ok && tag_ok
        });
        Ok(items)
    }

    fn inspect_query_file(
        &self,
        collection: Option<&str>,
        path: &Path,
        meta: &std::fs::Metadata,
    ) -> Option<SavedQuery> {
        if !path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case(EXTENSION))
        {
            return None;
        }
        let stem = path.file_stem().and_then(|stem| stem.to_str())?;
        let full_name = match collection {
            Some(col) => format!("{col}/{stem}"),
            None => stem.to_owned(),
        };
        if self.resolve_target(&full_name).is_err() {
            return None;
        }
        if meta.len() > MAX_QUERY_FILE_BYTES {
            return None;
        }

        match read_bounded_query_bytes(path, meta.len()) {
            Ok(Some(text)) => {
                let parsed = parse_query_metadata_and_preview(&text);
                Some(SavedQuery {
                    name: full_name,
                    collection: collection.map(str::to_owned),
                    tags: parsed.tags,
                    description: parsed.description,
                    path: path.to_path_buf(),
                    summary: summarise(&text),
                    preview: parsed.preview,
                })
            }
            Ok(None) => None,
            Err(_) => Some(SavedQuery {
                name: full_name,
                collection: collection.map(str::to_owned),
                tags: Vec::new(),
                description: None,
                path: path.to_path_buf(),
                summary: "could not be read".to_owned(),
                preview: String::new(),
            }),
        }
    }

    /// Any collection folder inside the library root must be a real directory,
    /// never a symlink or regular file.
    fn check_collection_dir(&self, col_dir: &Path) -> Result<(), Diagnostic> {
        match std::fs::symlink_metadata(col_dir) {
            Ok(metadata) if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() => {
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(self.failure("reading", &error)),
            Ok(_) => Err(Diagnostic::new(
                DiagnosticKind::Usage,
                "a saved query collection must be a real directory, not a link or file",
                "opening or saving a named query",
            )
            .next_action(
                "choose another collection name, or replace the non-directory entry with a real directory",
            )
            .technical("Path", col_dir.display().to_string())),
        }
    }

    /// The configured root is trusted, but a named leaf must be a plain file.
    fn check_entry(&self, path: &Path) -> Result<(), Diagnostic> {
        match std::fs::symlink_metadata(path) {
            Ok(metadata)
                if metadata.file_type().is_file() && !metadata.file_type().is_symlink() =>
            {
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(self.failure("reading", &error)),
            Ok(_) => Err(Diagnostic::new(
                DiagnosticKind::Usage,
                "a saved query must be a regular file, not a link or directory",
                "opening or saving a named query",
            )
            .next_action("choose another name, or replace the linked or non-regular entry with a plain .sql file")
            .technical("Path", path.display().to_string())),
        }
    }

    fn failure(&self, verb: &str, error: &std::io::Error) -> Diagnostic {
        Diagnostic::new(
            DiagnosticKind::Usage,
            format!("could not {verb} {}", self.directory.display()),
            "using the saved queries directory",
        )
        .likely_cause(error.to_string())
        .next_action("check the directory's permissions and free space")
    }
}

/// Validates a standalone collection directory name using the exact same rules
/// enforced by [`Library::path_for`].
pub fn validate_collection_name(name: &str) -> Result<String, Diagnostic> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return refuse_name(name, "a collection name cannot be empty");
    }
    if trimmed.chars().any(char::is_control) {
        return refuse_name(name, "a collection name cannot contain control characters");
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        return refuse_name(name, "a collection name cannot contain a path separator");
    }
    if trimmed.contains(':') {
        return refuse_name(name, "a collection name cannot contain a colon");
    }
    validate_collection_segment(name, trimmed)
}

fn validate_collection_segment(full_name: &str, segment: &str) -> Result<String, Diagnostic> {
    if segment.is_empty() || segment != segment.trim() {
        return refuse_name(
            full_name,
            "a collection name cannot be empty or have surrounding whitespace",
        );
    }
    if segment == "." || segment == ".." || segment.contains("..") {
        return refuse_name(full_name, "a name cannot walk up a directory");
    }
    check_windows_reserved(full_name, segment)?;
    Ok(segment.to_owned())
}

fn validate_query_segment(full_name: &str, segment: &str) -> Result<String, Diagnostic> {
    if segment.is_empty() || segment != segment.trim() {
        return refuse_name(
            full_name,
            "a query name cannot be empty or have surrounding whitespace",
        );
    }
    if segment == "." || segment == ".." || segment.contains("..") {
        return refuse_name(full_name, "a name cannot walk up a directory");
    }
    let (stem, file) = match segment.rsplit_once('.') {
        Some((stem, extension)) if extension.eq_ignore_ascii_case(EXTENSION) => {
            (stem, segment.to_owned())
        }
        _ => (segment, format!("{segment}.{EXTENSION}")),
    };
    if stem.is_empty() {
        return refuse_name(full_name, "a name cannot be empty");
    }
    if stem == "." || stem == ".." || stem.contains("..") {
        return refuse_name(full_name, "a name cannot walk up a directory");
    }
    check_windows_reserved(full_name, stem)?;
    Ok(file)
}

fn check_windows_reserved(full_name: &str, segment: &str) -> Result<(), Diagnostic> {
    // Windows reserves these whatever extension follows them: `CON.sql`
    // opens the console, not a file. Refused on every platform, because a
    // saved query is meant to travel between them.
    let device = segment.split('.').next().unwrap_or(segment);
    if RESERVED_ON_WINDOWS
        .iter()
        .any(|reserved| device.eq_ignore_ascii_case(reserved))
    {
        return refuse_name(full_name, "Windows reserves this name for a device");
    }
    Ok(())
}

fn refuse_name<T>(name: &str, reason: &str) -> Result<T, Diagnostic> {
    Err(Diagnostic::new(
        DiagnosticKind::Usage,
        format!("{name:?} is not a name a query can be saved under"),
        "naming a saved query",
    )
    .likely_cause(reason.to_owned())
    .next_action("use a plain name or collection/name, for example billing/monthly-revenue"))
}

fn read_bounded_query_file(path: &Path) -> Result<String, Diagnostic> {
    let metadata = std::fs::symlink_metadata(path).map_err(|err| read_failure(path, &err))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(Diagnostic::new(
            DiagnosticKind::Usage,
            "a saved query must be a regular file, not a link or directory",
            "opening or saving a named query",
        )
        .next_action("choose another name, or replace the linked or non-regular entry with a plain .sql file")
        .technical("Path", path.display().to_string()));
    }
    if metadata.len() > MAX_QUERY_FILE_BYTES {
        return Err(oversized_query_diagnostic(path, metadata.len()));
    }

    let file = std::fs::File::open(path).map_err(|err| read_failure(path, &err))?;
    let mut bytes = Vec::with_capacity(
        usize::try_from(metadata.len().min(MAX_QUERY_FILE_BYTES)).unwrap_or(4096),
    );
    file.take(MAX_QUERY_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|err| read_failure(path, &err))?;
    if bytes.len() as u64 > MAX_QUERY_FILE_BYTES {
        return Err(oversized_query_diagnostic(path, bytes.len() as u64));
    }

    String::from_utf8(bytes).map_err(|err| {
        Diagnostic::new(
            DiagnosticKind::Usage,
            format!("could not read {}", path.display()),
            "opening a saved query",
        )
        .likely_cause(err.to_string())
        .next_action("ensure the saved query file is valid UTF-8")
    })
}

/// Returns `Ok(Some(text))` within the 1 MiB bound, `Ok(None)` if oversized, or
/// `Err(())` if unreadable or invalid UTF-8.
fn read_bounded_query_bytes(path: &Path, known_len: u64) -> Result<Option<String>, ()> {
    if known_len > MAX_QUERY_FILE_BYTES {
        return Ok(None);
    }
    let file = std::fs::File::open(path).map_err(|_| ())?;
    let mut bytes = Vec::with_capacity(usize::try_from(known_len.min(65_536)).unwrap_or(4096));
    file.take(MAX_QUERY_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    if bytes.len() as u64 > MAX_QUERY_FILE_BYTES {
        return Ok(None);
    }
    String::from_utf8(bytes).map(Some).map_err(|_| ())
}

fn read_failure(path: &Path, err: &std::io::Error) -> Diagnostic {
    Diagnostic::new(
        DiagnosticKind::Usage,
        format!("could not read {}", path.display()),
        "opening a saved query",
    )
    .likely_cause(err.to_string())
    .next_action("check the name, or list what is saved")
}

fn oversized_query_diagnostic(path: &Path, bytes: u64) -> Diagnostic {
    Diagnostic::new(
        DiagnosticKind::Usage,
        format!(
            "saved query {} is {bytes} bytes, over the 1 MiB limit",
            path.display()
        ),
        "opening a saved query",
    )
    .likely_cause("the saved query file exceeds the 1 MiB read bound")
    .next_action("trim the file below 1 MiB or open it directly outside the saved library")
}

struct ParsedQueryMetadata {
    tags: Vec<String>,
    description: Option<String>,
    preview: String,
}

fn parse_query_metadata_and_preview(text: &str) -> ParsedQueryMetadata {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut tags: Vec<String> = Vec::new();
    let mut description: Option<String> = None;
    let mut in_header = true;
    let mut preview_source_lines: Vec<&str> = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if in_header {
            if trimmed.is_empty() {
                continue;
            }
            if let Some(comment_body) = trimmed.strip_prefix("--") {
                let body = comment_body.trim();
                if let Some(raw_tags) = strip_prefix_ascii_ci(body, "@tags:")
                    .or_else(|| strip_prefix_ascii_ci(body, "@tag:"))
                {
                    for token in raw_tags.split(|c: char| c == ',' || c.is_whitespace()) {
                        let norm = normalize_tag(token);
                        if !norm.is_empty() && !tags.iter().any(|existing| existing == &norm) {
                            tags.push(norm);
                        }
                    }
                    continue;
                }
                if let Some(raw_desc) = strip_prefix_ascii_ci(body, "@description:") {
                    let clean = crate::query::sanitize_for_display(raw_desc.trim());
                    if !clean.is_empty() && description.is_none() {
                        description = Some(clean);
                    }
                    continue;
                }
                preview_source_lines.push(line);
                continue;
            }
            in_header = false;
        }
        preview_source_lines.push(line);
    }

    while preview_source_lines
        .last()
        .is_some_and(|line| line.trim().is_empty())
    {
        preview_source_lines.pop();
    }

    let preview = build_bounded_preview(&preview_source_lines);
    ParsedQueryMetadata {
        tags,
        description,
        preview,
    }
}

fn build_bounded_preview(lines: &[&str]) -> String {
    let mut out = String::new();
    let mut truncated = false;

    for (idx, line) in lines.iter().enumerate() {
        if idx >= MAX_PREVIEW_LINES {
            truncated = true;
            break;
        }
        let clean = crate::query::sanitize_for_display(line);
        let needed = clean.len() + usize::from(!out.is_empty());
        if out.len() + needed > MAX_PREVIEW_BYTES {
            let budget = MAX_PREVIEW_BYTES.saturating_sub(out.len() + usize::from(!out.is_empty()));
            if budget > 0 {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(truncate_utf8_prefix(&clean, budget));
            }
            truncated = true;
            break;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&clean);
    }

    if truncated {
        if !out.is_empty() && out.len() + 4 <= MAX_PREVIEW_BYTES {
            out.push_str("\n...");
        } else if out.is_empty() {
            out.push_str("...");
        }
    }
    out
}

fn truncate_utf8_prefix(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

fn strip_prefix_ascii_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    if head.eq_ignore_ascii_case(prefix) {
        Some(&text[prefix.len()..])
    } else {
        None
    }
}

fn normalize_tag(raw: &str) -> String {
    let trimmed = raw.trim().trim_start_matches('#');
    let sanitized = crate::query::sanitize_for_display(trimmed);
    sanitized.trim().to_lowercase()
}

/// The first line worth showing from a saved query.
///
/// Comments are skipped: a file that starts with a header comment should be
/// listed by what it does, not by who wrote it.
fn summarise(text: &str) -> String {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("--"))
        .unwrap_or("");
    let sanitised = crate::query::sanitize_for_display(line);
    crate::query::truncate_to_width(&sanitised, 60, true)
}

#[cfg(test)]
mod tests;
