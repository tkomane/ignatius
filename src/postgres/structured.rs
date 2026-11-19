//! Optional metadata needed to interpret retained PostgreSQL text without guessing.

use crate::query::result::{ResultSet, retention_budget::RetentionBudget};
use crate::structured::{Column, Kind, Origin};
use std::collections::{BTreeMap, BTreeSet};
use tokio_postgres::types::{Kind as PgKind, Type};
use tokio_postgres::{Client, Column as PgColumn};

const MAX_CATALOG_TYPES: usize = 128;

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct EditSettings {
    exact_floats: bool,
    array_nulls: bool,
}

impl EditSettings {
    fn allows(self, kind: Kind) -> bool {
        self.exact_floats && (!matches!(kind, Kind::Array { .. }) || self.array_nulls)
    }
}

/// Observes the output/input posture without evaluating any user SQL.
pub(super) async fn edit_settings(client: &Client) -> Result<EditSettings, tokio_postgres::Error> {
    let messages = client
        .simple_query("SHOW extra_float_digits; SHOW array_nulls")
        .await?;
    let mut settings = messages.iter().filter_map(|message| match message {
        tokio_postgres::SimpleQueryMessage::Row(row) => row.get(0),
        _ => None,
    });
    Ok(EditSettings {
        exact_floats: settings
            .next()
            .and_then(|value| value.parse::<i32>().ok())
            .is_some_and(|value| value > 0),
        array_nulls: settings.next() == Some("on"),
    })
}

pub(super) async fn describe(
    client: &Client,
    set: &mut ResultSet,
    columns: &[PgColumn],
    budget: &mut RetentionBudget,
    before: EditSettings,
) {
    let mut oids = BTreeSet::new();
    for column in columns {
        if let Some(oid) = catalog_oid(base_type(column.type_())) {
            oids.insert(oid);
            if oids.len() >= MAX_CATALOG_TYPES {
                break;
            }
        }
    }
    let rows = catalog(client, oids.into_iter().collect()).await;
    if !columns
        .iter()
        .any(|column| describe_column(column, &rows).is_some())
        || !budget.admit_structured_columns(columns.len())
    {
        return;
    }
    let after = if before.exact_floats {
        edit_settings(client).await.unwrap_or_default()
    } else {
        EditSettings::default()
    };
    set.set_structured_columns(
        columns
            .iter()
            .map(|column| {
                let mut described = describe_column(column, &rows)?;
                if !before.allows(described.kind) || !after.allows(described.kind) {
                    described.origin = None;
                }
                Some(described)
            })
            .collect(),
    );
}

fn base_type(mut type_: &Type) -> &Type {
    while let PgKind::Domain(base) = type_.kind() {
        type_ = base;
    }
    type_
}

fn catalog_oid(type_: &Type) -> Option<u32> {
    match type_.kind() {
        PgKind::Array(element) if Type::from_oid(element.oid()).is_none() => Some(element.oid()),
        PgKind::Simple
            if matches!(
                type_.name(),
                "geometry" | "geography" | "vector" | "halfvec" | "sparsevec"
            ) =>
        {
            Some(type_.oid())
        }
        _ => None,
    }
}

#[derive(Debug)]
struct CatalogType {
    delimiter: Option<u8>,
    extension: Option<String>,
}

async fn catalog(client: &Client, oids: Vec<u32>) -> BTreeMap<u32, CatalogType> {
    if oids.is_empty() {
        return BTreeMap::new();
    }
    const TYPES: &str = "SELECT t.oid, t.typdelim::pg_catalog.text, e.extname::pg_catalog.text \
         FROM pg_catalog.pg_type t \
         LEFT JOIN pg_catalog.pg_depend d \
           ON d.classid = 'pg_catalog.pg_type'::pg_catalog.regclass \
          AND d.objid = t.oid AND d.objsubid = 0 AND d.deptype = 'e' \
          AND d.refclassid = 'pg_catalog.pg_extension'::pg_catalog.regclass \
         LEFT JOIN pg_catalog.pg_extension e ON e.oid = d.refobjid \
         WHERE t.oid = ANY($1)";
    let Ok(rows) = client.query(TYPES, &[&oids]).await else {
        // Type labels and exact server text remain available.
        return BTreeMap::new();
    };
    rows.iter()
        .filter_map(|row| {
            let delimiter: &str = row.try_get(1).ok()?;
            Some((
                row.try_get(0).ok()?,
                CatalogType {
                    delimiter: match delimiter.as_bytes() {
                        [byte] if byte.is_ascii_graphic() && !b"{}[]\"\\".contains(byte) => {
                            Some(*byte)
                        }
                        _ => None,
                    },
                    extension: row.try_get(2).ok()?,
                },
            ))
        })
        .collect()
}

fn describe_column(column: &PgColumn, catalog: &BTreeMap<u32, CatalogType>) -> Option<Column> {
    let kind = describe_type(column.type_(), catalog)?;
    let origin = column
        .table_oid()
        .zip(column.column_id())
        .map(|(table, attribute)| Origin {
            table,
            attribute,
            type_oid: column.type_().oid(),
            typmod: column.type_modifier(),
        });
    // User-defined scalar input/output functions may drop information. Keep
    // inspection and explicit whole-value replacement for those containers.
    let partial_edit = match base_type(column.type_()).kind() {
        PgKind::Array(element) | PgKind::Range(element) => {
            Type::from_oid(base_type(element).oid()).is_some()
        }
        _ => true,
    };
    Some(Column {
        kind,
        origin,
        partial_edit,
    })
}

fn describe_type(type_: &Type, catalog: &BTreeMap<u32, CatalogType>) -> Option<Kind> {
    let type_ = base_type(type_);
    if *type_ == Type::JSONB {
        return Some(Kind::Jsonb);
    }
    let kind = match type_.kind() {
        PgKind::Range(_) => Kind::Range,
        PgKind::Array(element) => {
            let delimiter = if *element == Type::BOX {
                b';'
            } else if Type::from_oid(element.oid()).is_some() {
                b','
            } else {
                catalog.get(&element.oid())?.delimiter?
            };
            Kind::Array { delimiter }
        }
        PgKind::Simple => {
            match (
                catalog.get(&type_.oid())?.extension.as_deref()?,
                type_.name(),
            ) {
                ("postgis", "geometry") => Kind::Geometry,
                ("postgis", "geography") => Kind::Geography,
                ("vector", "vector") => Kind::Vector,
                ("vector", "halfvec") => Kind::HalfVector,
                ("vector", "sparsevec") => Kind::SparseVector,
                _ => return None,
            }
        }
        _ => return None,
    };
    Some(kind)
}

#[cfg(test)]
mod tests;
