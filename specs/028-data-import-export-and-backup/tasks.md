# Tasks - Import transaction ownership

- [x] T001 Refuse contradictory dry-run flags and unsafe transaction
  ownership through the CLI and session.
- [x] T002 Prove confirmed rollback, zero durable rows, preserved caller
  transaction and intentional unwrapped import on the disposable fixture.
- [x] T003 Record the focused regressions, executable witness and local
  verification evidence.
- [x] T004 Require one-shot typed database approval for production-labelled
  wizard imports, including dry runs. Verify the shown destination, wrong name and
  dismissal, and retire pending and queued approvals when the query
  session changes.
- [x] T005 Report real preview and execution file errors. Prove that missing,
  invalid and removed sources produce zero COPY statements, and readable
  files recover through a fresh preview.

[Local evidence](../../docs/status.md#wave-92---keep-bulk-import-transactions-under-caller-control---26-november-2026)

[Production approval and queue evidence](../../docs/status.md#wave-95---require-typed-approval-for-production-imports---26-november-2026)

[Source read and COPY evidence](../../docs/status.md#wave-96---report-import-source-file-failures-before-copy---26-november-2026)
