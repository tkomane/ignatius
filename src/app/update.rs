//! The reducer.
//!
//! One pure function owns every state transition. It performs no I/O, takes no
//! locks, and reads no clock, so every rule below is testable in microseconds and
//! stays true regardless of timing:
//!
//! - A result whose job identity is not the one in flight is discarded. A slow
//!   query that finishes after a newer one cannot overwrite the newer result.
//! - Cancelling moves to `CancellationRequested`, never to "cancelled". Only the
//!   server's answer produces the cancelled state.
//! - Statements are never sent on a connection that is not usable.
//! - Nothing is ever re-run automatically.

use crate::app::message::{Action, Direction, Effect, Message};
use crate::app::model::{ConnectionState, Focus, Model, QueryPhase};
use crate::query::result::ExecutionStatus;
use crate::query::statements;

/// Applies a message to the model and returns the effects to perform.
pub fn update(model: &mut Model, message: Message) -> Vec<Effect> {
    match message {
        Message::Action(action) => apply_action(model, action),
        Message::Resized(columns, rows) => {
            model.size = (columns, rows);
            Vec::new()
        }
        Message::Connected(info) => {
            model.connection = ConnectionState::Connected(info);
            model.error = None;
            Vec::new()
        }
        Message::ConnectionFailed(diagnostic) => {
            model.error = Some((*diagnostic).clone());
            model.connection = ConnectionState::Failed(diagnostic);
            model.phase = QueryPhase::Idle;
            Vec::new()
        }
        Message::ConnectionLost => {
            if let Some(info) = model.connection.info() {
                model.connection = ConnectionState::Lost {
                    info: Box::new(info.clone()),
                };
            }
            model.phase = QueryPhase::Idle;
            Vec::new()
        }
        Message::ExecutionFinished(execution) => {
            // A result only counts if it belongs to the job still in flight.
            if model.phase.job() != Some(execution.job) {
                return Vec::new();
            }
            model.phase = QueryPhase::Idle;
            model.running_for = None;
            model.transaction = execution.transaction;
            model.last_elapsed = Some(execution.elapsed);
            model.error = execution.error.clone();
            model.error_expanded = false;
            model.notices = execution
                .statements
                .iter()
                .flat_map(|s| s.notices.clone())
                .collect();
            if execution.status == ExecutionStatus::ConnectionLost
                && let Some(info) = model.connection.info()
            {
                model.connection = ConnectionState::Lost {
                    info: Box::new(info.clone()),
                };
            }
            model.selected_row = 0;
            model.selected_column = 0;
            // A filter belongs to the rows it was typed against. Carrying it
            // into a different result would hide rows the user never filtered.
            model.result_filter.clear();
            model.result_filtering = false;
            // The cell it was showing belongs to a result that no longer exists.
            // Leaving it open would show a value from one query labelled as
            // though it came from another.
            model.inspector = None;
            let outcome = crate::history::Outcome::from_status(&execution.status);
            let elapsed = execution.elapsed;
            model.last_execution = Some(*execution);
            // The statement is recorded once it has an outcome, so what the
            // history holds is what really ran rather than what was submitted.
            match model.running_sql.take() {
                Some(sql) if model.records_history() => vec![Effect::RecordHistory {
                    sql,
                    outcome,
                    elapsed,
                }],
                _ => Vec::new(),
            }
        }
        Message::CancellationDelivered(_) => Vec::new(),
        Message::CancellationFailed(diagnostic) => {
            model.error = Some(*diagnostic);
            Vec::new()
        }
        Message::Notices(notices) => {
            model.notices.extend(notices);
            Vec::new()
        }
        Message::SchemasLoaded(result) => {
            match *result {
                Ok(schemas) => model.tree.set_schemas(schemas),
                Err(error) => model.tree.set_error(error),
            }
            Vec::new()
        }
        Message::MetadataLoaded {
            request,
            path,
            payload,
        } => {
            match *payload {
                Ok(loaded) => {
                    model.tree.apply(request, &path, loaded);
                }
                Err(error) => {
                    // The reason belongs in the tree, next to the node that
                    // could not be opened, rather than in a modal somewhere else.
                    model.tree.apply_failure(request, &path, &error.headline);
                }
            }
            Vec::new()
        }
        Message::DependenciesLoaded { request, result } => {
            // The palette is only filled when it is still the answer to what was
            // asked, so a slow lookup cannot replace a list the user has moved on
            // to something else in.
            if model.pending_dependencies != Some(request) {
                return Vec::new();
            }
            model.pending_dependencies = None;
            match *result {
                Ok(dependencies) => {
                    model.palette = Some(crate::app::palette::Palette::over_dependencies(
                        dependency_entries(&dependencies),
                    ));
                }
                Err(error) => {
                    model.error = Some(error);
                }
            }
            Vec::new()
        }
        Message::MetadataConnection(link) => {
            model.metadata_link = link;
            Vec::new()
        }
        Message::DefinitionLoaded { request, result } => {
            // A definition that is not the one being waited for is discarded,
            // exactly as a stale tree load or a stale result is.
            let Some(open) = model.definition.as_mut() else {
                return Vec::new();
            };
            if open.pending != Some(request) {
                return Vec::new();
            }
            open.pending = None;
            match *result {
                Ok(definition) => {
                    open.heading = definition.heading();
                    open.definition = Some(definition);
                }
                Err(error) => open.error = Some(error.headline),
            }
            Vec::new()
        }
        Message::HistoryLoaded(mut entries) => {
            entries.truncate(crate::history::IN_MEMORY_LIMIT);
            model.history = entries;
            Vec::new()
        }
        Message::HistoryRecorded { entry, recorded } => {
            // Only what actually reached the file appears here. A statement the
            // history refused must not be offered back as though it were kept.
            model.history_note = recorded.note();
            if let Some(entry) = *entry {
                model.history.insert(0, entry);
                // The bound is enforced here as well as at load, so a long
                // session cannot walk past it one statement at a time.
                model.history.truncate(crate::history::IN_MEMORY_LIMIT);
            }
            Vec::new()
        }
        Message::Tick { running_for } => {
            model.frame = model.frame.wrapping_add(1);
            model.running_for = running_for;
            Vec::new()
        }
    }
}

fn apply_action(model: &mut Model, action: Action) -> Vec<Effect> {
    // Modes are peeled in a fixed order, highest first. Getting this wrong is
    // how typing in the editor starts doing surprising things, so the order is
    // stated once here and tested directly.
    if model.prefix_pending {
        return resolve_prefix(model, action);
    }
    if model.pending_run.is_some() {
        return confirmation_action(model, action);
    }
    if model.palette.is_some() {
        return palette_action(model, action);
    }
    if model.tree.filtering {
        return filter_action(model, action);
    }
    if model.result_filtering {
        return result_filter_action(model, action);
    }
    if model.inspector.is_some() {
        return inspector_action(model, action);
    }
    if model.definition.is_some() {
        return definition_action(model, action);
    }
    if model.focus == Focus::Objects
        && !matches!(action, Action::Quit | Action::ToggleHelp)
        && let Some(effects) = objects_action(model, &action)
    {
        return effects;
    }

    match action {
        Action::Quit => {
            model.should_quit = true;
            vec![Effect::Quit]
        }
        Action::ToggleHelp => {
            model.help_open = !model.help_open;
            Vec::new()
        }
        Action::Dismiss => {
            // One layer at a time, in the order they were opened. Dismissing an
            // error the user has not read is the more expensive mistake, so it
            // goes last.
            if model.help_open {
                model.help_open = false;
            } else if !model.tree.filter.is_empty() {
                model.tree.filter.clear();
            } else {
                model.error = None;
                model.error_expanded = false;
            }
            Vec::new()
        }
        Action::ToggleErrorDetail => {
            model.error_expanded = !model.error_expanded;
            Vec::new()
        }
        Action::FocusNext => {
            model.focus = model.focus.next(model.sidebar_visible);
            Vec::new()
        }
        Action::ToggleSidebar => {
            model.sidebar_visible = !model.sidebar_visible;
            if !model.sidebar_visible && model.focus == Focus::Objects {
                model.focus = Focus::Editor;
            }
            // The tree loads the first time it is shown, not at startup, so a
            // user who never opens it never pays for it.
            if model.sidebar_visible && model.tree.roots.is_empty() && !model.tree.loading {
                model.tree.begin_loading();
                return vec![Effect::LoadSchemas];
            }
            Vec::new()
        }
        Action::OpenPalette => {
            model.palette = Some(crate::app::palette::Palette::new(palette_entries(model)));
            Vec::new()
        }
        Action::BeginPrefix => {
            model.prefix_pending = true;
            Vec::new()
        }
        // The filter key belongs to whatever is being looked at. In the results
        // that means the rows; anywhere else it means the object tree.
        Action::StartFilter if model.focus == Focus::Results => {
            model.result_filtering = true;
            Vec::new()
        }
        Action::StartFilter => {
            model.focus = Focus::Objects;
            model.sidebar_visible = true;
            model.tree.filtering = true;
            Vec::new()
        }
        Action::ReloadObjects => {
            model.tree.begin_loading();
            vec![Effect::LoadSchemas]
        }
        Action::Activate if model.focus == Focus::Editor => {
            model.editor.insert('\n');
            Vec::new()
        }
        // In the results pane, Enter means "show me this value in full", which
        // is the only thing there is to do to a cell.
        Action::Activate if model.focus == Focus::Results => {
            toggle_inspector(model);
            Vec::new()
        }
        Action::Activate => Vec::new(),
        Action::ShowDefinition => show_definition(model),
        Action::ShowDependencies => show_dependencies(model),
        Action::OpenHistory => {
            model.palette = Some(crate::app::palette::Palette::over_history(history_entries(
                model,
            )));
            Vec::new()
        }
        Action::ToggleHistoryRecording => {
            // Configuration wins: a session cannot turn on something the user
            // has switched off in their own file.
            if !model.history_disabled {
                model.history_paused = !model.history_paused;
            }
            Vec::new()
        }
        Action::ToggleExpandedRow => {
            model.expanded_row = !model.expanded_row;
            Vec::new()
        }
        Action::ToggleInspector => {
            toggle_inspector(model);
            Vec::new()
        }
        Action::RunBuffer => run(model, model.editor.text().to_owned()),
        Action::RunStatement => {
            let sql = statements::statement_at(model.editor.text(), model.editor.cursor())
                .map(|s| s.text)
                .unwrap_or_default();
            run(model, sql)
        }
        Action::Cancel => match model.phase {
            QueryPhase::Running { job, .. } => {
                model.phase = QueryPhase::CancellationRequested { job };
                vec![Effect::Cancel { job }]
            }
            // Cancelling twice must not send a second request or change wording.
            QueryPhase::CancellationRequested { .. } | QueryPhase::Idle => Vec::new(),
        },
        Action::Insert(ch) if model.focus == Focus::Editor => {
            model.editor.insert(ch);
            Vec::new()
        }
        Action::Backspace if model.focus == Focus::Editor => {
            model.editor.backspace();
            Vec::new()
        }
        Action::Newline if model.focus == Focus::Editor => {
            model.editor.insert('\n');
            Vec::new()
        }
        Action::Insert(_) | Action::Backspace | Action::Newline => Vec::new(),
        Action::Move(direction) => {
            move_selection(model, direction);
            Vec::new()
        }
        // Editing keys belong to the editor. Elsewhere they do nothing rather
        // than doing something that looks like an edit somewhere else.
        Action::DeleteForward if model.focus == Focus::Editor => {
            model.editor.delete_forward();
            Vec::new()
        }
        Action::DeleteWordLeft if model.focus == Focus::Editor => {
            model.editor.delete_word_left();
            Vec::new()
        }
        Action::MoveWord(direction) if model.focus == Focus::Editor => {
            match direction {
                Direction::Left => model.editor.move_word_left(),
                Direction::Right => model.editor.move_word_right(),
                Direction::Up | Direction::Down => {}
            }
            Vec::new()
        }
        Action::MoveLineStart if model.focus == Focus::Editor => {
            model.editor.move_line_start();
            Vec::new()
        }
        Action::MoveLineEnd if model.focus == Focus::Editor => {
            model.editor.move_line_end();
            Vec::new()
        }
        Action::Undo if model.focus == Focus::Editor => {
            model.editor.undo();
            Vec::new()
        }
        Action::Redo if model.focus == Focus::Editor => {
            model.editor.redo();
            Vec::new()
        }
        // These two are useful in every pane, so they are not editor-only.
        Action::MoveBufferStart => {
            match model.focus {
                Focus::Editor => model.editor.move_buffer_start(),
                Focus::Results => {
                    model.selected_row = 0;
                }
                Focus::Objects => model.tree.move_selection(isize::MIN / 2),
            }
            Vec::new()
        }
        Action::MoveBufferEnd => {
            match model.focus {
                Focus::Editor => model.editor.move_buffer_end(),
                Focus::Results => {
                    model.selected_row = model
                        .visible_result()
                        .map_or(0, |set| set.rows.len().saturating_sub(1));
                }
                Focus::Objects => model.tree.move_selection(isize::MAX / 2),
            }
            Vec::new()
        }
        Action::MovePage(direction) => {
            page(model, direction);
            Vec::new()
        }
        Action::DeleteForward
        | Action::DeleteWordLeft
        | Action::MoveWord(_)
        | Action::MoveLineStart
        | Action::MoveLineEnd
        | Action::Undo
        | Action::Redo => Vec::new(),
    }
}

/// Opens the definition of whatever object is selected.
///
/// Only an object has a definition. On anything else this does nothing rather
/// than opening an empty panel, and the tree is where the user can see what is
/// selected.
fn show_definition(model: &mut Model) -> Vec<Effect> {
    if model.definition.is_some() {
        model.definition = None;
        return Vec::new();
    }
    let Some(object) = selected_object(model) else {
        return Vec::new();
    };
    let request = model.tree.allocate_request();
    model.definition = Some(crate::app::model::Definition {
        pending: Some(request),
        heading: format!(
            "{} {}",
            object.kind.singular(),
            crate::postgres::metadata::quote_identifier(&object.name)
        ),
        definition: None,
        error: None,
        scroll: 0,
    });
    vec![Effect::LoadDefinition {
        request,
        object: Box::new(object),
    }]
}

/// Asks what the selected object depends on, and what depends on it.
fn show_dependencies(model: &mut Model) -> Vec<Effect> {
    let Some(object) = selected_object(model) else {
        return Vec::new();
    };
    let request = model.tree.allocate_request();
    model.pending_dependencies = Some(request);
    vec![Effect::LoadDependencies {
        request,
        object: Box::new(object),
    }]
}

/// The object the tree has selected, when it has one.
fn selected_object(model: &Model) -> Option<crate::postgres::metadata::ObjectSummary> {
    let row = model.tree.selected_row()?;
    let node = model.tree.node(&row.path)?;
    match &node.kind {
        crate::app::tree::NodeKind::Object(object) => Some(object.clone()),
        _ => None,
    }
}

/// An object's dependencies, as palette entries in both directions.
fn dependency_entries(
    dependencies: &crate::postgres::metadata::Dependencies,
) -> Vec<crate::app::palette::PaletteEntry> {
    use crate::app::palette::{PaletteCommand, PaletteEntry};
    let entry = |object: &crate::postgres::metadata::ObjectSummary, group| PaletteEntry {
        label: object.qualified_sql(),
        detail: format!(
            "{} {}",
            object.kind.singular(),
            object.detail.clone().unwrap_or_default()
        ),
        group,
        command: PaletteCommand::Insert(object.qualified_sql()),
    };
    dependencies
        .used_by
        .iter()
        .map(|object| entry(object, "Used by"))
        .chain(
            dependencies
                .depends_on
                .iter()
                .map(|object| entry(object, "Depends on")),
        )
        .collect()
}

/// Handles input while a definition is on screen.
fn definition_action(model: &mut Model, action: Action) -> Vec<Effect> {
    let height = crate::ui::layout::definition_viewport(model.size).1;
    match action {
        Action::Move(Direction::Down) | Action::MovePage(Direction::Down) => {
            let step = if matches!(action, Action::MovePage(_)) {
                height.max(1)
            } else {
                1
            };
            if let Some(open) = model.definition.as_mut() {
                for _ in 0..step {
                    open.scroll_down(height);
                }
            }
        }
        Action::Move(Direction::Up) | Action::MovePage(Direction::Up) => {
            let step = if matches!(action, Action::MovePage(_)) {
                height.max(1)
            } else {
                1
            };
            if let Some(open) = model.definition.as_mut() {
                for _ in 0..step {
                    open.scroll_up();
                }
            }
        }
        Action::Dismiss | Action::ShowDefinition => {
            model.definition = None;
        }
        Action::Quit => {
            model.should_quit = true;
            return vec![Effect::Quit];
        }
        Action::Insert(_) | Action::Backspace | Action::Newline => {}
        other => {
            model.definition = None;
            return apply_action(model, other);
        }
    }
    Vec::new()
}

/// The statements that have run, as palette entries.
///
/// A separate list rather than a section of the main palette: someone looking
/// for a past statement is not looking for a command, and a hundred statements
/// would bury the commands.
fn history_entries(model: &Model) -> Vec<crate::app::palette::PaletteEntry> {
    use crate::app::palette::{PaletteCommand, PaletteEntry};
    model
        .history
        .iter()
        .map(|entry| PaletteEntry {
            label: entry.one_line(),
            detail: format!(
                "{}  {}  {}",
                entry.when(),
                entry.database,
                entry.outcome.label()
            ),
            group: "History",
            command: PaletteCommand::Insert(entry.sql.clone()),
        })
        .collect()
}

/// Moves by a screenful in whichever pane has focus.
///
/// The height comes from the terminal size the model already knows, through the
/// same layout functions the renderer uses, so a page here is the page the user
/// can see rather than a number chosen in advance.
fn page(model: &mut Model, direction: Direction) {
    let lines = crate::ui::layout::editor_page(model.size);
    match model.focus {
        Focus::Editor => match direction {
            Direction::Up => model.editor.move_page_up(lines),
            Direction::Down => model.editor.move_page_down(lines),
            Direction::Left | Direction::Right => {}
        },
        Focus::Results => {
            let rows = model.visible_result().map_or(0, |set| set.rows.len());
            let step = crate::ui::layout::results_page(model.size);
            match direction {
                Direction::Up => model.selected_row = model.selected_row.saturating_sub(step),
                Direction::Down => {
                    model.selected_row = (model.selected_row + step).min(rows.saturating_sub(1));
                }
                Direction::Left | Direction::Right => {}
            }
        }
        Focus::Objects => {
            let step = isize::try_from(crate::ui::layout::results_page(model.size)).unwrap_or(10);
            match direction {
                Direction::Up => model.tree.move_selection(-step),
                Direction::Down => model.tree.move_selection(step),
                Direction::Left | Direction::Right => {}
            }
        }
    }
}

/// Opens the inspector on the selected cell, or closes it.
///
/// It opens only when there is a cell to show. An inspector over a result with
/// no rows would be a window onto nothing, and the selection can outlive the
/// result it was made in, so the cell is looked up rather than assumed.
fn toggle_inspector(model: &mut Model) {
    if model.inspector.is_some() {
        model.inspector = None;
        return;
    }
    let (width, _) = crate::ui::layout::inspector_viewport(model.size);
    let row = model.selected_source_row();
    let exists = row.is_some_and(|row| {
        model.visible_result().is_some_and(|set| {
            crate::app::inspect::CellView::build(set, row, model.selected_column, width).is_some()
        })
    });
    if exists {
        model.inspector = Some(crate::app::inspect::Inspector::new());
    }
}

/// How many lines the selected value wraps to, and how many fit at once.
fn inspected_extent(model: &Model) -> (usize, usize) {
    let (width, height) = crate::ui::layout::inspector_viewport(model.size);
    let row = model.selected_source_row();
    let lines = row
        .and_then(|row| {
            model.visible_result().and_then(|set| {
                crate::app::inspect::CellView::build(set, row, model.selected_column, width)
            })
        })
        .map_or(0, |view| view.lines.len());
    (lines, height)
}

/// Handles input while the result filter is being typed.
///
/// The filter narrows what is shown; it never re-runs anything and never asks
/// the server for more. What it searches is what is already here, which is why
/// the count line says so.
fn result_filter_action(model: &mut Model, action: Action) -> Vec<Effect> {
    match action {
        Action::Insert(ch) => {
            model.result_filter.push(ch);
            model.selected_row = 0;
        }
        Action::Backspace => {
            model.result_filter.pop();
            model.selected_row = 0;
        }
        Action::Move(Direction::Up) => {
            model.selected_row = model.selected_row.saturating_sub(1);
        }
        Action::Move(Direction::Down) => {
            let rows = model.filtered_rows().len();
            model.selected_row = (model.selected_row + 1).min(rows.saturating_sub(1));
        }
        // Enter keeps the filter and returns to the rows; Esc clears it.
        Action::Activate => {
            model.result_filtering = false;
        }
        Action::Dismiss => {
            model.result_filtering = false;
            model.result_filter.clear();
            model.selected_row = 0;
        }
        Action::Quit => {
            model.should_quit = true;
            return vec![Effect::Quit];
        }
        // Same rule as everywhere else that takes typing: characters go to the
        // filter, and a key that already means something ends the typing, keeps
        // the filter, and does what it means.
        Action::Newline => {}
        other => {
            model.result_filtering = false;
            return apply_action(model, other);
        }
    }
    Vec::new()
}

/// Handles input while the inspector is open.
///
/// Left and right move to the neighbouring column and keep it open, which is how
/// a wide row is read one value at a time. Up and down scroll the value. Nothing
/// else reaches the pane underneath.
fn inspector_action(model: &mut Model, action: Action) -> Vec<Effect> {
    match action {
        Action::Move(Direction::Up) => {
            if let Some(inspector) = model.inspector.as_mut() {
                inspector.scroll_up();
            }
        }
        Action::Move(Direction::Down) => {
            let (total, height) = inspected_extent(model);
            if let Some(inspector) = model.inspector.as_mut() {
                inspector.scroll_down(total, height);
            }
        }
        Action::Move(direction @ (Direction::Left | Direction::Right)) => {
            let previous = model.focus;
            model.focus = Focus::Results;
            move_selection(model, direction);
            model.focus = previous;
            // A different value starts at its beginning, not where the last one
            // happened to be scrolled to.
            if let Some(inspector) = model.inspector.as_mut() {
                inspector.reset();
            }
        }
        Action::Dismiss | Action::ToggleInspector | Action::Activate => {
            model.inspector = None;
        }
        Action::Quit => {
            model.should_quit = true;
            return vec![Effect::Quit];
        }
        Action::ToggleHelp => {
            model.help_open = !model.help_open;
        }
        // Typing is swallowed: the editor is underneath, and a keystroke that
        // silently edits SQL the user cannot see is the worst outcome here.
        Action::Insert(_) | Action::Backspace | Action::Newline => {}
        // Anything else is a key that already means something. It closes the
        // inspector and does that thing, so Ctrl+R still runs and the palette
        // still opens rather than the interface feeling stuck behind a modal.
        other => {
            model.inspector = None;
            return apply_action(model, other);
        }
    }
    Vec::new()
}

/// Handles input while a run is waiting to be confirmed.
fn confirmation_action(model: &mut Model, action: Action) -> Vec<Effect> {
    let Some(pending) = model.pending_run.as_mut() else {
        return Vec::new();
    };
    match action {
        Action::Insert(ch) => {
            pending.typed.push(ch);
            Vec::new()
        }
        Action::Backspace => {
            pending.typed.pop();
            Vec::new()
        }
        Action::Activate | Action::RunBuffer => {
            if !pending.is_satisfied() {
                // Nothing happens until the word is typed. Saying so is the
                // prompt's job; silently refusing would be worse than a modal.
                return Vec::new();
            }
            let sql = pending.sql.clone();
            model.pending_run = None;
            start(model, sql)
        }
        Action::Dismiss | Action::Cancel => {
            model.pending_run = None;
            Vec::new()
        }
        Action::Quit => {
            model.should_quit = true;
            vec![Effect::Quit]
        }
        _ => Vec::new(),
    }
}

/// Starts an execution, refusing when there is nothing to run or nowhere to run
/// it, and holding it back when the target is production and it is not a read.
fn run(model: &mut Model, sql: String) -> Vec<Effect> {
    if !model.connection.is_usable() || model.phase.is_busy() || sql.trim().is_empty() {
        return Vec::new();
    }
    let parsed = statements::split(&sql);
    if parsed.is_empty() {
        return Vec::new();
    }

    // The classification is advisory and the interface says so. What it buys is
    // that nobody writes to production without a deliberate second action.
    let impact = crate::query::classify_all(&parsed);
    if model.environment().is_production() && impact.needs_confirmation() {
        let required = model
            .connection
            .info()
            .map_or_else(String::new, |info| info.database.clone());
        model.pending_run = Some(crate::app::model::PendingRun {
            sql,
            impact,
            typed: String::new(),
            required,
        });
        return Vec::new();
    }

    start(model, sql)
}

/// Begins an execution that has already been allowed.
fn start(model: &mut Model, sql: String) -> Vec<Effect> {
    let parsed = statements::split(&sql);
    if parsed.is_empty() {
        return Vec::new();
    }
    let job = model.allocate_job();
    model.running_sql = Some(sql.clone());
    model.phase = QueryPhase::Running {
        job,
        statements: parsed.len(),
    };
    model.running_for = Some(std::time::Duration::ZERO);
    model.error = None;
    vec![Effect::Execute { job, sql }]
}

fn move_selection(model: &mut Model, direction: Direction) {
    match model.focus {
        Focus::Objects => match direction {
            Direction::Up => model.tree.move_selection(-1),
            Direction::Down => model.tree.move_selection(1),
            Direction::Left => model.tree.collapse_selected(),
            Direction::Right => {}
        },
        Focus::Editor => match direction {
            Direction::Left => model.editor.move_left(),
            Direction::Right => model.editor.move_right(),
            Direction::Up => model.editor.move_up(),
            Direction::Down => model.editor.move_down(),
        },
        Focus::Results => {
            let rows = model.filtered_rows().len();
            let columns = model.visible_result().map_or(0, |set| set.columns.len());
            match direction {
                Direction::Up => model.selected_row = model.selected_row.saturating_sub(1),
                Direction::Down => {
                    model.selected_row = (model.selected_row + 1).min(rows.saturating_sub(1));
                }
                Direction::Left => {
                    model.selected_column = model.selected_column.saturating_sub(1);
                }
                Direction::Right => {
                    model.selected_column =
                        (model.selected_column + 1).min(columns.saturating_sub(1));
                }
            }
        }
    }
}

/// Resolves the second key of a chord.
///
/// Whatever it is, the chord ends here: an unrecognised key cancels rather than
/// leaving the interface waiting in a state the user cannot see out of.
fn resolve_prefix(model: &mut Model, action: Action) -> Vec<Effect> {
    model.prefix_pending = false;
    match action {
        Action::Insert(ch) => match crate::ui::keymap::chord_action(ch) {
            Some(chord) => apply_action(model, chord),
            None => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// Handles input while the palette is open.
fn palette_action(model: &mut Model, action: Action) -> Vec<Effect> {
    let Some(palette) = model.palette.as_mut() else {
        return Vec::new();
    };
    match action {
        Action::Insert(ch) => {
            palette.push(ch);
            Vec::new()
        }
        Action::Backspace => {
            palette.backspace();
            Vec::new()
        }
        Action::Move(Direction::Up) => {
            palette.move_selection(-1);
            Vec::new()
        }
        Action::Move(Direction::Down) => {
            palette.move_selection(1);
            Vec::new()
        }
        Action::Activate => {
            let chosen = palette.selected_entry();
            model.palette = None;
            match chosen.map(|entry| entry.command) {
                Some(crate::app::palette::PaletteCommand::Run(next)) => apply_action(model, next),
                Some(crate::app::palette::PaletteCommand::Insert(text)) => {
                    model.focus = Focus::Editor;
                    for ch in text.chars() {
                        model.editor.insert(ch);
                    }
                    Vec::new()
                }
                None => Vec::new(),
            }
        }
        Action::Dismiss => {
            model.palette = None;
            Vec::new()
        }
        Action::Quit => {
            model.palette = None;
            model.should_quit = true;
            vec![Effect::Quit]
        }
        _ => Vec::new(),
    }
}

/// Handles input while the object filter is being typed.
fn filter_action(model: &mut Model, action: Action) -> Vec<Effect> {
    match action {
        Action::Insert(ch) => {
            model.tree.filter.push(ch);
            model.tree.selected = 0;
        }
        Action::Backspace => {
            model.tree.filter.pop();
            model.tree.selected = 0;
        }
        Action::Move(Direction::Up) => {
            model.tree.move_selection(-1);
        }
        Action::Move(Direction::Down) => {
            model.tree.move_selection(1);
        }
        // Enter keeps the filter and returns to navigating; Esc clears it.
        Action::Activate => {
            model.tree.filtering = false;
        }
        Action::Dismiss => {
            model.tree.filtering = false;
            model.tree.filter.clear();
        }
        Action::Quit => {
            model.should_quit = true;
            return vec![Effect::Quit];
        }
        // Typing goes to the filter and nowhere else. A key that already means
        // something ends the typing, keeps the filter, and does that thing, so
        // Ctrl+R still runs rather than the interface feeling stuck in a box.
        Action::Newline => {}
        other => {
            model.tree.filtering = false;
            return apply_action(model, other);
        }
    }
    Vec::new()
}

/// Handles the actions that mean something different in the object tree.
///
/// Returns `None` for anything the tree does not claim, which then falls through
/// to the ordinary handling.
fn objects_action(model: &mut Model, action: &Action) -> Option<Vec<Effect>> {
    match action {
        // The arrows navigate: right opens, left closes.
        Action::Move(Direction::Right) => Some(expand_selected(model)),
        // Enter uses the thing under the cursor. For something with a name that
        // means putting the name where SQL is written; for a container it means
        // opening it, because a container has no name worth pasting.
        Action::Activate => {
            let row = model.tree.selected_row()?;
            let node = model.tree.node(&row.path)?;
            let insertable = match &node.kind {
                crate::app::tree::NodeKind::Object(object) => Some(object.qualified_sql()),
                crate::app::tree::NodeKind::Column(column) => {
                    Some(crate::postgres::metadata::quote_identifier(&column.name))
                }
                _ => None,
            };
            match insertable {
                Some(name) => {
                    model.focus = Focus::Editor;
                    for ch in name.chars() {
                        model.editor.insert(ch);
                    }
                    Some(Vec::new())
                }
                None => Some(expand_selected(model)),
            }
        }
        // `/` starts a filter, the way it does in every tree people already use.
        Action::Insert('/') => {
            model.tree.filtering = true;
            Some(Vec::new())
        }
        // Ordinary typing must not leak into the editor from here.
        Action::Insert(_) | Action::Backspace => Some(Vec::new()),
        _ => None,
    }
}

/// Opens the selected node, asking for a load only when one is needed.
fn expand_selected(model: &mut Model) -> Vec<Effect> {
    let Some(row) = model.tree.selected_row() else {
        return Vec::new();
    };
    if !row.expandable {
        return Vec::new();
    }
    if matches!(row.row_kind, crate::app::tree::RowKind::Schema { .. }) {
        if row.expanded {
            model.tree.collapse_selected();
        } else {
            // Counts arrived with the schema list, so this needs no round trip.
            model.tree.expand_schema(&row.path);
        }
        return Vec::new();
    }
    if row.expanded {
        model.tree.collapse_selected();
        return Vec::new();
    }
    match model.tree.expand_selected() {
        Some((request, path, query)) => vec![Effect::LoadMetadata {
            request,
            path,
            query,
        }],
        None => Vec::new(),
    }
}

/// The entries the palette offers: every command, then every loaded object.
fn palette_entries(model: &Model) -> Vec<crate::app::palette::PaletteEntry> {
    use crate::app::palette::{PaletteCommand, PaletteEntry};
    let keymap = crate::ui::keymap::Keymap::new();
    let mut entries: Vec<PaletteEntry> = keymap
        .bindings()
        .iter()
        .filter(|binding| {
            // Movement and typing are not commands anyone looks up in a palette.
            !matches!(
                binding.action,
                Action::Move(_) | Action::Insert(_) | Action::Backspace | Action::Activate
            )
        })
        .map(|binding| PaletteEntry {
            label: binding.description.to_owned(),
            detail: binding.key_label(),
            group: "Command",
            command: PaletteCommand::Run(binding.action.clone()),
        })
        .collect();
    entries.dedup_by(|a, b| a.label == b.label);

    // Chords are commands too. Someone who cannot remember the second key should
    // still be able to reach the thing by typing its name.
    for (key, action, description) in crate::ui::keymap::CHORDS {
        if entries
            .iter()
            .any(|entry| entry.command == PaletteCommand::Run(action.clone()))
        {
            continue;
        }
        entries.push(PaletteEntry {
            label: (*description).to_owned(),
            detail: format!("Ctrl+K {key}"),
            group: "Command",
            command: PaletteCommand::Run(action.clone()),
        });
    }

    // Objects already in the tree, so the palette costs no round trip.
    for row in model.tree.rows() {
        if let crate::app::tree::RowKind::Object { kind, .. } = row.row_kind
            && let Some(node) = model.tree.node(&row.path)
            && let Some(sql) = node.qualified_sql()
        {
            entries.push(PaletteEntry {
                label: row.label.clone(),
                detail: format!("{} {}", kind.singular(), sql),
                group: "Object",
                command: PaletteCommand::Insert(sql),
            });
        }
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::Environment;
    use crate::diagnostics::{Diagnostic, DiagnosticKind};
    use crate::postgres::{SessionInfo, TlsState};
    use crate::query::result::{Execution, JobId, ResultSet, StatementResult};
    use crate::query::value::Cell;
    use std::time::Duration;

    fn session() -> Box<SessionInfo> {
        Box::new(SessionInfo {
            target: "app@localhost:5432/orders".into(),
            database: "orders".into(),
            user: "app".into(),
            server_version: "18.6".into(),
            backend_pid: "42".into(),
            search_path: "public".into(),
            read_only: false,
            tls: TlsState::Disabled,
            environment: Environment::Local,
        })
    }

    fn connected() -> Model {
        let mut model = Model::new(100);
        update(&mut model, Message::Connected(session()));
        model
    }

    fn execution(job: JobId, status: ExecutionStatus, rows: &[&str]) -> Box<Execution> {
        let mut set = ResultSet::new(vec!["value".into()], 100);
        for row in rows {
            set.push(vec![Cell::Text((*row).to_owned())]);
        }
        Box::new(Execution {
            job,
            statements: vec![StatementResult {
                result_set: Some(set),
                rows_affected: Some(rows.len() as u64),
                elapsed: Duration::from_millis(5),
                notices: Vec::new(),
            }],
            status,
            elapsed: Duration::from_millis(6),
            error: None,

            transaction: crate::query::result::TransactionState::Autocommit,
        })
    }

    #[test]
    fn running_a_buffer_starts_one_job_and_asks_for_execution() {
        let mut model = connected();
        model.editor.set_text("SELECT 1;");
        let effects = update(&mut model, Message::Action(Action::RunBuffer));

        assert_eq!(effects.len(), 1);
        let Effect::Execute { job, sql } = &effects[0] else {
            panic!("expected an execute effect, got {effects:?}");
        };
        assert_eq!(sql, "SELECT 1;");
        assert_eq!(
            model.phase,
            QueryPhase::Running {
                job: *job,
                statements: 1
            }
        );
        assert!(model.phase.is_busy());
    }

    #[test]
    fn nothing_runs_without_a_usable_connection() {
        let mut model = Model::new(100);
        model.editor.set_text("SELECT 1;");
        assert!(update(&mut model, Message::Action(Action::RunBuffer)).is_empty());
        assert_eq!(model.phase, QueryPhase::Idle);

        // A lost connection is equally unusable.
        let mut model = connected();
        update(&mut model, Message::ConnectionLost);
        model.editor.set_text("SELECT 1;");
        assert!(update(&mut model, Message::Action(Action::RunBuffer)).is_empty());
    }

    #[test]
    fn an_empty_or_comment_only_buffer_sends_nothing() {
        let mut model = connected();
        for text in ["", "   \n  ", "-- just a comment\n", ";"] {
            model.editor.set_text(text);
            assert!(
                update(&mut model, Message::Action(Action::RunBuffer)).is_empty(),
                "{text:?} should not be sent"
            );
        }
    }

    #[test]
    fn a_second_run_is_refused_while_one_is_in_flight() {
        let mut model = connected();
        model.editor.set_text("SELECT 1;");
        let first = update(&mut model, Message::Action(Action::RunBuffer));
        assert_eq!(first.len(), 1);
        let second = update(&mut model, Message::Action(Action::RunBuffer));
        assert!(second.is_empty(), "one statement at a time");
    }

    #[test]
    fn a_stale_result_cannot_overwrite_a_newer_query() {
        let mut model = connected();
        model.editor.set_text("SELECT 1;");
        update(&mut model, Message::Action(Action::RunBuffer));
        let first_job = model.phase.job().expect("running");

        // The first query finishes and a second is started.
        update(
            &mut model,
            Message::ExecutionFinished(execution(
                first_job,
                ExecutionStatus::Succeeded,
                &["first"],
            )),
        );
        model.editor.set_text("SELECT 2;");
        update(&mut model, Message::Action(Action::RunBuffer));
        let second_job = model.phase.job().expect("running");
        assert_ne!(first_job, second_job);

        // A late result from the first query arrives.
        update(
            &mut model,
            Message::ExecutionFinished(execution(
                first_job,
                ExecutionStatus::Succeeded,
                &["stale"],
            )),
        );

        assert_eq!(
            model.phase,
            QueryPhase::Running {
                job: second_job,
                statements: 1
            },
            "the in-flight job must still be running"
        );
        let shown = model.visible_result().expect("result").rows[0][0].clone();
        assert_eq!(
            shown,
            Cell::Text("first".into()),
            "stale rows must not be displayed"
        );
    }

    #[test]
    fn cancelling_requests_but_does_not_claim_cancellation() {
        let mut model = connected();
        model.editor.set_text("SELECT pg_sleep(30);");
        update(&mut model, Message::Action(Action::RunBuffer));
        let job = model.phase.job().expect("running");

        let effects = update(&mut model, Message::Action(Action::Cancel));
        assert_eq!(effects, vec![Effect::Cancel { job }]);
        assert_eq!(model.phase, QueryPhase::CancellationRequested { job });
        assert_eq!(model.phase.label(), "Cancellation requested");

        // Only the server's answer produces the cancelled wording.
        update(
            &mut model,
            Message::ExecutionFinished(Box::new(Execution {
                job,
                statements: Vec::new(),
                status: ExecutionStatus::Cancelled,
                elapsed: Duration::from_millis(20),
                error: None,

                transaction: crate::query::result::TransactionState::Autocommit,
            })),
        );
        assert_eq!(model.phase, QueryPhase::Idle);
        assert_eq!(
            model
                .last_execution
                .as_ref()
                .expect("execution")
                .status
                .label(),
            "Query cancelled by server"
        );
    }

    #[test]
    fn cancelling_twice_sends_one_request() {
        let mut model = connected();
        model.editor.set_text("SELECT pg_sleep(30);");
        update(&mut model, Message::Action(Action::RunBuffer));
        assert_eq!(update(&mut model, Message::Action(Action::Cancel)).len(), 1);
        assert!(update(&mut model, Message::Action(Action::Cancel)).is_empty());
    }

    #[test]
    fn cancelling_when_idle_does_nothing() {
        let mut model = connected();
        assert!(update(&mut model, Message::Action(Action::Cancel)).is_empty());
        assert_eq!(model.phase, QueryPhase::Idle);
    }

    #[test]
    fn a_lost_connection_during_execution_is_recorded_on_the_connection_too() {
        let mut model = connected();
        model.editor.set_text("SELECT 1;");
        update(&mut model, Message::Action(Action::RunBuffer));
        let job = model.phase.job().expect("running");
        update(
            &mut model,
            Message::ExecutionFinished(Box::new(Execution {
                job,
                statements: Vec::new(),
                status: ExecutionStatus::ConnectionLost,
                elapsed: Duration::from_millis(1),
                error: None,

                transaction: crate::query::result::TransactionState::Autocommit,
            })),
        );
        assert!(matches!(model.connection, ConnectionState::Lost { .. }));
        assert!(!model.connection.is_usable());
    }

    #[test]
    fn running_the_statement_at_the_cursor_sends_only_that_statement() {
        let mut model = connected();
        model.editor.set_text("SELECT 1;\nSELECT 2;\nSELECT 3;");
        // Cursor sits at the end, inside the third statement.
        let effects = update(&mut model, Message::Action(Action::RunStatement));
        let Effect::Execute { sql, .. } = &effects[0] else {
            panic!("expected execute");
        };
        assert_eq!(sql, "SELECT 3");
    }

    #[test]
    fn a_failed_execution_shows_its_diagnostic_collapsed() {
        let mut model = connected();
        model.editor.set_text("SELECT * FROM nope;");
        update(&mut model, Message::Action(Action::RunBuffer));
        let job = model.phase.job().expect("running");
        let diagnostic = Diagnostic::new(
            DiagnosticKind::Query,
            "relation does not exist",
            "running statement 1",
        )
        .technical("SQLSTATE", "42P01");
        update(
            &mut model,
            Message::ExecutionFinished(Box::new(Execution {
                job,
                statements: Vec::new(),
                status: ExecutionStatus::Failed,
                elapsed: Duration::from_millis(3),
                error: Some(diagnostic),

                transaction: crate::query::result::TransactionState::Autocommit,
            })),
        );

        assert!(model.error.is_some());
        assert!(!model.error_expanded, "detail starts collapsed");
        update(&mut model, Message::Action(Action::ToggleErrorDetail));
        assert!(model.error_expanded);
        update(&mut model, Message::Action(Action::Dismiss));
        assert!(model.error.is_none());
    }

    #[test]
    fn help_opens_and_dismiss_closes_help_before_errors() {
        let mut model = connected();
        model.error = Some(Diagnostic::new(DiagnosticKind::Query, "boom", "running"));
        update(&mut model, Message::Action(Action::ToggleHelp));
        assert!(model.help_open);

        update(&mut model, Message::Action(Action::Dismiss));
        assert!(!model.help_open);
        assert!(model.error.is_some(), "the error is still there to read");

        update(&mut model, Message::Action(Action::Dismiss));
        assert!(model.error.is_none());
    }

    #[test]
    fn result_selection_stays_inside_the_grid() {
        let mut model = connected();
        model.focus = Focus::Results;
        model.last_execution = Some(*execution(
            JobId(1),
            ExecutionStatus::Succeeded,
            &["a", "b"],
        ));

        for _ in 0..10 {
            update(&mut model, Message::Action(Action::Move(Direction::Down)));
        }
        assert_eq!(model.selected_row, 1, "cannot move past the last row");
        for _ in 0..10 {
            update(&mut model, Message::Action(Action::Move(Direction::Up)));
        }
        assert_eq!(model.selected_row, 0);
        for _ in 0..10 {
            update(&mut model, Message::Action(Action::Move(Direction::Right)));
        }
        assert_eq!(model.selected_column, 0, "single column grid");
    }

    #[test]
    fn typing_only_edits_when_the_editor_has_focus() {
        let mut model = connected();
        model.focus = Focus::Results;
        update(&mut model, Message::Action(Action::Insert('x')));
        assert_eq!(
            model.editor.text(),
            "",
            "results pane must not swallow text into the editor"
        );

        model.focus = Focus::Editor;
        update(&mut model, Message::Action(Action::Insert('x')));
        assert_eq!(model.editor.text(), "x");
    }

    #[test]
    fn a_tick_advances_the_frame_and_records_elapsed_time_without_a_clock() {
        let mut model = connected();
        assert!(!model.is_animating(), "an idle client animates nothing");

        model.editor.set_text("SELECT pg_sleep(5);");
        update(&mut model, Message::Action(Action::RunBuffer));
        assert!(
            model.is_animating(),
            "a running statement is worth animating"
        );

        let before = model.frame;
        update(
            &mut model,
            Message::Tick {
                running_for: Some(Duration::from_millis(1500)),
            },
        );
        assert_eq!(model.frame, before + 1);
        assert_eq!(model.running_for, Some(Duration::from_millis(1500)));
    }

    #[test]
    fn the_live_timer_is_cleared_when_the_statement_ends() {
        let mut model = connected();
        model.editor.set_text("SELECT 1;");
        update(&mut model, Message::Action(Action::RunBuffer));
        let job = model.phase.job().expect("running");
        update(
            &mut model,
            Message::Tick {
                running_for: Some(Duration::from_secs(2)),
            },
        );
        assert!(model.running_for.is_some());

        update(
            &mut model,
            Message::ExecutionFinished(execution(job, ExecutionStatus::Succeeded, &["x"])),
        );
        assert!(
            model.running_for.is_none(),
            "a stale timer would keep counting"
        );
        assert!(!model.is_animating());
    }

    #[test]
    fn ticks_change_nothing_a_user_could_mistake_for_progress() {
        // A frame counter must never move the query, the selection, or the state.
        let mut model = connected();
        model.editor.set_text("SELECT 1;");
        let before = model.clone();
        for _ in 0..50 {
            update(&mut model, Message::Tick { running_for: None });
        }
        assert_eq!(model.phase, before.phase);
        assert_eq!(model.editor.text(), before.editor.text());
        assert_eq!(model.selected_row, before.selected_row);
        assert!(model.last_execution.is_none());
    }

    // ------------------------------------------------- navigation and modes

    fn schema_summary(name: &str, tables: i64) -> crate::postgres::SchemaSummary {
        let mut counts = std::collections::BTreeMap::new();
        counts.insert(crate::postgres::ObjectKind::Table, tables);
        crate::postgres::SchemaSummary {
            name: name.to_owned(),
            usable: true,
            counts,
        }
    }

    #[test]
    fn editing_keys_reach_the_editor_and_nothing_else() {
        let mut model = connected();
        model.editor.set_text("SELECT customer_id\nFROM orders");

        update(&mut model, Message::Action(Action::MoveBufferStart));
        assert_eq!(model.editor.position(), (1, 1));
        update(&mut model, Message::Action(Action::MoveLineEnd));
        assert_eq!(model.editor.position(), (1, 19));
        update(
            &mut model,
            Message::Action(Action::MoveWord(Direction::Left)),
        );
        assert_eq!(model.editor.position(), (1, 8));
        update(&mut model, Message::Action(Action::DeleteWordLeft));
        assert_eq!(model.editor.text(), "customer_id\nFROM orders");
        update(&mut model, Message::Action(Action::Undo));
        assert_eq!(model.editor.text(), "SELECT customer_id\nFROM orders");
        update(&mut model, Message::Action(Action::Redo));
        assert_eq!(model.editor.text(), "customer_id\nFROM orders");

        update(&mut model, Message::Action(Action::MoveBufferStart));
        update(&mut model, Message::Action(Action::DeleteForward));
        assert_eq!(model.editor.text(), "ustomer_id\nFROM orders");

        // The same keys in another pane leave the buffer alone.
        let before = model.editor.text().to_owned();
        model.focus = Focus::Results;
        for action in [
            Action::DeleteForward,
            Action::DeleteWordLeft,
            Action::MoveWord(Direction::Left),
            Action::MoveLineStart,
            Action::MoveLineEnd,
            Action::Undo,
            Action::Redo,
        ] {
            update(&mut model, Message::Action(action));
        }
        assert_eq!(model.editor.text(), before);
    }

    #[test]
    fn vertical_movement_works_in_the_editor_where_it_used_to_do_nothing() {
        let mut model = connected();
        model
            .editor
            .set_text("SELECT customer_id\nFROM orders\nWHERE total > 100");
        update(&mut model, Message::Action(Action::MoveBufferStart));
        update(&mut model, Message::Action(Action::Move(Direction::Down)));
        assert_eq!(model.editor.position(), (2, 1));
        update(&mut model, Message::Action(Action::Move(Direction::Down)));
        update(&mut model, Message::Action(Action::Move(Direction::Up)));
        assert_eq!(model.editor.position(), (2, 1));
    }

    #[test]
    fn a_page_key_moves_by_what_is_on_screen_in_whichever_pane_has_focus() {
        let mut model = connected();
        model.size = (120, 40);
        let text = (1..=200)
            .map(|n| format!("-- line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        model.editor.set_text(text);
        update(&mut model, Message::Action(Action::MoveBufferStart));

        let page = crate::ui::layout::editor_page(model.size);
        assert!(page > 1, "a full-size terminal shows more than one line");
        update(
            &mut model,
            Message::Action(Action::MovePage(Direction::Down)),
        );
        assert_eq!(model.editor.position().0, page + 1);
        update(&mut model, Message::Action(Action::MovePage(Direction::Up)));
        assert_eq!(model.editor.position().0, 1);

        // In the results it moves the selection, and stops at the last row.
        model.editor.set_text("SELECT 1;");
        let effects = update(&mut model, Message::Action(Action::RunBuffer));
        let Effect::Execute { job, .. } = effects[0] else {
            panic!("expected an execute effect");
        };
        let rows: Vec<String> = (0..500).map(|n| n.to_string()).collect();
        let borrowed: Vec<&str> = rows.iter().map(String::as_str).collect();
        update(
            &mut model,
            Message::ExecutionFinished(execution(job, ExecutionStatus::Succeeded, &borrowed)),
        );
        model.focus = Focus::Results;
        update(
            &mut model,
            Message::Action(Action::MovePage(Direction::Down)),
        );
        assert_eq!(
            model.selected_row,
            crate::ui::layout::results_page(model.size)
        );
        update(&mut model, Message::Action(Action::MoveBufferEnd));
        assert_eq!(
            model.selected_row, 99,
            "the last row is the last retained row, not the last row the server sent"
        );
        update(&mut model, Message::Action(Action::MoveBufferStart));
        assert_eq!(model.selected_row, 0);
    }

    fn history_entry(sql: &str) -> crate::history::Entry {
        crate::history::Entry::now(
            "app@localhost:5432/orders",
            "orders",
            "local",
            sql,
            crate::history::Outcome::Succeeded,
            Duration::from_millis(3),
        )
    }

    fn with_tree() -> Model {
        let mut model = connected();
        update(
            &mut model,
            Message::SchemasLoaded(Box::new(Ok(vec![
                schema_summary("public", 2),
                schema_summary("reporting", 1),
            ]))),
        );
        model
    }

    #[test]
    fn the_tree_loads_the_first_time_the_sidebar_is_shown_and_not_again() {
        let mut model = connected();
        model.sidebar_visible = false;

        let effects = update(&mut model, Message::Action(Action::ToggleSidebar));
        assert_eq!(effects, vec![Effect::LoadSchemas], "showing it loads it");
        assert!(model.sidebar_visible);

        update(
            &mut model,
            Message::SchemasLoaded(Box::new(Ok(vec![schema_summary("public", 1)]))),
        );
        update(&mut model, Message::Action(Action::ToggleSidebar));
        let effects = update(&mut model, Message::Action(Action::ToggleSidebar));
        assert!(
            effects.is_empty(),
            "an already-loaded tree is not fetched again"
        );
    }

    #[test]
    fn hiding_the_sidebar_moves_focus_off_it() {
        let mut model = with_tree();
        model.focus = Focus::Objects;
        update(&mut model, Message::Action(Action::ToggleSidebar));
        assert!(!model.sidebar_visible);
        assert_eq!(
            model.focus,
            Focus::Editor,
            "focus cannot stay on a hidden pane"
        );
    }

    #[test]
    fn opening_a_schema_costs_no_round_trip_but_opening_a_group_does() {
        let mut model = with_tree();
        model.focus = Focus::Objects;

        let effects = update(&mut model, Message::Action(Action::Move(Direction::Right)));
        assert!(
            effects.is_empty(),
            "counts already arrived with the schema list"
        );
        assert_eq!(model.tree.rows()[1].label, "tables (2)");

        update(&mut model, Message::Action(Action::Move(Direction::Down)));
        let effects = update(&mut model, Message::Action(Action::Move(Direction::Right)));
        assert!(
            matches!(effects.as_slice(), [Effect::LoadMetadata { .. }]),
            "opening a group asks the server: {effects:?}"
        );
    }

    #[test]
    fn choosing_an_object_puts_its_quoted_name_in_the_editor() {
        let mut model = with_tree();
        model.focus = Focus::Objects;
        model.editor.set_text("SELECT * FROM ");
        update(&mut model, Message::Action(Action::Move(Direction::Right))); // open public
        update(&mut model, Message::Action(Action::Move(Direction::Down)));
        let effects = update(&mut model, Message::Action(Action::Move(Direction::Right))); // tables
        let Effect::LoadMetadata { request, path, .. } = &effects[0] else {
            panic!("expected a load");
        };
        update(
            &mut model,
            Message::MetadataLoaded {
                request: *request,
                path: path.clone(),
                payload: Box::new(Ok(crate::app::tree::MetadataPayload::Objects(vec![
                    crate::postgres::ObjectSummary {
                        kind: crate::postgres::ObjectKind::Table,
                        schema: "public".into(),
                        name: "we\"ird".into(),
                        readable: true,
                        detail: None,
                    },
                ]))),
            },
        );

        update(&mut model, Message::Action(Action::Move(Direction::Down)));
        // Enter uses the object: its quoted name goes where SQL is written.
        update(&mut model, Message::Action(Action::Activate));

        assert_eq!(
            model.editor.text(),
            "SELECT * FROM \"public\".\"we\"\"ird\"",
            "the name is quoted so a hostile one cannot change the statement"
        );
        assert_eq!(model.focus, Focus::Editor, "focus follows the insertion");
    }

    /// A tree with one table selected, ready to be asked about.
    fn with_selected_table() -> Model {
        let mut model = with_tree();
        model.focus = Focus::Objects;
        update(&mut model, Message::Action(Action::Move(Direction::Right)));
        update(&mut model, Message::Action(Action::Move(Direction::Down)));
        let effects = update(&mut model, Message::Action(Action::Move(Direction::Right)));
        let Effect::LoadMetadata { request, path, .. } = &effects[0] else {
            panic!("expected a load");
        };
        update(
            &mut model,
            Message::MetadataLoaded {
                request: *request,
                path: path.clone(),
                payload: Box::new(Ok(crate::app::tree::MetadataPayload::Objects(vec![
                    crate::postgres::ObjectSummary {
                        kind: crate::postgres::ObjectKind::Table,
                        schema: "public".into(),
                        name: "orders".into(),
                        readable: true,
                        detail: None,
                    },
                ]))),
            },
        );
        update(&mut model, Message::Action(Action::Move(Direction::Down)));
        model
    }

    fn definition_of(name: &str) -> crate::postgres::metadata::Definition {
        crate::postgres::metadata::Definition {
            kind: crate::postgres::ObjectKind::Table,
            schema: "public".into(),
            name: name.to_owned(),
            source: crate::postgres::metadata::DefinitionSource::Assembled,
            text: (1..=40)
                .map(|n| format!("    column_{n} text"))
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }

    #[test]
    fn asking_what_depends_on_an_object_lists_both_directions() {
        let mut model = with_selected_table();
        let effects = update(&mut model, Message::Action(Action::ShowDependencies));
        let Effect::LoadDependencies { request, object } = &effects[0] else {
            panic!("expected a dependency load, got {effects:?}");
        };
        assert_eq!(object.name, "orders");
        let request = *request;

        let related = |name: &str, reason: &str| crate::postgres::ObjectSummary {
            kind: crate::postgres::ObjectKind::View,
            schema: "public".into(),
            name: name.to_owned(),
            readable: true,
            detail: Some(reason.to_owned()),
        };
        update(
            &mut model,
            Message::DependenciesLoaded {
                request,
                result: Box::new(Ok(crate::postgres::metadata::Dependencies {
                    used_by: vec![related("recent_orders", "reads it")],
                    depends_on: vec![related("customers", "is referred to by it")],
                })),
            },
        );

        let palette = model.palette.as_ref().expect("open");
        assert_eq!(palette.purpose, crate::app::palette::Purpose::Dependencies);
        assert_eq!(palette.entries.len(), 2);
        assert_eq!(palette.entries[0].group, "Used by");
        assert_eq!(palette.entries[1].group, "Depends on");
        assert!(palette.entries[0].label.contains("recent_orders"));
        assert!(
            palette
                .purpose
                .standing_note()
                .is_some_and(|note| note.contains("function body")),
            "what the answer cannot see is said where it is read"
        );

        // Choosing one puts its quoted, qualified name where SQL is written.
        model.editor.set_text("SELECT * FROM ");
        update(&mut model, Message::Action(Action::Activate));
        assert_eq!(
            model.editor.text(),
            "SELECT * FROM \"public\".\"recent_orders\""
        );
    }

    #[test]
    fn a_dependency_answer_for_a_question_nobody_is_waiting_on_is_discarded() {
        let mut model = with_selected_table();
        let effects = update(&mut model, Message::Action(Action::ShowDependencies));
        let Effect::LoadDependencies { request, .. } = &effects[0] else {
            panic!("expected a dependency load");
        };
        let stale = *request;
        model.pending_dependencies = None;

        update(
            &mut model,
            Message::DependenciesLoaded {
                request: stale,
                result: Box::new(Ok(crate::postgres::metadata::Dependencies::default())),
            },
        );
        assert!(
            model.palette.is_none(),
            "an answer to a question nobody is waiting on opens nothing"
        );
    }

    #[test]
    fn a_dependency_lookup_that_fails_says_so_where_errors_are_shown() {
        let mut model = with_selected_table();
        let effects = update(&mut model, Message::Action(Action::ShowDependencies));
        let Effect::LoadDependencies { request, .. } = &effects[0] else {
            panic!("expected a dependency load");
        };
        update(
            &mut model,
            Message::DependenciesLoaded {
                request: *request,
                result: Box::new(Err(Diagnostic::new(
                    DiagnosticKind::Query,
                    "permission denied for pg_depend",
                    "reading dependencies",
                ))),
            },
        );
        assert!(model.palette.is_none());
        assert!(model.error.is_some(), "the failure is not silent");
    }

    #[test]
    fn where_the_tree_reads_from_is_recorded_and_said_in_words() {
        let mut model = connected();
        assert_eq!(model.metadata_link, crate::app::model::MetadataLink::Shared);

        update(
            &mut model,
            Message::MetadataConnection(crate::app::model::MetadataLink::Opening),
        );
        assert_eq!(model.metadata_link.label(), "opening its own connection");

        update(
            &mut model,
            Message::MetadataConnection(crate::app::model::MetadataLink::Dedicated),
        );
        assert_eq!(model.metadata_link.label(), "own connection");

        update(
            &mut model,
            Message::MetadataConnection(crate::app::model::MetadataLink::Unavailable(
                "too many connections for role".to_owned(),
            )),
        );
        assert!(
            model.metadata_link.label().contains("shared"),
            "falling back is stated rather than hidden: {}",
            model.metadata_link.label()
        );
    }

    #[test]
    fn asking_what_an_object_is_loads_its_definition_and_shows_where_it_came_from() {
        let mut model = with_selected_table();
        model.size = (120, 40);

        let effects = update(&mut model, Message::Action(Action::ShowDefinition));
        let Effect::LoadDefinition { request, object } = &effects[0] else {
            panic!("expected a definition load, got {effects:?}");
        };
        assert_eq!(object.name, "orders");
        let request = *request;
        assert!(
            model
                .definition
                .as_ref()
                .is_some_and(|d| d.pending.is_some()),
            "the panel opens while it waits, rather than after"
        );

        update(
            &mut model,
            Message::DefinitionLoaded {
                request,
                result: Box::new(Ok(definition_of("orders"))),
            },
        );
        let open = model.definition.as_ref().expect("open");
        assert!(open.pending.is_none());
        assert!(open.heading.contains("orders"), "{}", open.heading);
        assert_eq!(open.lines().len(), 40);

        // Scrolling stops with the last line on screen, both ways.
        let height = crate::ui::layout::definition_viewport(model.size).1;
        for _ in 0..200 {
            update(&mut model, Message::Action(Action::Move(Direction::Down)));
        }
        assert_eq!(
            model.definition.as_ref().expect("open").scroll,
            40usize.saturating_sub(height)
        );
        for _ in 0..200 {
            update(&mut model, Message::Action(Action::Move(Direction::Up)));
        }
        assert_eq!(model.definition.as_ref().expect("open").scroll, 0);

        update(&mut model, Message::Action(Action::Dismiss));
        assert!(model.definition.is_none(), "Esc closes it");
    }

    #[test]
    fn a_definition_that_arrives_late_for_a_closed_panel_is_discarded() {
        let mut model = with_selected_table();
        let effects = update(&mut model, Message::Action(Action::ShowDefinition));
        let Effect::LoadDefinition { request, .. } = &effects[0] else {
            panic!("expected a definition load");
        };
        let stale = *request;

        // Closed, then asked again: the first answer belongs to nothing.
        update(&mut model, Message::Action(Action::Dismiss));
        let effects = update(&mut model, Message::Action(Action::ShowDefinition));
        let Effect::LoadDefinition { request, .. } = &effects[0] else {
            panic!("expected a definition load");
        };
        assert_ne!(*request, stale);

        update(
            &mut model,
            Message::DefinitionLoaded {
                request: stale,
                result: Box::new(Ok(definition_of("stale"))),
            },
        );
        let open = model.definition.as_ref().expect("open");
        assert!(
            open.definition.is_none(),
            "a stale answer must not fill a panel that asked something else"
        );
        assert!(open.pending.is_some(), "it is still waiting for its own");
    }

    #[test]
    fn a_definition_that_cannot_be_read_says_so_rather_than_showing_nothing() {
        let mut model = with_selected_table();
        let effects = update(&mut model, Message::Action(Action::ShowDefinition));
        let Effect::LoadDefinition { request, .. } = &effects[0] else {
            panic!("expected a definition load");
        };
        update(
            &mut model,
            Message::DefinitionLoaded {
                request: *request,
                result: Box::new(Err(Diagnostic::new(
                    DiagnosticKind::Query,
                    "permission denied for table orders",
                    "reading an object definition",
                ))),
            },
        );
        let open = model.definition.as_ref().expect("open");
        assert!(
            open.error
                .as_deref()
                .is_some_and(|e| e.contains("permission")),
            "the reason belongs where the answer would have been"
        );
    }

    #[test]
    fn asking_for_a_definition_of_something_that_has_none_does_nothing() {
        // A schema and a group are not objects. Opening an empty panel over them
        // would be worse than not opening one.
        let mut model = with_tree();
        model.focus = Focus::Objects;
        assert!(update(&mut model, Message::Action(Action::ShowDefinition)).is_empty());
        assert!(model.definition.is_none());
    }

    #[test]
    fn typing_in_the_object_tree_never_leaks_into_the_editor() {
        let mut model = with_tree();
        model.focus = Focus::Objects;
        model.editor.set_text("SELECT 1");

        for ch in "drop".chars() {
            update(&mut model, Message::Action(Action::Insert(ch)));
        }
        update(&mut model, Message::Action(Action::Backspace));
        assert_eq!(
            model.editor.text(),
            "SELECT 1",
            "the editor was not touched"
        );
    }

    #[test]
    fn slash_starts_a_filter_and_escape_clears_it() {
        let mut model = with_tree();
        model.focus = Focus::Objects;

        update(&mut model, Message::Action(Action::Insert('/')));
        assert!(model.tree.filtering);

        for ch in "rep".chars() {
            update(&mut model, Message::Action(Action::Insert(ch)));
        }
        assert_eq!(model.tree.filter, "rep");
        let labels: Vec<String> = model.tree.rows().iter().map(|r| r.label.clone()).collect();
        assert_eq!(labels, vec!["reporting".to_owned()]);

        update(&mut model, Message::Action(Action::Activate));
        assert!(
            !model.tree.filtering,
            "Enter keeps the filter and stops typing"
        );
        assert_eq!(model.tree.filter, "rep");

        update(&mut model, Message::Action(Action::Dismiss));
        assert!(model.tree.filter.is_empty(), "Esc clears it");
    }

    #[test]
    fn the_palette_intercepts_typing_and_leaves_the_editor_alone() {
        let mut model = with_tree();
        model.editor.set_text("SELECT 1");

        update(&mut model, Message::Action(Action::OpenPalette));
        assert!(model.palette.is_some());

        for ch in "quit".chars() {
            update(&mut model, Message::Action(Action::Insert(ch)));
        }
        assert_eq!(
            model.editor.text(),
            "SELECT 1",
            "the editor was not typed into"
        );
        assert_eq!(model.palette.as_ref().expect("open").query, "quit");

        update(&mut model, Message::Action(Action::Dismiss));
        assert!(model.palette.is_none());
        assert_eq!(model.editor.text(), "SELECT 1");
    }

    #[test]
    fn the_palette_runs_the_command_it_shows() {
        let mut model = with_tree();
        update(&mut model, Message::Action(Action::OpenPalette));
        for ch in "help".chars() {
            update(&mut model, Message::Action(Action::Insert(ch)));
        }
        update(&mut model, Message::Action(Action::Activate));

        assert!(model.palette.is_none(), "choosing closes it");
        assert!(model.help_open, "the chosen command actually ran");
    }

    /// A model holding a result of several rows, focused on it.
    fn with_rows(rows: &[&str]) -> Model {
        let mut model = connected();
        model.size = (120, 40);
        model.editor.set_text("SELECT 1;");
        let effects = update(&mut model, Message::Action(Action::RunBuffer));
        let Effect::Execute { job, .. } = effects[0] else {
            panic!("expected an execute effect");
        };
        update(
            &mut model,
            Message::ExecutionFinished(execution(job, ExecutionStatus::Succeeded, rows)),
        );
        model.focus = Focus::Results;
        model
    }

    #[test]
    fn the_filter_narrows_the_rows_and_the_selection_follows_the_real_row() {
        let mut model = with_rows(&["alpha", "beta", "gamma", "delta"]);
        update(&mut model, Message::Action(Action::StartFilter));
        assert!(model.result_filtering, "the filter key belongs to the pane");
        assert_eq!(model.focus, Focus::Results, "and does not move focus");

        for ch in "ta".chars() {
            update(&mut model, Message::Action(Action::Insert(ch)));
        }
        assert_eq!(
            model.filtered_rows(),
            vec![1, 3],
            "beta and delta, by their own positions in the result"
        );
        assert_eq!(model.selected_source_row(), Some(1));

        update(&mut model, Message::Action(Action::Move(Direction::Down)));
        assert_eq!(model.selected_row, 1, "the second match");
        assert_eq!(
            model.selected_source_row(),
            Some(3),
            "which is the fourth row of the result"
        );
        update(&mut model, Message::Action(Action::Move(Direction::Down)));
        assert_eq!(model.selected_row, 1, "and there is no third match");

        // Enter keeps the filter and returns to the rows; Esc clears it.
        update(&mut model, Message::Action(Action::Activate));
        assert!(!model.result_filtering);
        assert_eq!(model.result_filter, "ta");
        update(&mut model, Message::Action(Action::StartFilter));
        update(&mut model, Message::Action(Action::Dismiss));
        assert!(model.result_filter.is_empty());
        assert_eq!(model.filtered_rows().len(), 4);
    }

    #[test]
    fn the_filter_is_typed_into_and_never_into_the_editor() {
        let mut model = with_rows(&["alpha", "beta"]);
        model.editor.set_text("SELECT 1;");
        update(&mut model, Message::Action(Action::StartFilter));
        for ch in "beta".chars() {
            update(&mut model, Message::Action(Action::Insert(ch)));
        }
        assert_eq!(model.result_filter, "beta");
        assert_eq!(model.editor.text(), "SELECT 1;");
        update(&mut model, Message::Action(Action::Backspace));
        assert_eq!(model.result_filter, "bet");
    }

    #[test]
    fn the_filter_matches_any_column_and_ignores_case() {
        let mut model = with_rows(&["Alpha", "beta"]);
        model.result_filter = "ALP".into();
        assert_eq!(model.filtered_rows(), vec![0]);
        model.result_filter = "nothing here".into();
        assert!(model.filtered_rows().is_empty());
    }

    #[test]
    fn a_filter_belongs_to_the_result_it_was_typed_against() {
        let mut model = with_rows(&["alpha", "beta"]);
        model.result_filter = "alpha".into();
        model.result_filtering = true;

        model.editor.set_text("SELECT 2;");
        let effects = update(&mut model, Message::Action(Action::RunBuffer));
        let Effect::Execute { job, .. } = effects[0] else {
            panic!("expected an execute effect");
        };
        update(
            &mut model,
            Message::ExecutionFinished(execution(job, ExecutionStatus::Succeeded, &["gamma"])),
        );
        assert!(
            model.result_filter.is_empty(),
            "carrying it over would hide rows nobody filtered"
        );
        assert!(!model.result_filtering);
        assert_eq!(model.filtered_rows(), vec![0]);
    }

    #[test]
    fn the_count_says_what_was_searched_and_what_was_never_received() {
        // The filter can only ever search what is here. Saying "3 rows" when a
        // result was truncated is three numbers collapsed into the wrong one.
        let mut model = with_rows(&["alpha", "beta", "gamma"]);
        assert_eq!(model.result_window_label(), "3 rows");

        model.result_filter = "a".into();
        assert_eq!(model.result_window_label(), "matching 3 of 3 rows");

        if let Some(execution) = model.last_execution.as_mut()
            && let Some(set) = execution.statements[0].result_set.as_mut()
        {
            set.rows_seen = 5000;
            set.cap = 3;
        }
        let label = model.result_window_label();
        assert!(label.contains("matching 3 of 3 retained rows"), "{label}");
        assert!(label.contains("5000 returned"), "{label}");
        assert!(label.contains("limit 3 reached"), "{label}");
    }

    #[test]
    fn the_inspector_opens_on_the_row_the_filter_is_pointing_at() {
        let mut model = with_rows(&["alpha", "beta", "gamma"]);
        model.result_filter = "gamma".into();
        assert_eq!(model.selected_source_row(), Some(2));
        update(&mut model, Message::Action(Action::ToggleInspector));
        assert!(model.inspector.is_some());

        // A filter matching nothing has nothing to inspect.
        model.inspector = None;
        model.result_filter = "nothing".into();
        update(&mut model, Message::Action(Action::ToggleInspector));
        assert!(model.inspector.is_none());
    }

    #[test]
    fn a_finished_statement_is_offered_to_the_history_with_its_outcome() {
        let mut model = connected();
        model.editor.set_text("SELECT 1;");
        let effects = update(&mut model, Message::Action(Action::RunBuffer));
        let Effect::Execute { job, .. } = effects[0] else {
            panic!("expected an execute effect");
        };

        let effects = update(
            &mut model,
            Message::ExecutionFinished(execution(job, ExecutionStatus::Failed, &[])),
        );
        match effects.as_slice() {
            [Effect::RecordHistory { sql, outcome, .. }] => {
                assert_eq!(sql, "SELECT 1;");
                assert_eq!(
                    *outcome,
                    crate::history::Outcome::Failed,
                    "a statement is recorded with what really happened to it"
                );
            }
            other => panic!("expected one record effect, got {other:?}"),
        }
        assert!(
            model.running_sql.is_none(),
            "the statement is recorded once, not on every later message"
        );
    }

    #[test]
    fn a_paused_or_disabled_history_asks_for_no_recording_at_all() {
        for (paused, disabled) in [(true, false), (false, true), (true, true)] {
            let mut model = connected();
            model.history_paused = paused;
            model.history_disabled = disabled;
            model.editor.set_text("SELECT 1;");
            let effects = update(&mut model, Message::Action(Action::RunBuffer));
            let Effect::Execute { job, .. } = effects[0] else {
                panic!("expected an execute effect");
            };
            let effects = update(
                &mut model,
                Message::ExecutionFinished(execution(job, ExecutionStatus::Succeeded, &[])),
            );
            assert!(
                effects.is_empty(),
                "paused={paused} disabled={disabled} still asked to record"
            );
            assert!(!model.records_history());
        }
    }

    #[test]
    fn a_private_session_can_be_turned_on_and_off_but_configuration_wins() {
        let mut model = connected();
        assert!(model.records_history());
        update(&mut model, Message::Action(Action::ToggleHistoryRecording));
        assert!(model.history_paused, "the session stops recording");
        update(&mut model, Message::Action(Action::ToggleHistoryRecording));
        assert!(!model.history_paused);

        model.history_disabled = true;
        update(&mut model, Message::Action(Action::ToggleHistoryRecording));
        assert!(
            !model.records_history(),
            "a session cannot switch on what the user switched off in their own file"
        );
    }

    #[test]
    fn only_what_reached_the_file_comes_back_as_history() {
        let mut model = connected();
        update(
            &mut model,
            Message::HistoryLoaded(vec![history_entry("SELECT 1")]),
        );
        assert_eq!(model.history.len(), 1);

        // A statement the history refused is reported as nothing kept, and the
        // reason is carried so the interface can say it.
        update(
            &mut model,
            Message::HistoryRecorded {
                entry: Box::new(None),
                recorded: crate::history::Recorded::LooksLikeACredential,
            },
        );
        assert_eq!(model.history.len(), 1, "nothing was added");
        assert!(
            model
                .history_note
                .is_some_and(|note| note.contains("credential")),
            "a silently missing entry would be a mystery"
        );

        update(
            &mut model,
            Message::HistoryRecorded {
                entry: Box::new(Some(history_entry("SELECT 2"))),
                recorded: crate::history::Recorded::Written,
            },
        );
        assert_eq!(model.history.len(), 2);
        assert_eq!(model.history[0].sql, "SELECT 2", "newest first");
        assert!(model.history_note.is_none(), "the note is not sticky");

        // The in-memory list is bounded whether entries arrive at load or one
        // at a time, so a long session cannot walk past the limit.
        for index in 0..crate::history::IN_MEMORY_LIMIT * 2 {
            update(
                &mut model,
                Message::HistoryRecorded {
                    entry: Box::new(Some(history_entry(&format!("SELECT {index}")))),
                    recorded: crate::history::Recorded::Written,
                },
            );
        }
        assert_eq!(model.history.len(), crate::history::IN_MEMORY_LIMIT);
    }

    #[test]
    fn the_history_search_puts_a_past_statement_in_the_editor() {
        let mut model = connected();
        update(
            &mut model,
            Message::HistoryLoaded(vec![
                history_entry("SELECT count(*) FROM orders"),
                history_entry("VACUUM ANALYZE orders"),
            ]),
        );
        model.editor.set_text("");

        update(&mut model, Message::Action(Action::OpenHistory));
        let palette = model.palette.as_ref().expect("open");
        assert_eq!(palette.purpose, crate::app::palette::Purpose::History);
        assert_eq!(palette.entries.len(), 2);

        for ch in "count".chars() {
            update(&mut model, Message::Action(Action::Insert(ch)));
        }
        update(&mut model, Message::Action(Action::Activate));
        assert!(model.palette.is_none());
        assert!(
            model.editor.text().contains("count(*)"),
            "the statement came back: {}",
            model.editor.text()
        );
    }

    #[test]
    fn every_chord_is_reachable_by_name_from_the_palette() {
        // A two-key chord is fast once you know it and invisible until then.
        // The palette is how it is found the first time.
        let model = with_tree();
        let entries = palette_entries(&model);
        for (_, action, description) in crate::ui::keymap::CHORDS {
            assert!(
                entries.iter().any(|entry| entry.command
                    == crate::app::palette::PaletteCommand::Run(action.clone())),
                "{description} cannot be found by name"
            );
        }

        let mut model = with_long_value();
        update(&mut model, Message::Action(Action::OpenPalette));
        for ch in "expandrow".chars() {
            update(&mut model, Message::Action(Action::Insert(ch)));
        }
        update(&mut model, Message::Action(Action::Activate));
        assert!(model.expanded_row, "the chosen command actually ran");
    }

    #[test]
    fn a_chord_shows_its_continuations_and_the_next_key_ends_it() {
        let mut model = with_tree();
        model.sidebar_visible = true;

        update(&mut model, Message::Action(Action::BeginPrefix));
        assert!(model.prefix_pending, "the popup is open");

        update(&mut model, Message::Action(Action::Insert('b')));
        assert!(!model.prefix_pending, "the chord ended");
        assert!(!model.sidebar_visible, "and it did what it said");

        // An unrecognised second key cancels rather than leaving it hanging.
        update(&mut model, Message::Action(Action::BeginPrefix));
        update(&mut model, Message::Action(Action::Insert('z')));
        assert!(!model.prefix_pending);
    }

    #[test]
    fn a_pending_chord_swallows_the_next_key_instead_of_typing_it() {
        let mut model = with_tree();
        model.focus = Focus::Editor;
        model.editor.set_text("SELECT ");
        update(&mut model, Message::Action(Action::BeginPrefix));
        update(&mut model, Message::Action(Action::Insert('z')));
        assert_eq!(
            model.editor.text(),
            "SELECT ",
            "the chord key did not become text"
        );
    }

    #[test]
    fn escape_peels_one_layer_at_a_time_in_a_fixed_order() {
        let mut model = with_tree();
        model.error = Some(Diagnostic::new(DiagnosticKind::Query, "boom", "running"));
        model.tree.filter = "orders".into();
        update(&mut model, Message::Action(Action::ToggleHelp));
        update(&mut model, Message::Action(Action::OpenPalette));

        update(&mut model, Message::Action(Action::Dismiss));
        assert!(model.palette.is_none(), "the palette goes first");
        assert!(model.help_open);

        update(&mut model, Message::Action(Action::Dismiss));
        assert!(!model.help_open, "then help");
        assert_eq!(model.tree.filter, "orders");

        update(&mut model, Message::Action(Action::Dismiss));
        assert!(model.tree.filter.is_empty(), "then the filter");
        assert!(model.error.is_some(), "the unread error is still there");

        update(&mut model, Message::Action(Action::Dismiss));
        assert!(model.error.is_none(), "and only then the error");
    }

    /// A model holding a result whose one row is wider than any pane.
    fn with_long_value() -> Model {
        let mut model = connected();
        model.size = (120, 40);
        model.editor.set_text("SELECT 1;");
        let effects = update(&mut model, Message::Action(Action::RunBuffer));
        let Effect::Execute { job, .. } = effects[0] else {
            panic!("expected an execute effect");
        };

        let mut set = ResultSet::new(vec!["id".into(), "document".into()], 100);
        set.push(vec![Cell::Text("1".into()), Cell::Text("x".repeat(5000))]);
        let mut execution = execution(job, ExecutionStatus::Succeeded, &[]);
        execution.statements[0].result_set = Some(set);
        update(&mut model, Message::ExecutionFinished(execution));
        model.focus = Focus::Results;
        model
    }

    #[test]
    fn enter_on_a_result_opens_the_inspector_and_closes_it_again() {
        let mut model = with_long_value();
        update(&mut model, Message::Action(Action::Activate));
        assert!(model.inspector.is_some(), "Enter on a cell inspects it");
        update(&mut model, Message::Action(Action::Activate));
        assert!(model.inspector.is_none());

        // The chord reaches it from anywhere, not only from the results pane.
        model.focus = Focus::Editor;
        update(&mut model, Message::Action(Action::ToggleInspector));
        assert!(model.inspector.is_some());
    }

    #[test]
    fn the_inspector_does_not_open_onto_a_result_with_no_rows() {
        let mut model = connected();
        model.focus = Focus::Results;
        // No execution at all.
        update(&mut model, Message::Action(Action::ToggleInspector));
        assert!(model.inspector.is_none());

        // An execution that returned no rows.
        model.editor.set_text("SELECT 1;");
        let effects = update(&mut model, Message::Action(Action::RunBuffer));
        let Effect::Execute { job, .. } = effects[0] else {
            panic!("expected an execute effect");
        };
        update(
            &mut model,
            Message::ExecutionFinished(execution(job, ExecutionStatus::Succeeded, &[])),
        );
        update(&mut model, Message::Action(Action::ToggleInspector));
        assert!(
            model.inspector.is_none(),
            "a window onto nothing is worse than no window"
        );
    }

    #[test]
    fn the_inspector_scrolls_within_the_value_and_stops_at_both_ends() {
        let mut model = with_long_value();
        model.selected_column = 1;
        update(&mut model, Message::Action(Action::ToggleInspector));
        let inspector = model.inspector.expect("open");
        assert_eq!(inspector.scroll(), 0);

        update(&mut model, Message::Action(Action::Move(Direction::Down)));
        assert_eq!(model.inspector.expect("open").scroll(), 1);

        for _ in 0..500 {
            update(&mut model, Message::Action(Action::Move(Direction::Down)));
        }
        let bottom = model.inspector.expect("open").scroll();
        update(&mut model, Message::Action(Action::Move(Direction::Down)));
        assert_eq!(
            model.inspector.expect("open").scroll(),
            bottom,
            "scrolling stops with the last line on screen"
        );

        for _ in 0..1000 {
            update(&mut model, Message::Action(Action::Move(Direction::Up)));
        }
        assert_eq!(model.inspector.expect("open").scroll(), 0);
    }

    #[test]
    fn moving_sideways_in_the_inspector_reads_the_next_column_of_the_same_row() {
        let mut model = with_long_value();
        model.selected_column = 1;
        update(&mut model, Message::Action(Action::ToggleInspector));
        update(&mut model, Message::Action(Action::Move(Direction::Down)));
        assert_eq!(model.inspector.expect("open").scroll(), 1);

        update(&mut model, Message::Action(Action::Move(Direction::Left)));
        assert!(model.inspector.is_some(), "it stays open");
        assert_eq!(model.selected_column, 0);
        assert_eq!(
            model.inspector.expect("open").scroll(),
            0,
            "a different value starts at its beginning"
        );

        // The row does not move, and the last column is the last column.
        update(&mut model, Message::Action(Action::Move(Direction::Right)));
        update(&mut model, Message::Action(Action::Move(Direction::Right)));
        assert_eq!(model.selected_column, 1);
        assert_eq!(model.selected_row, 0);
        assert_eq!(model.focus, Focus::Results);
    }

    #[test]
    fn the_inspector_closes_when_the_result_it_was_reading_is_replaced() {
        let mut model = with_long_value();
        update(&mut model, Message::Action(Action::ToggleInspector));
        assert!(model.inspector.is_some());

        model.editor.set_text("SELECT 2;");
        let effects = update(&mut model, Message::Action(Action::RunBuffer));
        let Effect::Execute { job, .. } = effects[0] else {
            panic!("expected an execute effect");
        };
        update(
            &mut model,
            Message::ExecutionFinished(execution(job, ExecutionStatus::Succeeded, &["a"])),
        );
        assert!(
            model.inspector.is_none(),
            "showing a cell from a result that no longer exists would be a lie"
        );
    }

    #[test]
    fn typing_while_the_inspector_is_open_never_reaches_the_editor() {
        let mut model = with_long_value();
        model.editor.set_text("SELECT 1;");
        update(&mut model, Message::Action(Action::ToggleInspector));
        for ch in "DROP".chars() {
            update(&mut model, Message::Action(Action::Insert(ch)));
        }
        update(&mut model, Message::Action(Action::Backspace));
        assert_eq!(model.editor.text(), "SELECT 1;");
        assert!(model.inspector.is_some());

        update(&mut model, Message::Action(Action::Dismiss));
        assert!(model.inspector.is_none(), "Esc closes it");
    }

    #[test]
    fn a_key_that_means_something_else_closes_the_inspector_and_does_it() {
        // A modal that swallows every other key is a modal people get stuck
        // behind. Typing is swallowed; a bound action is not.
        let mut model = with_long_value();
        update(&mut model, Message::Action(Action::ToggleInspector));
        update(&mut model, Message::Action(Action::OpenPalette));
        assert!(model.inspector.is_none(), "one overlay at a time");
        update(&mut model, Message::Action(Action::Insert('o')));
        assert_eq!(model.palette.expect("open").query, "o");
        assert_eq!(model.editor.text(), "SELECT 1;", "and not into the editor");

        let mut model = with_long_value();
        update(&mut model, Message::Action(Action::ToggleInspector));
        let effects = update(&mut model, Message::Action(Action::RunBuffer));
        assert!(model.inspector.is_none());
        assert!(
            matches!(effects.as_slice(), [Effect::Execute { .. }]),
            "Ctrl+R still runs: {effects:?}"
        );
    }

    #[test]
    fn the_expanded_row_view_is_a_preference_that_survives_the_next_query() {
        let mut model = with_long_value();
        update(&mut model, Message::Action(Action::ToggleExpandedRow));
        assert!(model.expanded_row);

        model.editor.set_text("SELECT 2;");
        let effects = update(&mut model, Message::Action(Action::RunBuffer));
        let Effect::Execute { job, .. } = effects[0] else {
            panic!("expected an execute effect");
        };
        update(
            &mut model,
            Message::ExecutionFinished(execution(job, ExecutionStatus::Succeeded, &["a"])),
        );
        assert!(
            model.expanded_row,
            "someone who turned it on wants it on for the next result too"
        );

        update(&mut model, Message::Action(Action::ToggleExpandedRow));
        assert!(!model.expanded_row);
    }

    #[test]
    fn a_pending_chord_is_dismissed_before_anything_else() {
        let mut model = with_tree();
        update(&mut model, Message::Action(Action::ToggleHelp));
        update(&mut model, Message::Action(Action::BeginPrefix));

        update(&mut model, Message::Action(Action::Dismiss));
        assert!(!model.prefix_pending, "the chord goes first");
        assert!(model.help_open, "and nothing beneath it was touched");
    }

    #[test]
    fn an_open_palette_swallows_a_chord_rather_than_stacking_two_popups() {
        let mut model = with_tree();
        update(&mut model, Message::Action(Action::OpenPalette));
        update(&mut model, Message::Action(Action::BeginPrefix));
        assert!(
            !model.prefix_pending,
            "two overlays at once would leave the user unsure which key applies"
        );
        assert!(model.palette.is_some());
    }

    #[test]
    fn enter_means_something_different_in_each_pane() {
        let mut model = with_tree();

        model.focus = Focus::Editor;
        model.editor.set_text("SELECT 1");
        update(&mut model, Message::Action(Action::Activate));
        assert_eq!(
            model.editor.text(),
            "SELECT 1\n",
            "a line break in the editor"
        );

        model.focus = Focus::Objects;
        update(&mut model, Message::Action(Action::Activate));
        assert!(model.tree.rows()[0].expanded, "opens a node in the tree");
    }

    #[test]
    fn a_failed_metadata_load_explains_itself_inside_the_tree() {
        let mut model = with_tree();
        model.focus = Focus::Objects;
        update(&mut model, Message::Action(Action::Move(Direction::Right)));
        update(&mut model, Message::Action(Action::Move(Direction::Down)));
        let effects = update(&mut model, Message::Action(Action::Move(Direction::Right)));
        let Effect::LoadMetadata { request, path, .. } = &effects[0] else {
            panic!("expected a load");
        };

        update(
            &mut model,
            Message::MetadataLoaded {
                request: *request,
                path: path.clone(),
                payload: Box::new(Err(Diagnostic::new(
                    DiagnosticKind::Query,
                    "permission denied for schema public",
                    "listing tables",
                ))),
            },
        );

        let labels: Vec<String> = model.tree.rows().iter().map(|r| r.label.clone()).collect();
        assert!(
            labels.contains(&"permission denied for schema public".to_owned()),
            "the reason appears where the user was looking: {labels:?}"
        );
        assert!(
            model.error.is_none(),
            "a tree failure is not a query failure"
        );
    }

    #[test]
    fn a_failure_to_load_the_tree_at_all_is_shown_in_the_tree() {
        let mut model = connected();
        update(
            &mut model,
            Message::SchemasLoaded(Box::new(Err(Diagnostic::new(
                DiagnosticKind::Connection,
                "the connection closed",
                "reading the catalogue",
            )))),
        );
        assert!(model.tree.error.is_some());
        assert!(!model.tree.loading);
    }

    #[test]
    fn the_transaction_state_comes_from_the_execution_that_reported_it() {
        let mut model = connected();
        model.editor.set_text("BEGIN;");
        update(&mut model, Message::Action(Action::RunBuffer));
        let job = model.phase.job().expect("running");

        let mut execution = execution(job, ExecutionStatus::Succeeded, &["x"]);
        execution.transaction = crate::query::result::TransactionState::Failed;
        update(&mut model, Message::ExecutionFinished(execution));

        assert_eq!(
            model.transaction,
            crate::query::result::TransactionState::Failed
        );
        assert!(
            model
                .transaction
                .recovery()
                .expect("a way out")
                .contains("ROLLBACK")
        );
    }

    #[test]
    fn a_write_to_production_is_held_back_until_it_is_confirmed() {
        let mut model = connected();
        // Reclassify the session as production.
        let mut info = session();
        info.environment = Environment::Production;
        update(&mut model, Message::Connected(info));

        model.editor.set_text("UPDATE orders SET total = 0;");
        let effects = update(&mut model, Message::Action(Action::RunBuffer));
        assert!(effects.is_empty(), "nothing runs yet");
        let pending = model
            .pending_run
            .as_ref()
            .expect("a confirmation is waiting");
        assert_eq!(pending.impact, crate::query::Impact::Write);

        let effects = update(&mut model, Message::Action(Action::Activate));
        assert!(
            matches!(effects.as_slice(), [Effect::Execute { .. }]),
            "confirming runs it: {effects:?}"
        );
        assert!(model.pending_run.is_none());
    }

    #[test]
    fn a_destructive_statement_needs_the_database_name_typed() {
        let mut model = connected();
        let mut info = session();
        info.environment = Environment::Production;
        update(&mut model, Message::Connected(info));

        model.editor.set_text("DROP TABLE orders;");
        update(&mut model, Message::Action(Action::RunBuffer));

        // Confirming without typing does nothing at all.
        assert!(update(&mut model, Message::Action(Action::Activate)).is_empty());
        assert!(model.pending_run.is_some(), "still waiting");

        for ch in "orders".chars() {
            update(&mut model, Message::Action(Action::Insert(ch)));
        }
        let effects = update(&mut model, Message::Action(Action::Activate));
        assert!(
            matches!(effects.as_slice(), [Effect::Execute { .. }]),
            "typing the name allows it: {effects:?}"
        );
    }

    #[test]
    fn cancelling_a_confirmation_runs_nothing_and_leaves_the_buffer_alone() {
        let mut model = connected();
        let mut info = session();
        info.environment = Environment::Production;
        update(&mut model, Message::Connected(info));

        model.editor.set_text("DELETE FROM orders;");
        update(&mut model, Message::Action(Action::RunBuffer));
        update(&mut model, Message::Action(Action::Dismiss));

        assert!(model.pending_run.is_none());
        assert_eq!(model.phase, QueryPhase::Idle, "nothing was sent");
        assert_eq!(
            model.editor.text(),
            "DELETE FROM orders;",
            "the SQL is still there"
        );
    }

    #[test]
    fn a_read_against_production_runs_without_asking() {
        let mut model = connected();
        let mut info = session();
        info.environment = Environment::Production;
        update(&mut model, Message::Connected(info));

        model.editor.set_text("SELECT count(*) FROM orders;");
        let effects = update(&mut model, Message::Action(Action::RunBuffer));
        assert!(
            matches!(effects.as_slice(), [Effect::Execute { .. }]),
            "a prompt on every read would be noise: {effects:?}"
        );
        assert!(model.pending_run.is_none());
    }

    #[test]
    fn a_write_to_anything_else_runs_without_asking() {
        // The guardrail follows the classification the user gave, never a guess
        // from a host name.
        let mut model = connected();
        model.editor.set_text("UPDATE orders SET total = 0;");
        let effects = update(&mut model, Message::Action(Action::RunBuffer));
        assert!(matches!(effects.as_slice(), [Effect::Execute { .. }]));
        assert!(model.pending_run.is_none());
    }

    #[test]
    fn typing_while_confirming_never_reaches_the_editor() {
        let mut model = connected();
        let mut info = session();
        info.environment = Environment::Production;
        update(&mut model, Message::Connected(info));

        model.editor.set_text("DROP TABLE orders;");
        update(&mut model, Message::Action(Action::RunBuffer));
        for ch in "xyz".chars() {
            update(&mut model, Message::Action(Action::Insert(ch)));
        }
        assert_eq!(model.editor.text(), "DROP TABLE orders;");
        assert_eq!(model.pending_run.as_ref().expect("pending").typed, "xyz");
    }

    #[test]
    fn quitting_sets_the_flag_and_asks_the_runtime_to_stop() {
        let mut model = connected();
        let effects = update(&mut model, Message::Action(Action::Quit));
        assert_eq!(effects, vec![Effect::Quit]);
        assert!(model.should_quit);
    }

    #[test]
    fn resizing_only_records_the_size() {
        let mut model = connected();
        let before = model.clone();
        update(&mut model, Message::Resized(200, 60));
        assert_eq!(model.size, (200, 60));
        assert_eq!(model.phase, before.phase);
        assert_eq!(model.editor.text(), before.editor.text());
    }

    #[test]
    fn a_new_run_clears_the_previous_error() {
        let mut model = connected();
        model.error = Some(Diagnostic::new(DiagnosticKind::Query, "old", "running"));
        model.editor.set_text("SELECT 1;");
        update(&mut model, Message::Action(Action::RunBuffer));
        assert!(model.error.is_none());
    }
}
