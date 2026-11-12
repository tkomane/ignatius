# Feature Specification: Data Masking and Anonymization Rules (Feature 038)

## 1. Overview
As part of **Milestone 8: Cross-Environment Data Synchronization, Masking & Seeding**, this feature provides automated PII detection, deterministic pseudonymization rules, and masking policy generation for PostgreSQL schemas. It enables engineering and QA teams to safely sanitize production database dumps for local development, staging environments, and compliance audits (GDPR, HIPAA, CCPA) without leaving the terminal.

## 2. User Scenarios
1. **Automated PII Discovery**: The user opens the Data Masking cockpit (`Ctrl+K )` or `ignatius mask-rules`) to instantly inspect relations for sensitive attributes (`email`, `phone`, `ssn`, `tax_id`, `first_name`, `last_name`, `dob`, `address`, `credit_card`).
2. **Deterministic Pseudonymization**: The user configures column masking strategies (`Email`, `Phone`, `Name`, `Hash`, `Redact`, `Nullify`, `RandomizeNumber`, `CustomSql`) that maintain referential consistency and data types.
3. **Safe SQL Generation**: The user exports non-destructive masking SQL views (`CREATE OR REPLACE VIEW ...`) or masked ETL queries ready to pipe into cross-database sync pipelines (`039`).
4. **Policy Persistence**: The user saves and loads masking policies as structured JSON/YAML files (`--policy <PATH>`) for automated CI/CD anonymization pipelines.

## 3. Requirements
- Pure domain models in `src/mask/` (`MaskRule`, `PiiCandidate`, `TableMaskPolicy`, `MaskReport`, `MaskFormat`).
- Live catalog heuristics probing column names, data types, and comments.
- Keymap chord `Ctrl+K )` and action `data-masking`.
- TEA state and Elm wiring in `src/app/`.
- Field Industrial TUI overlay with Dieter Rams status capsules (`[CANDIDATES]`, `[MASKED]`, `[UNMASKED]`, `[RULES]`, `[VIEW MODE]`), dual-pane rule selector and SQL preview.
- Headless scriptable CLI subcommand `ignatius mask-rules` (`--schema`, `--format table|json|plain|sql`, `--output`, `--force`, `--view-prefix`).
