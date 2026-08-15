//! Reading the catalogue.
//!
//! Two rules separate this module from [`crate::postgres::session`]:
//!
//! 1. **Metadata queries use the extended protocol with bound parameters.** User
//!    SQL uses the simple protocol because that preserves the server's own text
//!    rendering, but the simple protocol has no parameter binding, and a schema
//!    or relation name is attacker-controlled input the moment anyone can create
//!    an object. Interpolating one into a catalogue query would be an injection
//!    route straight through our own threat model. Here the shapes are known and
//!    fixed, so binding costs nothing.
//! 2. **Permission is reported, not assumed.** The catalogue is readable by
//!    everyone, so an object can be listed while its contents are not readable.
//!    Each entry carries whether the current role can actually select from it,
//!    and the interface says so rather than failing when the user opens it.
//!
//! Counts are gathered in one grouped query per catalogue rather than one query
//! per schema, so opening the tree on a database with hundreds of schemas is a
//! constant number of round trips.

use crate::diagnostics::Diagnostic;
use crate::postgres::error::from_query_error;
use std::collections::BTreeMap;
use tokio_postgres::Client;

/// A kind of database object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ObjectKind {
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
    /// An index.
    Index,
    /// An installed extension.
    Extension,
    /// A column of a relation.
    Column,
}

impl ObjectKind {
    /// The plural noun used for the group that holds objects of this kind.
    #[must_use]
    pub const fn plural(self) -> &'static str {
        match self {
            Self::Table => "tables",
            Self::View => "views",
            Self::MaterializedView => "materialized views",
            Self::Sequence => "sequences",
            Self::ForeignTable => "foreign tables",
            Self::PartitionedTable => "partitioned tables",
            Self::Function => "functions",
            Self::Index => "indexes",
            Self::Extension => "extensions",
            Self::Column => "columns",
        }
    }

    /// The singular noun, used in detail views and messages.
    #[must_use]
    pub const fn singular(self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::View => "view",
            Self::MaterializedView => "materialized view",
            Self::Sequence => "sequence",
            Self::ForeignTable => "foreign table",
            Self::PartitionedTable => "partitioned table",
            Self::Function => "function",
            Self::Index => "index",
            Self::Extension => "extension",
            Self::Column => "column",
        }
    }

    /// The `pg_class.relkind` character, for kinds that live in `pg_class`.
    #[must_use]
    pub const fn relkind(self) -> Option<&'static str> {
        match self {
            Self::Table => Some("r"),
            Self::View => Some("v"),
            Self::MaterializedView => Some("m"),
            Self::Sequence => Some("S"),
            Self::ForeignTable => Some("f"),
            Self::PartitionedTable => Some("p"),
            Self::Index => Some("i"),
            Self::Function | Self::Extension | Self::Column => None,
        }
    }

    /// Builds a kind from a `pg_class.relkind` character.
    #[must_use]
    pub fn from_relkind(value: &str) -> Option<Self> {
        match value {
            "r" => Some(Self::Table),
            "v" => Some(Self::View),
            "m" => Some(Self::MaterializedView),
            "S" => Some(Self::Sequence),
            "f" => Some(Self::ForeignTable),
            "p" => Some(Self::PartitionedTable),
            "i" => Some(Self::Index),
            _ => None,
        }
    }

    /// The kinds the object tree shows grouped beneath a schema, in order.
    pub const IN_SCHEMA: &'static [Self] = &[
        Self::Table,
        Self::PartitionedTable,
        Self::View,
        Self::MaterializedView,
        Self::Sequence,
        Self::ForeignTable,
        Self::Function,
    ];
}

/// A schema and how many of each kind of object it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaSummary {
    /// Schema name.
    pub name: String,
    /// Whether the current role may use the schema at all.
    pub usable: bool,
    /// Counts by kind. A kind absent from the map has none.
    pub counts: BTreeMap<ObjectKind, i64>,
}

impl SchemaSummary {
    /// Count for a kind, zero when there are none.
    #[must_use]
    pub fn count(&self, kind: ObjectKind) -> i64 {
        self.counts.get(&kind).copied().unwrap_or(0)
    }

    /// Total objects across every kind shown in the tree.
    #[must_use]
    pub fn total(&self) -> i64 {
        self.counts.values().sum()
    }
}

/// One object inside a schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectSummary {
    /// What it is.
    pub kind: ObjectKind,
    /// Schema it lives in.
    pub schema: String,
    /// Object name, exactly as stored.
    pub name: String,
    /// Whether the current role can read it. Listed objects may be unreadable.
    pub readable: bool,
    /// A short detail: a function's return type, an extension's version.
    pub detail: Option<String>,
}

impl ObjectSummary {
    /// The name qualified and quoted so it is safe to paste into SQL.
    #[must_use]
    pub fn qualified_sql(&self) -> String {
        format!(
            "{}.{}",
            quote_identifier(&self.schema),
            quote_identifier(&self.name)
        )
    }
}

/// A column of a relation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnInfo {
    /// Column name.
    pub name: String,
    /// Type as PostgreSQL formats it, for example `numeric(12,2)`.
    pub data_type: String,
    /// Whether the column accepts NULL.
    pub nullable: bool,
    /// Whether the column is part of the primary key.
    pub primary_key: bool,
    /// Column default, when one is defined.
    pub default: Option<String>,
}

/// Quotes an identifier for safe inclusion in SQL.
///
/// PostgreSQL doubles an embedded quote inside a quoted identifier. Quoting
/// unconditionally is deliberate: it is correct for reserved words, mixed case,
/// and names containing anything at all, and it never needs a judgement call
/// about which names are "safe".
#[must_use]
pub fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Lists schemas the current role may use, with counts by kind.
///
/// System schemas are excluded: they are noise for the job this tool does, and
/// showing them by default would bury the user's own objects.
pub async fn schemas(client: &Client) -> Result<Vec<SchemaSummary>, Diagnostic> {
    const SCHEMAS: &str = "SELECT n.nspname::text AS name, \
         has_schema_privilege(n.oid, 'USAGE') AS usable \
         FROM pg_catalog.pg_namespace n \
         WHERE n.nspname !~ '^pg_' AND n.nspname <> 'information_schema' \
         ORDER BY n.nspname";

    // One grouped query for every relation kind in every schema, rather than a
    // query per schema. Opening the tree costs the same on 3 schemas and 300.
    const RELATION_COUNTS: &str = "SELECT n.nspname::text AS schema, \
         c.relkind::text AS kind, count(*)::int8 AS total \
         FROM pg_catalog.pg_class c \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname !~ '^pg_' AND n.nspname <> 'information_schema' \
           AND c.relkind::text = ANY($1) \
         GROUP BY 1, 2";

    const FUNCTION_COUNTS: &str = "SELECT n.nspname::text AS schema, count(*)::int8 AS total \
         FROM pg_catalog.pg_proc p \
         JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace \
         WHERE n.nspname !~ '^pg_' AND n.nspname <> 'information_schema' \
         GROUP BY 1";

    let rows = client
        .query(SCHEMAS, &[])
        .await
        .map_err(|err| from_query_error(&err, 0))?;

    let mut summaries: Vec<SchemaSummary> = rows
        .iter()
        .map(|row| SchemaSummary {
            name: row.get::<_, String>("name"),
            usable: row.get::<_, bool>("usable"),
            counts: BTreeMap::new(),
        })
        .collect();

    let relkinds: Vec<String> = ObjectKind::IN_SCHEMA
        .iter()
        .filter_map(|kind| kind.relkind())
        .map(str::to_owned)
        .collect();

    let counts = client
        .query(RELATION_COUNTS, &[&relkinds])
        .await
        .map_err(|err| from_query_error(&err, 0))?;
    for row in counts {
        let schema: String = row.get("schema");
        let kind: String = row.get("kind");
        let total: i64 = row.get("total");
        if let Some(summary) = summaries.iter_mut().find(|s| s.name == schema)
            && let Some(kind) = ObjectKind::from_relkind(&kind)
        {
            summary.counts.insert(kind, total);
        }
    }

    let functions = client
        .query(FUNCTION_COUNTS, &[])
        .await
        .map_err(|err| from_query_error(&err, 0))?;
    for row in functions {
        let schema: String = row.get("schema");
        let total: i64 = row.get("total");
        if let Some(summary) = summaries.iter_mut().find(|s| s.name == schema) {
            summary.counts.insert(ObjectKind::Function, total);
        }
    }

    Ok(summaries)
}

/// Lists the objects of one kind inside one schema.
pub async fn objects(
    client: &Client,
    schema: &str,
    kind: ObjectKind,
) -> Result<Vec<ObjectSummary>, Diagnostic> {
    // Every name is bound, never interpolated. A schema called
    // `x'; DROP TABLE orders; --` is just a string that matches nothing.
    const RELATIONS: &str = "SELECT c.relname::text AS name, \
         has_table_privilege(c.oid, 'SELECT') AS readable, \
         obj_description(c.oid, 'pg_class')::text AS comment \
         FROM pg_catalog.pg_class c \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = $1 AND c.relkind::text = $2 \
         ORDER BY c.relname";

    const FUNCTIONS: &str = "SELECT p.proname::text AS name, \
         pg_get_function_result(p.oid)::text AS returns, \
         has_function_privilege(p.oid, 'EXECUTE') AS readable \
         FROM pg_catalog.pg_proc p \
         JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace \
         WHERE n.nspname = $1 \
         ORDER BY p.proname";

    let rows = match kind.relkind() {
        Some(relkind) => client
            .query(RELATIONS, &[&schema, &relkind])
            .await
            .map_err(|err| from_query_error(&err, 0))?
            .iter()
            .map(|row| ObjectSummary {
                kind,
                schema: schema.to_owned(),
                name: row.get::<_, String>("name"),
                readable: row.get::<_, bool>("readable"),
                detail: row.get::<_, Option<String>>("comment"),
            })
            .collect(),
        None if kind == ObjectKind::Function => client
            .query(FUNCTIONS, &[&schema])
            .await
            .map_err(|err| from_query_error(&err, 0))?
            .iter()
            .map(|row| ObjectSummary {
                kind,
                schema: schema.to_owned(),
                name: row.get::<_, String>("name"),
                readable: row.get::<_, bool>("readable"),
                detail: row
                    .get::<_, Option<String>>("returns")
                    .map(|r| format!("returns {r}")),
            })
            .collect(),
        None => Vec::new(),
    };

    Ok(rows)
}

/// Lists the columns of a relation, in catalogue order.
pub async fn columns(
    client: &Client,
    schema: &str,
    relation: &str,
) -> Result<Vec<ColumnInfo>, Diagnostic> {
    const COLUMNS: &str = "SELECT a.attname::text AS name, \
         format_type(a.atttypid, a.atttypmod)::text AS data_type, \
         a.attnotnull AS not_null, \
         coalesce(bool_or(i.indisprimary), false) AS primary_key, \
         pg_get_expr(d.adbin, d.adrelid)::text AS default_expression \
         FROM pg_catalog.pg_attribute a \
         JOIN pg_catalog.pg_class c ON c.oid = a.attrelid \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         LEFT JOIN pg_catalog.pg_index i \
              ON i.indrelid = c.oid AND i.indisprimary AND a.attnum = ANY(i.indkey) \
         LEFT JOIN pg_catalog.pg_attrdef d \
              ON d.adrelid = c.oid AND d.adnum = a.attnum \
         WHERE n.nspname = $1 AND c.relname = $2 \
           AND a.attnum > 0 AND NOT a.attisdropped \
         GROUP BY a.attname, a.atttypid, a.atttypmod, a.attnotnull, a.attnum, d.adbin, d.adrelid \
         ORDER BY a.attnum";

    let rows = client
        .query(COLUMNS, &[&schema, &relation])
        .await
        .map_err(|err| from_query_error(&err, 0))?;

    Ok(rows
        .iter()
        .map(|row| ColumnInfo {
            name: row.get::<_, String>("name"),
            data_type: row.get::<_, String>("data_type"),
            nullable: !row.get::<_, bool>("not_null"),
            primary_key: row.get::<_, bool>("primary_key"),
            default: row.get::<_, Option<String>>("default_expression"),
        })
        .collect())
}

/// Lists the indexes on a relation.
pub async fn indexes(
    client: &Client,
    schema: &str,
    relation: &str,
) -> Result<Vec<ObjectSummary>, Diagnostic> {
    const INDEXES: &str = "SELECT ic.relname::text AS name, \
         i.indisunique AS is_unique, i.indisprimary AS is_primary \
         FROM pg_catalog.pg_index i \
         JOIN pg_catalog.pg_class c ON c.oid = i.indrelid \
         JOIN pg_catalog.pg_class ic ON ic.oid = i.indexrelid \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = $1 AND c.relname = $2 \
         ORDER BY ic.relname";

    let rows = client
        .query(INDEXES, &[&schema, &relation])
        .await
        .map_err(|err| from_query_error(&err, 0))?;

    Ok(rows
        .iter()
        .map(|row| {
            let unique: bool = row.get("is_unique");
            let primary: bool = row.get("is_primary");
            ObjectSummary {
                kind: ObjectKind::Index,
                schema: schema.to_owned(),
                name: row.get::<_, String>("name"),
                readable: true,
                detail: Some(if primary {
                    "primary key".to_owned()
                } else if unique {
                    "unique".to_owned()
                } else {
                    "index".to_owned()
                }),
            }
        })
        .collect())
}

/// Lists installed extensions.
pub async fn extensions(client: &Client) -> Result<Vec<ObjectSummary>, Diagnostic> {
    const EXTENSIONS: &str = "SELECT e.extname::text AS name, \
         e.extversion::text AS version, n.nspname::text AS schema \
         FROM pg_catalog.pg_extension e \
         JOIN pg_catalog.pg_namespace n ON n.oid = e.extnamespace \
         ORDER BY e.extname";

    let rows = client
        .query(EXTENSIONS, &[])
        .await
        .map_err(|err| from_query_error(&err, 0))?;

    Ok(rows
        .iter()
        .map(|row| ObjectSummary {
            kind: ObjectKind::Extension,
            schema: row.get::<_, String>("schema"),
            name: row.get::<_, String>("name"),
            readable: true,
            detail: Some(format!("version {}", row.get::<_, String>("version"))),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_quoted_so_hostile_names_cannot_escape() {
        assert_eq!(quote_identifier("orders"), "\"orders\"");
        // Quoting unconditionally is what makes reserved words and mixed case work.
        assert_eq!(quote_identifier("select"), "\"select\"");
        assert_eq!(quote_identifier("Mixed Case"), "\"Mixed Case\"");
        // An embedded quote is doubled, which is PostgreSQL's own rule.
        assert_eq!(quote_identifier("we\"ird"), "\"we\"\"ird\"");
        // The shape someone would use to break out of a quoted identifier.
        assert_eq!(
            quote_identifier("x\"; DROP TABLE orders; --"),
            "\"x\"\"; DROP TABLE orders; --\""
        );
    }

    #[test]
    fn a_qualified_name_is_safe_to_paste_into_sql() {
        let object = ObjectSummary {
            kind: ObjectKind::Table,
            schema: "public".into(),
            name: "we\"ird name".into(),
            readable: true,
            detail: None,
        };
        assert_eq!(object.qualified_sql(), "\"public\".\"we\"\"ird name\"");
    }

    #[test]
    fn every_kind_has_a_singular_and_a_plural_noun() {
        for kind in [
            ObjectKind::Table,
            ObjectKind::View,
            ObjectKind::MaterializedView,
            ObjectKind::Sequence,
            ObjectKind::ForeignTable,
            ObjectKind::PartitionedTable,
            ObjectKind::Function,
            ObjectKind::Index,
            ObjectKind::Extension,
            ObjectKind::Column,
        ] {
            assert!(!kind.singular().is_empty());
            assert!(!kind.plural().is_empty());
            assert_ne!(kind.singular(), kind.plural());
        }
    }

    #[test]
    fn relkinds_round_trip() {
        for kind in ObjectKind::IN_SCHEMA {
            if let Some(relkind) = kind.relkind() {
                assert_eq!(ObjectKind::from_relkind(relkind), Some(*kind));
            }
        }
        assert_eq!(
            ObjectKind::from_relkind("t"),
            None,
            "toast tables are not shown"
        );
        assert_eq!(ObjectKind::from_relkind(""), None);
    }

    #[test]
    fn schema_counts_default_to_zero_rather_than_being_absent() {
        let mut counts = BTreeMap::new();
        counts.insert(ObjectKind::Table, 18);
        let summary = SchemaSummary {
            name: "public".into(),
            usable: true,
            counts,
        };
        assert_eq!(summary.count(ObjectKind::Table), 18);
        assert_eq!(summary.count(ObjectKind::View), 0);
        assert_eq!(summary.total(), 18);
    }
}
