# ADR-0002: Ratatui with Crossterm for the terminal interface

- Status: Accepted
- Date: 2026-08-15

## Context

The product needs a full-screen interface that behaves identically in Warp on
macOS and Windows Terminal on Windows, plus the ability to render into a buffer
in tests so layout can be asserted without a pseudo-terminal.

## Decision

Ratatui 0.30.2 for layout and widgets, with Crossterm 0.29.0 as the single
terminal abstraction for both output and input. No Win32 console calls are mixed
with virtual terminal sequences.

## Alternatives considered

- **Cursive.** Higher-level, but its callback model fights the unidirectional
  state model in ADR-0003 and makes pure-function rendering harder to test.
- **Writing escape sequences directly.** Full control, but re-implements width
  handling, diffing and Windows compatibility, all of which are where terminal
  bugs live.
- **Termwiz backend.** Capable, but Crossterm is the better-trodden path for the
  Windows target and Ratatui treats it as the default.

## Consequences

- Ratatui 0.30 is a restructuring release that splits the project into
  `ratatui-core`, `ratatui-crossterm` and `ratatui-widgets`. The facade crate is
  used, and the Crossterm version is pinned to the one the facade resolves, so
  the two cannot drift. Verified by dependency resolution on 2026-08-15:
  ratatui 0.30.2 resolves crossterm 0.29.0.
- Rendering into a `Buffer` gives layout tests without a pty. Reconstructing text
  from a buffer must step over the cell reserved after a wide character; the
  helper in `src/ui/layout.rs` does this and is covered by a test.
- Terminal acquisition and restoration are ours, not the library's: see
  `src/ui/terminal.rs`.

## Reversibility

Medium. Confined to `src/ui`, but every widget would need rewriting.
