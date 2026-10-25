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
