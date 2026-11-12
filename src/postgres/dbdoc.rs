//! Bounded catalog reads for the schema documentation generator.

use crate::dbdoc::{
    ColumnDocument, ConstraintDocument, IndexDocument, PolicyDocument, RelationDocument,
    RelationKind, SchemaDocument,
};
use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crate::postgres::error::from_query_error;
use crate::postgres::session::Session;

/// Largest number of relations documented for one schema.
pub const MAX_RELATIONS: usize = 500;

/// Largest number of columns documented for one schema.
pub const MAX_COLUMNS: usize = 5000;

impl Session {
    /// Reads one schema's catalog into a documentation model.
    ///
    /// This is read-only and bounded. It never reads table rows.
    ///
    /// # Errors
    ///
    /// Returns a usage diagnostic when the schema is absent or a catalog query
    /// fails for a reason other than absence.
    pub async fn schema_documentation(&self, schema: &str) -> Result<SchemaDocument, Diagnostic> {
        let exists = self
            .client()
            .query_opt("SELECT 1 FROM pg_namespace WHERE nspname = $1", &[&schema])
            .await
            .map_err(|error| from_query_error(&error, 1))?;
        if exists.is_none() {
            return Err(Diagnostic::new(
                DiagnosticKind::Usage,
                format!("schema {schema:?} does not exist or is not visible"),
                "generating schema documentation",
            )
            .likely_cause("the schema name is misspelled or the role cannot see it")
            .next_action("check the schema with `ignatius objects` or the object tree"));
        }

        let mut document = SchemaDocument::new(
            self.info().database.clone(),
            schema,
            chrono::Local::now().to_rfc3339(),
        );

        let relations = self
            .client()
            .query(
                "SELECT c.relname, c.relkind::text, obj_description(c.oid, 'pg_class'), \
                        c.reltuples::bigint, c.relrowsecurity, c.relforcerowsecurity \
                 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
                 WHERE n.nspname = $1 AND c.relkind IN ('r','p','v','m','f') \
                 ORDER BY c.relname LIMIT 501",
                &[&schema],
            )
            .await
            .map_err(|error| from_query_error(&error, 1))?;
        if relations.len() > MAX_RELATIONS {
            document.truncated = true;
        }
        for row in relations.into_iter().take(MAX_RELATIONS) {
            let kind = match row.get::<_, String>(1).as_str() {
                "p" => RelationKind::PartitionedTable,
                "v" => RelationKind::View,
                "m" => RelationKind::MaterializedView,
                "f" => RelationKind::ForeignTable,
                _ => RelationKind::Table,
            };
            document.relations.push(RelationDocument {
                name: row.get(0),
                kind,
                comment: row.get(2),
                estimated_rows: {
                    let estimate: i64 = row.get(3);
                    (estimate >= 0).then_some(estimate)
                },
                rls_enabled: row.get(4),
                rls_forced: row.get(5),
                columns: Vec::new(),
                indexes: Vec::new(),
                constraints: Vec::new(),
                policies: Vec::new(),
            });
        }

        self.load_columns(schema, &mut document).await?;
        self.load_indexes(schema, &mut document).await?;
        self.load_constraints(schema, &mut document).await?;
        self.load_policies(schema, &mut document).await?;
        self.load_references(schema, &mut document).await?;
        document.sort();
        Ok(document)
    }

    async fn load_columns(
        &self,
        schema: &str,
        document: &mut SchemaDocument,
    ) -> Result<(), Diagnostic> {
        let rows = self
            .client()
            .query(
                "SELECT c.relname, a.attnum::integer, a.attname, \
                        format_type(a.atttypid, a.atttypmod), NOT a.attnotnull, \
                        pg_get_expr(d.adbin, d.adrelid), \
                        EXISTS (SELECT 1 FROM pg_index i \
                                WHERE i.indrelid = c.oid AND i.indisprimary \
                                  AND a.attnum = ANY(i.indkey)), \
                        col_description(c.oid, a.attnum) \
                 FROM pg_attribute a \
                 JOIN pg_class c ON c.oid = a.attrelid \
                 JOIN pg_namespace n ON n.oid = c.relnamespace \
                 LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum \
                 WHERE n.nspname = $1 AND c.relkind IN ('r','p','v','m','f') \
                   AND a.attnum > 0 AND NOT a.attisdropped \
                 ORDER BY c.relname, a.attnum LIMIT 5001",
                &[&schema],
            )
            .await
            .map_err(|error| from_query_error(&error, 1))?;
        if rows.len() > MAX_COLUMNS {
            document.truncated = true;
        }
        for row in rows.into_iter().take(MAX_COLUMNS) {
            let relation: String = row.get(0);
            if let Some(table) = document.relations.iter_mut().find(|r| r.name == relation) {
                table.columns.push(ColumnDocument {
                    ordinal: row.get(1),
                    name: row.get(2),
                    data_type: row.get(3),
                    nullable: row.get(4),
                    default: row.get(5),
                    primary_key: row.get(6),
                    references: None,
                    comment: row.get(7),
                });
            }
        }
        Ok(())
    }

    async fn load_indexes(
        &self,
        schema: &str,
        document: &mut SchemaDocument,
    ) -> Result<(), Diagnostic> {
        let rows = self
            .client()
            .query(
                "SELECT c.relname, ic.relname, pg_get_indexdef(i.indexrelid), \
                        i.indisunique, i.indisprimary \
                 FROM pg_index i \
                 JOIN pg_class c ON c.oid = i.indrelid \
                 JOIN pg_class ic ON ic.oid = i.indexrelid \
                 JOIN pg_namespace n ON n.oid = c.relnamespace \
                 WHERE n.nspname = $1 AND c.relkind IN ('r','p','m') \
                 ORDER BY c.relname, ic.relname LIMIT 5000",
                &[&schema],
            )
            .await
            .map_err(|error| from_query_error(&error, 1))?;
        for row in rows {
            let relation: String = row.get(0);
            if let Some(table) = document.relations.iter_mut().find(|r| r.name == relation) {
                table.indexes.push(IndexDocument {
                    name: row.get(1),
                    definition: row.get(2),
                    unique: row.get(3),
                    primary: row.get(4),
                });
            }
        }
        Ok(())
    }

    async fn load_constraints(
        &self,
        schema: &str,
        document: &mut SchemaDocument,
    ) -> Result<(), Diagnostic> {
        let rows = self
            .client()
            .query(
                "SELECT c.relname, con.conname, con.contype::text, \
                        pg_get_constraintdef(con.oid) \
                 FROM pg_constraint con \
                 JOIN pg_class c ON c.oid = con.conrelid \
                 JOIN pg_namespace n ON n.oid = c.relnamespace \
                 WHERE n.nspname = $1 AND con.contype IN ('p','f','u','c','x') \
                 ORDER BY c.relname, con.conname LIMIT 5000",
                &[&schema],
            )
            .await
            .map_err(|error| from_query_error(&error, 1))?;
        for row in rows {
            let relation: String = row.get(0);
            if let Some(table) = document.relations.iter_mut().find(|r| r.name == relation) {
                table.constraints.push(ConstraintDocument {
                    name: row.get(1),
                    kind: row.get(2),
                    definition: row.get(3),
                });
            }
        }
        Ok(())
    }

    async fn load_policies(
        &self,
        schema: &str,
        document: &mut SchemaDocument,
    ) -> Result<(), Diagnostic> {
        let rows = self
            .client()
            .query(
                "SELECT c.relname, p.polname, p.polcmd::text, \
                        ARRAY(SELECT r.rolname FROM pg_roles r \
                              WHERE r.oid = ANY(p.polroles))::text[], \
                        pg_get_expr(p.polqual, p.polrelid), \
                        pg_get_expr(p.polwithcheck, p.polrelid) \
                 FROM pg_policy p \
                 JOIN pg_class c ON c.oid = p.polrelid \
                 JOIN pg_namespace n ON n.oid = c.relnamespace \
                 WHERE n.nspname = $1 \
                 ORDER BY c.relname, p.polname LIMIT 5000",
                &[&schema],
            )
            .await
            .map_err(|error| from_query_error(&error, 1))?;
        for row in rows {
            let relation: String = row.get(0);
            if let Some(table) = document.relations.iter_mut().find(|r| r.name == relation) {
                let raw_command: String = row.get(2);
                let command = match raw_command.as_str() {
                    "r" => "SELECT",
                    "a" => "INSERT",
                    "w" => "UPDATE",
                    "d" => "DELETE",
                    "*" => "ALL",
                    other => other,
                };
                table.policies.push(PolicyDocument {
                    name: row.get(1),
                    command: command.to_owned(),
                    roles: row.get(3),
                    using: row.get(4),
                    with_check: row.get(5),
                });
            }
        }
        Ok(())
    }

    async fn load_references(
        &self,
        schema: &str,
        document: &mut SchemaDocument,
    ) -> Result<(), Diagnostic> {
        let rows = self
            .client()
            .query(
                "SELECT c.relname, a.attname, fn.nspname, fc.relname, fa.attname \
                 FROM pg_constraint con \
                 JOIN pg_class c ON c.oid = con.conrelid \
                 JOIN pg_class fc ON fc.oid = con.confrelid \
                 JOIN pg_namespace fn ON fn.oid = fc.relnamespace \
                 CROSS JOIN LATERAL unnest(con.conkey, con.confkey) AS k(attnum, fattnum) \
                 JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = k.attnum \
                 JOIN pg_attribute fa ON fa.attrelid = fc.oid AND fa.attnum = k.fattnum \
                 JOIN pg_namespace n ON n.oid = c.relnamespace \
                 WHERE con.contype = 'f' AND n.nspname = $1 \
                 ORDER BY c.relname, a.attnum LIMIT 5000",
                &[&schema],
            )
            .await
            .map_err(|error| from_query_error(&error, 1))?;
        for row in rows {
            let relation: String = row.get(0);
            let column: String = row.get(1);
            let target_schema: String = row.get(2);
            let target_table: String = row.get(3);
            let target_column: String = row.get(4);
            if let Some(table) = document.relations.iter_mut().find(|r| r.name == relation)
                && let Some(found) = table.columns.iter_mut().find(|c| c.name == column)
            {
                found.references = Some(format!("{target_schema}.{target_table}.{target_column}"));
            }
        }
        Ok(())
    }
}
