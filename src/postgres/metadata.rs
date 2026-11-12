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

use crate::diagnostics::{Diagnostic, DiagnosticKind};
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

/// The live relation facts required before a result-cell update can be planned.
///
/// The relation name is resolved in the same session that will execute the
/// generated statement. That matters for an unqualified source query: the
/// session's effective `search_path`, not the client's assumptions, decides
/// which relation is addressed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateRelation {
    /// The schema selected by PostgreSQL.
    pub schema: String,
    /// The relation name exactly as stored by PostgreSQL.
    pub relation: String,
    /// The live relation kind.
    pub kind: ObjectKind,
    /// Whether the current role may select from it.
    pub readable: bool,
    /// Whether the current role may update it.
    pub writable: bool,
    /// Columns and primary-key membership in catalogue order.
    pub columns: Vec<ColumnInfo>,
}

pub use crate::query::identifiers::quote_identifier;

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

/// Resolves one source relation and reads the permission and key facts needed
/// for a generated cell update.
///
/// Schema and relation names are data throughout this query. In particular,
/// an unqualified relation is matched against the session's effective
/// `current_schemas(false)` in search-path order, while a qualified relation
/// is matched exactly. No name is interpolated into catalogue SQL.
pub async fn update_relation(
    client: &Client,
    schema: Option<&str>,
    relation: &str,
) -> Result<UpdateRelation, Diagnostic> {
    const RELATION: &str = "SELECT n.nspname::text AS schema, \
         c.relname::text AS relation, c.relkind::text AS relkind, \
         has_table_privilege(c.oid, 'SELECT') AS readable, \
         has_table_privilege(c.oid, 'UPDATE') AS writable \
         FROM pg_catalog.pg_class c \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         WHERE c.relname = $1 \
           AND (($2::text IS NOT NULL AND n.nspname = $2) \
             OR ($2::text IS NULL AND n.nspname = ANY(pg_catalog.current_schemas(false)))) \
         ORDER BY CASE WHEN $2::text IS NULL \
                      THEN pg_catalog.array_position(pg_catalog.current_schemas(false), n.nspname) \
                      ELSE 0 END, n.nspname \
         LIMIT 1";

    let schema_value = schema.map(str::to_owned);
    let row = client
        .query_opt(RELATION, &[&relation, &schema_value])
        .await
        .map_err(|err| from_query_error(&err, 0))?
        .ok_or_else(|| {
            let qualified = schema.map_or_else(
                || format!("unqualified relation {relation:?}"),
                |schema| {
                    format!(
                        "{}.{}",
                        quote_identifier(schema),
                        quote_identifier(relation)
                    )
                },
            );
            Diagnostic::new(
                DiagnosticKind::Query,
                format!("could not resolve {qualified} for a cell update"),
                "reading update target metadata",
            )
            .likely_cause("the relation is gone, outside the session search path, or not visible")
            .next_action("refresh the result and check the relation name and privileges")
        })?;

    let relkind: String = row.get("relkind");
    let kind = ObjectKind::from_relkind(&relkind).ok_or_else(|| {
        Diagnostic::new(
            DiagnosticKind::Query,
            "the selected relation kind cannot be updated from a result cell",
            "reading update target metadata",
        )
        .likely_cause(format!("PostgreSQL reported relkind {relkind:?}"))
        .next_action("edit an ordinary or partitioned table result instead")
    })?;
    let resolved_schema: String = row.get("schema");
    let resolved_relation: String = row.get("relation");
    let columns = columns(client, &resolved_schema, &resolved_relation).await?;

    Ok(UpdateRelation {
        schema: resolved_schema,
        relation: resolved_relation,
        kind,
        readable: row.get("readable"),
        writable: row.get("writable"),
        columns,
    })
}

/// Reads the relation and function names used by local completion in one
/// snapshot. The query shapes are fixed and every value that comes from the
/// server is returned as data; no identifier is interpolated into catalogue
/// SQL. This is intentionally a separate read from the tree counts: the tree
/// can remain useful if a server role permits one metadata query but not the
/// other, and completion can say which snapshot it actually has.
pub async fn completion_catalog(
    client: &Client,
) -> Result<crate::query::completion::CompletionCatalog, Diagnostic> {
    use crate::query::completion::{
        CatalogColumn, CatalogObject, CatalogObjectKind, CatalogRelation,
    };

    const RELATIONS: &str = "SELECT n.nspname::text AS schema, \
         c.relname::text AS name, c.relkind::text AS relkind, \
         has_table_privilege(c.oid, 'SELECT') AS readable \
         FROM pg_catalog.pg_class c \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname !~ '^pg_' AND n.nspname <> 'information_schema' \
           AND c.relkind::text = ANY($1) \
         ORDER BY n.nspname, c.relname, c.relkind";
    const FUNCTIONS: &str = "SELECT n.nspname::text AS schema, \
         p.proname::text AS name, pg_get_function_result(p.oid)::text AS returns, \
         has_function_privilege(p.oid, 'EXECUTE') AS readable \
         FROM pg_catalog.pg_proc p \
         JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace \
         WHERE n.nspname !~ '^pg_' AND n.nspname <> 'information_schema' \
         ORDER BY n.nspname, p.proname, p.oid";
    const COLUMNS: &str = "SELECT n.nspname::text AS schema, c.relname::text AS relation, \
         a.attname::text AS name, format_type(a.atttypid, a.atttypmod)::text AS data_type, \
         a.attnum::int4 AS position \
         FROM pg_catalog.pg_attribute a \
         JOIN pg_catalog.pg_class c ON c.oid = a.attrelid \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname !~ '^pg_' AND n.nspname <> 'information_schema' \
           AND c.relkind::text = ANY($1) \
           AND a.attnum > 0 AND NOT a.attisdropped \
         ORDER BY n.nspname, c.relname, a.attnum";

    let relkinds = ["r", "v", "m", "S", "f", "p"]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();

    let relation_rows = client
        .query(RELATIONS, &[&relkinds])
        .await
        .map_err(|err| from_query_error(&err, 0))?;
    let function_rows = client
        .query(FUNCTIONS, &[])
        .await
        .map_err(|err| from_query_error(&err, 0))?;
    let column_rows = client
        .query(COLUMNS, &[&relkinds])
        .await
        .map_err(|err| from_query_error(&err, 0))?;

    let mut objects = relation_rows
        .iter()
        .filter_map(|row| {
            let kind = CatalogObjectKind::from_relkind(row.get::<_, String>("relkind").as_str())?;
            Some(CatalogObject {
                kind,
                schema: row.get("schema"),
                name: row.get("name"),
                readable: row.get("readable"),
                detail: None,
            })
        })
        .collect::<Vec<_>>();
    objects.extend(function_rows.iter().map(|row| {
        CatalogObject {
            kind: CatalogObjectKind::Function,
            schema: row.get("schema"),
            name: row.get("name"),
            readable: row.get("readable"),
            detail: row
                .get::<_, Option<String>>("returns")
                .map(|returns| format!("returns {returns}")),
        }
    }));

    let mut relations = Vec::<CatalogRelation>::new();
    for row in &column_rows {
        let schema: String = row.get("schema");
        let name: String = row.get("relation");
        let Some(relation) = relations
            .iter_mut()
            .find(|relation| relation.schema == schema && relation.name == name)
        else {
            relations.push(CatalogRelation {
                schema: schema.clone(),
                name: name.clone(),
                columns: Vec::new(),
            });
            let relation = relations.last_mut().expect("just pushed");
            relation.columns.push(CatalogColumn {
                name: row.get("name"),
                data_type: row.get("data_type"),
            });
            continue;
        };
        relation.columns.push(CatalogColumn {
            name: row.get("name"),
            data_type: row.get("data_type"),
        });
    }

    Ok(crate::query::completion::CompletionCatalog { objects, relations })
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

/// Where the text of a definition came from.
///
/// The distinction is the honest part: PostgreSQL can render a view, an index
/// and a function exactly as it will execute them, and cannot do that for a
/// table. A table's structure is therefore assembled from the catalogue and
/// labelled as such, so nobody mistakes a description for a `pg_dump` script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefinitionSource {
    /// The server rendered it, through `pg_get_viewdef` and friends.
    Server,
    /// Assembled here from catalogue rows.
    Assembled,
}

impl DefinitionSource {
    /// The sentence shown above the text.
    #[must_use]
    pub const fn note(self) -> &'static str {
        match self {
            Self::Server => "As PostgreSQL renders it.",
            Self::Assembled => {
                "Assembled from the catalogue. A description of the object, not a \
                 script that recreates it."
            }
        }
    }
}

/// An object's definition, as text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    /// What the object is.
    pub kind: ObjectKind,
    /// Schema it lives in.
    pub schema: String,
    /// Its name.
    pub name: String,
    /// Where the text came from.
    pub source: DefinitionSource,
    /// The definition itself.
    pub text: String,
}

impl Definition {
    /// A one-line heading: what this is and what it is called.
    #[must_use]
    pub fn heading(&self) -> String {
        format!(
            "{} {}.{}",
            self.kind.singular(),
            quote_identifier(&self.schema),
            quote_identifier(&self.name)
        )
    }

    /// Canonical `DROP ... IF EXISTS ... CASCADE;` statement for this object.
    #[must_use]
    pub fn drop_statement(&self) -> String {
        crate::ddl::canonical_drop_statement(self.kind, &self.schema, &self.name, None)
    }

    /// Returns the definition text optionally prefixed by its `DROP ... IF EXISTS` statement.
    #[must_use]
    pub fn full_script(&self, include_drop: bool) -> String {
        if include_drop {
            format!("{}\n\n{}", self.drop_statement(), self.text)
        } else {
            self.text.clone()
        }
    }
}

/// Reads an object's definition.
///
/// Every catalogue lookup binds the schema and the name as parameters, exactly
/// as everywhere else in this module: an object named to break a client that
/// interpolates identifiers must be as safe to inspect as it is to list.
pub async fn definition(client: &Client, object: &ObjectSummary) -> Result<Definition, Diagnostic> {
    let (schema, name) = (object.schema.as_str(), object.name.as_str());
    let (source, text) = match object.kind {
        ObjectKind::View | ObjectKind::MaterializedView => {
            let rendered = one_text(
                client,
                "SELECT pg_catalog.pg_get_viewdef(c.oid, true)::text AS definition \
                 FROM pg_catalog.pg_class c \
                 JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
                 WHERE n.nspname = $1 AND c.relname = $2",
                schema,
                name,
            )
            .await?;
            let keyword = if object.kind == ObjectKind::MaterializedView {
                "CREATE MATERIALIZED VIEW"
            } else {
                "CREATE OR REPLACE VIEW"
            };
            (
                DefinitionSource::Server,
                format!(
                    "{keyword} {}.{} AS\n{}",
                    quote_identifier(schema),
                    quote_identifier(name),
                    rendered.trim_end()
                ),
            )
        }
        ObjectKind::Index => (
            DefinitionSource::Server,
            one_text(
                client,
                "SELECT pg_catalog.pg_get_indexdef(c.oid)::text AS definition \
                 FROM pg_catalog.pg_class c \
                 JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
                 WHERE n.nspname = $1 AND c.relname = $2",
                schema,
                name,
            )
            .await?,
        ),
        ObjectKind::Function => (
            DefinitionSource::Server,
            one_text(
                client,
                "SELECT pg_catalog.pg_get_functiondef(p.oid)::text AS definition \
                 FROM pg_catalog.pg_proc p \
                 JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace \
                 WHERE n.nspname = $1 AND p.proname = $2 \
                 ORDER BY p.oid \
                 LIMIT 1",
                schema,
                name,
            )
            .await?,
        ),
        ObjectKind::Sequence => (
            DefinitionSource::Assembled,
            assemble_sequence(client, schema, name).await?,
        ),
        ObjectKind::Extension => (
            DefinitionSource::Assembled,
            assemble_extension(client, name).await?,
        ),
        _ => (
            DefinitionSource::Assembled,
            assemble_relation(client, schema, name).await?,
        ),
    };

    Ok(Definition {
        kind: object.kind,
        schema: object.schema.clone(),
        name: object.name.clone(),
        source,
        text,
    })
}

/// Runs a definition query that returns one text column.
async fn one_text(
    client: &Client,
    statement: &str,
    schema: &str,
    name: &str,
) -> Result<String, Diagnostic> {
    let rows = client
        .query(statement, &[&schema, &name])
        .await
        .map_err(|err| from_query_error(&err, 0))?;
    rows.first()
        .and_then(|row| row.get::<_, Option<String>>("definition"))
        .ok_or_else(|| {
            Diagnostic::new(
                DiagnosticKind::Query,
                format!(
                    "no definition for {}.{}",
                    quote_identifier(schema),
                    quote_identifier(name)
                ),
                "reading an object definition",
            )
            .likely_cause("the object is gone, or the role may not see it")
            .next_action("reload the object tree")
        })
}

/// Describes a relation from its columns, constraints and indexes.
///
/// PostgreSQL renders views, indexes and functions itself and does not render
/// tables. Rather than pretend otherwise, this assembles a description in the
/// shape of the statement that would create it, and the panel says where it
/// came from.
async fn assemble_relation(
    client: &Client,
    schema: &str,
    relation: &str,
) -> Result<String, Diagnostic> {
    let columns = columns(client, schema, relation).await?;
    if columns.is_empty() {
        return Err(Diagnostic::new(
            DiagnosticKind::Query,
            format!(
                "no columns for {}.{}",
                quote_identifier(schema),
                quote_identifier(relation)
            ),
            "reading an object definition",
        )
        .likely_cause("the object is gone, or the role may not see its columns")
        .next_action("reload the object tree"));
    }

    let mut text = format!(
        "CREATE TABLE {}.{} (\n",
        quote_identifier(schema),
        quote_identifier(relation)
    );
    let width = columns
        .iter()
        .map(|column| quote_identifier(&column.name).chars().count())
        .max()
        .unwrap_or(0);
    for (index, column) in columns.iter().enumerate() {
        let quoted = quote_identifier(&column.name);
        text.push_str(&format!("    {quoted:width$} {}", column.data_type));
        if let Some(default) = &column.default {
            text.push_str(&format!(" DEFAULT {default}"));
        }
        if !column.nullable {
            text.push_str(" NOT NULL");
        }
        if index + 1 < columns.len() {
            text.push(',');
        }
        text.push('\n');
    }
    text.push_str(");\n");

    let constraints = client
        .query(
            "SELECT pg_catalog.pg_get_constraintdef(t.oid)::text AS definition, \
                    t.conname::text AS name \
             FROM pg_catalog.pg_constraint t \
             JOIN pg_catalog.pg_class c ON c.oid = t.conrelid \
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = $1 AND c.relname = $2 \
             ORDER BY t.conname",
            &[&schema, &relation],
        )
        .await
        .map_err(|err| from_query_error(&err, 0))?;
    if !constraints.is_empty() {
        text.push('\n');
        for row in &constraints {
            let name: String = row.get("name");
            let definition: String = row.get("definition");
            text.push_str(&format!(
                "ALTER TABLE {}.{} ADD CONSTRAINT {} {definition};\n",
                quote_identifier(schema),
                quote_identifier(relation),
                quote_identifier(&name)
            ));
        }
    }

    let indexes = client
        .query(
            "SELECT pg_catalog.pg_get_indexdef(i.indexrelid)::text AS definition \
             FROM pg_catalog.pg_index i \
             JOIN pg_catalog.pg_class c ON c.oid = i.indrelid \
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = $1 AND c.relname = $2 \
               AND NOT i.indisprimary \
               AND NOT EXISTS ( \
                   SELECT 1 FROM pg_catalog.pg_constraint con \
                   WHERE con.conindid = i.indexrelid \
               ) \
             ORDER BY i.indexrelid",
            &[&schema, &relation],
        )
        .await
        .map_err(|err| from_query_error(&err, 0))?;
    if !indexes.is_empty() {
        text.push('\n');
        for row in &indexes {
            let definition: String = row.get("definition");
            text.push_str(&format!("{definition};\n"));
        }
    }

    let triggers = client
        .query(
            "SELECT pg_catalog.pg_get_triggerdef(t.oid, true)::text AS definition \
             FROM pg_catalog.pg_trigger t \
             JOIN pg_catalog.pg_class c ON c.oid = t.tgrelid \
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = $1 AND c.relname = $2 AND NOT t.tgisinternal \
             ORDER BY t.tgname",
            &[&schema, &relation],
        )
        .await
        .map_err(|err| from_query_error(&err, 0))?;
    if !triggers.is_empty() {
        text.push('\n');
        for row in &triggers {
            let definition: String = row.get("definition");
            text.push_str(&format!("{definition};\n"));
        }
    }

    Ok(text)
}

/// Assembles a `CREATE SEQUENCE` statement from `pg_sequence`.
async fn assemble_sequence(
    client: &Client,
    schema: &str,
    sequence: &str,
) -> Result<String, Diagnostic> {
    let rows = client
        .query(
            "SELECT pg_catalog.format_type(s.seqtypid, NULL)::text AS data_type, \
                    s.seqstart AS start_value, \
                    s.seqincrement AS increment_by, \
                    s.seqmin AS min_value, \
                    s.seqmax AS max_value, \
                    s.seqcache AS cache_size, \
                    s.seqcycle AS is_cycled \
             FROM pg_catalog.pg_sequence s \
             JOIN pg_catalog.pg_class c ON c.oid = s.seqrelid \
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = $1 AND c.relname = $2",
            &[&schema, &sequence],
        )
        .await
        .map_err(|err| from_query_error(&err, 0))?;
    let row = rows.first().ok_or_else(|| {
        Diagnostic::new(
            DiagnosticKind::Query,
            format!(
                "no sequence definition for {}.{}",
                quote_identifier(schema),
                quote_identifier(sequence)
            ),
            "reading a sequence definition",
        )
        .likely_cause("the sequence does not exist, or the role may not see it")
        .next_action("reload the object tree")
    })?;
    Ok(format!(
        "{}\n",
        crate::ddl::format_sequence_ddl(crate::ddl::SequenceDdlSpec {
            schema,
            name: sequence,
            data_type: &row.get::<_, String>("data_type"),
            start_value: row.get::<_, i64>("start_value"),
            increment_by: row.get::<_, i64>("increment_by"),
            min_value: row.get::<_, i64>("min_value"),
            max_value: row.get::<_, i64>("max_value"),
            cache_size: row.get::<_, i64>("cache_size"),
            is_cycled: row.get::<_, bool>("is_cycled"),
        })
    ))
}

/// Assembles a `CREATE EXTENSION` statement from `pg_extension`.
async fn assemble_extension(client: &Client, extension: &str) -> Result<String, Diagnostic> {
    let rows = client
        .query(
            "SELECT e.extname::text AS name, e.extversion::text AS version, n.nspname::text AS schema \
             FROM pg_catalog.pg_extension e \
             JOIN pg_catalog.pg_namespace n ON n.oid = e.extnamespace \
             WHERE e.extname = $1",
            &[&extension],
        )
        .await
        .map_err(|err| from_query_error(&err, 0))?;
    let row = rows.first().ok_or_else(|| {
        Diagnostic::new(
            DiagnosticKind::Query,
            format!(
                "no extension definition for {}",
                quote_identifier(extension)
            ),
            "reading an extension definition",
        )
        .likely_cause("the extension is not installed")
        .next_action("reload the object tree")
    })?;
    let ext_schema: String = row.get("schema");
    let ext_version: String = row.get("version");
    Ok(format!(
        "CREATE EXTENSION IF NOT EXISTS {} WITH SCHEMA {} VERSION '{}';\n",
        quote_identifier(extension),
        quote_identifier(&ext_schema),
        ext_version.replace('\'', "''")
    ))
}

/// What an object needs, and what needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Dependencies {
    /// Objects this one reads from or refers to.
    pub depends_on: Vec<ObjectSummary>,
    /// Objects that read from or refer to this one.
    pub used_by: Vec<ObjectSummary>,
}

impl Dependencies {
    /// Whether nothing was found in either direction.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.depends_on.is_empty() && self.used_by.is_empty()
    }
}

/// Reads what an object depends on and what depends on it.
///
/// Each edge is labelled with the kind of edge it is rather than with a
/// direction: the list the entry appears in already says which way it points,
/// and "reads it" in a list of two directions is a sentence that has to be read
/// twice.
///
/// Two kinds of edge are followed, and the limits are worth stating because a
/// dependency answer people trust must be one they can check:
///
/// - **Rewrite rules**, which is how a view records the relations it reads.
/// - **Foreign keys**, which is how a table records the table it refers to.
///
/// Not followed: dependencies inside function bodies, which PostgreSQL does not
/// record; anything reached only at run time, such as dynamic SQL; and
/// dependencies on types, operators or extensions. What is shown is real. What
/// is missing is not proof of absence, and `docs/` says so where a user reads
/// it.
pub async fn dependencies(
    client: &Client,
    object: &ObjectSummary,
) -> Result<Dependencies, Diagnostic> {
    const USED_BY: &str = "SELECT DISTINCT n.nspname::text AS schema, \
         c.relname::text AS name, c.relkind::text AS relkind, \
         'view rule'::text AS reason \
         FROM pg_catalog.pg_depend d \
         JOIN pg_catalog.pg_rewrite r ON r.oid = d.objid \
         JOIN pg_catalog.pg_class c ON c.oid = r.ev_class \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         JOIN pg_catalog.pg_class source ON source.oid = d.refobjid \
         JOIN pg_catalog.pg_namespace sn ON sn.oid = source.relnamespace \
         WHERE sn.nspname = $1 AND source.relname = $2 AND c.oid <> source.oid \
         UNION \
         SELECT DISTINCT n.nspname::text, c.relname::text, c.relkind::text, \
         'foreign key'::text \
         FROM pg_catalog.pg_constraint con \
         JOIN pg_catalog.pg_class c ON c.oid = con.conrelid \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         JOIN pg_catalog.pg_class ref ON ref.oid = con.confrelid \
         JOIN pg_catalog.pg_namespace rn ON rn.oid = ref.relnamespace \
         WHERE con.contype = 'f' AND rn.nspname = $1 AND ref.relname = $2 \
         ORDER BY 1, 2";

    const DEPENDS_ON: &str = "SELECT DISTINCT n.nspname::text AS schema, \
         c.relname::text AS name, c.relkind::text AS relkind, \
         'view rule'::text AS reason \
         FROM pg_catalog.pg_depend d \
         JOIN pg_catalog.pg_rewrite r ON r.oid = d.objid \
         JOIN pg_catalog.pg_class source ON source.oid = r.ev_class \
         JOIN pg_catalog.pg_namespace sn ON sn.oid = source.relnamespace \
         JOIN pg_catalog.pg_class c ON c.oid = d.refobjid \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         WHERE sn.nspname = $1 AND source.relname = $2 AND c.oid <> source.oid \
         UNION \
         SELECT DISTINCT rn.nspname::text, ref.relname::text, ref.relkind::text, \
         'foreign key'::text \
         FROM pg_catalog.pg_constraint con \
         JOIN pg_catalog.pg_class c ON c.oid = con.conrelid \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         JOIN pg_catalog.pg_class ref ON ref.oid = con.confrelid \
         JOIN pg_catalog.pg_namespace rn ON rn.oid = ref.relnamespace \
         WHERE con.contype = 'f' AND n.nspname = $1 AND c.relname = $2 \
         ORDER BY 1, 2";

    let (schema, name) = (object.schema.as_str(), object.name.as_str());
    Ok(Dependencies {
        used_by: related(client, USED_BY, schema, name).await?,
        depends_on: related(client, DEPENDS_ON, schema, name).await?,
    })
}

/// Runs one dependency query and shapes its rows as objects.
async fn related(
    client: &Client,
    statement: &str,
    schema: &str,
    name: &str,
) -> Result<Vec<ObjectSummary>, Diagnostic> {
    let rows = client
        .query(statement, &[&schema, &name])
        .await
        .map_err(|err| from_query_error(&err, 0))?;
    Ok(rows
        .iter()
        .map(|row| {
            let relkind: String = row.get("relkind");
            ObjectSummary {
                kind: ObjectKind::from_relkind(&relkind).unwrap_or(ObjectKind::Table),
                schema: row.get::<_, String>("schema"),
                name: row.get::<_, String>("name"),
                readable: true,
                detail: Some(row.get::<_, String>("reason")),
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

/// Reads the live schema and foreign-key relationship graph from `pg_catalog`
/// for ER diagram rendering (`026-schema-er-diagrams`).
pub async fn schema_graph(
    client: &Client,
    schema: Option<&str>,
) -> Result<crate::diagram::SchemaGraph, Diagnostic> {
    use crate::diagram::{Cardinality, DiagramColumn, DiagramTable, ForeignKeyEdge, SchemaGraph};

    const COLUMNS_QUERY: &str = "SELECT \
         n.nspname::text AS schema, \
         c.relname::text AS table_name, \
         a.attname::text AS column_name, \
         pg_catalog.format_type(a.atttypid, a.atttypmod)::text AS data_type, \
         NOT a.attnotnull AS nullable, \
         COALESCE(BOOL_OR(i.indisprimary), false) AS is_pk, \
         COALESCE(BOOL_OR(i.indisunique AND NOT i.indisprimary), false) AS is_unique \
         FROM pg_catalog.pg_class c \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         LEFT JOIN pg_catalog.pg_attribute a \
           ON a.attrelid = c.oid AND a.attnum > 0 AND NOT a.attisdropped \
         LEFT JOIN pg_catalog.pg_index i \
           ON i.indrelid = c.oid AND a.attnum = ANY(i.indkey) \
         WHERE c.relkind IN ('r', 'p') \
           AND n.nspname !~ '^pg_' AND n.nspname <> 'information_schema' \
           AND ($1::text IS NULL OR lower(n.nspname::text) = lower($1::text)) \
         GROUP BY n.nspname, c.relname, a.attnum, a.attname, a.atttypid, a.atttypmod, a.attnotnull \
         ORDER BY n.nspname, c.relname, a.attnum";

    const FK_QUERY: &str = "SELECT \
         con.conname::text AS constraint_name, \
         src_ns.nspname::text AS source_schema, \
         src_cls.relname::text AS source_table, \
         ARRAY( \
             SELECT att.attname::text \
             FROM unnest(con.conkey) WITH ORDINALITY AS k(attnum, ord) \
             JOIN pg_catalog.pg_attribute att \
               ON att.attrelid = con.conrelid AND att.attnum = k.attnum \
             ORDER BY k.ord \
         ) AS source_columns, \
         tgt_ns.nspname::text AS target_schema, \
         tgt_cls.relname::text AS target_table, \
         ARRAY( \
             SELECT att.attname::text \
             FROM unnest(con.confkey) WITH ORDINALITY AS k(attnum, ord) \
             JOIN pg_catalog.pg_attribute att \
               ON att.attrelid = con.confrelid AND att.attnum = k.attnum \
             ORDER BY k.ord \
         ) AS target_columns, \
         EXISTS( \
             SELECT 1 FROM pg_catalog.pg_index idx \
             WHERE idx.indrelid = con.conrelid \
               AND idx.indisunique \
               AND idx.indkey::smallint[] @> con.conkey \
               AND con.conkey @> idx.indkey::smallint[] \
         ) AS is_one_to_one, \
         EXISTS( \
             SELECT 1 FROM unnest(con.conkey) AS k(attnum) \
             JOIN pg_catalog.pg_attribute att \
               ON att.attrelid = con.conrelid AND att.attnum = k.attnum \
             WHERE NOT att.attnotnull \
         ) AS is_nullable \
         FROM pg_catalog.pg_constraint con \
         JOIN pg_catalog.pg_class src_cls ON src_cls.oid = con.conrelid \
         JOIN pg_catalog.pg_namespace src_ns ON src_ns.oid = src_cls.relnamespace \
         JOIN pg_catalog.pg_class tgt_cls ON tgt_cls.oid = con.confrelid \
         JOIN pg_catalog.pg_namespace tgt_ns ON tgt_ns.oid = tgt_cls.relnamespace \
         WHERE con.contype = 'f' \
           AND src_ns.nspname !~ '^pg_' AND src_ns.nspname <> 'information_schema' \
           AND tgt_ns.nspname !~ '^pg_' AND tgt_ns.nspname <> 'information_schema' \
           AND ($1::text IS NULL OR lower(src_ns.nspname::text) = lower($1::text) OR lower(tgt_ns.nspname::text) = lower($1::text)) \
         ORDER BY src_ns.nspname, src_cls.relname, con.conname";

    let col_rows = client
        .query(COLUMNS_QUERY, &[&schema])
        .await
        .map_err(|err| from_query_error(&err, 0))?;
    let fk_rows = client
        .query(FK_QUERY, &[&schema])
        .await
        .map_err(|err| from_query_error(&err, 0))?;

    let mut edges: Vec<ForeignKeyEdge> = Vec::with_capacity(fk_rows.len());
    let mut fk_columns: std::collections::BTreeSet<(String, String, String)> =
        std::collections::BTreeSet::new();

    for row in &fk_rows {
        let constraint_name: String = row.get("constraint_name");
        let source_schema: String = row.get("source_schema");
        let source_table: String = row.get("source_table");
        let source_columns: Vec<String> = row.get("source_columns");
        let target_schema: String = row.get("target_schema");
        let target_table: String = row.get("target_table");
        let target_columns: Vec<String> = row.get("target_columns");
        let is_one_to_one: bool = row.get("is_one_to_one");
        let is_nullable: bool = row.get("is_nullable");

        for col in &source_columns {
            fk_columns.insert((source_schema.clone(), source_table.clone(), col.clone()));
        }

        edges.push(ForeignKeyEdge {
            constraint_name,
            source_schema,
            source_table,
            source_columns,
            target_schema,
            target_table,
            target_columns,
            cardinality: if is_one_to_one {
                Cardinality::OneToOne
            } else {
                Cardinality::OneToMany
            },
            nullable: is_nullable,
        });
    }

    let mut tables: Vec<DiagramTable> = Vec::new();
    for row in &col_rows {
        let tbl_schema: String = row.get("schema");
        let tbl_name: String = row.get("table_name");
        if tables
            .last()
            .is_none_or(|last| last.schema != tbl_schema || last.name != tbl_name)
        {
            tables.push(DiagramTable {
                schema: tbl_schema.clone(),
                name: tbl_name.clone(),
                columns: Vec::new(),
            });
        }
        let col_name: Option<String> = row.get("column_name");
        if let Some(col_name) = col_name {
            let data_type: Option<String> = row.get("data_type");
            let nullable: Option<bool> = row.get("nullable");
            let is_pk: bool = row.get("is_pk");
            let is_unique: bool = row.get("is_unique");
            let is_fk = fk_columns.contains(&(tbl_schema, tbl_name, col_name.clone()));
            if let Some(last) = tables.last_mut() {
                last.columns.push(DiagramColumn {
                    name: col_name,
                    data_type: data_type.unwrap_or_else(|| "unknown".to_owned()),
                    is_primary_key: is_pk,
                    is_foreign_key: is_fk,
                    is_unique,
                    nullable: nullable.unwrap_or(true),
                });
            }
        }
    }

    let kept: std::collections::BTreeSet<(String, String)> = tables
        .iter()
        .map(|t| (t.schema.clone(), t.name.clone()))
        .collect();
    edges.retain(|e| {
        kept.contains(&(e.source_schema.clone(), e.source_table.clone()))
            && kept.contains(&(e.target_schema.clone(), e.target_table.clone()))
    });

    Ok(SchemaGraph {
        tables,
        edges,
        truncated_from: None,
    }
    .bounded())
}

/// Generates a deterministic [`crate::ddl::DdlBundle`] for either one named object
/// (`object = Some(...)`) or all user objects in `schema` (`027-deep-catalog-and-ddl-generator`).
pub async fn generate_ddl(
    client: &Client,
    schema: &str,
    object: Option<&str>,
) -> Result<crate::ddl::DdlBundle, Diagnostic> {
    use crate::ddl::{
        DdlBundle, DdlCategory, DdlObject, canonical_drop_statement, format_enum_ddl,
        format_sequence_ddl,
    };

    let mut objects: Vec<DdlObject> = Vec::new();

    // 1. Custom ENUM types in schema
    let enum_rows = client
        .query(
            "SELECT t.typname::text AS name, \
                    ARRAY( \
                        SELECT e.enumlabel::text \
                        FROM pg_catalog.pg_enum e \
                        WHERE e.enumtypid = t.oid \
                        ORDER BY e.enumsortorder \
                    ) AS labels \
             FROM pg_catalog.pg_type t \
             JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace \
             WHERE n.nspname = $1 \
               AND t.typtype = 'e' \
               AND ($2::text IS NULL OR t.typname::text = $2::text) \
             ORDER BY t.typname",
            &[&schema, &object],
        )
        .await
        .map_err(|err| from_query_error(&err, 0))?;
    for row in &enum_rows {
        let name: String = row.get("name");
        let labels: Vec<String> = row.get("labels");
        objects.push(DdlObject {
            category: DdlCategory::Type,
            kind: ObjectKind::Table,
            schema: schema.to_owned(),
            name: name.clone(),
            drop_statement: format!(
                "DROP TYPE IF EXISTS {}.{} CASCADE;",
                quote_identifier(schema),
                quote_identifier(&name)
            ),
            create_statement: format_enum_ddl(schema, &name, &labels),
            dependent_statements: Vec::new(),
        });
    }

    // 2. Sequences in schema (when requested directly or for whole-schema export)
    let seq_rows = client
        .query(
            "SELECT c.relname::text AS name, \
                    pg_catalog.format_type(s.seqtypid, NULL)::text AS data_type, \
                    s.seqstart AS start_value, \
                    s.seqincrement AS increment_by, \
                    s.seqmin AS min_value, \
                    s.seqmax AS max_value, \
                    s.seqcache AS cache_size, \
                    s.seqcycle AS is_cycled \
             FROM pg_catalog.pg_sequence s \
             JOIN pg_catalog.pg_class c ON c.oid = s.seqrelid \
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = $1 \
               AND ($2::text IS NULL OR c.relname::text = $2::text) \
             ORDER BY c.relname",
            &[&schema, &object],
        )
        .await
        .map_err(|err| from_query_error(&err, 0))?;
    for row in &seq_rows {
        let name: String = row.get("name");
        objects.push(DdlObject {
            category: DdlCategory::Sequence,
            kind: ObjectKind::Sequence,
            schema: schema.to_owned(),
            name: name.clone(),
            drop_statement: canonical_drop_statement(ObjectKind::Sequence, schema, &name, None),
            create_statement: format_sequence_ddl(crate::ddl::SequenceDdlSpec {
                schema,
                name: &name,
                data_type: &row.get::<_, String>("data_type"),
                start_value: row.get::<_, i64>("start_value"),
                increment_by: row.get::<_, i64>("increment_by"),
                min_value: row.get::<_, i64>("min_value"),
                max_value: row.get::<_, i64>("max_value"),
                cache_size: row.get::<_, i64>("cache_size"),
                is_cycled: row.get::<_, bool>("is_cycled"),
            }),
            dependent_statements: Vec::new(),
        });
    }

    // 3. Relations (Tables, Partitioned Tables, Foreign Tables, Views, Materialized Views, standalone Indexes)
    let rel_rows = client
        .query(
            "SELECT c.relname::text AS name, c.relkind::text AS relkind \
             FROM pg_catalog.pg_class c \
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = $1 \
               AND c.relkind IN ('r', 'p', 'f', 'v', 'm', 'i') \
               AND ($2::text IS NULL OR c.relname::text = $2::text) \
             ORDER BY c.relname",
            &[&schema, &object],
        )
        .await
        .map_err(|err| from_query_error(&err, 0))?;

    for row in &rel_rows {
        let name: String = row.get("name");
        let relkind: String = row.get("relkind");
        let Some(kind) = ObjectKind::from_relkind(&relkind) else {
            continue;
        };
        // In whole-schema mode, skip standalone indexes because table DDL already includes them.
        if object.is_none() && kind == ObjectKind::Index {
            continue;
        }
        let summary = ObjectSummary {
            kind,
            schema: schema.to_owned(),
            name: name.clone(),
            readable: true,
            detail: None,
        };
        let def = definition(client, &summary).await?;
        let category = match kind {
            ObjectKind::View | ObjectKind::MaterializedView => DdlCategory::View,
            ObjectKind::Index => DdlCategory::Index,
            _ => DdlCategory::Table,
        };
        objects.push(DdlObject {
            category,
            kind,
            schema: schema.to_owned(),
            name: name.clone(),
            drop_statement: canonical_drop_statement(kind, schema, &name, None),
            create_statement: def.text,
            dependent_statements: Vec::new(),
        });
    }

    // 4. Functions & Procedures (supporting overloaded signatures)
    let fn_rows = client
        .query(
            "SELECT p.proname::text AS name, \
                    pg_catalog.pg_get_functiondef(p.oid)::text AS definition, \
                    pg_catalog.pg_get_function_identity_arguments(p.oid)::text AS identity_args \
             FROM pg_catalog.pg_proc p \
             JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace \
             WHERE n.nspname = $1 \
               AND p.prokind IN ('f', 'p') \
               AND ($2::text IS NULL OR p.proname::text = $2::text) \
             ORDER BY p.proname, p.oid",
            &[&schema, &object],
        )
        .await
        .map_err(|err| from_query_error(&err, 0))?;
    for row in &fn_rows {
        let name: String = row.get("name");
        let Some(def_text) = row.get::<_, Option<String>>("definition") else {
            continue;
        };
        let identity_args: String = row.get("identity_args");
        objects.push(DdlObject {
            category: DdlCategory::Routine,
            kind: ObjectKind::Function,
            schema: schema.to_owned(),
            name: name.clone(),
            drop_statement: canonical_drop_statement(
                ObjectKind::Function,
                schema,
                &name,
                Some(&identity_args),
            ),
            create_statement: def_text,
            dependent_statements: Vec::new(),
        });
    }

    if let Some(target_name) = object
        && objects.is_empty()
    {
        return Err(Diagnostic::new(
            DiagnosticKind::Query,
            format!(
                "no DDL definition found for {}.{}",
                quote_identifier(schema),
                quote_identifier(target_name)
            ),
            "generating object DDL",
        )
        .likely_cause("the object does not exist in that schema, or the role may not see it")
        .next_action("check the schema and object name with ignatius query or the object tree"));
    }

    Ok(DdlBundle {
        schema: schema.to_owned(),
        target_object: object.map(str::to_owned),
        objects,
        truncated_from: None,
    }
    .bounded())
}

/// Reads the live server activity, lock contention (`pg_blocking_pids` / `pg_locks`),
/// and database health metrics (`pg_stat_database`) for the Server Activity & Locks
/// Dashboard (`029-server-activity-and-locks-dashboard`).
pub async fn server_activity(
    client: &Client,
) -> Result<crate::activity::ServerActivitySnapshot, Diagnostic> {
    use crate::activity::{BackendProcess, DatabaseHealthMetrics, ServerActivitySnapshot};

    const DB_STATS_QUERY: &str = "SELECT \
         d.datname::text AS database, \
         COALESCE(d.numbackends, 0)::bigint AS active_connections, \
         COALESCE(d.xact_commit, 0)::bigint AS xact_commit, \
         COALESCE(d.xact_rollback, 0)::bigint AS xact_rollback, \
         CASE \
             WHEN COALESCE(d.blks_hit, 0) + COALESCE(d.blks_read, 0) = 0 THEN 10000::bigint \
             ELSE ((COALESCE(d.blks_hit, 0)::numeric * 10000) / (COALESCE(d.blks_hit, 0) + COALESCE(d.blks_read, 0)))::bigint \
         END AS cache_hit_bps, \
         COALESCE(d.deadlocks, 0)::bigint AS deadlocks, \
         COALESCE(d.temp_bytes, 0)::bigint AS temp_bytes \
         FROM pg_catalog.pg_stat_database d \
         WHERE d.datname = pg_catalog.current_database()";

    const ACTIVITY_QUERY: &str = "SELECT \
         a.pid AS pid, \
         COALESCE(a.datname::text, '-') AS database, \
         COALESCE(a.usename::text, 'system') AS username, \
         COALESCE(a.application_name::text, '') AS application_name, \
         COALESCE(pg_catalog.host(a.client_addr)::text, 'local') AS client_addr, \
         COALESCE(a.state::text, 'unknown') AS state, \
         a.wait_event_type::text AS wait_event_type, \
         a.wait_event::text AS wait_event, \
         GREATEST(0, (EXTRACT(EPOCH FROM (pg_catalog.clock_timestamp() - COALESCE(a.query_start, a.backend_start))) * 1000)::bigint) AS query_age_ms, \
         CASE \
             WHEN a.xact_start IS NULL THEN NULL \
             ELSE GREATEST(0, (EXTRACT(EPOCH FROM (pg_catalog.clock_timestamp() - a.xact_start)) * 1000)::bigint) \
         END AS xact_age_ms, \
         COALESCE(a.query::text, '') AS query, \
         COALESCE(pg_catalog.pg_blocking_pids(a.pid), ARRAY[]::integer[]) AS blocked_by, \
         ( \
             SELECT l.mode::text || ' on ' || COALESCE(c.relname::text, l.locktype::text) \
             FROM pg_catalog.pg_locks l \
             LEFT JOIN pg_catalog.pg_class c ON c.oid = l.relation \
             WHERE l.pid = a.pid \
               AND (NOT l.granted OR l.mode IN ('AccessExclusiveLock', 'ExclusiveLock', 'ShareRowExclusiveLock', 'RowExclusiveLock')) \
             ORDER BY l.granted ASC, l.mode DESC \
             LIMIT 1 \
         ) AS lock_summary, \
         (a.pid = pg_catalog.pg_backend_pid()) AS is_self \
         FROM pg_catalog.pg_stat_activity a \
         WHERE a.backend_type = 'client backend' \
           AND (a.datname IS NULL OR a.datname = pg_catalog.current_database()) \
         ORDER BY a.pid";

    let db_rows = client
        .query(DB_STATS_QUERY, &[])
        .await
        .map_err(|err| from_query_error(&err, 0))?;
    let metrics = db_rows
        .first()
        .map_or_else(DatabaseHealthMetrics::default, |row| {
            DatabaseHealthMetrics {
                database: row.get("database"),
                active_connections: row.get("active_connections"),
                xact_commit: row.get("xact_commit"),
                xact_rollback: row.get("xact_rollback"),
                cache_hit_bps: row.get("cache_hit_bps"),
                deadlocks: row.get("deadlocks"),
                temp_bytes: row.get("temp_bytes"),
            }
        });

    let act_rows = client
        .query(ACTIVITY_QUERY, &[])
        .await
        .map_err(|err| from_query_error(&err, 0))?;

    let mut backends = Vec::with_capacity(act_rows.len());
    for row in &act_rows {
        backends.push(BackendProcess {
            pid: row.get("pid"),
            database: row.get("database"),
            username: row.get("username"),
            application_name: row.get("application_name"),
            client_addr: row.get("client_addr"),
            state: row.get("state"),
            wait_event_type: row.get("wait_event_type"),
            wait_event: row.get("wait_event"),
            query_age_ms: row.get("query_age_ms"),
            xact_age_ms: row.get("xact_age_ms"),
            query: row.get("query"),
            blocked_by: row.get("blocked_by"),
            lock_summary: row.get("lock_summary"),
            is_self: row.get("is_self"),
        });
    }

    Ok(ServerActivitySnapshot {
        metrics,
        backends,
        truncated_from: None,
    }
    .bounded())
}

/// Reads the live roles (`pg_roles`, `pg_auth_members`), table Row-Level Security
/// posture and policies (`pg_class`, `pg_policy`), and table privilege grants
/// (`information_schema.role_table_grants`) for the Roles, Privileges, & RLS Auditor
/// (`031-roles-privileges-and-rls-auditor`).
pub async fn security_audit(
    client: &Client,
    schema: Option<&str>,
) -> Result<crate::audit::SecurityAuditSnapshot, Diagnostic> {
    use crate::audit::{
        RlsPolicyEntry, RoleAuditEntry, SecurityAuditSnapshot, TableGrantEntry, TableRlsAudit,
    };

    const ROLES_QUERY: &str = "SELECT \
         r.rolname::text AS name, \
         r.rolsuper AS is_superuser, \
         r.rolcanlogin AS can_login, \
         r.rolcreatedb AS can_create_db, \
         r.rolcreaterole AS can_create_role, \
         r.rolinherit AS inherit, \
         r.rolbypassrls AS bypass_rls, \
         r.rolconnlimit AS connection_limit, \
         ARRAY( \
             SELECT g.rolname::text \
             FROM pg_catalog.pg_auth_members m \
             JOIN pg_catalog.pg_roles g ON g.oid = m.roleid \
             WHERE m.member = r.oid \
             ORDER BY g.rolname \
         ) AS member_of \
         FROM pg_catalog.pg_roles r \
         WHERE r.rolname !~ '^pg_' \
         ORDER BY r.rolname";

    const TABLES_RLS_QUERY: &str = "SELECT \
         n.nspname::text AS schema, \
         c.relname::text AS table_name, \
         pg_catalog.pg_get_userbyid(c.relowner)::text AS owner, \
         c.relrowsecurity AS rls_enabled, \
         c.relforcerowsecurity AS rls_forced \
         FROM pg_catalog.pg_class c \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         WHERE c.relkind IN ('r', 'p') \
           AND n.nspname !~ '^pg_' AND n.nspname <> 'information_schema' \
           AND ($1::text IS NULL OR lower(n.nspname::text) = lower($1::text)) \
         ORDER BY n.nspname, c.relname";

    const POLICIES_QUERY: &str = "SELECT \
         n.nspname::text AS schema, \
         c.relname::text AS table_name, \
         p.polname::text AS policy_name, \
         p.polpermissive AS permissive, \
         CASE p.polcmd \
             WHEN '*' THEN 'ALL' \
             WHEN 'r' THEN 'SELECT' \
             WHEN 'a' THEN 'INSERT' \
             WHEN 'w' THEN 'UPDATE' \
             WHEN 'd' THEN 'DELETE' \
             ELSE p.polcmd::text \
         END AS command, \
         CASE \
             WHEN p.polroles = ARRAY[0::oid] THEN ARRAY['PUBLIC'::text] \
             ELSE ARRAY( \
                 SELECT r.rolname::text \
                 FROM unnest(p.polroles) AS roid \
                 JOIN pg_catalog.pg_roles r ON r.oid = roid \
                 ORDER BY r.rolname \
             ) \
         END AS roles, \
         pg_catalog.pg_get_expr(p.polqual, p.polrelid)::text AS using_expr, \
         pg_catalog.pg_get_expr(p.polwithcheck, p.polrelid)::text AS with_check_expr \
         FROM pg_catalog.pg_policy p \
         JOIN pg_catalog.pg_class c ON c.oid = p.polrelid \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname !~ '^pg_' AND n.nspname <> 'information_schema' \
           AND ($1::text IS NULL OR lower(n.nspname::text) = lower($1::text)) \
         ORDER BY n.nspname, c.relname, p.polname";

    const GRANTS_QUERY: &str = "SELECT \
         g.table_schema::text AS schema, \
         g.table_name::text AS table_name, \
         g.grantee::text AS grantee, \
         ARRAY_AGG(DISTINCT g.privilege_type::text ORDER BY g.privilege_type::text) AS privileges, \
         BOOL_OR(g.is_grantable = 'YES') AS is_grantable \
         FROM information_schema.role_table_grants g \
         WHERE g.table_schema !~ '^pg_' AND g.table_schema <> 'information_schema' \
           AND ($1::text IS NULL OR lower(g.table_schema::text) = lower($1::text)) \
         GROUP BY g.table_schema, g.table_name, g.grantee \
         ORDER BY g.table_schema, g.table_name, g.grantee";

    let role_rows = client
        .query(ROLES_QUERY, &[])
        .await
        .map_err(|err| from_query_error(&err, 0))?;
    let mut roles = Vec::with_capacity(role_rows.len());
    for row in &role_rows {
        roles.push(RoleAuditEntry {
            name: row.get("name"),
            is_superuser: row.get("is_superuser"),
            can_login: row.get("can_login"),
            can_create_db: row.get("can_create_db"),
            can_create_role: row.get("can_create_role"),
            inherit: row.get("inherit"),
            bypass_rls: row.get("bypass_rls"),
            connection_limit: row.get("connection_limit"),
            member_of: row.get("member_of"),
        });
    }

    let tbl_rows = client
        .query(TABLES_RLS_QUERY, &[&schema])
        .await
        .map_err(|err| from_query_error(&err, 0))?;
    let pol_rows = client
        .query(POLICIES_QUERY, &[&schema])
        .await
        .map_err(|err| from_query_error(&err, 0))?;

    let mut policies_by_table: std::collections::BTreeMap<(String, String), Vec<RlsPolicyEntry>> =
        std::collections::BTreeMap::new();
    for row in &pol_rows {
        let s: String = row.get("schema");
        let t: String = row.get("table_name");
        policies_by_table
            .entry((s.clone(), t.clone()))
            .or_default()
            .push(RlsPolicyEntry {
                schema: s,
                table: t,
                policy_name: row.get("policy_name"),
                permissive: row.get("permissive"),
                command: row.get("command"),
                roles: row.get("roles"),
                using_expr: row.get("using_expr"),
                with_check_expr: row.get("with_check_expr"),
            });
    }

    let mut tables = Vec::with_capacity(tbl_rows.len());
    for row in &tbl_rows {
        let s: String = row.get("schema");
        let t: String = row.get("table_name");
        let policies = policies_by_table
            .remove(&(s.clone(), t.clone()))
            .unwrap_or_default();
        tables.push(TableRlsAudit {
            schema: s,
            table: t,
            owner: row.get("owner"),
            rls_enabled: row.get("rls_enabled"),
            rls_forced: row.get("rls_forced"),
            policies,
        });
    }

    let grant_rows = client
        .query(GRANTS_QUERY, &[&schema])
        .await
        .map_err(|err| from_query_error(&err, 0))?;
    let mut grants = Vec::with_capacity(grant_rows.len());
    for row in &grant_rows {
        grants.push(TableGrantEntry {
            schema: row.get("schema"),
            table: row.get("table_name"),
            grantee: row.get("grantee"),
            privileges: row.get("privileges"),
            is_grantable: row.get("is_grantable"),
        });
    }

    Ok(SecurityAuditSnapshot {
        roles,
        tables,
        grants,
        truncated: false,
    }
    .bounded())
}

/// Reads the complete schema definition (tables, columns, constraints, indexes)
/// from `pg_catalog` for schema diffing and migrations (`030-visual-schema-designer-and-migrations`).
pub async fn schema_snapshot(
    client: &Client,
    schema: &str,
) -> Result<crate::migration::SchemaSnapshot, Diagnostic> {
    use crate::migration::{
        ColumnDef, ConstraintDef, ConstraintKind, IndexDef, SchemaSnapshot, TableDef,
    };

    const TABLES_QUERY: &str = "SELECT \
         c.relname::text AS table_name, \
         n.nspname::text AS schema_name, \
         obj_description(c.oid, 'pg_class')::text AS comment \
         FROM pg_catalog.pg_class c \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         WHERE c.relkind IN ('r', 'p') \
           AND n.nspname = $1 \
         ORDER BY c.relname";

    const COLUMNS_QUERY: &str = "SELECT \
         c.relname::text AS table_name, \
         a.attname::text AS column_name, \
         pg_catalog.format_type(a.atttypid, a.atttypmod)::text AS data_type, \
         NOT a.attnotnull AS is_nullable, \
         pg_catalog.pg_get_expr(d.adbin, d.adrelid)::text AS default_expression, \
         (a.attidentity = 'a' OR a.attidentity = 'd') AS is_identity, \
         COALESCE(BOOL_OR(i.indisprimary), false) AS is_pk, \
         COALESCE(BOOL_OR(i.indisunique AND NOT i.indisprimary), false) AS is_unique \
         FROM pg_catalog.pg_class c \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         JOIN pg_catalog.pg_attribute a ON a.attrelid = c.oid AND a.attnum > 0 AND NOT a.attisdropped \
         LEFT JOIN pg_catalog.pg_attrdef d ON d.adrelid = c.oid AND d.adnum = a.attnum \
         LEFT JOIN pg_catalog.pg_index i ON i.indrelid = c.oid AND a.attnum = ANY(i.indkey) \
         WHERE c.relkind IN ('r', 'p') \
           AND n.nspname = $1 \
         GROUP BY c.relname, a.attnum, a.attname, a.atttypid, a.atttypmod, a.attnotnull, a.attidentity, d.adbin, d.adrelid \
         ORDER BY c.relname, a.attnum";

    const CONSTRAINTS_QUERY: &str = "SELECT \
         c.relname::text AS table_name, \
         con.conname::text AS constraint_name, \
         con.contype::text AS constraint_type, \
         pg_catalog.pg_get_constraintdef(con.oid)::text AS definition \
         FROM pg_catalog.pg_constraint con \
         JOIN pg_catalog.pg_class c ON c.oid = con.conrelid \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = $1 \
         ORDER BY c.relname, con.conname";

    const INDEXES_QUERY: &str = "SELECT \
         c.relname::text AS table_name, \
         i.relname::text AS index_name, \
         idx.indisunique AS is_unique, \
         pg_catalog.pg_get_indexdef(idx.indexrelid)::text AS definition \
         FROM pg_catalog.pg_index idx \
         JOIN pg_catalog.pg_class c ON c.oid = idx.indrelid \
         JOIN pg_catalog.pg_class i ON i.oid = idx.indexrelid \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = $1 \
           AND NOT idx.indisprimary \
           AND NOT EXISTS ( \
               SELECT 1 FROM pg_catalog.pg_constraint con \
               WHERE con.conindid = idx.indexrelid \
           ) \
         ORDER BY c.relname, i.relname";

    let tbl_rows = client
        .query(TABLES_QUERY, &[&schema])
        .await
        .map_err(|err| from_query_error(&err, 0))?;
    let col_rows = client
        .query(COLUMNS_QUERY, &[&schema])
        .await
        .map_err(|err| from_query_error(&err, 0))?;
    let con_rows = client
        .query(CONSTRAINTS_QUERY, &[&schema])
        .await
        .map_err(|err| from_query_error(&err, 0))?;
    let idx_rows = client
        .query(INDEXES_QUERY, &[&schema])
        .await
        .map_err(|err| from_query_error(&err, 0))?;

    let mut tables = Vec::with_capacity(tbl_rows.len());
    for row in &tbl_rows {
        let name: String = row.get("table_name");
        let comment: Option<String> = row.get("comment");
        tables.push(TableDef {
            schema: schema.to_owned(),
            name,
            columns: Vec::new(),
            constraints: Vec::new(),
            indexes: Vec::new(),
            comment,
        });
    }

    for row in &col_rows {
        let tbl_name: String = row.get("table_name");
        if let Some(tbl) = tables.iter_mut().find(|t| t.name == tbl_name) {
            tbl.columns.push(ColumnDef {
                name: row.get("column_name"),
                data_type: row.get("data_type"),
                is_nullable: row.get("is_nullable"),
                default_expr: row.get("default_expression"),
                is_identity: row.get("is_identity"),
                is_pk: row.get("is_pk"),
                is_unique: row.get("is_unique"),
            });
        }
    }

    for row in &con_rows {
        let tbl_name: String = row.get("table_name");
        if let Some(tbl) = tables.iter_mut().find(|t| t.name == tbl_name) {
            let ctype: String = row.get("constraint_type");
            let kind = match ctype.as_str() {
                "p" => ConstraintKind::PrimaryKey,
                "f" => ConstraintKind::ForeignKey,
                "u" => ConstraintKind::Unique,
                "c" => ConstraintKind::Check,
                _ => ConstraintKind::Other,
            };
            tbl.constraints.push(ConstraintDef {
                name: row.get("constraint_name"),
                kind,
                definition: row.get("definition"),
            });
        }
    }

    for row in &idx_rows {
        let tbl_name: String = row.get("table_name");
        if let Some(tbl) = tables.iter_mut().find(|t| t.name == tbl_name) {
            tbl.indexes.push(IndexDef {
                name: row.get("index_name"),
                definition: row.get("definition"),
                is_unique: row.get("is_unique"),
            });
        }
    }

    Ok(SchemaSnapshot {
        schema: schema.to_owned(),
        tables,
    })
}

/// Computes the diff between two schemas using the provided client.
pub async fn schema_diff(
    client: &Client,
    source_schema: &str,
    target_schema: &str,
) -> Result<crate::migration::SchemaDiff, Diagnostic> {
    let source = schema_snapshot(client, source_schema).await?;
    let target = schema_snapshot(client, target_schema).await?;
    Ok(crate::migration::compute_schema_diff(&source, &target))
}

/// Discovers routine signatures with parameter lists, data types, and default values.
pub async fn discover_routines(
    client: &Client,
    schema_filter: Option<&str>,
    routine_name: Option<&str>,
) -> Result<Vec<crate::routine::RoutineSignature>, Diagnostic> {
    let query = "\
        SELECT \
            n.nspname::text AS schema_name, \
            p.proname::text AS routine_name, \
            p.prokind::text AS prokind, \
            l.lanname::text AS language_name, \
            pg_catalog.pg_get_function_result(p.oid)::text AS return_type, \
            pg_catalog.pg_get_function_arguments(p.oid)::text AS arguments, \
            p.prosrc::text AS source_code, \
            p.proretset AS is_set_returning, \
            p.prosecdef AS security_definer, \
            p.provolatile::text AS provolatile \
        FROM pg_catalog.pg_proc p \
        JOIN pg_catalog.pg_namespace n ON p.pronamespace = n.oid \
        JOIN pg_catalog.pg_language l ON p.prolang = l.oid \
        WHERE n.nspname NOT IN ('pg_catalog', 'information_schema') \
          AND ($1::text IS NULL OR n.nspname = $1::text) \
          AND ($2::text IS NULL OR p.proname = $2::text) \
        ORDER BY n.nspname, p.proname;";

    let rows = client
        .query(query, &[&schema_filter, &routine_name])
        .await
        .map_err(|e| from_query_error(&e, 1))?;

    let mut routines = Vec::new();
    for row in rows {
        let schema: String = row.get("schema_name");
        let name: String = row.get("routine_name");
        let prokind_str: String = row.get("prokind");
        let kind = match prokind_str.chars().next() {
            Some('p') => crate::routine::RoutineKind::Procedure,
            Some('a') => crate::routine::RoutineKind::Aggregate,
            Some('w') => crate::routine::RoutineKind::Window,
            _ => crate::routine::RoutineKind::Function,
        };
        let language: String = row.get("language_name");
        let return_type: String = row.get("return_type");
        let raw_args: String = row.get("arguments");
        let params = crate::routine::parse_routine_arguments(&raw_args);
        let source: String = row.get("source_code");
        let is_set_returning: bool = row.get("is_set_returning");
        let security_definer: bool = row.get("security_definer");
        let provolatile_str: String = row.get("provolatile");
        let volatility = match provolatile_str.chars().next() {
            Some('i') => "immutable".to_string(),
            Some('s') => "stable".to_string(),
            _ => "volatile".to_string(),
        };

        routines.push(crate::routine::RoutineSignature {
            schema,
            name,
            kind,
            language,
            return_type,
            is_set_returning,
            params,
            source,
            volatility,
            security_definer,
        });
    }

    Ok(routines)
}

/// Queries PostgreSQL progress reporting views for active maintenance operations (`033-listen-notify-and-maintenance-ops`).
pub async fn query_maintenance_progress(
    client: &tokio_postgres::Client,
) -> Result<Vec<crate::ops::MaintenanceProgressEntry>, Diagnostic> {
    let sql = r#"
        SELECT
            p.pid,
            d.datname,
            c.relname,
            'VACUUM' AS command,
            p.phase,
            p.heap_blks_total,
            p.heap_blks_scanned,
            p.heap_blks_vacuumed,
            p.index_vacuum_count
        FROM pg_stat_progress_vacuum p
        JOIN pg_database d ON d.oid = p.datid
        LEFT JOIN pg_class c ON c.oid = p.relid
        UNION ALL
        SELECT
            p.pid,
            d.datname,
            c.relname,
            'CLUSTER' AS command,
            p.phase,
            p.heap_blks_total,
            p.heap_blks_scanned,
            0::bigint AS heap_blks_vacuumed,
            0::bigint AS index_vacuum_count
        FROM pg_stat_progress_cluster p
        JOIN pg_database d ON d.oid = p.datid
        LEFT JOIN pg_class c ON c.oid = p.relid
        UNION ALL
        SELECT
            p.pid,
            d.datname,
            c.relname,
            'CREATE INDEX' AS command,
            p.phase,
            p.blocks_total AS heap_blks_total,
            p.blocks_done AS heap_blks_scanned,
            0::bigint AS heap_blks_vacuumed,
            0::bigint AS index_vacuum_count
        FROM pg_stat_progress_create_index p
        JOIN pg_database d ON d.oid = p.datid
        LEFT JOIN pg_class c ON c.oid = p.relid;
    "#;

    let rows = client
        .query(sql, &[])
        .await
        .map_err(|e| from_query_error(&e, 1))?;

    let mut entries = Vec::new();
    for row in rows {
        let pid: i32 = row.get("pid");
        let datname: String = row.get("datname");
        let relname: Option<String> = row.get("relname");
        let command: String = row.get("command");
        let phase: String = row.get("phase");
        let heap_blks_total: Option<i64> = row.get("heap_blks_total");
        let heap_blks_scanned: Option<i64> = row.get("heap_blks_scanned");
        let heap_blks_vacuumed: Option<i64> = row.get("heap_blks_vacuumed");
        let index_vacuum_count: Option<i64> = row.get("index_vacuum_count");

        let percent = match (heap_blks_scanned, heap_blks_total) {
            (Some(s), Some(t)) if t > 0 => {
                Some(((s as f64) / (t as f64) * 100.0).clamp(0.0, 100.0))
            }
            _ => None,
        };

        entries.push(crate::ops::MaintenanceProgressEntry {
            pid: pid as u32,
            datname,
            relname,
            command,
            phase,
            heap_blks_total,
            heap_blks_scanned,
            heap_blks_vacuumed,
            index_vacuum_count,
            percent,
        });
    }

    Ok(entries)
}

/// Gathers query execution telemetry from pg_stat_statements.
pub async fn query_performance(
    client: &Client,
    sort_order: crate::perf::PerformanceSortOrder,
    limit: usize,
) -> Result<crate::perf::QueryPerformanceSnapshot, Diagnostic> {
    use crate::perf::{QueryPerformanceSnapshot, StatStatementEntry};

    // 1. Check if pg_stat_statements extension is installed
    const EXTENSION_CHECK_SQL: &str =
        "SELECT 1 FROM pg_catalog.pg_extension WHERE extname = 'pg_stat_statements'";
    let ext_rows = client
        .query(EXTENSION_CHECK_SQL, &[])
        .await
        .map_err(|e| from_query_error(&e, 1))?;

    if ext_rows.is_empty() {
        return Ok(QueryPerformanceSnapshot::unavailable(
            "Extension 'pg_stat_statements' is not installed in the current database.",
        ));
    }

    // 2. Query pg_stat_statements
    const PERF_SQL: &str = r#"
        SELECT
            s.queryid,
            COALESCE(s.query, '') AS query,
            s.calls,
            COALESCE(s.total_exec_time, 0.0)::float8 AS total_exec_time,
            COALESCE(s.min_exec_time, 0.0)::float8 AS min_exec_time,
            COALESCE(s.max_exec_time, 0.0)::float8 AS max_exec_time,
            COALESCE(s.mean_exec_time, 0.0)::float8 AS mean_exec_time,
            COALESCE(s.stddev_exec_time, 0.0)::float8 AS stddev_exec_time,
            s.rows,
            COALESCE(s.shared_blks_hit, 0) AS shared_blks_hit,
            COALESCE(s.shared_blks_read, 0) AS shared_blks_read,
            COALESCE(s.shared_blks_dirtied, 0) AS shared_blks_dirtied,
            COALESCE(s.shared_blks_written, 0) AS shared_blks_written
        FROM pg_stat_statements s
        WHERE s.dbid = (SELECT oid FROM pg_database WHERE datname = current_database())
        LIMIT $1;
    "#;

    let limit_param = (limit.clamp(1, crate::perf::MAX_PERFORMANCE_QUERIES)) as i64;
    let rows = match client.query(PERF_SQL, &[&limit_param]).await {
        Ok(rows) => rows,
        Err(err) => {
            return Ok(QueryPerformanceSnapshot::unavailable(format!(
                "Failed to read pg_stat_statements: {err}"
            )));
        }
    };

    let mut entries = Vec::new();
    for row in rows {
        let query_id: i64 = row.get("queryid");
        let query: String = row.get("query");
        let calls: i64 = row.get("calls");
        let total_exec_time: f64 = row.get("total_exec_time");
        let min_exec_time: f64 = row.get("min_exec_time");
        let max_exec_time: f64 = row.get("max_exec_time");
        let mean_exec_time: f64 = row.get("mean_exec_time");
        let stddev_exec_time: f64 = row.get("stddev_exec_time");
        let rows_count: i64 = row.get("rows");
        let shared_blks_hit: i64 = row.get("shared_blks_hit");
        let shared_blks_read: i64 = row.get("shared_blks_read");
        let shared_blks_dirtied: i64 = row.get("shared_blks_dirtied");
        let shared_blks_written: i64 = row.get("shared_blks_written");

        entries.push(StatStatementEntry {
            query_id,
            query,
            calls: calls.max(0) as u64,
            total_exec_time_ms: total_exec_time,
            min_exec_time_ms: min_exec_time,
            max_exec_time_ms: max_exec_time,
            mean_exec_time_ms: mean_exec_time,
            stddev_exec_time_ms: stddev_exec_time,
            rows: rows_count.max(0) as u64,
            shared_blks_hit: shared_blks_hit.max(0) as u64,
            shared_blks_read: shared_blks_read.max(0) as u64,
            shared_blks_dirtied: shared_blks_dirtied.max(0) as u64,
            shared_blks_written: shared_blks_written.max(0) as u64,
        });
    }

    Ok(QueryPerformanceSnapshot::new(
        entries, true, None, sort_order,
    ))
}

/// Resets execution statistics in pg_stat_statements.
pub async fn reset_performance_stats(client: &Client) -> Result<(), Diagnostic> {
    client
        .execute("SELECT pg_stat_statements_reset();", &[])
        .await
        .map_err(|e| from_query_error(&e, 1))?;
    Ok(())
}

/// Queries the database catalog for index redundancy, bloat metrics, and unindexed foreign keys.
pub async fn analyze_indexes(
    client: &Client,
    min_size_bytes: u64,
) -> Result<crate::advisor::AdvisorReport, Diagnostic> {
    use crate::advisor::{
        AdvisorReport, IndexBloatMetric, RedundantIndex, RedundantReason, UnindexedForeignKey,
    };
    use std::collections::HashMap;

    // 1. Check if hypopg is available
    const EXT_CHECK: &str = "SELECT 1 FROM pg_catalog.pg_extension WHERE extname = 'hypopg'";
    let hypopg_installed = client
        .query(EXT_CHECK, &[])
        .await
        .map(|rows| !rows.is_empty())
        .unwrap_or(false);

    // 2. Query user indexes
    const INDEXES_SQL: &str = r#"
        SELECT
            n.nspname AS schema_name,
            t.relname AS table_name,
            i.relname AS index_name,
            idx.indisunique AS is_unique,
            idx.indisprimary AS is_primary,
            pg_relation_size(i.oid) AS size_bytes,
            COALESCE(stat.idx_scan, 0) AS scans,
            ARRAY(
                SELECT pg_get_indexdef(idx.indexrelid, k + 1, true)
                FROM generate_subscripts(idx.indkey, 1) as k
                ORDER BY k
            ) AS columns,
            am.amname AS access_method
        FROM pg_index idx
        JOIN pg_class i ON i.oid = idx.indexrelid
        JOIN pg_class t ON t.oid = idx.indrelid
        JOIN pg_namespace n ON n.oid = t.relnamespace
        JOIN pg_am am ON am.oid = i.relam
        LEFT JOIN pg_stat_user_indexes stat ON stat.indexrelid = idx.indexrelid
        WHERE n.nspname NOT IN ('pg_catalog', 'pg_toast', 'information_schema')
          AND t.relkind = 'r'
        ORDER BY n.nspname, t.relname, pg_relation_size(i.oid) DESC;
    "#;

    let index_rows = client
        .query(INDEXES_SQL, &[])
        .await
        .map_err(|e| from_query_error(&e, 1))?;

    struct RawIdx {
        schema: String,
        table: String,
        index_name: String,
        is_unique: bool,
        is_primary: bool,
        size_bytes: u64,
        scans: u64,
        columns: Vec<String>,
        access_method: String,
    }

    let mut table_indexes: HashMap<(String, String), Vec<RawIdx>> = HashMap::new();
    for row in index_rows {
        let schema: String = row.get("schema_name");
        let table: String = row.get("table_name");
        let index_name: String = row.get("index_name");
        let is_unique: bool = row.get("is_unique");
        let is_primary: bool = row.get("is_primary");
        let size_i64: i64 = row.get("size_bytes");
        let scans_i64: i64 = row.get("scans");
        let columns: Vec<String> = row.get("columns");
        let access_method: String = row.get("access_method");

        table_indexes
            .entry((schema.clone(), table.clone()))
            .or_default()
            .push(RawIdx {
                schema,
                table,
                index_name,
                is_unique,
                is_primary,
                size_bytes: size_i64.max(0) as u64,
                scans: scans_i64.max(0) as u64,
                columns,
                access_method,
            });
    }

    let mut redundant_indexes = Vec::new();
    for ((_schema, _table), idxs) in &table_indexes {
        for (i, current) in idxs.iter().enumerate() {
            let mut flagged = false;
            for (j, other) in idxs.iter().enumerate() {
                if i == j {
                    continue;
                }
                if current.access_method != other.access_method {
                    continue;
                }
                // Check exact duplicate
                if current.columns == other.columns {
                    // Pick the smaller or lexicographically later as redundant
                    if (current.size_bytes < other.size_bytes)
                        || (current.size_bytes == other.size_bytes
                            && current.index_name > other.index_name)
                    {
                        redundant_indexes.push(RedundantIndex {
                            schema: current.schema.clone(),
                            table: current.table.clone(),
                            index_name: current.index_name.clone(),
                            covering_index: Some(other.index_name.clone()),
                            reason: RedundantReason::ExactDuplicate,
                            columns: current.columns.clone(),
                            size_bytes: current.size_bytes,
                            scans: current.scans,
                        });
                        flagged = true;
                        break;
                    }
                }
                // Check prefix covered
                else if other.columns.len() > current.columns.len()
                    && other.columns[..current.columns.len()] == current.columns[..]
                    && (!current.is_unique || other.is_unique)
                {
                    redundant_indexes.push(RedundantIndex {
                        schema: current.schema.clone(),
                        table: current.table.clone(),
                        index_name: current.index_name.clone(),
                        covering_index: Some(other.index_name.clone()),
                        reason: RedundantReason::PrefixCovered,
                        columns: current.columns.clone(),
                        size_bytes: current.size_bytes,
                        scans: current.scans,
                    });
                    flagged = true;
                    break;
                }
            }

            if !flagged
                && current.scans == 0
                && !current.is_unique
                && !current.is_primary
                && current.size_bytes >= min_size_bytes
            {
                redundant_indexes.push(RedundantIndex {
                    schema: current.schema.clone(),
                    table: current.table.clone(),
                    index_name: current.index_name.clone(),
                    covering_index: None,
                    reason: RedundantReason::Unused,
                    columns: current.columns.clone(),
                    size_bytes: current.size_bytes,
                    scans: 0,
                });
            }
        }
    }

    // 3. Query B-Tree bloat metrics
    const BLOAT_SQL: &str = r#"
        SELECT
            n.nspname AS schema_name,
            t.relname AS table_name,
            i.relname AS index_name,
            pg_relation_size(i.oid) AS total_bytes,
            COALESCE(i.reltuples, 0)::float8 AS reltuples,
            COALESCE(i.relpages, 0) AS relpages
        FROM pg_class i
        JOIN pg_index idx ON idx.indexrelid = i.oid
        JOIN pg_class t ON t.oid = idx.indrelid
        JOIN pg_namespace n ON n.oid = t.relnamespace
        JOIN pg_am am ON am.oid = i.relam
        WHERE am.amname = 'btree'
          AND n.nspname NOT IN ('pg_catalog', 'pg_toast', 'information_schema')
          AND t.relkind = 'r'
          AND i.relpages > 5
        ORDER BY pg_relation_size(i.oid) DESC;
    "#;

    let mut bloated_indexes = Vec::new();
    if let Ok(bloat_rows) = client.query(BLOAT_SQL, &[]).await {
        for row in bloat_rows {
            let schema: String = row.get("schema_name");
            let table: String = row.get("table_name");
            let index_name: String = row.get("index_name");
            let total_bytes_i64: i64 = row.get("total_bytes");
            let reltuples: f64 = row.get("reltuples");
            let relpages: i32 = row.get("relpages");

            let total_bytes = total_bytes_i64.max(0) as u64;
            let relpages = relpages.max(0) as u64;

            // Estimated live pages assuming ~32 bytes per index tuple and 7960 usable page bytes
            let expected_pages = ((reltuples.max(0.0) * 32.0) / 7960.0).ceil() as u64;
            if relpages > expected_pages && expected_pages > 0 {
                let wasted_pages = relpages.saturating_sub(expected_pages);
                let wasted_bytes = (wasted_pages.saturating_mul(8192)).min(total_bytes);
                let bloat_pct =
                    ((wasted_bytes as f64) / (total_bytes.max(1) as f64) * 100.0).clamp(0.0, 99.9);

                if bloat_pct >= 25.0 && wasted_bytes >= min_size_bytes {
                    bloated_indexes.push(IndexBloatMetric {
                        schema,
                        table,
                        index_name,
                        total_bytes,
                        wasted_bytes,
                        bloat_pct,
                    });
                }
            }
        }
    }

    // 4. Query unindexed foreign keys
    const FK_SQL: &str = r#"
        SELECT
            c.conname AS constraint_name,
            n.nspname AS schema_name,
            child.relname AS child_table,
            parent.relname AS parent_table,
            ARRAY(
                SELECT a.attname
                FROM unnest(c.conkey) WITH ORDINALITY AS k(attnum, ord)
                JOIN pg_attribute a ON a.attrelid = c.conrelid AND a.attnum = k.attnum
                ORDER BY k.ord
            ) AS fk_columns
        FROM pg_constraint c
        JOIN pg_class child ON child.oid = c.conrelid
        JOIN pg_class parent ON parent.oid = c.confrelid
        JOIN pg_namespace n ON n.oid = child.relnamespace
        WHERE c.contype = 'f'
          AND n.nspname NOT IN ('pg_catalog', 'pg_toast', 'information_schema')
        ORDER BY n.nspname, child.relname, c.conname;
    "#;

    let mut unindexed_foreign_keys = Vec::new();
    if let Ok(fk_rows) = client.query(FK_SQL, &[]).await {
        for row in fk_rows {
            let constraint_name: String = row.get("constraint_name");
            let schema: String = row.get("schema_name");
            let child_table: String = row.get("child_table");
            let parent_table: String = row.get("parent_table");
            let fk_columns: Vec<String> = row.get("fk_columns");

            if let Some(table_idxs) = table_indexes.get(&(schema.clone(), child_table.clone())) {
                let has_covering_index = table_idxs.iter().any(|idx| {
                    idx.columns.len() >= fk_columns.len()
                        && idx.columns[..fk_columns.len()] == fk_columns[..]
                });
                if !has_covering_index {
                    unindexed_foreign_keys.push(UnindexedForeignKey {
                        constraint_name,
                        schema,
                        child_table,
                        columns: fk_columns,
                        parent_table,
                    });
                }
            } else {
                unindexed_foreign_keys.push(UnindexedForeignKey {
                    constraint_name,
                    schema,
                    child_table,
                    columns: fk_columns,
                    parent_table,
                });
            }
        }
    }

    Ok(AdvisorReport {
        redundant_indexes,
        bloated_indexes,
        unindexed_foreign_keys,
        hypo_tests: Vec::new(),
        hypopg_installed,
    })
}

/// Tests a proposed index against a query using the HypoPG extension.
pub async fn test_hypothetical_index(
    client: &Client,
    query_text: &str,
    create_index_sql: &str,
) -> Result<crate::advisor::HypoIndexTestResult, Diagnostic> {
    use crate::advisor::HypoIndexTestResult;

    // Check if hypopg is available
    const EXT_CHECK: &str = "SELECT 1 FROM pg_catalog.pg_extension WHERE extname = 'hypopg'";
    let hypopg_installed = client
        .query(EXT_CHECK, &[])
        .await
        .map(|rows| !rows.is_empty())
        .unwrap_or(false);

    if !hypopg_installed {
        return Ok(HypoIndexTestResult {
            proposed_index_sql: create_index_sql.to_string(),
            query_text: query_text.to_string(),
            hypo_available: false,
            original_cost: 0.0,
            hypo_cost: 0.0,
            cost_reduction_pct: 0.0,
            used_in_plan: false,
            notice: Some(
                "HypoPG extension is not installed in the target database. Run 'CREATE EXTENSION hypopg;' to enable hypothetical index profiling.".into(),
            ),
        });
    }

    // 1. Measure baseline plan cost
    let baseline_explain_sql = format!("EXPLAIN (FORMAT JSON) {}", query_text);
    let baseline_rows = client
        .query(&baseline_explain_sql, &[])
        .await
        .map_err(|e| from_query_error(&e, 1))?;
    let baseline_json: String = baseline_rows[0].get(0);
    let orig_cost = extract_total_cost(&baseline_json).unwrap_or(0.0);

    // 2. Create hypothetical index
    client
        .execute(
            "SELECT * FROM hypopg_create_index($1);",
            &[&create_index_sql],
        )
        .await
        .map_err(|e| from_query_error(&e, 1))?;

    // 3. Measure plan cost with hypothetical index active
    let hypo_rows = client
        .query(&baseline_explain_sql, &[])
        .await
        .map_err(|e| from_query_error(&e, 1))?;
    let hypo_json: String = hypo_rows[0].get(0);
    let hypo_cost = extract_total_cost(&hypo_json).unwrap_or(orig_cost);
    let used_in_plan = hypo_json.contains("Index Scan") || hypo_json.contains("Index Only Scan");

    // 4. Always reset HypoPG state
    let _ = client.execute("SELECT hypopg_reset();", &[]).await;

    let savings_pct = if orig_cost > 0.0 && hypo_cost < orig_cost {
        ((orig_cost - hypo_cost) / orig_cost * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };

    Ok(HypoIndexTestResult {
        proposed_index_sql: create_index_sql.to_string(),
        query_text: query_text.to_string(),
        hypo_available: true,
        original_cost: orig_cost,
        hypo_cost,
        cost_reduction_pct: savings_pct,
        used_in_plan,
        notice: None,
    })
}

fn extract_total_cost(json_str: &str) -> Option<f64> {
    let val: serde_json::Value = serde_json::from_str(json_str).ok()?;
    val.as_array()?
        .first()?
        .get("Plan")?
        .get("Total Cost")?
        .as_f64()
}

/// Queries PostgreSQL telemetry catalogs for checkpointer, WAL, database transaction,
/// XID freeze, and replication standby operational metrics.
pub async fn wal_and_transaction_telemetry(
    client: &tokio_postgres::Client,
) -> Result<crate::telemetry::TelemetryReport, Diagnostic> {
    use crate::telemetry::{
        CheckpointerStats, DatabaseXactStats, ReplicationStandby, TelemetryReport, WalStats,
        XidFreezeMetric,
    };

    // 1. Detect if pg_stat_checkpointer exists (PostgreSQL 17+)
    let checkpointer_exists_row = client
        .query_opt(
            "SELECT 1 FROM pg_catalog.pg_class c \
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = 'pg_catalog' AND c.relname = 'pg_stat_checkpointer';",
            &[],
        )
        .await
        .map_err(|e| from_query_error(&e, 1))?;
    let has_stat_checkpointer = checkpointer_exists_row.is_some();

    let checkpointer = if has_stat_checkpointer {
        let ckpt_row = client
            .query_opt(
                "SELECT num_timed::bigint, num_requested::bigint, write_time::float8, \
                 sync_time::float8, buffers_written::bigint \
                 FROM pg_catalog.pg_stat_checkpointer;",
                &[],
            )
            .await
            .map_err(|e| from_query_error(&e, 1))?;

        let bgw_row = client
            .query_opt(
                "SELECT buffers_clean::bigint, buffers_alloc::bigint \
                 FROM pg_catalog.pg_stat_bgwriter;",
                &[],
            )
            .await
            .map_err(|e| from_query_error(&e, 1))?;

        let (timed, req, w_time, s_time, ckpt_bufs) = if let Some(r) = ckpt_row {
            (
                r.get::<_, i64>(0).max(0) as u64,
                r.get::<_, i64>(1).max(0) as u64,
                r.get::<_, f64>(2).max(0.0),
                r.get::<_, f64>(3).max(0.0),
                r.get::<_, i64>(4).max(0) as u64,
            )
        } else {
            (0, 0, 0.0, 0.0, 0)
        };

        let (clean_bufs, alloc_bufs) = if let Some(r) = bgw_row {
            (
                r.get::<_, i64>(0).max(0) as u64,
                r.get::<_, i64>(1).max(0) as u64,
            )
        } else {
            (0, 0)
        };

        CheckpointerStats {
            timed_checkpoints: timed,
            requested_checkpoints: req,
            write_time_ms: w_time,
            sync_time_ms: s_time,
            buffers_checkpoint: ckpt_bufs,
            buffers_clean: clean_bufs,
            buffers_backend: 0,
            buffers_backend_fsync: 0,
            buffers_alloc: alloc_bufs,
        }
    } else {
        let bgw_row = client
            .query_opt(
                "SELECT checkpoints_timed::bigint, checkpoints_req::bigint, \
                 checkpoint_write_time::float8, checkpoint_sync_time::float8, \
                 buffers_checkpoint::bigint, buffers_clean::bigint, \
                 buffers_backend::bigint, buffers_backend_fsync::bigint, \
                 buffers_alloc::bigint \
                 FROM pg_catalog.pg_stat_bgwriter;",
                &[],
            )
            .await
            .map_err(|e| from_query_error(&e, 1))?;

        if let Some(r) = bgw_row {
            CheckpointerStats {
                timed_checkpoints: r.get::<_, i64>(0).max(0) as u64,
                requested_checkpoints: r.get::<_, i64>(1).max(0) as u64,
                write_time_ms: r.get::<_, f64>(2).max(0.0),
                sync_time_ms: r.get::<_, f64>(3).max(0.0),
                buffers_checkpoint: r.get::<_, i64>(4).max(0) as u64,
                buffers_clean: r.get::<_, i64>(5).max(0) as u64,
                buffers_backend: r.get::<_, i64>(6).max(0) as u64,
                buffers_backend_fsync: r.get::<_, i64>(7).max(0) as u64,
                buffers_alloc: r.get::<_, i64>(8).max(0) as u64,
            }
        } else {
            CheckpointerStats::default()
        }
    };

    // 2. WAL and Archiver Metrics
    let lsn_row = client
        .query_opt(
            "SELECT CASE WHEN pg_is_in_recovery() THEN pg_last_wal_replay_lsn()::text \
             ELSE pg_current_wal_lsn()::text END AS lsn;",
            &[],
        )
        .await
        .map_err(|e| from_query_error(&e, 1))?;
    let current_lsn = lsn_row
        .and_then(|r| r.get::<_, Option<String>>(0))
        .unwrap_or_else(|| "0/0".to_string());

    let wal_row = client
        .query_opt(
            "SELECT wal_records::bigint, wal_fpi::bigint, wal_bytes::numeric::text, \
             wal_buffers_full::bigint, \
             COALESCE(wal_write_time::float8, 0.0), COALESCE(wal_sync_time::float8, 0.0) \
             FROM pg_catalog.pg_stat_wal;",
            &[],
        )
        .await
        .ok()
        .flatten();

    let (records, fpi, bytes, buffers_full, write_ms, sync_ms) = if let Some(r) = wal_row {
        let b_str: Option<String> = r.get(2);
        let b_val = b_str.and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
        (
            r.get::<_, i64>(0).max(0) as u64,
            r.get::<_, i64>(1).max(0) as u64,
            b_val,
            r.get::<_, i64>(3).max(0) as u64,
            r.get::<_, f64>(4).max(0.0),
            r.get::<_, f64>(5).max(0.0),
        )
    } else {
        (0, 0, 0, 0, 0.0, 0.0)
    };

    let arch_row = client
        .query_opt(
            "SELECT archived_count::bigint, failed_count::bigint, \
             last_archived_wal::text, last_failed_wal::text \
             FROM pg_catalog.pg_stat_archiver;",
            &[],
        )
        .await
        .ok()
        .flatten();

    let (arch_count, fail_count, last_arch, last_fail) = if let Some(r) = arch_row {
        (
            r.get::<_, i64>(0).max(0) as u64,
            r.get::<_, i64>(1).max(0) as u64,
            r.get::<_, Option<String>>(2),
            r.get::<_, Option<String>>(3),
        )
    } else {
        (0, 0, None, None)
    };

    let wal = WalStats {
        current_lsn,
        wal_records: records,
        wal_fpi: fpi,
        wal_bytes: bytes,
        wal_buffers_full: buffers_full,
        wal_write_time_ms: write_ms,
        wal_sync_time_ms: sync_ms,
        archived_count: arch_count,
        failed_count: fail_count,
        last_archived_wal: last_arch,
        last_failed_wal: last_fail,
    };

    // 3. Database Transaction Telemetry
    let db_rows = client
        .query(
            "SELECT datname::text, xact_commit::bigint, xact_rollback::bigint, \
             deadlocks::bigint, conflicts::bigint, temp_files::bigint, \
             temp_bytes::bigint \
             FROM pg_catalog.pg_stat_database \
             WHERE datname IS NOT NULL \
             ORDER BY (xact_commit + xact_rollback) DESC LIMIT 50;",
            &[],
        )
        .await
        .map_err(|e| from_query_error(&e, 1))?;

    let mut databases = Vec::with_capacity(db_rows.len());
    for r in db_rows {
        databases.push(DatabaseXactStats {
            database_name: r.get::<_, String>(0),
            xact_commit: r.get::<_, i64>(1).max(0) as u64,
            xact_rollback: r.get::<_, i64>(2).max(0) as u64,
            deadlocks: r.get::<_, i64>(3).max(0) as u64,
            conflicts: r.get::<_, i64>(4).max(0) as u64,
            temp_files: r.get::<_, i64>(5).max(0) as u64,
            temp_bytes: r.get::<_, i64>(6).max(0) as u64,
        });
    }

    // 4. Transaction ID (XID) Freeze Forensics
    let freeze_rows = client
        .query(
            "SELECT d.datname::text, age(d.datfrozenxid)::bigint AS datfrozenxid_age, \
             current_setting('autovacuum_freeze_max_age')::bigint AS autovacuum_freeze_max_age \
             FROM pg_catalog.pg_database d \
             WHERE d.datallowconn \
             ORDER BY age(d.datfrozenxid) DESC;",
            &[],
        )
        .await
        .map_err(|e| from_query_error(&e, 1))?;

    // Query the oldest table in the connected database
    let oldest_table_row = client
        .query_opt(
            "SELECT c.relname::text, age(c.relfrozenxid)::bigint \
             FROM pg_catalog.pg_class c \
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
             WHERE c.relkind IN ('r', 'm') \
               AND n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast') \
             ORDER BY age(c.relfrozenxid) DESC LIMIT 1;",
            &[],
        )
        .await
        .ok()
        .flatten();

    let (oldest_tbl_name, oldest_tbl_age) = if let Some(r) = oldest_table_row {
        (
            Some(r.get::<_, String>(0)),
            r.get::<_, i64>(1).max(0) as u64,
        )
    } else {
        (None, 0)
    };

    const EMERGENCY_AUTOVAC_LIMIT: f64 = 2_000_000_000.0;
    let mut xid_freeze = Vec::with_capacity(freeze_rows.len());
    for r in freeze_rows {
        let age_val = r.get::<_, i64>(1).max(0) as u64;
        let freeze_max = r.get::<_, i64>(2).max(1) as u64;
        let wraparound_pct = ((age_val as f64 / EMERGENCY_AUTOVAC_LIMIT) * 100.0).clamp(0.0, 100.0);
        xid_freeze.push(XidFreezeMetric {
            database_name: r.get::<_, String>(0),
            datfrozenxid_age: age_val,
            autovacuum_freeze_max_age: freeze_max,
            wraparound_pct,
            oldest_table_name: oldest_tbl_name.clone(),
            oldest_table_age: oldest_tbl_age,
        });
    }

    // 5. Streaming Replication Standbys
    let rep_rows = client
        .query(
            "SELECT pid::int4, client_addr::text, COALESCE(application_name, '')::text, \
             COALESCE(state, 'unknown')::text, COALESCE(sync_state, 'async')::text, \
             sent_lsn::text, \
             COALESCE(pg_wal_lsn_diff(sent_lsn, write_lsn)::bigint, 0), \
             COALESCE(pg_wal_lsn_diff(sent_lsn, flush_lsn)::bigint, 0), \
             COALESCE(pg_wal_lsn_diff(sent_lsn, replay_lsn)::bigint, 0), \
             EXTRACT(EPOCH FROM (now() - reply_time))::float8 * 1000.0 \
             FROM pg_catalog.pg_stat_replication;",
            &[],
        )
        .await
        .ok()
        .unwrap_or_default();

    let mut standbys = Vec::with_capacity(rep_rows.len());
    for r in rep_rows {
        standbys.push(ReplicationStandby {
            pid: r.get::<_, i32>(0).max(0) as u32,
            client_addr: r.get::<_, Option<String>>(1),
            application_name: r.get::<_, String>(2),
            state: r.get::<_, String>(3),
            sync_state: r.get::<_, String>(4),
            sent_lsn: r.get::<_, Option<String>>(5),
            write_lag_bytes: r.get::<_, i64>(6).max(0) as u64,
            flush_lag_bytes: r.get::<_, i64>(7).max(0) as u64,
            replay_lag_bytes: r.get::<_, i64>(8).max(0) as u64,
            replay_lag_ms: r.get::<_, Option<f64>>(9),
        });
    }

    Ok(TelemetryReport {
        checkpointer,
        wal,
        databases,
        xid_freeze,
        standbys,
    })
}

/// Queries connection pool metrics, server connection capacity/headroom, and blocked client forensics.
/// Supports both PgBouncer pooler environments and native PostgreSQL instances.
pub async fn connection_pool_telemetry(
    client: &tokio_postgres::Client,
) -> Result<crate::pool::PoolReport, Diagnostic> {
    use crate::pool::{ClientWaitMetric, PgBouncerPool, PoolReport, ServerConnectionStats};

    // 1. Check if connected to a PgBouncer admin console or direct PostgreSQL.
    // PgBouncer console responds to SHOW POOLS and SHOW STATS.
    let pgbouncer_pools_res = client.query("SHOW POOLS;", &[]).await;

    let (is_pgbouncer, pools) = match pgbouncer_pools_res {
        Ok(rows) => {
            let mut pools = Vec::with_capacity(rows.len());
            for r in rows {
                let db_name: String = r.try_get("database").unwrap_or_else(|_| "-".to_string());
                let user_name: String = r.try_get("user").unwrap_or_else(|_| "-".to_string());
                let cl_active: i64 = r.try_get("cl_active").unwrap_or(0);
                let cl_waiting: i64 = r.try_get("cl_waiting").unwrap_or(0);
                let sv_active: i64 = r.try_get("sv_active").unwrap_or(0);
                let sv_idle: i64 = r.try_get("sv_idle").unwrap_or(0);
                let sv_used: i64 = r.try_get("sv_used").unwrap_or(0);
                let sv_tested: i64 = r.try_get("sv_tested").unwrap_or(0);
                let sv_login: i64 = r.try_get("sv_login").unwrap_or(0);
                let maxwait: i64 = r.try_get("maxwait").unwrap_or(0);
                let pool_mode: String = r
                    .try_get("pool_mode")
                    .unwrap_or_else(|_| "transaction".to_string());

                pools.push(PgBouncerPool {
                    database: db_name,
                    user: user_name,
                    cl_active: cl_active.max(0) as u64,
                    cl_waiting: cl_waiting.max(0) as u64,
                    sv_active: sv_active.max(0) as u64,
                    sv_idle: sv_idle.max(0) as u64,
                    sv_used: sv_used.max(0) as u64,
                    sv_tested: sv_tested.max(0) as u64,
                    sv_login: sv_login.max(0) as u64,
                    maxwait_seconds: maxwait.max(0) as f64,
                    avg_query_time_ms: 0.0,
                    pool_mode,
                });
            }
            (true, pools)
        }
        Err(_) => {
            // Native PostgreSQL fallback: group pg_stat_activity by datname, usename
            let group_query = "SELECT \
                COALESCE(datname::text, '[global]') AS database, \
                COALESCE(usename::text, '[unknown]') AS username, \
                COUNT(*) FILTER (WHERE state = 'active')::bigint AS cl_active, \
                COUNT(*) FILTER (WHERE wait_event_type IS NOT NULL AND state = 'active')::bigint AS cl_waiting, \
                COUNT(*) FILTER (WHERE state = 'active')::bigint AS sv_active, \
                COUNT(*) FILTER (WHERE state = 'idle')::bigint AS sv_idle, \
                COUNT(*) FILTER (WHERE state = 'idle in transaction')::bigint AS sv_used, \
                0::bigint AS sv_tested, \
                0::bigint AS sv_login, \
                COALESCE(MAX(EXTRACT(EPOCH FROM (clock_timestamp() - state_change))) FILTER (WHERE state = 'active' AND wait_event_type IS NOT NULL), 0.0)::float8 AS maxwait, \
                0.0::float8 AS avg_query, \
                'direct'::text AS pool_mode \
                FROM pg_catalog.pg_stat_activity \
                WHERE backend_type = 'client backend' \
                GROUP BY datname, usename \
                ORDER BY cl_active DESC, sv_idle DESC LIMIT 50;";

            let rows = client
                .query(group_query, &[])
                .await
                .map_err(|e| from_query_error(&e, 1))?;

            let mut pools = Vec::with_capacity(rows.len());
            for r in rows {
                pools.push(PgBouncerPool {
                    database: r.get::<_, String>(0),
                    user: r.get::<_, String>(1),
                    cl_active: r.get::<_, i64>(2).max(0) as u64,
                    cl_waiting: r.get::<_, i64>(3).max(0) as u64,
                    sv_active: r.get::<_, i64>(4).max(0) as u64,
                    sv_idle: r.get::<_, i64>(5).max(0) as u64,
                    sv_used: r.get::<_, i64>(6).max(0) as u64,
                    sv_tested: r.get::<_, i64>(7).max(0) as u64,
                    sv_login: r.get::<_, i64>(8).max(0) as u64,
                    maxwait_seconds: r.get::<_, f64>(9).max(0.0),
                    avg_query_time_ms: r.get::<_, f64>(10).max(0.0),
                    pool_mode: r.get::<_, String>(11),
                });
            }
            (false, pools)
        }
    };

    // 2. Server connection capacity, state breakdown, and headroom.
    let server_stats = {
        let stats_query = "SELECT \
            COUNT(*)::bigint AS total_connections, \
            COUNT(*) FILTER (WHERE state = 'active')::bigint AS active, \
            COUNT(*) FILTER (WHERE state = 'idle')::bigint AS idle, \
            COUNT(*) FILTER (WHERE state = 'idle in transaction')::bigint AS idle_in_transaction, \
            COUNT(*) FILTER (WHERE state = 'idle in transaction (aborted)')::bigint AS idle_in_transaction_aborted, \
            COUNT(*) FILTER (WHERE state = 'fastpath function call')::bigint AS fastpath, \
            COUNT(*) FILTER (WHERE state = 'disabled')::bigint AS disabled, \
            current_setting('max_connections')::bigint AS max_connections, \
            current_setting('superuser_reserved_connections')::bigint AS superuser_reserved_connections \
            FROM pg_catalog.pg_stat_activity \
            WHERE backend_type = 'client backend';";

        if let Ok(row) = client.query_one(stats_query, &[]).await {
            ServerConnectionStats {
                total_connections: row.get::<_, i64>(0).max(0) as u64,
                active: row.get::<_, i64>(1).max(0) as u64,
                idle: row.get::<_, i64>(2).max(0) as u64,
                idle_in_transaction: row.get::<_, i64>(3).max(0) as u64,
                idle_in_transaction_aborted: row.get::<_, i64>(4).max(0) as u64,
                fastpath: row.get::<_, i64>(5).max(0) as u64,
                disabled: row.get::<_, i64>(6).max(0) as u64,
                max_connections: row.get::<_, i64>(7).max(0) as u64,
                superuser_reserved_connections: row.get::<_, i64>(8).max(0) as u64,
            }
        } else {
            let total = pools
                .iter()
                .map(|p| p.sv_active + p.sv_idle + p.sv_used)
                .sum();
            let active = pools.iter().map(|p| p.sv_active).sum();
            let idle = pools.iter().map(|p| p.sv_idle).sum();
            ServerConnectionStats {
                total_connections: total,
                active,
                idle,
                idle_in_transaction: 0,
                idle_in_transaction_aborted: 0,
                fastpath: 0,
                disabled: 0,
                max_connections: total.max(100),
                superuser_reserved_connections: 3,
            }
        }
    };

    // 3. Waiting or lock-blocked client forensics
    let waiting_clients = {
        let wait_query = "SELECT \
            pid::int4, \
            COALESCE(datname::text, '-') AS database, \
            COALESCE(usename::text, 'unknown') AS username, \
            COALESCE(application_name::text, '') AS application_name, \
            COALESCE(host(client_addr)::text, 'local') AS client_addr, \
            wait_event_type::text AS wait_event_type, \
            wait_event::text AS wait_event, \
            COALESCE(query::text, '') AS query, \
            COALESCE(state::text, 'unknown') AS state, \
            GREATEST(0.0, EXTRACT(EPOCH FROM (clock_timestamp() - COALESCE(query_start, state_change)))::float8) AS duration_seconds \
            FROM pg_catalog.pg_stat_activity \
            WHERE backend_type = 'client backend' \
              AND wait_event_type IS NOT NULL \
              AND wait_event_type != 'Activity' \
              AND wait_event_type != 'Client' \
            ORDER BY duration_seconds DESC \
            LIMIT 50;";

        let rows = client.query(wait_query, &[]).await.ok().unwrap_or_default();

        let mut metrics = Vec::with_capacity(rows.len());
        for r in rows {
            metrics.push(ClientWaitMetric {
                pid: r.get::<_, i32>(0).max(0) as u32,
                database: r.get::<_, String>(1),
                user: r.get::<_, String>(2),
                application_name: r.get::<_, String>(3),
                client_addr: Some(r.get::<_, String>(4)),
                wait_event_type: r.get::<_, Option<String>>(5),
                wait_event: r.get::<_, Option<String>>(6),
                query: r.get::<_, String>(7),
                state: r.get::<_, String>(8),
                duration_seconds: r.get::<_, f64>(9).max(0.0),
            });
        }
        metrics
    };

    Ok(PoolReport {
        is_pgbouncer,
        server_stats,
        pools,
        waiting_clients,
    })
}

/// Queries PostgreSQL catalog for tables and columns, scoring them with PII detection
/// heuristics to generate automated masking rules and view policies.
pub async fn analyze_data_masking(
    client: &tokio_postgres::Client,
    schema: Option<&str>,
) -> Result<crate::mask::MaskReport, Diagnostic> {
    use crate::mask::{MaskReport, MaskRule, PiiCandidate, PiiType, TableMaskPolicy};
    use std::collections::HashMap;

    let columns_query = "SELECT \
        c.table_schema::text, \
        c.table_name::text, \
        c.column_name::text, \
        c.data_type::text \
        FROM information_schema.columns c \
        JOIN information_schema.tables t \
          ON t.table_schema = c.table_schema AND t.table_name = c.table_name \
        WHERE t.table_type = 'BASE TABLE' \
          AND c.table_schema NOT IN ('pg_catalog', 'information_schema', 'pg_toast') \
          AND ($1::text IS NULL OR c.table_schema = $1::text) \
        ORDER BY c.table_schema, c.table_name, c.ordinal_position;";

    let rows = client
        .query(columns_query, &[&schema])
        .await
        .map_err(|e| from_query_error(&e, 1))?;

    let mut candidates = Vec::new();
    let mut table_rules: HashMap<(String, String), Vec<(String, MaskRule)>> = HashMap::new();
    let mut table_cols: HashMap<(String, String), Vec<String>> = HashMap::new();

    for r in rows {
        let sch: String = r.get(0);
        let tbl: String = r.get(1);
        let col: String = r.get(2);
        let dtype: String = r.get(3);

        table_cols
            .entry((sch.clone(), tbl.clone()))
            .or_default()
            .push(col.clone());

        if let Some(pii) = PiiType::infer_from_name(&col) {
            let rule = pii.default_rule();
            let confidence = if col.eq_ignore_ascii_case(pii.label()) {
                95
            } else {
                85
            };
            candidates.push(PiiCandidate {
                schema: sch.clone(),
                table: tbl.clone(),
                column: col.clone(),
                data_type: dtype,
                pii_type: pii,
                rule: rule.clone(),
                confidence,
            });
            table_rules.entry((sch, tbl)).or_default().push((col, rule));
        }
    }

    let mut policies = Vec::with_capacity(table_rules.len());
    for ((sch, tbl), rules) in table_rules {
        let all_columns = table_cols
            .remove(&(sch.clone(), tbl.clone()))
            .unwrap_or_default();
        policies.push(TableMaskPolicy {
            schema: sch,
            table: tbl,
            all_columns,
            column_rules: rules,
        });
    }

    policies.sort_by(|a, b| (&a.schema, &a.table).cmp(&(&b.schema, &b.table)));

    Ok(MaskReport {
        candidates,
        policies,
    })
}

#[cfg(test)]
mod tests;
