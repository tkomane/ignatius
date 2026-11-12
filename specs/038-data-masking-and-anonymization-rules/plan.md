# Implementation Plan: Data Masking and Anonymization Rules (Feature 038)

## Architecture & Boundaries
1. **Domain (`src/mask/mod.rs` & `tests.rs`)**:
   - `MaskRule`: enum covering `Email`, `Phone`, `FullName`, `HashHex { salt: String }`, `Redact { text: String }`, `Nullify`, `RandomNumber { min: i64, max: i64 }`, `CustomSql(String)`.
   - `PiiType`: classification enum (`Email`, `Phone`, `PersonName`, `Financial`, `NationalId`, `Address`, `Secret`, `GenericSensitive`).
   - `PiiCandidate`: detected column, data type, inferred PII type, proposed mask rule, and confidence.
   - `TableMaskPolicy`: list of column rules for a given table.
   - `MaskReport`: collection of discovered candidates and generated view SQL statements.
   - SQL generator: renders `CREATE OR REPLACE VIEW masked_<table_name> AS SELECT ...` with masking transforms for targeted columns and direct pass-through for safe columns.
2. **Catalog Extractor (`src/postgres/metadata.rs` & `session.rs`)**:
   - Query `information_schema.columns` and `pg_catalog.pg_description` to extract schema, table, column name, and data type.
   - Score each column against PII regex patterns and common naming heuristics.
3. **Keymap & UI (`src/ui/keymap.rs`, `src/ui/widgets/overlays.rs`, `src/ui/layout.rs`)**:
   - Bind chord `Ctrl+K )` to `Action::DataMasking`.
   - Add Field Industrial TUI overlay with status capsules, candidates table, rule switcher (`1-6`), and live SQL preview.
4. **CLI Subcommand (`src/cli/mod.rs`)**:
   - Implement `ignatius mask-rules` with `--schema`, `--format table|json|plain|sql`, `--output`, `--force`, `--view-prefix`.
5. **Contract Tests (`tests/mask_rules_contract.rs`)**:
   - Verify CLI arguments, overwrite protections, live database execution, and generated SQL validity.
