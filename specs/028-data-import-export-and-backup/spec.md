# Feature 028 - Import transaction ownership

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
- **SC-2801.** With disposable PostgreSQL, valid dry-run returns the row count
  and leaves zero imported rows. Contradictory options leave zero rows.
  A refused nested import leaves the caller's transaction and its sentinel
  row intact until the user ends that transaction. Intentional unwrapped
  import still writes the supplied rows.
