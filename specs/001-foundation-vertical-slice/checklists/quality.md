# Quality Checklist: Foundation and proven vertical slice

**Purpose**: Confirm the requirements in `spec.md` are complete, unambiguous and
testable before and during implementation.
**Created**: 2026-08-15
**Feature**: [spec.md](../spec.md)

**Marker semantics**: `[x]` means the requirements-quality criterion has been
reviewed and satisfied. It does not mean the implementation is complete.

## Requirement quality

- [x] CHK001 Every requirement has a stable identifier and one testable claim
- [x] CHK002 No requirement states an implementation choice as a requirement
- [x] CHK003 Success criteria are measurable and technology-agnostic
- [x] CHK004 Every requirement maps to at least one test in the traceability table
- [x] CHK005 Requirements that this release does not meet are named, not omitted

## User experience

- [x] CHK006 Every state carried by colour is also carried by text
- [x] CHK007 Error requirements specify all four layers, not just a message
- [x] CHK008 Status wording is specified for uncertain states, not only success and failure
- [x] CHK009 Narrow and below-minimum terminals have specified behaviour
- [x] CHK010 The empty state specifies what the user should do next

## Accessibility

- [x] CHK011 Every action is reachable from the keyboard, with visible focus
- [x] CHK012 No requirement depends on mouse input
- [x] CHK013 Palettes have a stated contrast threshold that tests enforce
- [x] CHK014 An ASCII presentation is required, not optional
- [x] CHK015 A plain line-oriented mode for screen readers is specified and built

## Security

- [x] CHK016 Every credential route is named and its handling specified
- [x] CHK017 TLS requirements distinguish encryption from identity verification
- [x] CHK018 Behaviour for unsupported security parameters is specified as refusal
- [x] CHK019 Server-supplied text is treated as hostile in the requirements
- [x] CHK020 What redaction cannot promise is written down

## PostgreSQL correctness

- [x] CHK021 Statement splitting is specified by quoting rules, not by delimiter
- [x] CHK022 NULL, empty string and the text `NULL` are required to be distinct
- [x] CHK023 Value fidelity requirements cover numerics and time zones
- [x] CHK024 Cancellation is specified as a request with a separate confirmation
- [x] CHK025 The supported server version window is stated with its source date

## Release readiness

- [x] CHK026 Exit codes are enumerated and declared stable
- [x] CHK027 Release identity facts are enumerated and required to be distinct
- [x] CHK028 No claim is made about signing, notarisation or provenance
- [x] CHK029 Unsupported libpq surface is documented rather than implied
- [ ] CHK030 Cross-platform verification evidence exists for all three targets
