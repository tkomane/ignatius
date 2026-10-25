# W10 local-state hardening plan

**Date:** 2026-10-25. This repairs the existing saved-query confinement and
recoverability requirements; it does not add a new product workflow.

Source findings: query saves followed a destination symlink; configuration
and history replacements truncated predictable temporary paths, including
symlinks. The governing requirements are SEC-1002 here, foundation REL-004,
history FR-604/605 and SEC-604, and constitution X.

Use one dependency-free private atomic-write helper in `src/platform/atomic.rs`.
Create a unique sibling temporary with `create_new` and Unix mode `0600`, write,
flush and sync, then close and rename. Never reuse an existing temporary; clean
up only a temporary created by this attempt. Route configuration writes,
history trimming and saved-query replacements through it. Query load/save
refuse links and non-regular leaf entries; listings skip them. Ordinary named
replacement and damaged-history-line recovery remain supported.

Tests use isolated `tempfile` roots and establish unchanged outside targets,
preserved old state on failed replacement, stale temporary preservation,
successful replacement and owner-only permissions. No production directories
or database fixtures are involved. Same-user directory races, directory-entry
durability across power loss and multi-process history locking are not claimed.
Load/list leaf checks do not defend against same-user path replacement races.

The integrator owns these files, security documentation, changelog and status.
Run focused platform/config/history/query tests, then the locked repository
verifier. Record native Windows/Linux execution separately from macOS evidence.

## Migration-backup follow-up - W10-006

The review also identified a distinct unsafe route: migration selected its
backup name with `exists()` and then copied by path. A dangling link could be
followed, source permissions could be inherited, and a second source read could
back up different bytes from the parsed configuration. Foundation REL-004 now
states this boundary explicitly.

Reuse the private exclusive-file primitive, independently of atomic replacement.
Capture one timestamp; attempt its base name then numbered suffixes, at most
128 times, retrying only `AlreadyExists`. Write the exact parsed source bytes
through the reserved owner-only handle, flush and sync, and only then replace
the configuration. On an incomplete backup, close and remove only that newly
reserved file. Keep a complete backup if the later replacement fails.

A private timestamp seam and narrow writer failure injection make ordinary and
dangling-link collisions, reservation exhaustion, private initial permissions,
partial-write cleanup and old-configuration preservation deterministic tests.
They do not claim real power-loss recovery or protection from same-user path
replacement races. Configuration source, backup and replacement remain inside
the existing trusted directory; no new filesystem root or credential route.
