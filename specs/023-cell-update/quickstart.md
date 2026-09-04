# Feature 023 quickstart

This walkthrough uses only the disposable PostgreSQL fixture and synthetic
values. It does not contact a production database and it does not publish a
commit.

## Local checks

```bash
cargo fmt --all -- --check
cargo test --locked --lib update
cargo test --locked --test documentation_matches_the_build cell_update
cargo test --locked --test postgres_integration cell_update
cargo --locked xtask verify
```

The focused reducer checks cover source-shape refusal, key discovery state,
value entry, review, cancellation, stale identity, production/read-only
refusal and the parameterized effect boundary. Renderer checks cover the fully
bound review statement, row identity, no-automatic-rerun wording, narrow and
ASCII output, and absence of debug value leakage.

## Disposable live check

Start the synthetic PostgreSQL service:

```bash
cargo xtask db up
```

Use the plain URI printed by `db up` and the synthetic password from
`docker/dev.env` only for the live test environment. The integration test will:

1. create a temporary primary-key table;
2. read one row through a single-table direct projection;
3. discover the primary key and generate a parameterized update plan;
4. prove that no row changes before the execution effect is confirmed;
5. execute once and assert exactly one row is affected;
6. read the row deliberately and assert only the intended row changed.

Stop and inspect the fixture after the test:

```bash
cargo xtask db down
cargo xtask db status
```

If PostgreSQL is unavailable, report the integration check as skipped, not
passed. Unix-socket, terminal, screen-reader, Windows Terminal and Linux
hand checks remain separate evidence gates.
