# Feature Specification: Session State Persistence and Automatic Crash Recovery (043)

## 1. Context and Problem Statement
When database administrators, data engineers, and developers write ad-hoc SQL, craft schema migrations, or analyze incident forensics in a terminal workbench, unexpected interruptions can occur:
- Terminal emulator crashes, PTY disconnects, network drops (SSH), or battery exhaustion.
- OS signals (`SIGKILL`, `SIGTERM`, system restart, kernel panic).
- Accidental terminal window closures while multiple editor tabs contain unsaved scratchpad queries.

Without persistent session state and automatic crash recovery, critical queries, uncommitted transaction templates, and multi-tab scratchpads are lost permanently, forcing operators to rebuild queries from memory or scour shell histories.

Feature 043 delivers automated, atomic session state persistence and crash recovery, completing **Milestone 9: Multi-Buffer Editor & Scratchpad Persistence**.

## 2. User Scenarios & Capabilities

### Scenario 1: Automatic Recovery Detection on Startup
An operator had 3 open editor tabs with unsaved schema migration drafts when their SSH session dropped. Upon reconnecting and launching `ignatius`:
- Ignatius detects an unrecovered session journal in `<data_dir>/recovery/session_journal.json`.
- A prominent Dieter Rams Field Industrial hardware capsule appears:
  `[CRASH RECOVERY: 3 UNSAVED BUFFERS DETECTED] [Ctrl+K r] Restore  [Ctrl+K d] Discard`
- Pressing `Ctrl+K r` immediately restores all 3 tabs with their respective titles, file path associations, cursor positions, dirty markers, and split layout.

### Scenario 2: Headless Scriptable Recovery
An operator working in an automated terminal environment or headless server checks if any orphaned queries remain:
- `ignatius recovery status` prints a clear summary of unrecovered buffers, timestamps, and session ID.
- `ignatius recovery restore --format sql` outputs the recovered SQL statements directly to stdout.
- `ignatius recovery discard` cleans up orphaned recovery files.

### Scenario 3: Clean Exit Safety
When the operator exits normally via `q`, `Quit`, or `Ctrl+C`:
- The recovery journal is cleanly cleared or flagged with `clean_exit: true`.
- Subsequent launches start fresh without false crash alarms.

## 3. Non-Functional Requirements & Design Principles
- **Dieter Rams Field Industrial Design**: Hardware recovery capsule styled with `Token::Focus` (phosphor amber `#FBBF24`) and high-contrast labels.
- **Strict ASCII Rule**: No Unicode em dashes or en dashes in code, markdown, or stdout.
- **Atomic Journal Writes**: Recovery state is written to a temporary sibling file and atomically renamed to prevent corruption during power cuts or crashes mid-write.
- **Resource Bounds**: Recovery files are capped in size; individual buffer retention obeys system memory bounds.
