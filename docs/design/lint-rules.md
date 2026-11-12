# Lint rules

`ignatius lint` is an offline, lexical check. It never connects to a server,
never executes SQL and never rewrites a file. The rules below are the complete
catalogue in this build. `tests/documentation_matches_the_build.rs` compares
this document with the build in both directions.

Severity is fixed per rule:

| Code | Severity | What it means | Why it matters |
| --- | --- | --- | --- |
| `delete-without-where` | error | `DELETE` has no `WHERE` clause | It removes every row in the target |
| `update-without-where` | error | `UPDATE` has no `WHERE` clause | It rewrites every row in the target |
| `truncate-table` | error | `TRUNCATE` removes every row | It cannot be filtered and takes a strong lock |
| `drop-object` | warning | `DROP` removes an object | The change is permanent without the object's definition |
| `grant-all` | warning | `GRANT ... ALL` | It gives every privilege the object supports |
| `create-index-without-concurrently` | warning | `CREATE INDEX` without `CONCURRENTLY` | It blocks writes while the index builds |
| `reindex-without-concurrently` | warning | `REINDEX` without `CONCURRENTLY` | It blocks writes while it rebuilds |
| `vacuum-full` | warning | `VACUUM FULL` | It takes an exclusive lock and rewrites the table |
| `select-star` | info | `SELECT *` | The result shape depends on the table, not the query |
| `leading-wildcard-like` | warning | `LIKE '%...'` or `LIKE '_...'` | A B-tree index cannot serve a leading wildcard |
| `non-sargable-predicate` | warning | a function or cast wraps the column in a comparison | A plain index scan may be unavailable |
| `implicit-cross-join` | warning | a comma in `FROM` | The row count is the product of both relations |
| `missing-transaction` | info | DDL in a file with no explicit `BEGIN` | A failed later statement can leave an earlier change committed |

## Limits

- The linter is lexical, not a parser. It reports what the text supports and
  does not claim a statement is safe. A CTE, view or function can filter rows in
  ways the text does not show.
- String literals, quoted identifiers, dollar-quoted bodies and comments are
  masked before matching, so their contents cannot trigger a rule. That also
  means a keyword hidden inside a dynamic SQL string is not inspected.
- `missing-transaction` accepts any `BEGIN` or `START TRANSACTION` in the file
  as intent. It does not prove the transaction covers every statement.
- A rule fires at most once per statement, at the first match. Multiple
  independent problems in one statement may produce one finding.
- Directory walks skip `target`, `.git`, `node_modules` and symbolic links.
  At most 1000 files and 1 MiB per file are accepted.

## Machine formats

- Table: `PATH:LINE:COL SEVERITY CODE MESSAGE`.
- JSON: `version`, `files`, `statements`, `findings`.
- SARIF: 2.1.0 with one result per finding and the full rule catalogue.

`--min-severity` filters the report. `--fail-on` controls the exit code
independently; the default is `error`, so style findings never fail a build
unless asked. Exit code 1 means the threshold was met; a clean run exits 0.
