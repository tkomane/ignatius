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

use crate::app::grid::GridCommand;
use crate::app::message::{Action, Direction, Effect, Message};
use crate::app::model::{
    ConnectionState, ExportFormat, Focus, Model, ParameterPrompt, PendingUpdate, QueryPhase,
    UpdateCandidate, UpdateLookup, UpdatePrompt,
};
use crate::query::error_location::{self, StatementSource};
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
            clear_error_location(model);
            Vec::new()
        }
        Message::ConnectionFailed(diagnostic) => {
            model.phase = QueryPhase::Idle;
            model.update_lookup = None;
            model.update_prompt = None;
            model.pending_update = None;
            // The server was reached and said no about credentials. That is the
            // one connection failure a person can answer from here, so it is
            // asked rather than merely reported.
            if diagnostic.kind == crate::diagnostics::DiagnosticKind::Authentication
                && model.credential_provider.is_none()
            {
                model.password_prompt = Some(crate::app::model::PasswordPrompt::new(
                    model
                        .connection
                        .info()
                        .map_or_else(|| diagnostic.attempted.clone(), |info| info.target.clone()),
                    diagnostic.headline.clone(),
                ));
            }
            model.error = Some((*diagnostic).clone());
            clear_error_location(model);
            model.connection = ConnectionState::Failed(diagnostic);
            Vec::new()
        }
        Message::ConnectionLost => {
            let cell_update_in_flight = model.running_cell_update
                || model.update_lookup.is_some()
                || model.update_prompt.is_some()
                || model.pending_update.is_some();
            let refresh_in_flight = model.running_refresh || model.refresh_pending;
            if let Some(info) = model.connection.info() {
                model.connection = ConnectionState::Lost {
                    info: Box::new(info.clone()),
                };
            }
            if model.plan.is_loading() {
                let analyzed = model.plan.analyzed();
                model.plan.failed(
                    analyzed,
                    "the connection was lost while reading the query plan",
                    "reconnect and check the session before trying again; nothing was retried",
                );
                model.transaction = crate::query::result::TransactionState::Unknown;
                model.focus = Focus::Results;
                model.running_for = None;
            }
            model.phase = QueryPhase::Idle;
            model.transaction = crate::query::result::TransactionState::Unknown;
            model.completion.menu = None;
            // The runtime sends this message before the late execution result
            // when a session closes. Clear every server-bound prompt and
            // running marker now: the result that follows is deliberately
            // stale, and confirming one of these prompts must not bypass the
            // ordinary usable-connection gate.
            model.pending_run = None;
            model.parameter_prompt = None;
            model.pending_plan = None;
            model.running_sql = None;
            model.running_parameterized = false;
            model.running_cell_update = false;
            model.running_editor_revision = None;
            model.running_source = None;
            model.update_lookup = None;
            model.update_prompt = None;
            model.pending_update = None;
            model.running_refresh = false;
            model.refresh_pending = false;
            model.cell_update_notice = cell_update_in_flight
                .then(|| "The connection was lost; no cell update was retried.".to_owned());
            if refresh_in_flight && !cell_update_in_flight {
                model.refresh_notice = Some(
                    "The connection was lost while refreshing; the outcome is unknown and nothing was retried."
                        .to_owned(),
                );
            }
            Vec::new()
        }
        Message::ExecutionFinished(execution) => {
            // A result only counts if it belongs to the job still in flight.
            if model.phase.job() != Some(execution.job) {
                return Vec::new();
            }
            let current_revision = model.editor.revision();
            let running_revision = model.running_editor_revision.take();
            let running_source = model.running_source.take();
            let running_parameterized = model.running_parameterized;
            let running_cell_update = model.running_cell_update;
            let running_refresh = model.running_refresh;
            let completed_sql = model.running_sql.take();
            let retained_execution = running_cell_update
                .then(|| model.last_execution.take())
                .flatten();
            let retained_sql = running_cell_update.then(|| model.last_sql.take()).flatten();
            model.running_parameterized = false;
            model.running_cell_update = false;
            model.running_refresh = false;
            model.refresh_pending = false;
            let (location, location_note) = map_error_location(
                model,
                execution.error.as_ref(),
                running_revision,
                running_source,
                running_parameterized,
            );
            model.phase = QueryPhase::Idle;
            model.running_for = None;
            model.transaction = execution.transaction;
            model.last_elapsed = Some(execution.elapsed);
            model.error = execution.error.clone();
            model.error_expanded = false;
            model.error_location = location;
            model.error_location_note = location_note;
            if let Some(location) = model.error_location.as_ref()
                && running_revision == Some(current_revision)
            {
                model.editor.set_cursor(location.cursor);
                model.focus = Focus::Editor;
            }
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
            if !running_cell_update {
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
            }
            model.pending_copy = None;
            model.clipboard_notice = None;
            model.update_lookup = None;
            model.update_prompt = None;
            model.pending_update = None;
            let outcome = crate::history::Outcome::from_status(&execution.status);
            let elapsed = execution.elapsed;
            if running_cell_update {
                // A generated UPDATE is a write against the retained result,
                // not a replacement result. Keep the source snapshot and its
                // identity available so the notice can honestly say that a
                // deliberate SELECT is needed to read fresh data.
                model.last_execution = retained_execution;
                model.last_sql = retained_sql;
                model.cell_update_notice = Some(match outcome {
                    crate::history::Outcome::Succeeded => {
                        let affected = execution
                            .statements
                            .first()
                            .and_then(|statement| statement.rows_affected);
                        match affected {
                            Some(affected) => format!(
                                "UPDATE completed once; PostgreSQL reported {affected} row(s) affected. The prior result is a snapshot; run the SELECT again to read fresh data."
                            ),
                            None => "UPDATE completed once; PostgreSQL did not report an affected-row count. The prior result is a snapshot; run the SELECT again to read fresh data.".to_owned(),
                        }
                    }
                    crate::history::Outcome::Cancelled => {
                        "The generated UPDATE was cancelled; nothing was retried.".to_owned()
                    }
                    crate::history::Outcome::Failed => {
                        "The generated UPDATE failed; nothing was retried. Read the error for details.".to_owned()
                    }
                    crate::history::Outcome::Unknown => {
                        "The connection was lost while sending the generated UPDATE; the outcome is unknown and nothing was retried.".to_owned()
                    }
                });
            } else {
                model.last_execution = Some(*execution);
                model.last_sql = completed_sql.clone();
                model.cell_update_notice = None;
                model.refresh_notice = running_refresh.then(|| match outcome {
                    crate::history::Outcome::Succeeded => {
                        "Retained result refreshed once; no follow-up query was run.".to_owned()
                    }
                    crate::history::Outcome::Cancelled => {
                        "Retained result refresh was cancelled; nothing was retried.".to_owned()
                    }
                    crate::history::Outcome::Failed => {
                        "Retained result refresh failed; nothing was retried. Read the error for details."
                            .to_owned()
                    }
                    crate::history::Outcome::Unknown => {
                        "The connection was lost while refreshing; the outcome is unknown and nothing was retried."
                            .to_owned()
                    }
                });
                let columns = model
                    .visible_result()
                    .map_or_else(Vec::new, |set| set.columns.clone());
                model.result_grid.reset_for_result(&columns);
            }
            // The statement is recorded once it has an outcome, so what the
            // history holds is what really ran rather than what was submitted.
            match completed_sql {
                Some(sql) if model.records_history() => vec![Effect::RecordHistory {
                    sql,
                    outcome,
                    elapsed,
                }],
                _ => Vec::new(),
            }
        }
        Message::PlanFinished(execution) => finish_plan(model, *execution),
        Message::ClipboardSent { bytes, characters } => {
            model.pending_copy = None;
            model.clipboard_notice =
                Some(crate::app::model::ClipboardNotice::Sent { bytes, characters });
            Vec::new()
        }
        Message::ClipboardFailed(diagnostic) => {
            model.pending_copy = None;
            model.error_expanded = false;
            clear_error_location(model);
            model.error = Some(*diagnostic);
            Vec::new()
        }
        Message::CancellationDelivered(_) => Vec::new(),
        Message::CancellationFailed(diagnostic) => {
            model.error = Some(*diagnostic);
            clear_error_location(model);
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
        Message::CompletionLoaded {
            request,
            loaded_at,
            result,
        } => {
            if !model.completion.accepts(request) {
                return Vec::new();
            }
            match *result {
                Ok(catalog) => {
                    model.completion.catalog =
                        crate::app::completion::CatalogStatus::Ready { catalog, loaded_at };
                    refresh_completion_menu(model);
                }
                Err(error) => {
                    model.completion.catalog = crate::app::completion::CatalogStatus::Unavailable {
                        message: error.headline,
                    };
                    refresh_completion_menu(model);
                }
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
        Message::UpdateTargetLoaded { request, result } => {
            finish_update_target(model, request, *result)
        }
        Message::QueriesListed(queries) => {
            model.palette = Some(crate::app::palette::Palette::over_saved_queries(
                queries
                    .iter()
                    .map(|query| crate::app::palette::PaletteEntry {
                        label: query.name.clone(),
                        detail: query.summary.clone(),
                        group: "Saved",
                        command: crate::app::palette::PaletteCommand::Open(query.name.clone()),
                    })
                    .collect(),
            ));
            Vec::new()
        }
        Message::RowsExported(result) => {
            match *result {
                Ok(message) => model.notices.push(crate::query::result::Notice {
                    severity: "WROTE".to_owned(),
                    message,
                    code: None,
                }),
                Err(error) => model.error = Some(error),
            }
            Vec::new()
        }
        Message::QuerySaved(result) => {
            match *result {
                Ok(path) => {
                    // The buffer is no longer unsaved, and where it went is worth
                    // knowing: it is an ordinary file the user now owns.
                    model.editor.mark_saved();
                    model.notices.push(crate::query::result::Notice {
                        severity: "SAVED".to_owned(),
                        message: path.display().to_string(),
                        code: None,
                    });
                }
                Err(error) => model.error = Some(error),
            }
            Vec::new()
        }
        Message::QueryLoaded(result) => {
            match *result {
                // Loading is an edit like any other, so it can be undone.
                Ok(text) => {
                    edit_editor(model, |editor| editor.set_text(text));
                    model.focus = Focus::Editor;
                }
                Err(error) => {
                    model.loaded_query = None;
                    model.error = Some(error);
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
                    // The waiting palette goes with the answer that never came.
                    model.palette = None;
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

/// Maps a server error only when its source buffer is still the one on screen.
fn map_error_location(
    model: &Model,
    error: Option<&crate::diagnostics::Diagnostic>,
    running_revision: Option<u64>,
    running_source: Option<StatementSource>,
    running_parameterized: bool,
) -> (
    Option<crate::query::error_location::ErrorLocation>,
    Option<String>,
) {
    let Some(error) = error else {
        return (None, None);
    };
    if error.kind != crate::diagnostics::DiagnosticKind::Query {
        return (None, None);
    }
    if running_revision != Some(model.editor.revision()) {
        return (
            None,
            Some(
                "The buffer changed while this statement was running; the reported position \
                 belongs to the previous submission."
                    .to_owned(),
            ),
        );
    }
    if running_parameterized && error.position.is_some() {
        return (
            None,
            Some(
                "This statement used prompted parameters; the server position belongs to the \
                 expanded request, so no editor location was guessed."
                    .to_owned(),
            ),
        );
    }
    let Some(position) = error.position else {
        return (
            None,
            Some("PostgreSQL did not report an editor position for this error.".to_owned()),
        );
    };
    let Some(statement_number) = running_source
        .map(|source| source.number)
        .or(error.statement_number)
        .filter(|number| *number > 0)
    else {
        return (
            None,
            Some(
                "The failed statement number is unavailable, so no editor location was guessed."
                    .to_owned(),
            ),
        );
    };
    match error_location::locate(
        model.editor.text(),
        running_source,
        statement_number,
        position,
    ) {
        Some(location) => (Some(location), None),
        None => (
            None,
            Some(
                "The server position is outside the submitted statement, so no editor location \
                 was guessed."
                    .to_owned(),
            ),
        ),
    }
}

/// Clears the transient marker and its explanation.
fn clear_error_location(model: &mut Model) {
    model.error_location = None;
    model.error_location_note = None;
}

/// Applies an editor text mutation and invalidates an old server marker if it
/// actually changed the buffer.
fn edit_editor<F>(model: &mut Model, edit: F)
where
    F: FnOnce(&mut crate::app::editor::Editor),
{
    let before = model.editor.revision();
    edit(&mut model.editor);
    if model.editor.revision() != before {
        if model.error_location.take().is_some() {
            model.error_location_note = Some(
                "The buffer changed; this error location belongs to the previous submission."
                    .to_owned(),
            );
        }
        model.format_notice = None;
    }
}

/// Formats the editor as one local, undoable edit.
fn format_buffer(model: &mut Model) {
    let source = model.editor.text().to_owned();
    if source.trim().is_empty() || crate::query::format::is_comment_only(&source) {
        model.format_notice = Some(crate::app::model::FormatNotice::Empty);
        return;
    }

    let cursor = model.editor.cursor();
    let formatted = match crate::query::format_sql(&source, cursor) {
        Ok(formatted) => formatted,
        Err(error) => {
            model.format_notice = Some(crate::app::model::FormatNotice::Refused {
                message: error.message(&source),
            });
            return;
        }
    };

    if formatted.text == source {
        model.format_notice = Some(crate::app::model::FormatNotice::AlreadyFormatted);
        return;
    }

    let before_lines = source.matches('\n').count() + 1;
    let after_lines = formatted.text.matches('\n').count() + 1;
    let mapped_cursor = formatted.cursor;
    let replacement = formatted.text;
    edit_editor(model, |editor| {
        editor.replace_range(0..source.len(), &replacement)
    });
    model.editor.set_cursor(mapped_cursor);
    model.format_notice = Some(crate::app::model::FormatNotice::Applied {
        before_lines,
        after_lines,
    });
}

fn apply_action(model: &mut Model, action: Action) -> Vec<Effect> {
    // Modes are peeled in a fixed order, highest first. Getting this wrong is
    // how typing in the editor starts doing surprising things, so the order is
    // stated once here and tested directly.
    // The password prompt is peeled first. Nothing else can be done until the
    // connection is open, and a character typed into it must not reach anything
    // that could keep it.
    if model.password_prompt.is_some() {
        return password_action(model, action);
    }
    if model.parameter_prompt.is_some() {
        return parameter_action(model, action);
    }
    if model.name_prompt.is_some() {
        return name_action(model, action);
    }
    if model.connection_details {
        return connection_details_action(model, action);
    }
    if model.prefix_pending {
        return resolve_prefix(model, action);
    }
    if model.pending_run.is_some() {
        return confirmation_action(model, action);
    }
    if model.pending_plan.is_some() {
        return plan_confirmation_action(model, action);
    }
    if model.pending_copy.is_some() {
        return copy_confirmation_action(model, action);
    }
    if model.update_prompt.is_some() {
        return update_prompt_action(model, action);
    }
    if model.pending_update.is_some() {
        return pending_update_action(model, action);
    }
    if model.palette.is_some() {
        return palette_action(model, action);
    }
    if model.completion.menu.is_some() {
        return completion_action(model, action);
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
            if model.plan.is_visible() && !model.plan.is_loading() {
                model.plan.clear();
            } else if model.help_open {
                model.help_open = false;
            } else if !model.tree.filter.is_empty() {
                model.tree.filter.clear();
            } else {
                model.error = None;
                model.error_expanded = false;
                clear_error_location(model);
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
                return reload_effects(model);
            }
            Vec::new()
        }
        Action::OpenPalette => {
            model.palette = Some(crate::app::palette::Palette::over_commands(
                palette_entries(model),
                crate::app::discovery::palette_note(model),
            ));
            Vec::new()
        }
        Action::OpenConnectionPicker => {
            if crate::app::discovery::action_is_available(model, &Action::OpenConnectionPicker) {
                model.palette = Some(crate::app::palette::Palette::over_connections(
                    connection_entries(model),
                ));
            }
            Vec::new()
        }
        Action::OpenResultControls if model.plan.is_visible() => Vec::new(),
        Action::OpenResultControls => open_result_controls(model),
        Action::BeginPrefix => {
            model.prefix_pending = true;
            Vec::new()
        }
        // The filter key belongs to whatever is being looked at. In the results
        // that means the rows; anywhere else it means the object tree.
        Action::StartFilter if model.plan.is_visible() => Vec::new(),
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
            reload_effects(model)
        }
        Action::Complete if model.focus == Focus::Editor => {
            open_completion(model, true);
            Vec::new()
        }
        Action::Complete => Vec::new(),
        // Enter in the editor is a line break that keeps the indentation of the
        // line it left. It must go through `insert_newline` rather than
        // `insert('\n')`: the indentation rule lives there, and this is the only
        // path a key press ever takes to reach it.
        Action::Activate if model.focus == Focus::Editor => {
            edit_editor(model, |editor| editor.insert_newline());
            Vec::new()
        }
        // In the results pane, Enter means "show me this value in full", which
        // is the only thing there is to do to a cell.
        Action::Activate if model.focus == Focus::Results => {
            if model.plan.document().is_some() {
                model.plan.toggle_selected();
            } else {
                toggle_inspector(model);
            }
            Vec::new()
        }
        Action::Activate => Vec::new(),
        Action::SaveQuery => {
            if model.editor.text().trim().is_empty() {
                return Vec::new();
            }
            model.name_prompt = Some(crate::app::model::NamePrompt::for_query(
                model.loaded_query.clone().unwrap_or_default(),
            ));
            Vec::new()
        }
        Action::ExportRows => {
            if model.plan.is_visible() {
                return Vec::new();
            }
            // Only what is on screen can be written, so there has to be
            // something on screen.
            let Some(set) = model.visible_result() else {
                return Vec::new();
            };
            let matching = model.filtered_rows().len();
            if matching == 0 {
                return Vec::new();
            }
            model.palette = Some(crate::app::palette::Palette::over_export_formats(
                export_format_entries(),
                export_note(model, set, matching),
            ));
            Vec::new()
        }
        Action::OpenQuery => vec![Effect::ListQueries],
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
            if !model.plan.is_visible() {
                model.expanded_row = !model.expanded_row;
            }
            Vec::new()
        }
        Action::ToggleInspector => {
            if !model.plan.is_visible() {
                toggle_inspector(model);
            }
            Vec::new()
        }
        Action::ExplainPlan => begin_plan(model, false),
        Action::AnalyzePlan => begin_plan(model, true),
        Action::CopyValue => begin_copy(model),
        Action::GenerateCellUpdate => begin_cell_update(model),
        Action::RefreshResult => begin_refresh_result(model),
        Action::FormatBuffer => {
            format_buffer(model);
            Vec::new()
        }
        Action::RunBuffer => {
            let sql = model.editor.text().to_owned();
            run(model, sql, None)
        }
        Action::RunStatement => {
            let source =
                error_location::source_at_cursor(model.editor.text(), model.editor.cursor());
            let sql = source
                .map(|source| model.editor.text()[source.start..source.end].to_owned())
                .unwrap_or_default();
            run(model, sql, source)
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
            edit_editor(model, |editor| editor.insert(ch));
            open_completion(model, false);
            Vec::new()
        }
        Action::Backspace if model.focus == Focus::Editor => {
            edit_editor(model, |editor| editor.backspace());
            open_completion(model, false);
            Vec::new()
        }
        Action::Insert(_) | Action::Backspace => Vec::new(),
        Action::Move(direction) => {
            move_selection(model, direction);
            Vec::new()
        }
        // Editing keys belong to the editor. Elsewhere they do nothing rather
        // than doing something that looks like an edit somewhere else.
        Action::DeleteForward if model.focus == Focus::Editor => {
            edit_editor(model, |editor| editor.delete_forward());
            Vec::new()
        }
        Action::DeleteWordLeft if model.focus == Focus::Editor => {
            edit_editor(model, |editor| editor.delete_word_left());
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
            edit_editor(model, |editor| editor.undo());
            Vec::new()
        }
        Action::Redo if model.focus == Focus::Editor => {
            edit_editor(model, |editor| editor.redo());
            Vec::new()
        }
        // These two are useful in every pane, so they are not editor-only.
        Action::MoveBufferStart => {
            match model.focus {
                Focus::Editor => model.editor.move_buffer_start(),
                Focus::Results => {
                    if model.plan.document().is_some() {
                        model.plan.selected_path.clear();
                    } else {
                        model.selected_row = 0;
                    }
                }
                Focus::Objects => model.tree.move_selection(isize::MIN / 2),
            }
            Vec::new()
        }
        Action::MoveBufferEnd => {
            match model.focus {
                Focus::Editor => model.editor.move_buffer_end(),
                Focus::Results => {
                    if model.plan.document().is_some() {
                        model.plan.move_selection(isize::MAX / 2);
                    } else {
                        model.selected_row = model.displayed_rows().len().saturating_sub(1);
                    }
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

/// Starts the tree and completion reads as one visible refresh operation.
fn reload_effects(model: &mut Model) -> Vec<Effect> {
    let request = model.completion.begin_loading();
    vec![
        Effect::LoadSchemas,
        Effect::LoadCompletionCatalog { request },
    ]
}

/// Opens completion at the current editor cursor.
fn open_completion(model: &mut Model, explicit: bool) {
    if model.focus != Focus::Editor {
        model.completion.menu = None;
        return;
    }
    if !explicit && !model.completion.enabled {
        model.completion.menu = None;
        return;
    }
    let result = crate::query::completion::complete(
        model.editor.text(),
        model.editor.cursor(),
        model.completion.catalog(),
    );
    if !explicit && !automatic_completion_should_open(&result) {
        model.completion.menu = None;
        return;
    }
    model.completion.menu = Some(crate::app::completion::CompletionMenu::new(
        result, explicit,
    ));
}

/// Recomputes a menu after a local edit or a catalogue response.
fn refresh_completion_menu(model: &mut Model) {
    let Some(menu) = model.completion.menu.take() else {
        return;
    };
    let result = crate::query::completion::complete(
        model.editor.text(),
        model.editor.cursor(),
        model.completion.catalog(),
    );
    if !menu.explicit && !automatic_completion_should_open(&result) {
        return;
    }
    model.completion.menu = Some(menu.refreshed(result));
}

/// Automatic completion must not steal Enter after the user has already typed
/// one complete, unambiguous candidate. Explicit completion still shows it.
fn automatic_completion_should_open(result: &crate::query::completion::CompletionResult) -> bool {
    if result.prefix.trim().chars().count() < 2 || result.candidates.is_empty() {
        return false;
    }
    !(result.matching_count == 1
        && result.candidates[0]
            .label
            .eq_ignore_ascii_case(result.prefix.trim()))
}

/// Handles input while the completion menu is open.
fn completion_action(model: &mut Model, action: Action) -> Vec<Effect> {
    match action {
        Action::Insert(ch) if model.focus == Focus::Editor => {
            edit_editor(model, |editor| editor.insert(ch));
            refresh_completion_menu(model);
            Vec::new()
        }
        Action::Backspace if model.focus == Focus::Editor => {
            edit_editor(model, |editor| editor.backspace());
            refresh_completion_menu(model);
            Vec::new()
        }
        Action::Move(Direction::Up) => {
            if let Some(menu) = model.completion.menu.as_mut() {
                menu.move_selection(-1);
            }
            Vec::new()
        }
        Action::Move(Direction::Down) => {
            if let Some(menu) = model.completion.menu.as_mut() {
                menu.move_selection(1);
            }
            Vec::new()
        }
        Action::Activate => {
            let accepted = model.completion.menu.as_ref().and_then(|menu| {
                menu.selected_candidate().map(|candidate| {
                    (
                        menu.result.replacement.clone(),
                        candidate.insert_text.clone(),
                    )
                })
            });
            model.completion.menu = None;
            if let Some((range, replacement)) = accepted {
                edit_editor(model, |editor| editor.replace_range(range, &replacement));
            }
            Vec::new()
        }
        Action::Dismiss => {
            model.completion.menu = None;
            Vec::new()
        }
        Action::Complete => {
            open_completion(model, true);
            Vec::new()
        }
        Action::Quit => {
            model.completion.menu = None;
            model.should_quit = true;
            vec![Effect::Quit]
        }
        other => {
            // The menu is a transient layer. A key with another meaning closes
            // it and then performs that meaning, so Ctrl+R and pane navigation
            // never feel swallowed by an invisible popup state.
            model.completion.menu = None;
            apply_action(model, other)
        }
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
    // Opened now, filled when the answer arrives. A key that appears to do
    // nothing for a second is a key people press twice.
    model.palette = Some(crate::app::palette::Palette::awaiting_dependencies());
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
        Action::Insert(_) | Action::Backspace => {}
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
            if model.plan.document().is_some() {
                match direction {
                    Direction::Up => model.plan.move_selection(-1),
                    Direction::Down => model.plan.move_selection(1),
                    Direction::Left | Direction::Right => {}
                }
                return;
            }
            let rows = model.displayed_rows().len();
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
    if model.plan.is_visible() {
        return;
    }
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
        Action::Insert(_) | Action::Backspace => {}
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

/// Handles input while a password is being typed.
///
/// Every key either edits the password, sends it, or abandons it. Nothing falls
/// through to another handler: a character typed here must not end up in the
/// editor, in the palette, or in a filter, where it would be visible and could
/// be run.
fn password_action(model: &mut Model, action: Action) -> Vec<Effect> {
    let Some(prompt) = model.password_prompt.as_mut() else {
        return Vec::new();
    };
    match action {
        Action::Insert(ch) => {
            prompt.push(ch);
            Vec::new()
        }
        Action::Backspace => {
            prompt.backspace();
            Vec::new()
        }
        Action::Activate | Action::RunBuffer => {
            if prompt.is_empty() {
                // An empty password is not an answer. Saying nothing and
                // retrying would produce the same refusal and look like a bug.
                return Vec::new();
            }
            let password = prompt.take();
            model.password_prompt = None;
            model.error = None;
            model.connection = ConnectionState::Connecting;
            vec![Effect::Reconnect {
                password: crate::app::message::TypedPassword::new(password),
            }]
        }
        Action::Dismiss | Action::Cancel => {
            // The error that opened the prompt is still there to read.
            model.password_prompt = None;
            Vec::new()
        }
        Action::Quit => {
            model.should_quit = true;
            vec![Effect::Quit]
        }
        _ => Vec::new(),
    }
}

/// Handles input while named parameter values are being collected.
fn parameter_action(model: &mut Model, action: Action) -> Vec<Effect> {
    match action {
        Action::Insert(character) => {
            if let Some(prompt) = model.parameter_prompt.as_mut() {
                prompt.push(character);
            }
            Vec::new()
        }
        Action::Backspace => {
            if let Some(prompt) = model.parameter_prompt.as_mut() {
                prompt.backspace();
            }
            Vec::new()
        }
        Action::Activate | Action::RunBuffer => {
            let accepted = model.parameter_prompt.as_mut().map(ParameterPrompt::accept);
            match accepted {
                Some(Ok(None)) => Vec::new(),
                Some(Ok(Some(parameters))) => {
                    let prompt = model
                        .parameter_prompt
                        .take()
                        .expect("parameter prompt still exists after acceptance");
                    start_parameterized(model, prompt.sql, prompt.source, parameters)
                }
                Some(Err(error)) => {
                    model.parameter_prompt = None;
                    if model.refresh_pending {
                        model.refresh_pending = false;
                        model.refresh_notice = Some(
                            "Retained result refresh was refused before sending; nothing was retried."
                                .to_owned(),
                        );
                    }
                    model.error = Some(error.diagnostic("accepting named parameter values"));
                    clear_error_location(model);
                    Vec::new()
                }
                None => Vec::new(),
            }
        }
        Action::Dismiss | Action::Cancel => {
            model.parameter_prompt = None;
            if model.refresh_pending {
                model.refresh_pending = false;
                model.refresh_notice = None;
            }
            Vec::new()
        }
        Action::Quit => {
            model.should_quit = true;
            vec![Effect::Quit]
        }
        _ => Vec::new(),
    }
}

/// Handles input while a name is being typed.
fn name_action(model: &mut Model, action: Action) -> Vec<Effect> {
    let Some(prompt) = model.name_prompt.as_mut() else {
        return Vec::new();
    };
    match action {
        Action::Insert(ch) => {
            prompt.typed.push(ch);
            Vec::new()
        }
        Action::Backspace => {
            prompt.typed.pop();
            Vec::new()
        }
        Action::Activate | Action::RunBuffer => {
            if prompt.typed.trim().is_empty() {
                return Vec::new();
            }
            let name = prompt.typed.trim().to_owned();
            let purpose = prompt.purpose;
            let export_format = prompt.export_format;
            model.name_prompt = None;
            match purpose {
                crate::app::model::NamePurpose::SaveQuery => {
                    model.loaded_query = Some(name.clone());
                    vec![Effect::SaveQuery {
                        name,
                        sql: model.editor.text().to_owned(),
                    }]
                }
                crate::app::model::NamePurpose::ExportRows => {
                    let Some(format) = export_format else {
                        return Vec::new();
                    };
                    vec![Effect::ExportRows { path: name, format }]
                }
            }
        }
        Action::Dismiss | Action::Cancel => {
            model.name_prompt = None;
            Vec::new()
        }
        Action::Quit => {
            model.should_quit = true;
            vec![Effect::Quit]
        }
        _ => Vec::new(),
    }
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
            let source = pending.source;
            model.pending_run = None;
            start(model, sql, source)
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

/// Handles the explicit confirmation required before EXPLAIN ANALYZE can be
/// sent. A plain Enter is enough for read-looking statements; destructive
/// impact still requires the database name, just as an ordinary run does.
fn plan_confirmation_action(model: &mut Model, action: Action) -> Vec<Effect> {
    let Some(pending) = model.pending_plan.as_mut() else {
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
                return Vec::new();
            }
            let sql = pending.sql.clone();
            let impact = pending.impact;
            model.pending_plan = None;
            start_plan(model, sql, true, impact)
        }
        Action::Dismiss | Action::Cancel => {
            model.pending_plan = None;
            Vec::new()
        }
        Action::Quit => {
            model.should_quit = true;
            vec![Effect::Quit]
        }
        _ => Vec::new(),
    }
}

/// Handles input while a result value is waiting for explicit copy approval.
///
/// This is a stricter modal than the inspector: typing and navigation are
/// swallowed, and Enter rechecks the retained result before a payload exists.
fn copy_confirmation_action(model: &mut Model, action: Action) -> Vec<Effect> {
    let Some(candidate) = model.pending_copy.take() else {
        return Vec::new();
    };
    match action {
        Action::Activate | Action::RunBuffer => {
            let Some(value) = copy_payload_for_candidate(model, candidate) else {
                model.clipboard_notice = Some(crate::app::model::ClipboardNotice::Stale);
                return Vec::new();
            };
            model.clipboard_notice = None;
            vec![Effect::CopyValue { payload: value }]
        }
        Action::Dismiss | Action::Cancel => {
            // There is no "copied" state to undo: nothing was written before
            // this point, and the retained result and selection stay intact.
            model.clipboard_notice = None;
            Vec::new()
        }
        Action::Quit => {
            model.should_quit = true;
            vec![Effect::Quit]
        }
        _ => {
            // A copy prompt owns the keyboard until it is confirmed or
            // dismissed, so an editor action cannot leak through underneath.
            model.pending_copy = Some(candidate);
            Vec::new()
        }
    }
}

/// Opens a value-free copy confirmation for the selected retained cell.
fn begin_copy(model: &mut Model) -> Vec<Effect> {
    if model.focus != Focus::Results
        || model.phase.is_busy()
        || model.plan.is_visible()
        || model.visible_result().is_none()
    {
        model.clipboard_notice = Some(crate::app::model::ClipboardNotice::Unavailable);
        return Vec::new();
    }

    let Some(result_job) = model.last_execution.as_ref().map(|execution| execution.job) else {
        model.clipboard_notice = Some(crate::app::model::ClipboardNotice::Unavailable);
        return Vec::new();
    };
    let Some(source_row) = model.selected_source_row() else {
        model.clipboard_notice = Some(crate::app::model::ClipboardNotice::Unavailable);
        return Vec::new();
    };
    let selected = model.visible_result().and_then(|set| {
        let visible = model.result_grid.visible_columns(set.columns.len());
        if !visible.contains(&model.selected_column) {
            return None;
        }
        let cell = set.rows.get(source_row)?.get(model.selected_column)?;
        Some((
            cell.is_null(),
            cell.raw().map(|value| (value.len(), value.chars().count())),
        ))
    });
    let Some((is_null, counts)) = selected else {
        model.clipboard_notice = Some(crate::app::model::ClipboardNotice::Unavailable);
        return Vec::new();
    };
    if is_null {
        model.clipboard_notice = Some(crate::app::model::ClipboardNotice::Null);
        return Vec::new();
    }
    if !model.clipboard_osc52 {
        model.clipboard_notice = Some(crate::app::model::ClipboardNotice::Disabled);
        return Vec::new();
    }
    let Some((bytes, characters)) = counts else {
        model.clipboard_notice = Some(crate::app::model::ClipboardNotice::Null);
        return Vec::new();
    };
    if bytes > crate::clipboard::MAX_OSC52_BYTES {
        model.clipboard_notice = Some(crate::app::model::ClipboardNotice::TooLarge { bytes });
        return Vec::new();
    }

    model.clipboard_notice = None;
    model.pending_copy = Some(crate::app::model::PendingCopy {
        result_job,
        source_row,
        column: model.selected_column,
        bytes,
        characters,
    });
    Vec::new()
}

/// Re-derives the selected value after a copy confirmation and wraps it for the
/// transport. Any changed identity or count is a stale candidate, not a guess.
fn copy_payload_for_candidate(
    model: &Model,
    candidate: crate::app::model::PendingCopy,
) -> Option<crate::clipboard::ClipboardPayload> {
    if model.focus != Focus::Results
        || model.phase.is_busy()
        || model.plan.is_visible()
        || model.last_execution.as_ref()?.job != candidate.result_job
        || model.selected_source_row() != Some(candidate.source_row)
        || model.selected_column != candidate.column
    {
        return None;
    }
    let set = model.visible_result()?;
    if !model
        .result_grid
        .visible_columns(set.columns.len())
        .contains(&candidate.column)
    {
        return None;
    }
    let value = set
        .rows
        .get(candidate.source_row)?
        .get(candidate.column)?
        .raw()?;
    if value.len() != candidate.bytes || value.chars().count() != candidate.characters {
        return None;
    }
    crate::clipboard::ClipboardPayload::try_new(value.to_owned()).ok()
}

/// Handles input while a replacement value is being typed for a cell update.
///
/// The value stays in the secret prompt until the planner has accepted it. A
/// refusal leaves the prompt open so the operator can correct the value, while
/// a changed result clears it as stale rather than guessing at a new row.
fn update_prompt_action(model: &mut Model, action: Action) -> Vec<Effect> {
    let Some(mut prompt) = model.update_prompt.take() else {
        return Vec::new();
    };
    match action {
        Action::Insert(character) => {
            prompt.push(character);
            model.update_prompt = Some(prompt);
            Vec::new()
        }
        Action::Backspace => {
            prompt.backspace();
            model.update_prompt = Some(prompt);
            Vec::new()
        }
        Action::Activate | Action::RunBuffer => {
            if !update_candidate_is_current(model, &prompt.candidate) {
                model.cell_update_notice = Some(
                    "The result or selection changed; the cell update was discarded.".to_owned(),
                );
                return Vec::new();
            }
            let Some(set) = model.visible_result() else {
                model.cell_update_notice =
                    Some("The retained result is gone; the cell update was discarded.".to_owned());
                return Vec::new();
            };
            let Some(selected_row) = model.selected_source_row() else {
                model.cell_update_notice = Some(
                    "The selected result row is gone; the cell update was discarded.".to_owned(),
                );
                return Vec::new();
            };
            let Some(row) = set.rows.get(selected_row) else {
                model.cell_update_notice = Some(
                    "The selected result row is gone; the cell update was discarded.".to_owned(),
                );
                return Vec::new();
            };
            let relation_columns = prompt
                .relation
                .columns
                .iter()
                .map(|column| crate::query::UpdateColumn {
                    name: column.name.clone(),
                    primary_key: column.primary_key,
                })
                .collect::<Vec<_>>();
            let plan = crate::query::plan_update(
                &prompt.source,
                &prompt.relation.schema,
                &prompt.relation.relation,
                &set.columns,
                row,
                &relation_columns,
                prompt.replacement(),
            );
            match plan {
                Ok(plan) => {
                    model.pending_update = Some(PendingUpdate {
                        candidate: prompt.candidate,
                        relation: prompt.relation,
                        plan,
                    });
                    model.cell_update_notice =
                        Some("Review the generated UPDATE. Nothing has been sent yet.".to_owned());
                }
                Err(error) => {
                    model.cell_update_notice = Some(error.to_string());
                    model.update_prompt = Some(prompt);
                }
            }
            Vec::new()
        }
        Action::Dismiss | Action::Cancel => {
            model.cell_update_notice = None;
            Vec::new()
        }
        Action::Quit => {
            model.should_quit = true;
            vec![Effect::Quit]
        }
        _ => {
            // The value prompt owns the keyboard until review or dismissal.
            model.update_prompt = Some(prompt);
            Vec::new()
        }
    }
}

/// Handles the final, one-shot confirmation of a generated update.
fn pending_update_action(model: &mut Model, action: Action) -> Vec<Effect> {
    let Some(pending) = model.pending_update.take() else {
        return Vec::new();
    };
    match action {
        Action::Activate | Action::RunBuffer => {
            if !update_candidate_is_current(model, &pending.candidate) {
                model.cell_update_notice = Some(
                    "The result or selection changed; the cell update was discarded.".to_owned(),
                );
                return Vec::new();
            }
            if !model.connection.is_usable() {
                model.cell_update_notice =
                    Some("The connection is not usable; nothing was sent.".to_owned());
                return Vec::new();
            }
            if model.environment().is_production() {
                model.cell_update_notice = Some(
                    "Cell updates are refused on production-classified connections; nothing was sent."
                        .to_owned(),
                );
                return Vec::new();
            }
            if model.connection.info().is_some_and(|info| info.read_only) {
                model.cell_update_notice = Some(
                    "Cell updates are refused on a read-only session; nothing was sent.".to_owned(),
                );
                return Vec::new();
            }
            if model.transaction == crate::query::result::TransactionState::Failed {
                model.cell_update_notice = Some(
                    "The transaction is failed; recover it before sending a cell update."
                        .to_owned(),
                );
                return Vec::new();
            }
            let sql = pending.plan.sql_template.clone();
            let parameters = pending.plan.parameters.clone();
            let effects = start_with_parameters(model, sql, None, Some(parameters));
            if effects.is_empty() {
                model.cell_update_notice =
                    Some("The generated UPDATE could not be started; nothing was sent.".to_owned());
            } else {
                model.running_cell_update = true;
                model.cell_update_notice =
                    Some("Sending the generated UPDATE once. The SELECT will not be rerun automatically.".to_owned());
            }
            effects
        }
        Action::Dismiss | Action::Cancel => {
            model.cell_update_notice = None;
            Vec::new()
        }
        Action::Quit => {
            model.should_quit = true;
            vec![Effect::Quit]
        }
        _ => {
            // Review owns the keyboard so navigation cannot change the row
            // underneath the statement shown for confirmation.
            model.pending_update = Some(pending);
            Vec::new()
        }
    }
}

/// Returns whether a value-free update candidate still names the current cell.
fn update_candidate_is_current(model: &Model, candidate: &UpdateCandidate) -> bool {
    if model.focus != Focus::Results
        || model.phase.is_busy()
        || model.plan.is_visible()
        || model.last_execution.as_ref().map(|execution| execution.job)
            != Some(candidate.result_job)
        || model.selected_source_row() != Some(candidate.source_row)
        || model.selected_column != candidate.result_column
        || model.last_sql.as_deref() != Some(candidate.source_sql.as_str())
    {
        return false;
    }
    model.visible_result().is_some_and(|set| {
        model
            .result_grid
            .visible_columns(set.columns.len())
            .contains(&candidate.result_column)
            && set
                .rows
                .get(candidate.source_row)
                .and_then(|row| row.get(candidate.result_column))
                .is_some()
    })
}

/// Starts an explicit refresh from the retained result source.
fn begin_refresh_result(model: &mut Model) -> Vec<Effect> {
    let source = match crate::app::discovery::retained_result_refresh_source(model) {
        Ok(source) => source,
        Err(reason) => {
            model.refresh_notice = Some(reason);
            return Vec::new();
        }
    };

    // Keep this marker through the existing named-parameter prompt. It is
    // cleared when the prompt is dismissed or when execution starts, so a
    // later ordinary run cannot inherit refresh semantics.
    model.refresh_pending = true;
    model.refresh_notice = Some(
        "Refreshing the retained result; the editor is unchanged and nothing is sent until any parameters are entered."
            .to_owned(),
    );
    let effects = start(model, source, None);
    if model.parameter_prompt.is_none() && effects.is_empty() {
        model.refresh_pending = false;
        model.refresh_notice =
            Some("The retained result refresh could not be started; nothing was sent.".to_owned());
    }
    effects
}

/// Starts the value-free source analysis and live metadata lookup for a cell.
fn begin_cell_update(model: &mut Model) -> Vec<Effect> {
    if model.focus != Focus::Results
        || model.phase.is_busy()
        || model.plan.is_visible()
        || model.visible_result().is_none()
    {
        model.cell_update_notice =
            Some("Focus Results on a retained cell before generating an UPDATE.".to_owned());
        return Vec::new();
    }
    if model.environment().is_production() {
        model.cell_update_notice = Some(
            "Cell updates are refused on production-classified connections; nothing was sent."
                .to_owned(),
        );
        return Vec::new();
    }
    if model.connection.info().is_some_and(|info| info.read_only) {
        model.cell_update_notice =
            Some("Cell updates are refused on a read-only session; nothing was sent.".to_owned());
        return Vec::new();
    }
    if !model.connection.is_usable() {
        model.cell_update_notice = Some(
            "The connection is not usable; nothing was sent. Reconnect before generating an UPDATE."
                .to_owned(),
        );
        return Vec::new();
    }
    if model.transaction == crate::query::result::TransactionState::Failed {
        model.cell_update_notice =
            Some("The transaction is failed; recover it before generating an UPDATE.".to_owned());
        return Vec::new();
    }

    let Some(result_job) = model.last_execution.as_ref().map(|execution| execution.job) else {
        model.cell_update_notice =
            Some("There is no retained execution to update from.".to_owned());
        return Vec::new();
    };
    let Some(source_row) = model.selected_source_row() else {
        model.cell_update_notice =
            Some("Select a retained result row before generating an UPDATE.".to_owned());
        return Vec::new();
    };
    let Some(source_sql) = model.last_sql.clone() else {
        model.cell_update_notice = Some(
            "The retained result has no SQL source, so a safe UPDATE cannot be generated."
                .to_owned(),
        );
        return Vec::new();
    };
    let Some(set) = model.visible_result() else {
        model.cell_update_notice = Some("There is no retained result to update from.".to_owned());
        return Vec::new();
    };
    let visible = model.result_grid.visible_columns(set.columns.len());
    if !visible.contains(&model.selected_column)
        || set
            .rows
            .get(source_row)
            .and_then(|row| row.get(model.selected_column))
            .is_none()
    {
        model.cell_update_notice =
            Some("Select a visible result cell before generating an UPDATE.".to_owned());
        return Vec::new();
    }
    let source =
        match crate::query::parse_update_source(&source_sql, &set.columns, model.selected_column) {
            Ok(source) => source,
            Err(error) => {
                model.cell_update_notice = Some(error.to_string());
                return Vec::new();
            }
        };
    let request = model.allocate_update_request();
    let candidate = UpdateCandidate {
        result_job,
        source_row,
        result_column: model.selected_column,
        source_sql,
    };
    model.update_lookup = Some(UpdateLookup {
        request,
        candidate,
        source: source.clone(),
    });
    model.cell_update_notice = Some(format!(
        "Reading primary-key metadata for {}.{}.",
        crate::postgres::metadata::quote_identifier(
            source.relation.schema.as_deref().unwrap_or("search_path")
        ),
        crate::postgres::metadata::quote_identifier(&source.relation.relation)
    ));
    vec![Effect::LoadUpdateTarget {
        request,
        relation: source.relation,
    }]
}

/// Applies a live metadata response to the still-current update candidate.
fn finish_update_target(
    model: &mut Model,
    request: u64,
    result: Result<crate::postgres::metadata::UpdateRelation, crate::diagnostics::Diagnostic>,
) -> Vec<Effect> {
    if model
        .update_lookup
        .as_ref()
        .is_none_or(|lookup| lookup.request != request)
    {
        return Vec::new();
    }
    let lookup = model
        .update_lookup
        .take()
        .expect("checked update lookup identity");
    if !update_candidate_is_current(model, &lookup.candidate) {
        model.cell_update_notice =
            Some("The result or selection changed; the cell update was discarded.".to_owned());
        return Vec::new();
    }
    let relation = match result {
        Ok(relation) => relation,
        Err(error) => {
            model.cell_update_notice = Some(format!(
                "Cell update metadata is unavailable: {}",
                error.headline
            ));
            return Vec::new();
        }
    };
    if !matches!(
        relation.kind,
        crate::postgres::metadata::ObjectKind::Table
            | crate::postgres::metadata::ObjectKind::PartitionedTable
    ) {
        model.cell_update_notice = Some(format!(
            "Cell updates are supported only for ordinary or partitioned tables, not {}.",
            relation.kind.singular()
        ));
        return Vec::new();
    }
    if !relation.readable {
        model.cell_update_notice = Some(
            "The current role can no longer read the resolved relation; nothing was sent."
                .to_owned(),
        );
        return Vec::new();
    }
    if !relation.writable {
        model.cell_update_notice = Some(
            "The current role cannot UPDATE the resolved relation; nothing was sent.".to_owned(),
        );
        return Vec::new();
    }
    model.update_prompt = Some(UpdatePrompt::new(lookup.candidate, lookup.source, relation));
    model.cell_update_notice = None;
    Vec::new()
}

/// Starts a plain plan immediately or stages an analyzed plan for confirmation.
fn begin_plan(model: &mut Model, analyze: bool) -> Vec<Effect> {
    if !model.connection.is_usable()
        || model.phase.is_busy()
        || model.transaction == crate::query::result::TransactionState::Failed
    {
        return Vec::new();
    }
    let Some(source) = error_location::source_at_cursor(model.editor.text(), model.editor.cursor())
    else {
        return Vec::new();
    };
    let sql = model.editor.text()[source.start..source.end]
        .trim()
        .to_owned();
    if sql.is_empty() || is_explain_statement(&sql) {
        return Vec::new();
    }
    let parsed = statements::split(&sql);
    if parsed.len() != 1 {
        return Vec::new();
    }
    let impact = crate::query::classify_all(&parsed);
    if analyze {
        let required = model
            .connection
            .info()
            .map_or_else(String::new, |info| info.database.clone());
        model.pending_plan = Some(crate::app::model::PendingPlan {
            sql,
            impact,
            typed: String::new(),
            required,
        });
        return Vec::new();
    }
    start_plan(model, sql, false, impact)
}

/// Starts a plan request after its safety gate has been satisfied.
fn start_plan(
    model: &mut Model,
    sql: String,
    analyzed: bool,
    _impact: crate::query::Impact,
) -> Vec<Effect> {
    if !model.connection.is_usable()
        || model.phase.is_busy()
        || model.transaction == crate::query::result::TransactionState::Failed
    {
        return Vec::new();
    }
    let parsed = statements::split(&sql);
    if parsed.len() != 1 || is_explain_statement(&sql) {
        return Vec::new();
    }
    let job = model.allocate_job();
    model.plan = crate::app::PlanView::loading(job, analyzed);
    model.pending_copy = None;
    model.clipboard_notice = None;
    model.update_lookup = None;
    model.update_prompt = None;
    model.pending_update = None;
    model.cell_update_notice = None;
    model.refresh_notice = None;
    model.running_sql = None;
    model.running_parameterized = false;
    model.running_cell_update = false;
    model.running_refresh = false;
    model.refresh_pending = false;
    model.running_editor_revision = None;
    model.running_source = None;
    model.focus = Focus::Results;
    model.phase = QueryPhase::Running { job, statements: 1 };
    model.running_for = Some(std::time::Duration::ZERO);
    model.error = None;
    clear_error_location(model);
    vec![Effect::Explain {
        job,
        sql,
        analyze: analyzed,
    }]
}

/// Completes a plan request without touching ordinary result or history state.
fn finish_plan(model: &mut Model, execution: crate::app::PlanExecution) -> Vec<Effect> {
    if model.phase.job() != Some(execution.job) || model.plan.job() != Some(execution.job) {
        return Vec::new();
    }
    model.phase = QueryPhase::Idle;
    model.running_for = None;
    model.transaction = execution.transaction;
    model.focus = Focus::Results;

    if execution.connection_lost {
        if let Some(info) = model.connection.info() {
            model.connection = ConnectionState::Lost {
                info: Box::new(info.clone()),
            };
        }
        model.plan.failed(
            execution.analyzed,
            "the connection was lost while reading the query plan",
            "reconnect and check the session before trying again; nothing was retried",
        );
        return Vec::new();
    }

    match execution.result {
        Ok(document) => model.plan.ready(document),
        Err(error) => model.plan.failed(
            execution.analyzed,
            error.headline,
            error
                .next_action
                .unwrap_or_else(|| "correct the statement, then try the plan again".to_owned()),
        ),
    }
    Vec::new()
}

/// Whether the statement is already a generated or user-authored EXPLAIN.
fn is_explain_statement(sql: &str) -> bool {
    let uncommented = statements::strip_comments(sql);
    uncommented
        .split_whitespace()
        .next()
        .is_some_and(|word| word.eq_ignore_ascii_case("explain"))
}

/// Starts an execution, refusing when there is nothing to run or nowhere to run
/// it, and holding it back when the target is production and it is not a read.
fn run(model: &mut Model, sql: String, source: Option<StatementSource>) -> Vec<Effect> {
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
            source,
        });
        return Vec::new();
    }

    start(model, sql, source)
}

/// Begins an execution that has already been allowed.
fn start(model: &mut Model, sql: String, source: Option<StatementSource>) -> Vec<Effect> {
    if !model.connection.is_usable() || model.phase.is_busy() || sql.trim().is_empty() {
        return Vec::new();
    }
    let parsed = statements::split(&sql);
    if parsed.is_empty() {
        return Vec::new();
    }
    if !model.refresh_pending {
        model.refresh_notice = None;
    }
    let template = match crate::query::discover_parameters(&sql) {
        Ok(template) => template,
        Err(error) => {
            model.error = Some(error.diagnostic("discovering named parameters"));
            clear_error_location(model);
            return Vec::new();
        }
    };
    if !template.is_empty() {
        model.parameter_prompt = Some(ParameterPrompt::new(sql, source, template.names().to_vec()));
        model.error = None;
        clear_error_location(model);
        return Vec::new();
    }
    start_with_parameters(model, sql, source, None)
}

/// Begins an execution after a parameter prompt has supplied complete values.
fn start_parameterized(
    model: &mut Model,
    sql: String,
    source: Option<StatementSource>,
    parameters: crate::query::ParameterBindings,
) -> Vec<Effect> {
    start_with_parameters(model, sql, source, Some(parameters))
}

/// Begins an ordinary or parameterized execution after all local gates pass.
fn start_with_parameters(
    model: &mut Model,
    sql: String,
    source: Option<StatementSource>,
    parameters: Option<crate::query::ParameterBindings>,
) -> Vec<Effect> {
    if !model.connection.is_usable() || model.phase.is_busy() || sql.trim().is_empty() {
        return Vec::new();
    }
    let parsed = statements::split(&sql);
    if parsed.is_empty() {
        return Vec::new();
    }
    let refreshing = model.refresh_pending;
    model.refresh_pending = false;
    let job = model.allocate_job();
    model.plan.clear();
    model.pending_plan = None;
    model.pending_copy = None;
    model.update_lookup = None;
    model.update_prompt = None;
    model.pending_update = None;
    model.cell_update_notice = None;
    model.refresh_notice = refreshing.then(|| {
        "Refreshing the retained result; the editor is unchanged and the request will run once."
            .to_owned()
    });
    model.clipboard_notice = None;
    model.parameter_prompt = None;
    model.running_sql = Some(sql.clone());
    model.running_parameterized = parameters.is_some();
    model.running_cell_update = false;
    model.running_refresh = refreshing;
    model.running_editor_revision = Some(model.editor.revision());
    model.running_source = source;
    model.phase = QueryPhase::Running {
        job,
        statements: parsed.len(),
    };
    model.running_for = Some(std::time::Duration::ZERO);
    model.error = None;
    clear_error_location(model);
    match parameters {
        Some(parameters) => vec![Effect::ExecuteParameterized {
            job,
            sql,
            parameters,
        }],
        None => vec![Effect::Execute { job, sql }],
    }
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
            if model.plan.document().is_some() {
                match direction {
                    Direction::Up => model.plan.move_selection(-1),
                    Direction::Down => model.plan.move_selection(1),
                    Direction::Left | Direction::Right => {}
                }
                return;
            }
            let rows = model.displayed_rows().len();
            let columns = model.visible_result().map_or_else(Vec::new, |set| {
                model.result_grid.visible_columns(set.columns.len())
            });
            match direction {
                Direction::Up => model.selected_row = model.selected_row.saturating_sub(1),
                Direction::Down => {
                    model.selected_row = (model.selected_row + 1).min(rows.saturating_sub(1));
                }
                Direction::Left => {
                    move_result_column(model, &columns, -1);
                }
                Direction::Right => {
                    move_result_column(model, &columns, 1);
                }
            }
        }
    }
}

/// Moves through the visible source columns, keeping the selected column's
/// source identity intact when hidden columns create gaps.
fn move_result_column(model: &mut Model, columns: &[usize], delta: isize) {
    if columns.is_empty() {
        model.selected_column = 0;
        model.result_grid.horizontal_start = 0;
        return;
    }
    let current = columns
        .iter()
        .position(|index| *index == model.selected_column)
        .unwrap_or(0);
    let last = isize::try_from(columns.len().saturating_sub(1)).unwrap_or(0);
    let next = (isize::try_from(current).unwrap_or(0) + delta).clamp(0, last);
    let next = usize::try_from(next).unwrap_or(0);
    model.selected_column = columns[next];
    model.result_grid.horizontal_start = next;
}

/// The visible source column the grid controls should act on.
fn selected_grid_column(model: &Model) -> Option<usize> {
    let set = model.visible_result()?;
    let visible = model.result_grid.visible_columns(set.columns.len());
    visible
        .iter()
        .copied()
        .find(|index| *index == model.selected_column)
        .or_else(|| visible.first().copied())
}

/// Opens the named, contextual result-grid controls surface.
fn open_result_controls(model: &mut Model) -> Vec<Effect> {
    let Some(set) = model.visible_result() else {
        model.result_grid.note = Some("Run a query with rows before opening grid controls.".into());
        return Vec::new();
    };
    if model.focus != Focus::Results {
        model.result_grid.note = Some("Focus Results before opening result grid controls.".into());
        return Vec::new();
    }
    if set.columns.is_empty() {
        model.result_grid.note = Some("This result has no columns to shape.".into());
        return Vec::new();
    }
    model.palette = Some(crate::app::palette::Palette::over_result_controls(
        grid_control_entries(model),
    ));
    Vec::new()
}

/// Applies one view-only grid command and never returns an I/O effect.
fn apply_grid_command(model: &mut Model, command: GridCommand) -> Vec<Effect> {
    let Some(column_count) = model.visible_result().map(|set| set.columns.len()) else {
        model.result_grid.note = Some("There is no retained result to change.".into());
        return Vec::new();
    };
    if column_count == 0 {
        model.result_grid.note = Some("This result has no columns to shape.".into());
        return Vec::new();
    }

    match command {
        GridCommand::ChooseColumns => {
            model.palette = Some(crate::app::palette::Palette::over_result_columns(
                result_column_entries(model),
            ));
        }
        GridCommand::SortSelected => {
            let Some(column) = selected_grid_column(model) else {
                model.result_grid.note = Some("There is no visible result column to sort.".into());
                return Vec::new();
            };
            let selected_source_row = model.selected_source_row();
            model.result_grid.toggle_sort(column);
            restore_selected_source_row(model, selected_source_row);
        }
        GridCommand::WidenSelected | GridCommand::NarrowSelected => {
            let Some(column) = selected_grid_column(model) else {
                model.result_grid.note =
                    Some("There is no visible result column to resize.".into());
                return Vec::new();
            };
            let automatic = model
                .visible_result()
                .map_or(crate::app::grid::MIN_COLUMN_WIDTH, |set| {
                    crate::app::grid::automatic_column_width(set, column)
                });
            let step = isize::try_from(crate::app::grid::COLUMN_WIDTH_STEP).unwrap_or(4);
            let delta = if matches!(command, GridCommand::WidenSelected) {
                step
            } else {
                -step
            };
            let _ = model.result_grid.adjust_width(column, automatic, delta);
        }
        GridCommand::ToggleTypes => {
            let has_types = model
                .visible_result()
                .is_some_and(|set| set.column_types.iter().any(Option::is_some));
            model.result_grid.show_types = !model.result_grid.show_types;
            if model.result_grid.show_types && !has_types {
                model.result_grid.note = Some(
                    "Type information is unavailable for this result; values are not inferred."
                        .into(),
                );
            } else {
                model.result_grid.note = None;
            }
        }
        GridCommand::ToggleFreezeFirst => {
            if model.result_grid.visible_columns(column_count).is_empty() {
                model.result_grid.note =
                    Some("There is no visible result column to freeze.".into());
            } else {
                model.result_grid.freeze_first = !model.result_grid.freeze_first;
                model.result_grid.note = None;
            }
        }
        GridCommand::Reset => {
            let selected_source_row = model.selected_source_row();
            model.result_grid.reset_view();
            restore_selected_source_row(model, selected_source_row);
        }
    }
    Vec::new()
}

/// Toggles a source column chosen from the searchable chooser.
fn toggle_result_column(model: &mut Model, index: usize) {
    let Some(set) = model.visible_result() else {
        model.result_grid.note = Some("There is no retained result to change.".into());
        return;
    };
    let count = set.columns.len();
    let before = model.result_grid.visible_columns(count);
    let was_hidden = model.result_grid.hidden_columns.contains(&index);
    if !model.result_grid.toggle_column(index, count) {
        return;
    }
    let after = model.result_grid.visible_columns(count);
    if was_hidden {
        // Restoring a column makes it the thing the user is looking for.
        model.selected_column = index;
        model.result_grid.horizontal_start = after
            .iter()
            .position(|candidate| *candidate == index)
            .unwrap_or(0);
        return;
    }
    if !after.contains(&model.selected_column) {
        let old_position = before
            .iter()
            .position(|candidate| *candidate == model.selected_column)
            .unwrap_or(0);
        let new_position = old_position.min(after.len().saturating_sub(1));
        model.selected_column = after[new_position];
        model.result_grid.horizontal_start = new_position;
    }
}

/// Restores the selected source row after a view transition changes positions.
fn restore_selected_source_row(model: &mut Model, source_row: Option<usize>) {
    let rows = model.displayed_rows();
    model.selected_row = match source_row
        .and_then(|source| rows.iter().position(|candidate| *candidate == source))
    {
        Some(position) => position,
        None => model.selected_row.min(rows.len().saturating_sub(1)),
    };
}

/// Palette entries for the current grid's local actions.
fn grid_control_entries(model: &Model) -> Vec<crate::app::palette::PaletteEntry> {
    use crate::app::palette::{PaletteCommand, PaletteEntry};
    let Some(set) = model.visible_result() else {
        return Vec::new();
    };
    let selected = selected_grid_column(model).unwrap_or(0);
    let selected_name = set.columns.get(selected).map_or_else(
        || "none".to_owned(),
        |name| crate::query::value::sanitize_for_display(name),
    );
    let sort_detail = match model.result_grid.sort {
        Some(sort) if sort.column == selected => {
            format!("{} on column {}", sort.direction.label(), selected + 1)
        }
        Some(sort) => format!(
            "{} on column {}; selected column is {}",
            sort.direction.label(),
            sort.column + 1,
            selected + 1
        ),
        None => "original server order".to_owned(),
    };
    let types = if model.result_grid.show_types {
        "shown"
    } else {
        "hidden"
    };
    let freeze = if model.result_grid.freeze_first {
        "on"
    } else {
        "off"
    };
    let selected_detail = format!("selected column {}: {selected_name}", selected + 1);
    vec![
        PaletteEntry {
            label: "Sort selected column".into(),
            detail: sort_detail,
            group: "Result grid",
            command: PaletteCommand::Grid(GridCommand::SortSelected),
        },
        PaletteEntry {
            label: "Choose visible columns".into(),
            detail: format!("{} source columns", set.columns.len()),
            group: "Result grid",
            command: PaletteCommand::Grid(GridCommand::ChooseColumns),
        },
        PaletteEntry {
            label: "Widen selected column".into(),
            detail: selected_detail.clone(),
            group: "Result grid",
            command: PaletteCommand::Grid(GridCommand::WidenSelected),
        },
        PaletteEntry {
            label: "Narrow selected column".into(),
            detail: selected_detail,
            group: "Result grid",
            command: PaletteCommand::Grid(GridCommand::NarrowSelected),
        },
        PaletteEntry {
            label: "Show or hide type labels".into(),
            detail: format!("currently {types}"),
            group: "Result grid",
            command: PaletteCommand::Grid(GridCommand::ToggleTypes),
        },
        PaletteEntry {
            label: "Freeze first visible column".into(),
            detail: format!("currently {freeze}"),
            group: "Result grid",
            command: PaletteCommand::Grid(GridCommand::ToggleFreezeFirst),
        },
        PaletteEntry {
            label: "Reset result grid view".into(),
            detail: "automatic widths, shown columns, original order".into(),
            group: "Result grid",
            command: PaletteCommand::Grid(GridCommand::Reset),
        },
    ]
}

/// Palette entries for source columns, with duplicate labels kept distinct.
fn result_column_entries(model: &Model) -> Vec<crate::app::palette::PaletteEntry> {
    use crate::app::palette::{PaletteCommand, PaletteEntry};
    let Some(set) = model.visible_result() else {
        return Vec::new();
    };
    set.columns
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let visibility = if model.result_grid.hidden_columns.contains(&index) {
                "hidden"
            } else {
                "shown"
            };
            let type_name = set.column_type(index).map_or_else(
                || "type unavailable".to_owned(),
                crate::query::value::sanitize_for_display,
            );
            PaletteEntry {
                label: format!(
                    "Column {}: {}",
                    index + 1,
                    crate::query::value::sanitize_for_display(name)
                ),
                detail: format!("{visibility}  {type_name}"),
                group: "Result columns",
                command: PaletteCommand::ResultColumn(index),
            }
        })
        .collect()
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
                Some(crate::app::palette::PaletteCommand::ConnectionDetails) => {
                    model.connection_details = true;
                    Vec::new()
                }
                Some(crate::app::palette::PaletteCommand::ConnectProfile(profile)) => {
                    begin_connection(model, profile)
                }
                Some(crate::app::palette::PaletteCommand::ChooseExportFormat(format)) => {
                    begin_export_format(model, format)
                }
                Some(crate::app::palette::PaletteCommand::Open(name)) => {
                    model.loaded_query = Some(name.clone());
                    vec![Effect::LoadQuery { name }]
                }
                Some(crate::app::palette::PaletteCommand::Insert(text)) => {
                    model.focus = Focus::Editor;
                    edit_editor(model, |editor| {
                        for ch in text.chars() {
                            editor.insert(ch);
                        }
                    });
                    Vec::new()
                }
                Some(crate::app::palette::PaletteCommand::Grid(command)) => {
                    apply_grid_command(model, command)
                }
                Some(crate::app::palette::PaletteCommand::ResultColumn(index)) => {
                    toggle_result_column(model, index);
                    Vec::new()
                }
                None => Vec::new(),
            }
        }
        Action::Dismiss => {
            model.palette = None;
            // Whatever it was waiting for is no longer wanted, so its answer
            // will be discarded rather than reopening a palette nobody asked
            // for a second time.
            model.pending_dependencies = None;
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

/// Handles input while the read-only connection trust surface is open.
///
/// The panel is intentionally a quiet inspection state. Typing and navigation
/// are swallowed so a key cannot mutate an editor or selection hidden beneath
/// it; Escape closes one layer, and the existing quit/palette paths remain
/// available without requiring a second dismissal.
fn connection_details_action(model: &mut Model, action: Action) -> Vec<Effect> {
    match action {
        Action::Dismiss | Action::ToggleHelp => {
            model.connection_details = false;
            Vec::new()
        }
        Action::OpenPalette => {
            model.connection_details = false;
            model.palette = Some(crate::app::palette::Palette::over_commands(
                palette_entries(model),
                crate::app::discovery::palette_note(model),
            ));
            Vec::new()
        }
        Action::Quit => {
            model.connection_details = false;
            model.should_quit = true;
            vec![Effect::Quit]
        }
        // A trust panel must not leak keystrokes into the hidden editor.
        Action::Insert(_) | Action::Backspace => Vec::new(),
        // Other actions are deliberately ignored until the panel is closed.
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
                    edit_editor(model, |editor| {
                        for ch in name.chars() {
                            editor.insert(ch);
                        }
                    });
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

/// Builds the rows for the searchable connection picker.
fn connection_entries(model: &Model) -> Vec<crate::app::palette::PaletteEntry> {
    use crate::app::palette::{PaletteCommand, PaletteEntry};

    let mut entries = vec![PaletteEntry {
        label: "Use default connection settings".to_owned(),
        detail: "CLI, service file, environment and built-in defaults; no profile selected"
            .to_owned(),
        group: "Default",
        command: PaletteCommand::ConnectProfile(None),
    }];
    entries.extend(
        model
            .connection_profiles
            .iter()
            .map(|profile| PaletteEntry {
                label: profile.name.clone(),
                detail: profile.detail(),
                group: "Profiles",
                command: PaletteCommand::ConnectProfile(Some(profile.name.clone())),
            }),
    );
    entries
}

/// Clears facts belonging to the old server and starts one deliberate route.
///
/// The editor and local reading preferences survive a switch. Resolved targets
/// and credentials do not enter this function: the runtime receives only the
/// selected profile name and owns those values outside the model.
fn begin_connection(model: &mut Model, profile: Option<String>) -> Vec<Effect> {
    if model.phase.is_busy() || matches!(model.connection, ConnectionState::Connecting) {
        return Vec::new();
    }
    let summary = profile.as_deref().and_then(|name| {
        model
            .connection_profiles
            .iter()
            .find(|item| item.name == name)
    });
    model.credential_provider = summary.and_then(|item| item.auth.clone());
    model.credential_presentation = summary.and_then(|item| item.provider_presentation.clone());
    model.connection = ConnectionState::Connecting;
    model.phase = QueryPhase::Idle;
    model.last_execution = None;
    model.last_elapsed = None;
    model.running_for = None;
    model.running_sql = None;
    model.running_parameterized = false;
    model.running_cell_update = false;
    model.running_refresh = false;
    model.refresh_pending = false;
    model.running_editor_revision = None;
    model.running_source = None;
    model.notices.clear();
    model.transaction = crate::query::result::TransactionState::Unknown;
    model.error = None;
    model.error_expanded = false;
    clear_error_location(model);
    model.password_prompt = None;
    model.pending_run = None;
    model.pending_plan = None;
    model.pending_copy = None;
    model.update_lookup = None;
    model.update_prompt = None;
    model.pending_update = None;
    model.cell_update_notice = None;
    model.refresh_notice = None;
    model.clipboard_notice = None;
    model.inspector = None;
    model.definition = None;
    model.pending_dependencies = None;
    model.connection_details = false;
    model.result_filter.clear();
    model.result_filtering = false;
    model.selected_row = 0;
    model.selected_column = 0;
    model.result_grid.reset_for_result(&[]);
    model.plan.clear();
    model.tree.reset_for_connection();
    model.completion.reset_for_connection();
    model.metadata_link = crate::app::model::MetadataLink::Shared;
    model.focus = Focus::Editor;
    vec![Effect::ConnectProfile { profile }]
}

fn export_format_entries() -> Vec<crate::app::palette::PaletteEntry> {
    use crate::app::palette::{PaletteCommand, PaletteEntry};

    [
        ExportFormat::Csv,
        ExportFormat::Tsv,
        ExportFormat::Json,
        ExportFormat::Ndjson,
        ExportFormat::Markdown,
    ]
    .into_iter()
    .map(|format| PaletteEntry {
        label: format.label().to_owned(),
        detail: format!(
            "{}; suggested extension {}",
            format.description(),
            format.extension()
        ),
        group: "Formats",
        command: PaletteCommand::ChooseExportFormat(format),
    })
    .collect()
}

fn export_note(model: &Model, set: &crate::query::result::ResultSet, matching: usize) -> String {
    if set.is_truncated() || !model.result_filter.trim().is_empty() {
        format!(
            "{matching} row(s) ready: this is what is on screen, not the {} the server returned. Choose a format; --output writes the full result.",
            set.rows_seen
        )
    } else {
        format!(
            "{matching} row(s) ready from the retained result. Choose a format before naming the destination."
        )
    }
}

fn begin_export_format(model: &mut Model, format: ExportFormat) -> Vec<Effect> {
    let Some(note) = (|| {
        let set = model.visible_result()?;
        let matching = model.filtered_rows().len();
        (matching > 0).then(|| export_note(model, set, matching))
    })() else {
        return Vec::new();
    };
    model.name_prompt = Some(crate::app::model::NamePrompt::for_export(format, note));
    Vec::new()
}

/// The entries the palette offers: applicable commands, useful chord fallbacks,
/// then every loaded object.
fn palette_entries(model: &Model) -> Vec<crate::app::palette::PaletteEntry> {
    use crate::app::palette::{PaletteCommand, PaletteEntry};
    // Unit models are intentionally constructible without the interactive
    // runtime. Their empty snapshot uses the built-in keymap only as a test
    // compatibility fallback; the live model always carries the configured
    // snapshot supplied by `cli::interactive`.
    let snapshot = if model.keymap_snapshot.is_empty() {
        crate::ui::keymap::Keymap::new().snapshot()
    } else {
        model.keymap_snapshot.clone()
    };
    let mut entries: Vec<PaletteEntry> = vec![PaletteEntry {
        label: "Connection and auth details".to_owned(),
        detail: format!(
            "{}; trust, transport, and cloud identity",
            model.connection.label()
        ),
        group: "Session",
        command: PaletteCommand::ConnectionDetails,
    }];

    // A command appears once even when several active keys invoke it. The
    // detail keeps every active direct key, with configured replacements first.
    for action in crate::app::discovery::palette_actions(model) {
        let bindings: Vec<&crate::app::discovery::KeyBindingSnapshot> = snapshot
            .bindings
            .iter()
            .filter(|binding| binding.action == action)
            .collect();
        let Some(first) = bindings.first() else {
            continue;
        };
        let label = first.description.clone();
        let mut keys: Vec<&str> = Vec::new();
        for binding in &bindings {
            if !keys.contains(&binding.key.as_str()) {
                keys.push(binding.key.as_str());
            }
        }
        entries.push(PaletteEntry {
            label,
            detail: keys.join(" / "),
            group: crate::app::discovery::palette_group(&action),
            command: PaletteCommand::Run(action),
        });
    }

    // Chords are commands too. Someone who cannot remember the second key should
    // still be able to reach the thing by typing its name. Unavailable chord
    // actions remain visible with an explicit prerequisite rather than a
    // selectable row that appears to do nothing.
    for chord in &snapshot.chords {
        if entries
            .iter()
            .any(|entry| entry.command == PaletteCommand::Run(chord.action.clone()))
        {
            continue;
        }
        let prefix = snapshot.prefix.as_deref().unwrap_or("Ctrl+K");
        let detail = if crate::app::discovery::action_is_available(model, &chord.action) {
            format!("{prefix} {}", chord.key)
        } else {
            format!(
                "{prefix} {}; {}",
                chord.key,
                crate::app::discovery::action_prerequisite(model, &chord.action)
                    .unwrap_or_else(|| "not useful in this state".to_owned())
            )
        };
        let label = if chord.action == Action::OpenConnectionPicker
            && model.connection_profiles.is_empty()
        {
            "Add a profile in config.toml".to_owned()
        } else {
            chord.description.clone()
        };
        entries.push(PaletteEntry {
            label,
            detail,
            group: crate::app::discovery::palette_group(&chord.action),
            command: PaletteCommand::Run(chord.action.clone()),
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
                group: "Objects",
                command: PaletteCommand::Insert(sql),
            });
        }
    }
    entries
}

#[cfg(test)]
mod tests;
