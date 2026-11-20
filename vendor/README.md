# Local PostgreSQL driver patch

The `tokio-postgres` directory contains the published crates.io package for
`tokio-postgres` 0.7.18. Its original archive has SHA-256
`a528f7d280f6d5b9cd149635c8705b0dd049754bc67d81d31fa25169a93809d3`.
The package records upstream revision
`f1cb6ec0d5766b136cbd68f3010d64142a5daa66` from [rust-postgres](https://github.com/rust-postgres/rust-postgres).
The upstream Apache-2.0 and MIT licenses are included in the package.

The change in `src/simple_query.rs` exposes the table OID, attribute number,
type OID and type modifier on `SimpleColumn`. Ignatius uses the metadata from
the same query that delivered the text so it can validate the origin of a
reviewed structured edit. The workspace patch applies to the TLS adapter's
driver dependency as well.

When a compatible upstream release exposes these fields, remove this copy,
the Cargo patch and the patch-specific inventory and advisory checks.
