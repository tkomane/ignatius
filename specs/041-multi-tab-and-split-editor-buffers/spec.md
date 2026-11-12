# Feature Specification: Multi-Tab and Split Editor Buffers (Feature 041)

## 1. Overview
In graphical database tools like pgAdmin 4, DBeaver, and DataGrip, database engineers routinely work across multiple distinct SQL queries simultaneously:
- Drafting schema migrations in one buffer while testing ad-hoc queries in another.
- Running performance profiling queries in one buffer while comparing against an unoptimized baseline in a side-by-side split.
- Retaining long-lived scratchpad notes and diagnostic scripts without overwriting active work.

Feature `041-multi-tab-and-split-editor-buffers` delivers a multi-tab SQL workspace with split-buffer editing to Ignatius, allowing engineers to maintain multiple independent SQL buffers, switch between them instantly, view and edit two buffers side-by-side or stacked (split layout), and run queries independently with full undo/redo isolation.

## 2. User Scenarios & Capabilities

### Scenario 1: Multi-Tab SQL Workspaces
- An engineer opens Ignatius. By default, Buffer 1 is active (`buffer-1.sql`).
- The engineer presses `Ctrl+K =` (`new-buffer`) or clicks the `[+]` button on the tab bar.
- A new buffer (`buffer-2.sql`) is created and focused immediately.
- The engineer types a new query in Buffer 2.
- Pressing `Ctrl+K }` (`next-buffer`) or `Ctrl+K {` (`prev-buffer`) cycles between open buffers seamlessly.
- Clicking any tab pill in the top tab bar switches to that buffer instantly.
- Each buffer maintains its own independent text, cursor position, undo/redo history stack, and modification status (`*`).

### Scenario 2: Split-Buffer Editing (Side-by-Side & Stacked)
- An engineer wants to compare a query in Buffer 1 with a query in Buffer 2.
- The engineer presses `Ctrl+K \` (`split-buffer`) to toggle split view mode:
  - Default: `None` (single active buffer fills editor pane).
  - First toggle: `Vertical` (side-by-side dual editor panes).
  - Second toggle: `Horizontal` (stacked dual editor panes).
  - Third toggle: returns to `None`.
- In split mode, the primary buffer is rendered in the left/top pane, and the secondary buffer is rendered in the right/bottom pane.
- Clicking inside either split pane focuses that buffer.
- Running a statement (`Ctrl+R` / `F5`) executes the query belonging to the currently focused active split buffer!

### Scenario 3: Buffer Management & Safety
- Pressing `Ctrl+K -` (`close-buffer`) closes the current buffer:
  - If it is the only open buffer, its contents are cleared to an empty scratch buffer.
  - If multiple buffers are open, the closed buffer is removed from the tab bar and the adjacent buffer is focused.
- Buffer rename: Each buffer can be named, or automatically takes the filename if loaded from disk via `--file` or file dialog.
- Bounded memory: Maximum of 16 concurrent buffers allowed, preventing unbounded memory consumption.

## 3. Visual & Aesthetic Tenets (Dieter Rams / Field Industrial)
- **Top Tab Bar**: Sits elegantly above the editor content area within the editor pane.
- **Hardware Status Capsules**:
  - `[BUFFERS: n/16]`
  - `[ACTIVE: buffer-1.sql *]`
  - `[SPLIT: OFF | VERTICAL | HORIZONTAL]`
- **Tab Pills**:
  - Unfocused tabs: rendered with muted slate border and text (`[1: buffer-1.sql]`).
  - Active focused tab: highlighted with Dieter Rams amber (`#FBBF24` / `Token::Focus`) and bold weight (`[1: buffer-1.sql *]`).
  - Modified indicator: clean asterisk `*` indicating uncommitted/unsaved edits.
  - Add tab button: `[+]` at the right end of the tab bar.

## 4. Keymap & Action Bindings
- `Ctrl+K =`: `Action::NewEditorBuffer` (`new-buffer`) - Create new SQL editor buffer.
- `Ctrl+K -`: `Action::CloseEditorBuffer` (`close-buffer`) - Close current SQL editor buffer.
- `Ctrl+K }`: `Action::NextEditorBuffer` (`next-buffer`) - Switch to next editor buffer.
- `Ctrl+K {`: `Action::PrevEditorBuffer` (`prev-buffer`) - Switch to previous editor buffer.
- `Ctrl+K \`: `Action::ToggleEditorSplit` (`split-buffer`) - Cycle editor split mode (None -> Vertical -> Horizontal -> None).
- Mouse: Left-click on tab pills switches active buffer; left-click on `[+]` opens a new buffer.

## 5. Non-Interactive CLI Parity
- `ignatius --file query1.sql --file query2.sql`: starts workbench with specified files pre-loaded into distinct tabs.
