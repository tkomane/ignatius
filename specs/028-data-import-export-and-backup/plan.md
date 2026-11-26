# Plan - COPY import ownership and export completion

Use the CLI argument contract plus a defensive check at the session boundary.
Check transaction state under the existing user SQL gate before beginning an
owned transaction. Ensure the COPY sink ends before COMMIT or ROLLBACK.
Use the PostgreSQL response to report rollback and preserve the COPY failure
when cleanup also fails. Leave target resolution, production confirmation
and the explicitly unwrapped route intact.

Prove the CLI refuses the flags without a readable input file. Exercise
transaction boundaries with synthetic rows on disposable PostgreSQL, and
verify the actual executable reports a rejected combination and zero new
rows. Run the full local verifier and check the exact pull request and main.

For production confirmation, use the common workbench review with a snapshot
of the options, target and installed query session. Check that session after
waiting for the user SQL gate. The runtime uses the same installed query
backend that the user approved. Verify the reducer and the live workbench
against a disposable database with an explicit production label.

Use the existing preview request and query session identities to track the
source. Preserve the established parsing, mapping and transaction paths for
readable files. Prove initial absence, invalid text, removal after preview,
and recovery through a readable header-only and one-row source.

Use opt-in completion-aware iteration on the existing COPY response stream.
After the end-of-data message, require its COPY command tag and ReadyForQuery.
Keep the same connection under the session's SQL gate while processing
those messages. Report the first server, connection or protocol failure and
return buffered data only after confirmation. Reuse value-free transfer
diagnostics from startup through completion.

Use a deferred unique constraint in a disposable database to distinguish
data arrival from confirmed completion. Compare exact stdout and file
results and verify recovery on the same session. Exercise successful
nonempty and header-only CSV.
[PostgreSQL protocol flow](https://www.postgresql.org/docs/18/protocol-flow.html#PROTOCOL-COPY)

Feed data from the confirmed COPY stream into the existing file writer for
named destinations. Reserve the partial on the first nonempty chunk and
publish through the writer only after this request confirms completion.
Reserve on completion when no bytes arrive. On server failure, preserve
the typed diagnostic and refer to the partial if one was opened.

Use an independently held advisory lock to observe partial-file data
before completion. Verify the prior final bytes, then release the lock
and verify the new file. Measure peak process RSS across repeated runs
using fixed-width CSV at two sizes and compare output digests on matched
builds. Exercise startup and deferred failures and zero-byte publication
through the real CLI.

Opt bulk COPY import into completion checking on its existing sink.
Retain the COPY row count while waiting for the same request's readiness
message. Propagate a deferred rejection through the value-free transfer
diagnostic. Verify with a deferred unique constraint that rejects only
at the end of autocommit. Exercise subsequent COPY on the same session
and confirm that an explicit caller transaction still owns its rollback.
