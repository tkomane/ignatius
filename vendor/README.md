# Local PostgreSQL driver patch

The `tokio-postgres` directory contains the library from the published crates.io
package for `tokio-postgres` 0.7.18. The original archive has SHA-256
`a528f7d280f6d5b9cd149635c8705b0dd049754bc67d81d31fa25169a93809d3`.
The package records upstream revision
`f1cb6ec0d5766b136cbd68f3010d64142a5daa66` from [rust-postgres](https://github.com/rust-postgres/rust-postgres).
The upstream Apache-2.0 and MIT licenses are included in the package.
The test and benchmark fixtures and their manifest entries are omitted
because Ignatius builds only the library target.

The change in `src/simple_query.rs` exposes the table OID, attribute number,
type OID and type modifier on `SimpleColumn`. Ignatius uses the metadata from
the same query that delivered the text so it can validate the origin of a
reviewed structured edit. The workspace patch applies to the TLS adapter's
driver dependency as well.

`src/client.rs` also matches the path import to its Unix-only use so the
repository dependency compiles on Windows with warnings denied.

`src/copy_out.rs` provides opt-in completion-aware iteration. Bulk export
waits for the same request's COPY command tag and ReadyForQuery before
publishing data, so PostgreSQL errors after CopyDone reach the caller.
Ordinary COPY streams still end at CopyDone.

When a compatible upstream release provides these capabilities, remove this copy,
the Cargo patch and the patch-specific inventory and advisory checks.
