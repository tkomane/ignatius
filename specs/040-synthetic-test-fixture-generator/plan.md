# Implementation Plan: Synthetic Test Fixture Generator (040)

## Architecture
1. **Pure Domain Module (`src/seed/mod.rs` & `src/seed/tests.rs`)**:
   - `SeedStrategy`: `IntegerSequence`, `NumericRange { min, max, scale }`, `UuidV4`, `TextFaker { category }`, `EmailFaker`, `BooleanRatio(f64)`, `TimestampRange { start, end }`, `EnumChoice(Vec<String>)`, `ForeignKeySample { parent_table, parent_column }`.
   - `ColumnGenerator`: maps column name and type to generator strategy.
   - `TableGenerator`: holds ordered columns and primary key tracking.
   - `SyntheticFixturePlan`: ordered tables following topological DAG order from `src/sync/`.
   - `DeterministicPrng`: simple, robust linear congruential or xorshift PRNG for reproducible values without external heavy crates.
   - `FixtureReport`: holds generated rows per table, total byte count, and formatters (`render_sql`, `render_jsonl`, `render_csv`, `render_table`).
2. **PostgreSQL Catalog Inspection (`src/postgres/metadata.rs` & `src/postgres/session.rs`)**:
   - `analyze_seed_plan(client, schema)`: discovers tables, columns, constraints, enums, and foreign keys, assembling `SyntheticFixturePlan`.
   - `execute_synthetic_seed(client, plan, options)`: executes generated fixtures inside a guarded transaction (`BEGIN; ... COMMIT;`).
3. **Keymap & Configuration (`src/ui/keymap.rs` & `docs/design/keymap.md`)**:
   - Chord `Ctrl+K +` (`synthetic-seed`) bound to `Action::SyntheticSeed`.
4. **TEA Model, Messages & Update (`src/app/`)**:
   - `SyntheticSeedState` in `src/app/model.rs`.
   - `Message::SyntheticSeedLoaded` & `Effect::LoadSyntheticSeed` in `src/app/message.rs`.
   - Actions and overlay handling in `src/app/update.rs`.
   - Runtime background loading in `src/cli/interactive.rs`.
5. **Field Industrial TUI Cockpit (`src/ui/widgets/overlays.rs` & `src/ui/layout.rs`)**:
   - Dieter Rams hardware capsules (`[TABLES]`, `[ROWS/TABLE]`, `[SEED]`, `[INTEGRITY]`, `[FORMAT]`).
   - Tab 0: Table Generator Strategies (columns, inferred strategy, constraints).
   - Tab 1: Generated Fixture Preview (SQL statements or tabular preview).
6. **CLI Subcommand `ignatius seed` (`src/cli/mod.rs`)**:
   - Flags: `--schema`, `--table`, `-n, --rows`, `--seed`, `-f, --format`, `-o, --output`, `--force`, `--insert`.
7. **Verification & Testing**:
   - Sidecar unit tests in `src/seed/tests.rs`.
   - CLI contract tests in `tests/synthetic_seed_contract.rs`.
   - Full battery passing `cargo xtask verify`.
