# ADR-0007: Secrets in dedicated types, keyring deferred to Feature 002

- Status: Accepted
- Date: 2026-08-15

## Context

A database client handles passwords from URIs, environment variables, prompts and
eventually the OS credential store. Every one of them can end up in a log line, a
diagnostic, a screenshot or a crash report.

## Decision

- Passwords are held in `secrecy::SecretString`, whose `Debug` implementation
  prints a placeholder, so a struct containing one cannot leak it through
  ordinary formatting. Covered by a test.
- All displayable text passes through one redaction implementation in
  `src/diagnostics/redaction.rs`, which understands URI userinfo and libpq
  keyword forms. Diagnostics redact on construction, so a caller cannot forget.
- `PGPASSWORD` is consumed when present, never displayed, and produces a note
  offering the safer route. It is never recommended as persistent configuration.
- OS credential store integration (`keyring` 4.1.6, verified available on
  2026-08-15) is deferred to Feature 002, together with prompting, `.pgpass` and
  service files.

## Alternatives considered

- **Redacting at each call site.** Two implementations eventually differ, and the
  difference is a credential leak.
- **Encrypting a local secret file ourselves.** Rejected: the platform credential
  stores exist, and a home-grown encrypted file invites claims the code cannot
  support.

## Consequences

- Until Feature 002 lands, the supported credential routes are a URI, a keyword
  string, and `PGPASSWORD`. This is stated in `docs/support/compatibility.md`
  rather than implied.
- Redaction is documented as pattern-based: it recognises the shapes PostgreSQL
  clients use, and cannot recognise a bare secret pasted into prose. Callers must
  not put bare secrets into messages.

## Reversibility

High. Adding the credential store is additive.
