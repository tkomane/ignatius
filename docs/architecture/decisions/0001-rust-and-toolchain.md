# ADR-0001: Rust as the implementation language

- Status: Accepted
- Date: 2026-08-15

## Context

The product is a single local binary that must start instantly, idle cheaply, own
raw terminal state, hold credentials in memory without leaking them, and ship to
macOS, Windows and Linux without a runtime for the user to install.

## Decision

Rust, on the stable channel, pinned in `rust-toolchain.toml`. The lockfile is
committed. Edition 2024.

## Alternatives considered

- **Go.** Comparable single-binary story and good TUI libraries. Rejected on
  secret handling: the garbage collector makes "this value is now gone" hard to
  claim honestly, and the type system gives less help modelling states like
  "cancellation requested but unconfirmed" exhaustively.
- **Python.** pgcli and Harlequin prove it works. Rejected on distribution: a
  user-installable runtime is exactly the onboarding tax this product exists to
  remove.
- **C or C++ against libpq directly.** Maximum fidelity, and the memory-safety
  cost is not worth it for a tool that parses hostile server output.

## Consequences

- Compile times are a real cost to contributors; the verification command is
  structured so the fast gates run first.
- The `unsafe_code = "deny"` lint is set at the crate level. Any future need for
  unsafe requires an ADR.
- Verified on 2026-08-15: local toolchain is rustc 1.97.1. `rust-toolchain.toml`
  is honoured by rustup; the primary development machine uses a Homebrew rustc,
  where the pin is inert. This is an environment limitation, recorded in
  `docs/operations/local-development.md`, not a defect.

## Reversibility

Low. This decision is effectively permanent once the codebase exists.
