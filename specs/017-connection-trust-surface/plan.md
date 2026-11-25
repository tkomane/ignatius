# Implementation plan for provider presentation provenance

W06 advances Feature 017 FR-1303 and FR-1308. Explicit startup and the profile
picker use one display-only projection of the same exact-key registry used by
authentication. Validate configured providers first. Only entries supplied by
the built-in registry receive vendor-specific command hints and lifetimes.

Build safe facts in `src/connection/cloud.rs`. `src/cli/interactive.rs` shares
that map with the initial target and the profile summaries in
`src/app/connection_picker.rs`. Keep token acquisition in the existing
execution boundary and display sanitization in the connection-details widget.

Prove the baseline emits a stock label for a configured `entra` replacement.
Cover an identical stock copy, exact-key case variants and retained stock
facts. Exercise the real startup and picker routes against a disposable
verified-TLS fixture, including argument and token privacy and repeated panel
opening. Run the local gate, PR checks and post-merge CI and CodeQL. Native
vendor CLI and live provider acceptance are separate evidence.
