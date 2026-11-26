# Feature 028 - COPY import ownership and export completion

This maintenance slice extends the [Feature 028 import and export contract](../../docs/architecture/pgadmin-parity-plan.md#feature-028-data-importexport-wizard-and-backuprestore-028-data-import-export-and-backup).

- **FR-2801.** Dry-run import requires Ignatius to own a transaction.
  The command rejects an explicit conflicting transaction flag before
  reading input, acquiring credentials or connecting. The session rejects
  the same invalid options before starting COPY, including wizard requests.
- **FR-2802.** A transaction-wrapped import requires an autocommit session
  before it starts. If the current transaction is open, failed or unknown,
  refuse before COPY and leave the existing transaction under user control.
  Explicitly unwrapped import keeps its existing transaction semantics.
- **FR-2803.** Successful dry-run output requires acknowledgment of ROLLBACK
  from PostgreSQL. Preserve the original COPY error if failure also prevents
  rollback confirmation, and report that rollback could not be confirmed.
  COPY and commit diagnostics preserve safe SQLSTATE codes and omit text
  that can disclose row values.
  The production confirmation requirement applies to dry runs.
- **FR-2804.** For a production-classified interactive connection, an
  import requires the exact resolved database name and an explicit
  acceptance. The review shows the destination and commit or rollback
  intent. Approval applies to one configuration and one query session.
  Cancelling, switching sessions or entering a mismatched name prevents
  the import from starting. A queued import rechecks its approved session
  before starting under the user SQL gate.
- **FR-2805.** The wizard requires a completed preview from the current
  source. A preview read or decode failure removes cached rows, mappings
  and results. An execution-time read failure reports the error before
  COPY and returns to the source step. A readable header-only source can
  complete a zero-row import.
- **FR-2806.** Bulk COPY export publishes its bytes only after PostgreSQL
  reports the COPY command completion and readiness on the same request.
  A server rejection preserves its SQLSTATE and omits server-controlled
  text that can disclose result values or query inputs. An unconfirmed
  completion keeps both output routes unpublished. Explicit replacement
  applies after confirmed completion and successful local writing. The
  caller's transaction remains under caller control.
- **SC-2801.** With disposable PostgreSQL, valid dry-run returns the row count
  and leaves zero imported rows. Contradictory options leave zero rows.
  A refused nested import leaves the caller's transaction and its sentinel
  row intact until the user ends that transaction. Intentional unwrapped
  import still writes the supplied rows.
- **SC-2802.** Synthetic production confirmation emits one import after
  exact typed approval and none after an empty, mismatched or dismissed
  review. A retired session cannot begin a queued import.
- **SC-2803.** Failed preview and execution reads emit no COPY. A statement
  trigger or independent backend observation distinguishes this from a
  confirmed zero-row COPY with a readable source.
- **SC-2804.** An owned deferred constraint rejects an autocommit COPY
  after it produced data. Standard output stays empty and a forced file
  destination retains its original bytes. The error carries SQLSTATE
  23505 without the duplicate value. The same session can run another
  query, and successful empty and nonempty CSV retain exact bytes.
