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
             WHERE n.nspname = $1 AND c.relname = $2 AND NOT i.indisprimary \
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

    Ok(text)
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
