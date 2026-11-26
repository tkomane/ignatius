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
return data only after confirmation. Reuse value-free transfer diagnostics
from startup through completion. Keep the existing buffering and publication
boundary on both CLI routes.

Use a deferred unique constraint in a disposable database to distinguish
data arrival from confirmed completion. Compare exact stdout and file
results and verify recovery on the same session. Exercise successful
nonempty and header-only CSV. The bounded-memory export work remains open.
[PostgreSQL protocol flow](https://www.postgresql.org/docs/18/protocol-flow.html#PROTOCOL-COPY)
