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
    ConnectingStep, ConnectionState, ExportFormat, Focus, LayoutNotice, Model, ParameterPrompt,
    PasteNotice, PendingUpdate, QueryPhase, UpdateCandidate, UpdateLookup, UpdatePrompt,
};
use crate::query::error_location::{self, StatementSource};
use crate::query::result::ExecutionStatus;
use crate::query::statements;

/// Applies a message to the model and returns the effects to perform.
pub fn update(model: &mut Model, message: Message) -> Vec<Effect> {
    let effects = match message {
        Message::ForConnection {
            generation,
            message,
        } => {
            if model.connection_generation != generation {
                return Vec::new();
            }
            update(model, *message)
        }
        Message::Action(action) => {
            model.banner.finish();
            model.mouse.drag = None;
            model.mouse.last_click = None;
            apply_action(model, action)
        }
        Message::InputAtSize {
            action,
            columns,
            rows,
        } => {
            model.banner.finish();
            model.mouse.drag = None;
            model.mouse.last_click = None;
            let changed = model.size != (columns, rows);
            let accepting_review = matches!(action, Action::Activate | Action::RunBuffer)
                && crate::ui::review::active_kind(model).is_some();
            if changed {
                update(model, Message::Resized(columns, rows));
            }
            if changed && accepting_review {
                // The newly observed geometry has not been painted yet. A
                // second explicit action follows the next frame and its gate.
                Vec::new()
            } else {
                apply_action(model, action)
            }
        }
        Message::Resized(columns, rows) => {
            let was_dragging = model.mouse.drag.take().is_some();
            model.mouse.last_click = None;
            if model.size != (columns, rows) {
                model.size = (columns, rows);
                model.review_scroll = 0;
                model.help_scroll = 0;
                if let Some(details) = model.notice_details.as_mut() {
                    details.scroll = 0;
                }
            }
            if was_dragging {
                clamp_splits(model);
            }
            Vec::new()
        }
        Message::Mouse {
            kind,
            column,
            row,
            modifiers,
        } => {
            model.banner.finish();
            handle_mouse(model, kind, column, row, modifiers)
        }
        Message::Pasted(text) => {
            model.mouse.drag = None;
            model.mouse.last_click = None;
            model.layout_notice = None;
            paste_action(model, text)
        }
        Message::Connected(info) => {
            let was_connecting = matches!(model.connection, ConnectionState::Connecting { .. });
            model.banner.finish();
            model.connection = ConnectionState::Connected(info);
            model.error = None;
            model.connecting_step = None;
            // The first connected frame after a real attempt is productive:
            // focus the editor and open the tree's first level as soon as the
            // catalogue arrives. A connection established by other means (a
            // test fixture or a restored session) does not steal focus.
            if was_connecting {
                model.focus = Focus::Editor;
                model.expand_first_schema_on_load = true;
            }
            clear_error_location(model);
            Vec::new()
        }
        Message::ConnectingStep { step, target } => {
            if !model.banner_played {
                model.banner_played = true;
                model.banner = crate::ui::widgets::banner::BannerState::new(
                    model.reduced_motion || model.presentation.reduced_motion,
                );
            }
            model.connection = ConnectionState::Connecting { step, target };
            model.connecting_step = Some(step);
            Vec::new()
        }
        Message::ConnectionFailed(diagnostic) => {
            model.banner.finish();
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
            }
            model.phase = QueryPhase::Idle;
            model.running_for = None;
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
            model.notices.clear();
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
                model.retained_data_loss =
                    crate::query::result::RetentionLossSummary::from_execution(&execution);
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
        Message::RetainedDataUnavailable => {
            refuse_unavailable_result(model);
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
        Message::NoticeSourceInstalled { source, session_id } => {
            model
                .notice_state
                .install(source, model.connection_generation, session_id);
            Vec::new()
        }
        Message::NoticeStatus {
            source,
            session_id,
            checkpoint,
            latest,
        } => {
            model.notice_state.observe(
                source,
                model.connection_generation,
                &session_id,
                checkpoint,
                latest,
            );
            Vec::new()
        }
        Message::NoticeAttemptReport {
            source,
            attempt_id,
            summary,
        } => {
            let index = source.index();
            if model.notice_state.last_attempt[index].as_ref() != Some(&attempt_id) {
                model.notice_state.last_attempt[index] = Some(attempt_id);
                model.notice_state.pending = model.notice_state.pending.plus(summary.counts);
                model.notice_state.previous = model.notice_state.previous.plus(summary.counts);
                if summary.counts.needs_report() {
                    model.notice_state.revision = model.notice_state.revision.saturating_add(1);
                }
            }
            Vec::new()
        }
        Message::NoticeUninstalledRetirementStarted(generation) => {
            if generation == model.connection_generation
                || model.notice_state.retiring_generation == Some(generation)
            {
                if model.notice_state.uninstalled_token_generation != Some(generation) {
                    model.notice_state.uninstalled_tokens = [None, None, None, None];
                    model.notice_state.uninstalled_token_generation = Some(generation);
                }
                model.notice_state.uninstalled_retirement = Some(generation);
            }
            Vec::new()
        }
        Message::NoticeUninstalledRetired {
            generation,
            session_id,
            counts,
        } => {
            if (generation == model.connection_generation
                || model.notice_state.retiring_generation == Some(generation))
                && model.notice_state.uninstalled_retirement == Some(generation)
                && !model
                    .notice_state
                    .uninstalled_tokens
                    .iter()
                    .flatten()
                    .any(|known| *known == session_id)
            {
                if let Some(empty) = model
                    .notice_state
                    .uninstalled_tokens
                    .iter_mut()
                    .find(|slot| slot.is_none())
                {
                    *empty = Some(session_id);
                    model.notice_state.pending = model.notice_state.pending.plus(counts);
                    model.notice_state.previous = model.notice_state.previous.plus(counts);
                    if counts.needs_report() {
                        model.notice_state.revision = model.notice_state.revision.saturating_add(1);
                    }
                } else {
                    model.notice_state.pending = crate::query::result::NoticeCounts::UNKNOWN;
                    model.notice_state.previous = crate::query::result::NoticeCounts::UNKNOWN;
                    model.notice_state.revision = model.notice_state.revision.saturating_add(1);
                }
            }
            Vec::new()
        }
        Message::NoticeUninstalledRetirementFinished(generation) => {
            if model.notice_state.uninstalled_retirement == Some(generation) {
                model.notice_state.uninstalled_retirement = None;
            }
            Vec::new()
        }
        Message::NoticeRetirementStarted(generation) => {
            if generation == model.connection_generation {
                model.notice_state.begin_retirement(generation);
            }
            Vec::new()
        }
        Message::NoticeRetiringSourceInstalled {
            generation,
            source,
            session_id,
        } => {
            if model.notice_state.retiring_generation == Some(generation)
                && model.notice_state.retirement_open[source.index()]
            {
                model.notice_state.install(source, generation, session_id);
            }
            Vec::new()
        }
        Message::NoticeRetired {
            generation,
            source,
            session_id,
            counts,
        } => {
            model
                .notice_state
                .retire(source, generation, &session_id, counts);
            Vec::new()
        }
        Message::NoticeRetirementFinished(generation) => {
            model.notice_state.finish_retirement(generation);
            Vec::new()
        }
        Message::SchemasLoaded(result) => {
            match *result {
                Ok(schemas) => model.tree.set_schemas(schemas),
                Err(error) => model.tree.set_error(error),
            }
            // Consume the one-shot only when there is a first level to open, so
            // a failed first load does not burn the promise.
            if model.expand_first_schema_on_load && !model.tree.roots.is_empty() {
                model.expand_first_schema_on_load = false;
                let _ = model.tree.expand_schema(&[0]);
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
            let tick_delta = match (model.running_for, running_for) {
                (Some(previous), Some(current)) if current > previous => {
                    current.saturating_sub(previous)
                }
                _ => std::time::Duration::from_millis(90),
            };
            model.advance_mouse_clock(tick_delta);
            model.frame = model.frame.wrapping_add(1);
            model.running_for = running_for;
            if matches!(model.connection, ConnectionState::Connecting { .. }) {
                if model.reduced_motion || model.presentation.reduced_motion {
                    model.banner.finish();
                } else {
                    model.banner.advance();
                }
            }
            Vec::new()
        }
        Message::SchemaDiagramLoaded { request, result } => {
            if request != model.schema_diagram_request {
                return Vec::new();
            }
            if let Some(diagram) = &mut model.schema_diagram {
                diagram.loading = false;
                match *result {
                    Ok(graph) => {
                        diagram.graph = graph;
                        if diagram.selected_table >= diagram.graph.tables.len() {
                            diagram.selected_table = diagram.graph.tables.len().saturating_sub(1);
                        }
                        diagram.rebuild_rendered(!model.presentation.glyphs.is_ascii());
                    }
                    Err(diagnostic) => {
                        diagram.error = Some(diagnostic.headline);
                    }
                }
            }
            Vec::new()
        }
        Message::ServerActivityLoaded { request, result } => {
            if request != model.server_activity_request {
                return Vec::new();
            }
            if let Some(activity) = &mut model.server_activity {
                activity.loading = false;
                match *result {
                    Ok(snapshot) => {
                        activity.snapshot = snapshot;
                        if activity.selected_backend >= activity.snapshot.backends.len() {
                            activity.selected_backend =
                                activity.snapshot.backends.len().saturating_sub(1);
                        }
                        activity.rebuild_rendered(!model.presentation.glyphs.is_ascii());
                    }
                    Err(diagnostic) => {
                        activity.error = Some(diagnostic.headline);
                    }
                }
            }
            Vec::new()
        }
        Message::SecurityAuditLoaded { request, result } => {
            if request != model.security_audit_request {
                return Vec::new();
            }
            if let Some(audit) = &mut model.security_audit {
                audit.loading = false;
                match *result {
                    Ok(snapshot) => {
                        audit.snapshot = snapshot;
                        let count = audit.item_count();
                        if audit.selected_index >= count {
                            audit.selected_index = count.saturating_sub(1);
                        }
                        audit.rebuild_rendered(!model.presentation.glyphs.is_ascii());
                    }
                    Err(diagnostic) => {
                        audit.error = Some(diagnostic.headline);
                    }
                }
            }
            Vec::new()
        }
        Message::SchemaDiffLoaded { request, result } => {
            if request != model.schema_diff_request {
                return Vec::new();
            }
            if let Some(diff_state) = &mut model.schema_diff {
                diff_state.loading = false;
                match *result {
                    Ok(diff) => {
                        diff_state.diff = Some(diff);
                        diff_state.error = None;
                        diff_state.rebuild(!model.presentation.glyphs.is_ascii());
                    }
                    Err(diagnostic) => {
                        diff_state.error = Some(diagnostic.headline);
                    }
                }
            }
            Vec::new()
        }
        Message::ImportPreviewLoaded { request, result } => {
            if request != model.import_wizard_request {
                return Vec::new();
            }
            if let Some(wizard) = &mut model.import_wizard {
                wizard.loading = false;
                match *result {
                    Ok(prev) => {
                        wizard.preview = Some(prev);
                        wizard.error = None;
                        wizard.rebuild_sql();
                    }
                    Err(diagnostic) => {
                        wizard.error = Some(diagnostic.headline);
                    }
                }
            }
            Vec::new()
        }
        Message::ImportExecuted { request, result } => {
            if request != model.import_wizard_request {
                return Vec::new();
            }
            if let Some(wizard) = &mut model.import_wizard {
                wizard.loading = false;
                match *result {
                    Ok(res) => {
                        wizard.result = Some(res);
                        wizard.error = None;
                    }
                    Err(diagnostic) => {
                        wizard.error = Some(diagnostic.headline);
                    }
                }
            }
            Vec::new()
        }
        Message::RoutinesLoaded { request, result } => {
            if request != model.routine_runner_request {
                return Vec::new();
            }
            if let Some(runner) = &mut model.routine_runner {
                runner.executing = false;
                if let Ok(routines) = *result {
                    runner.routines = routines;
                }
            }
            Vec::new()
        }
        Message::RoutineExecuted { request, result } => {
            if request != model.routine_runner_request {
                return Vec::new();
            }
            if let Some(runner) = &mut model.routine_runner {
                runner.executing = false;
                if let Ok(res) = *result {
                    runner.last_result = Some(res);
                    runner.step = crate::app::model::RoutineRunnerStep::Arguments;
                }
            }
            Vec::new()
        }
        Message::RoutineProfiled { request, result } => {
            if request != model.routine_runner_request {
                return Vec::new();
            }
            if let Some(runner) = &mut model.routine_runner {
                runner.executing = false;
                if let Ok(report) = *result {
                    runner.last_profile = Some(report);
                    runner.step = crate::app::model::RoutineRunnerStep::Profile;
                }
            }
            Vec::new()
        }
        Message::MaintenanceProgressLoaded { request, result } => {
            if request != model.maintenance_ops_request {
                return Vec::new();
            }
            if let Some(ops) = &mut model.maintenance_ops {
                ops.executing = false;
                match *result {
                    Ok(progress) => {
                        ops.progress = progress;
                        ops.status_message = Some("Progress updated".to_owned());
                    }
                    Err(diag) => {
                        ops.status_message = Some(format!("Error: {}", diag.headline));
                    }
                }
            }
            Vec::new()
        }
        Message::MaintenanceExecuted { request, result } => {
            if request != model.maintenance_ops_request {
                return Vec::new();
            }
            if let Some(ops) = &mut model.maintenance_ops {
                ops.executing = false;
                match *result {
                    Ok(res) => {
                        ops.status_message = Some(res.message.clone());
                        ops.last_result = Some(res);
                    }
                    Err(diag) => {
                        ops.status_message = Some(format!("Execution failed: {}", diag.headline));
                    }
                }
            }
            Vec::new()
        }
        Message::ChannelSubscribed {
            request,
            channel,
            result,
        } => {
            if request != model.maintenance_ops_request {
                return Vec::new();
            }
            if let Some(ops) = &mut model.maintenance_ops {
                ops.executing = false;
                match *result {
                    Ok(()) => {
                        if !ops.channels.contains(&channel) {
                            ops.channels.push(channel.clone());
                        }
                        ops.status_message = Some(format!("Subscribed to \"{channel}\""));
                    }
                    Err(diag) => {
                        ops.status_message = Some(format!("Subscribe failed: {}", diag.headline));
                    }
                }
            }
            Vec::new()
        }
        Message::NotificationDispatched {
            request,
            channel,
            result,
        } => {
            if request != model.maintenance_ops_request {
                return Vec::new();
            }
            if let Some(ops) = &mut model.maintenance_ops {
                ops.executing = false;
                match *result {
                    Ok(()) => {
                        ops.status_message = Some(format!("Notification sent to \"{channel}\""));
                    }
                    Err(diag) => {
                        ops.status_message = Some(format!("Notify failed: {}", diag.headline));
                    }
                }
            }
            Vec::new()
        }
        Message::NotificationsPolled {
            request,
            notifications,
        } => {
            if request != model.maintenance_ops_request {
                return Vec::new();
            }
            if let Some(ops) = &mut model.maintenance_ops {
                ops.executing = false;
                for notification in notifications {
                    ops.notifications.push(notification);
                }
            }
            Vec::new()
        }
        Message::QueryPerformanceLoaded { request, result } => {
            if request != model.query_performance_request {
                return Vec::new();
            }
            if let Some(perf) = &mut model.query_performance {
                perf.loading = false;
                match *result {
                    Ok(snapshot) => {
                        perf.snapshot = Some(snapshot);
                        perf.selected_idx = 0;
                    }
                    Err(diag) => {
                        perf.status_message = Some(format!(
                            "Failed to load performance telemetry: {}",
                            diag.headline
                        ));
                    }
                }
            }
            Vec::new()
        }
        Message::QueryPerformanceReset { request, result } => {
            if request != model.query_performance_request {
                return Vec::new();
            }
            if let Some(perf) = &mut model.query_performance {
                perf.loading = false;
                perf.confirm_reset = false;
                match *result {
                    Ok(()) => {
                        perf.status_message =
                            Some("Execution statistics reset successfully".to_owned());
                        model.query_performance_request =
                            model.query_performance_request.wrapping_add(1);
                        let req = model.query_performance_request;
                        let sort_order = perf.sort_order;
                        perf.loading = true;
                        return vec![Effect::LoadQueryPerformance {
                            request: req,
                            sort_order,
                            limit: crate::perf::DEFAULT_PERFORMANCE_LIMIT,
                        }];
                    }
                    Err(diag) => {
                        perf.status_message = Some(format!("Reset failed: {}", diag.headline));
                    }
                }
            }
            Vec::new()
        }
        Message::IndexAdvisorLoaded { request, result } => {
            if model.index_advisor_request != request {
                return Vec::new();
            }
            if let Some(advisor) = &mut model.index_advisor {
                advisor.loading = false;
                match *result {
                    Ok(report) => {
                        advisor.report = Some(report);
                        advisor.status_message = None;
                    }
                    Err(diag) => {
                        advisor.status_message =
                            Some(format!("Catalog analysis failed: {}", diag.headline));
                    }
                }
            }
            Vec::new()
        }
        Message::HypoIndexTested { request, result } => {
            if model.index_advisor_request != request {
                return Vec::new();
            }
            if let Some(advisor) = &mut model.index_advisor {
                advisor.loading = false;
                match *result {
                    Ok(hypo_result) => {
                        if let Some(report) = &mut advisor.report {
                            report.hypo_tests.push(hypo_result);
                        }
                        advisor.status_message = Some("Hypothetical index evaluated".to_owned());
                    }
                    Err(diag) => {
                        advisor.status_message =
                            Some(format!("HypoPG test failed: {}", diag.headline));
                    }
                }
            }
            Vec::new()
        }
        Message::WalObservabilityLoaded { request, result } => {
            if model.wal_observability_request != request {
                return Vec::new();
            }
            if let Some(wal) = &mut model.wal_observability {
                wal.loading = false;
                match *result {
                    Ok(report) => {
                        wal.report = Some(report);
                        wal.status_message = None;
                    }
                    Err(diag) => {
                        wal.status_message =
                            Some(format!("Telemetry fetch failed: {}", diag.headline));
                    }
                }
            }
            Vec::new()
        }
        Message::ConnectionPoolLoaded { request, result } => {
            if model.connection_pool_request != request {
                return Vec::new();
            }
            if let Some(pool) = &mut model.connection_pool {
                pool.loading = false;
                match *result {
                    Ok(report) => {
                        pool.report = Some(report);
                        pool.status_message = None;
                    }
                    Err(diag) => {
                        pool.status_message =
                            Some(format!("Pool telemetry fetch failed: {}", diag.headline));
                    }
                }
            }
            Vec::new()
        }
        Message::DataMaskingLoaded { request, result } => {
            if model.data_masking_request != request {
                return Vec::new();
            }
            if let Some(mask) = &mut model.data_masking {
                mask.loading = false;
                match *result {
                    Ok(report) => {
                        mask.report = Some(report);
                        mask.status_message = None;
                    }
                    Err(diag) => {
                        mask.status_message =
                            Some(format!("Mask analysis failed: {}", diag.headline));
                    }
                }
            }
            Vec::new()
        }
        Message::DataSyncLoaded { request, result } => {
            if model.data_sync_request != request {
                return Vec::new();
            }
            if let Some(sync) = &mut model.data_sync {
                sync.loading = false;
                match *result {
                    Ok(graph) => {
                        let plan = graph.compute_sync_plan();
                        sync.plan = Some(plan);
                        sync.graph = Some(graph);
                        sync.status_message = None;
                    }
                    Err(diag) => {
                        sync.status_message = Some(format!(
                            "Sync dependency discovery failed: {}",
                            diag.headline
                        ));
                    }
                }
            }
            Vec::new()
        }
        Message::SyntheticSeedLoaded { request, result } => {
            if model.synthetic_seed_request != request {
                return Vec::new();
            }
            if let Some(seed_state) = &mut model.synthetic_seed {
                seed_state.loading = false;
                match *result {
                    Ok(tables) => {
                        seed_state.tables = tables;
                        seed_state.regenerate();
                        seed_state.status_message = None;
                    }
                    Err(diag) => {
                        seed_state.status_message = Some(format!(
                            "Synthetic seed plan discovery failed: {}",
                            diag.headline
                        ));
                    }
                }
            }
            Vec::new()
        }
    };
    if crate::ui::widgets::overlay_blocks_panes(model) {
        model.mouse.drag = None;
    }
    if effects.iter().any(|effect| matches!(effect, Effect::Quit)) {
        model
            .notice_state
            .begin_retirement(model.connection_generation);
    }
    // All diagnostic origins share this safety boundary. Restore the saved
    // shape without stealing editor focus or changing transaction semantics.
    if model.focus != Focus::Results && needs_safety_details(model) {
        model.splits.zoomed = false;
    }
    effects
}

fn needs_safety_details(model: &Model) -> bool {
    model.error.is_some() || model.transaction == crate::query::result::TransactionState::Failed
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
        model.paste_notice = None;
    }
    model.sync_active_buffer_from_editor();
}

/// Formats the editor as one local, undoable edit.
fn format_buffer(model: &mut Model) {
    model.paste_notice = None;
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

/// Help owns reading/navigation, while explicit commands reveal their target
/// surface before using the existing handler and its safety checks.
fn help_action(model: &mut Model, action: Action) -> Vec<Effect> {
    let view = crate::ui::help::build(
        model,
        &model.presentation,
        ratatui::layout::Rect::new(0, 0, model.size.0, model.size.1),
    );
    if let Some(offset) = view.navigate(&action) {
        model.help_scroll = offset;
        return Vec::new();
    }
    match action {
        Action::ToggleHelp | Action::Dismiss | Action::Activate => {
            model.help_open = false;
            Vec::new()
        }
        Action::OpenPalette => {
            model.palette = Some(crate::app::palette::Palette::over_commands(
                palette_entries(model),
                crate::app::discovery::palette_note(model),
            ));
            Vec::new()
        }
        Action::BeginPrefix => {
            // Retain help until the continuation identifies the command.
            // Pane chords must still respect their existing modal no-op rule.
            model.prefix_pending = true;
            Vec::new()
        }
        Action::Insert(_)
        | Action::Backspace
        | Action::DeleteForward
        | Action::DeleteWordLeft
        | Action::Undo
        | Action::Redo
        | Action::Move(_)
        | Action::MovePage(_)
        | Action::MoveWord(_)
        | Action::FocusNext
        | Action::ShrinkPane
        | Action::GrowPane
        | Action::ZoomPane => Vec::new(),
        other => {
            model.help_open = false;
            apply_action(model, other)
        }
    }
}

fn switch_theme_action(model: &mut Model) -> Vec<Effect> {
    if model.password_prompt.is_some()
        || model.parameter_prompt.is_some()
        || model.name_prompt.is_some()
        || model.connection_details
        || model.prefix_pending
        || model.pending_run.is_some()
        || model.pending_plan.is_some()
        || model.pending_copy.is_some()
        || model.update_prompt.is_some()
        || model.pending_update.is_some()
        || model.palette.is_some()
        || model.help_open
        || pane_input_owned(model)
    {
        return Vec::new();
    }
    match model.theme_catalog.cycle(&model.presentation.theme) {
        Ok(theme) => model.presentation.theme = theme,
        Err(reason) => {
            let diagnostic = crate::diagnostics::Diagnostic::new(
                crate::diagnostics::DiagnosticKind::Internal,
                "the theme could not be switched",
                "switching the session theme",
            )
            .likely_cause(reason.to_string())
            .next_action("continue in the current theme, or save your SQL before restarting");
            if let Some(existing) = model.error.as_mut() {
                let note = crate::diagnostics::TechnicalField {
                    label: "Theme switching".into(),
                    value: diagnostic.headline,
                };
                if let Some(field) = existing
                    .technical
                    .iter_mut()
                    .find(|field| field.label == note.label)
                {
                    *field = note;
                } else {
                    existing.technical.push(note);
                }
            } else {
                model.error = Some(diagnostic);
            }
        }
    }
    Vec::new()
}

fn pane_input_owned(model: &Model) -> bool {
    model.notice_details.is_some()
        || model.inspector.is_some()
        || model.definition.is_some()
        || model.schema_diagram.is_some()
        || model.server_activity.is_some()
        || model.security_audit.is_some()
        || model.schema_diff.is_some()
        || model.import_wizard.is_some()
        || model.routine_runner.is_some()
        || model.maintenance_ops.is_some()
        || model.completion.menu.is_some()
        || model.tree.filtering
        || model.result_filtering
}

fn apply_action(model: &mut Model, action: Action) -> Vec<Effect> {
    if matches!(action, Action::SwitchTheme) {
        return switch_theme_action(model);
    }
    model.layout_notice = None;
    // Reshaping a pane must not dismiss an input owner and act underneath it.
    if (model.help_open || pane_input_owned(model))
        && matches!(
            action,
            Action::ShrinkPane | Action::GrowPane | Action::ZoomPane
        )
    {
        return Vec::new();
    }
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
    if model.notice_details.is_some() {
        return notice_details_action(model, action);
    }
    if model.help_open {
        return help_action(model, action);
    }
    // Keep the owner until the continuation identifies the command. Layout
    // actions then remain no-ops; other commands retain their normal handler.
    if matches!(action, Action::BeginPrefix) && pane_input_owned(model) {
        model.prefix_pending = true;
        return Vec::new();
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
    if model.schema_diagram.is_some() {
        return schema_diagram_action(model, action);
    }
    if model.server_activity.is_some() {
        return server_activity_action(model, action);
    }
    if model.security_audit.is_some() {
        return security_audit_action(model, action);
    }
    if model.schema_diff.is_some() {
        return schema_diff_action(model, action);
    }
    if model.import_wizard.is_some() {
        return import_wizard_action(model, action);
    }
    if model.routine_runner.is_some() {
        return routine_runner_action(model, action);
    }
    if model.maintenance_ops.is_some() {
        return maintenance_ops_action(model, action);
    }
    if model.query_performance.is_some() {
        return query_performance_action(model, action);
    }
    if model.index_advisor.is_some() {
        return index_advisor_action(model, action);
    }
    if model.wal_observability.is_some() {
        return wal_observability_action(model, action);
    }
    if model.connection_pool.is_some() {
        return connection_pool_action(model, action);
    }
    if model.data_masking.is_some() {
        return data_masking_action(model, action);
    }
    if model.data_sync.is_some() {
        return data_sync_action(model, action);
    }
    if model.synthetic_seed.is_some() {
        return synthetic_seed_action(model, action);
    }
    if model.snippet_library.is_some() {
        return snippet_library_action(model, action);
    }
    if model.lint_findings.is_some() {
        return lint_findings_action(model, action);
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
        Action::SwitchTheme => switch_theme_action(model),
        Action::ToggleHelp => {
            model.help_open = !model.help_open;
            if model.help_open {
                model.help_scroll = 0;
            }
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
        Action::ShrinkPane | Action::GrowPane | Action::ZoomPane => {
            adjust_panes(model, &action);
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
        Action::NarrowColumn if model.plan.is_visible() => Vec::new(),
        Action::NarrowColumn => apply_grid_command(model, GridCommand::NarrowSelected),
        Action::WidenColumn if model.plan.is_visible() => Vec::new(),
        Action::WidenColumn => apply_grid_command(model, GridCommand::WidenSelected),
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
            if refuse_unavailable_result(model) {
                return Vec::new();
            }
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
        Action::SchemaDiagram => schema_diagram_open(model),
        Action::ServerActivity => server_activity_open(model),
        Action::SecurityAudit => security_audit_open(model),
        Action::SchemaDiff => schema_diff_open(model),
        Action::ImportData => import_wizard_open(model),
        Action::RoutineRunner => routine_runner_open(model),
        Action::MaintenanceOps => maintenance_ops_open(model),
        Action::QueryPerformance => query_performance_open(model),
        Action::IndexAdvisor => index_advisor_open(model),
        Action::WalObservability => wal_observability_open(model),
        Action::ConnectionPool => connection_pool_open(model),
        Action::DataMasking => data_masking_open(model),
        Action::DataSync => data_sync_open(model),
        Action::SyntheticSeed => synthetic_seed_open(model),
        Action::NewEditorBuffer => {
            model.sync_active_buffer_from_editor();
            model.buffers.new_buffer(None, None);
            model.sync_editor_from_active_buffer();
            model.focus = Focus::Editor;
            Vec::new()
        }
        Action::CloseEditorBuffer => {
            model.buffers.close_active();
            model.sync_editor_from_active_buffer();
            Vec::new()
        }
        Action::NextEditorBuffer => {
            model.sync_active_buffer_from_editor();
            model.buffers.next_buffer();
            model.sync_editor_from_active_buffer();
            Vec::new()
        }
        Action::PrevEditorBuffer => {
            model.sync_active_buffer_from_editor();
            model.buffers.prev_buffer();
            model.sync_editor_from_active_buffer();
            Vec::new()
        }
        Action::ToggleEditorSplit => {
            model.sync_active_buffer_from_editor();
            model.buffers.cycle_split_mode();
            Vec::new()
        }
        Action::SnippetLibrary => snippet_library_open(model),
        Action::RestoreSession => restore_session_action(model),
        Action::DiscardRecovery => discard_recovery_action(model),
        Action::LintBuffer => lint_buffer_open(model),
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
        Action::Cancel => cancel_query(model),
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

/// The existing cancellation transition, also used by read-only notice recovery.
fn cancel_query(model: &mut Model) -> Vec<Effect> {
    match model.phase {
        QueryPhase::Running { job, .. } => {
            model.phase = QueryPhase::CancellationRequested { job };
            vec![Effect::Cancel { job }]
        }
        QueryPhase::CancellationRequested { .. } | QueryPhase::Idle => Vec::new(),
    }
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
        include_drop: false,
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
    let height = crate::ui::layout::definition_viewport(model.size, &model.presentation).1;
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
        Action::Insert('d' | 'D') => {
            if let Some(open) = model.definition.as_mut()
                && open.definition.is_some()
            {
                open.include_drop = !open.include_drop;
                open.scroll = 0;
            }
        }
        Action::Activate | Action::Insert('e' | 'E') => {
            if model
                .definition
                .as_ref()
                .is_some_and(|open| open.definition.is_some())
            {
                let Some(open) = model.definition.take() else {
                    return Vec::new();
                };
                if let Some(definition) = open.definition {
                    let script = definition.full_script(open.include_drop);
                    edit_editor(model, |editor| {
                        editor.set_text(script);
                    });
                    model.focus = Focus::Editor;
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

/// Opens the schema entity-relationship (ER) diagram overlay.
fn schema_diagram_open(model: &mut Model) -> Vec<Effect> {
    if !model.connection.is_ready() && model.completion.catalog().is_none() {
        model.schema_diagram_notice =
            Some("Connect to a database before opening the schema diagram.".to_owned());
        return Vec::new();
    }
    let unicode = !model.presentation.glyphs.is_ascii();
    let initial_graph = if let Some(catalog) = model.completion.catalog() {
        crate::diagram::SchemaGraph::from_completion_catalog(catalog)
    } else {
        crate::diagram::SchemaGraph::default()
    };
    model.schema_diagram_request = model.schema_diagram_request.wrapping_add(1);
    let request = model.schema_diagram_request;
    let mut state = crate::app::model::SchemaDiagramState::new(initial_graph, unicode);
    if model.connection.is_ready() {
        state.loading = true;
        model.schema_diagram = Some(state);
        vec![Effect::LoadSchemaDiagram {
            request,
            schema: None,
        }]
    } else {
        state.loading = false;
        model.schema_diagram = Some(state);
        Vec::new()
    }
}

/// Handles input while the schema ER diagram overlay is on screen.
fn schema_diagram_action(model: &mut Model, action: Action) -> Vec<Effect> {
    let unicode = !model.presentation.glyphs.is_ascii();
    match action {
        Action::Dismiss | Action::SchemaDiagram => {
            model.schema_diagram = None;
        }
        Action::Quit => {
            model.should_quit = true;
            return vec![Effect::Quit];
        }
        Action::Move(Direction::Up) => {
            if let Some(diagram) = &mut model.schema_diagram {
                diagram.select_prev(unicode);
            }
        }
        Action::Move(Direction::Down) => {
            if let Some(diagram) = &mut model.schema_diagram {
                diagram.select_next(unicode);
            }
        }
        Action::MovePage(Direction::Up) => {
            if let Some(diagram) = &mut model.schema_diagram {
                diagram.scroll_up(10);
            }
        }
        Action::MovePage(Direction::Down) => {
            if let Some(diagram) = &mut model.schema_diagram {
                diagram.scroll_down(10);
            }
        }
        Action::Move(Direction::Left) => {
            if let Some(diagram) = &mut model.schema_diagram {
                diagram.scroll_left(4);
            }
        }
        Action::Move(Direction::Right) => {
            if let Some(diagram) = &mut model.schema_diagram {
                diagram.scroll_right(4);
            }
        }
        Action::Insert('1') => {
            if let Some(diagram) = &mut model.schema_diagram {
                diagram.set_depth(crate::diagram::DiagramHopDepth::OneHop, unicode);
            }
        }
        Action::Insert('2') => {
            if let Some(diagram) = &mut model.schema_diagram {
                diagram.set_depth(crate::diagram::DiagramHopDepth::TwoHops, unicode);
            }
        }
        Action::Insert('a' | 'A') => {
            if let Some(diagram) = &mut model.schema_diagram {
                diagram.set_depth(crate::diagram::DiagramHopDepth::AllSchema, unicode);
            }
        }
        Action::Insert('m' | 'M') => {
            if let Some(diagram) = &mut model.schema_diagram {
                let next_format = if diagram.format == crate::diagram::DiagramFormat::Mermaid {
                    if unicode {
                        crate::diagram::DiagramFormat::Unicode
                    } else {
                        crate::diagram::DiagramFormat::Ascii
                    }
                } else {
                    crate::diagram::DiagramFormat::Mermaid
                };
                diagram.set_format(next_format, unicode);
            }
        }
        Action::Insert('u' | 'U') => {
            if let Some(diagram) = &mut model.schema_diagram {
                let format = if unicode {
                    crate::diagram::DiagramFormat::Unicode
                } else {
                    crate::diagram::DiagramFormat::Ascii
                };
                diagram.set_format(format, unicode);
            }
        }
        _ => {}
    }
    Vec::new()
}

/// Opens the server activity and locks dashboard overlay.
fn server_activity_open(model: &mut Model) -> Vec<Effect> {
    if !model.connection.is_ready() {
        return Vec::new();
    }
    let unicode = !model.presentation.glyphs.is_ascii();
    model.server_activity_request = model.server_activity_request.wrapping_add(1);
    let request = model.server_activity_request;
    let mut state = crate::app::model::ServerActivityState::new(
        crate::activity::ServerActivitySnapshot::default(),
        unicode,
    );
    state.loading = true;
    model.server_activity = Some(state);
    vec![Effect::LoadServerActivity { request }]
}

/// Handles input while the server activity and locks dashboard overlay is on screen.
fn server_activity_action(model: &mut Model, action: Action) -> Vec<Effect> {
    let unicode = !model.presentation.glyphs.is_ascii();
    match action {
        Action::Dismiss | Action::ServerActivity => {
            model.server_activity = None;
        }
        Action::Quit => {
            model.should_quit = true;
            return vec![Effect::Quit];
        }
        Action::Move(Direction::Up) => {
            if let Some(act) = &mut model.server_activity {
                act.select_prev();
            }
        }
        Action::Move(Direction::Down) => {
            if let Some(act) = &mut model.server_activity {
                act.select_next();
            }
        }
        Action::MovePage(Direction::Up) => {
            if let Some(act) = &mut model.server_activity {
                act.scroll_up(10);
            }
        }
        Action::MovePage(Direction::Down) => {
            if let Some(act) = &mut model.server_activity {
                act.scroll_down(10);
            }
        }
        Action::Move(Direction::Left) => {
            if let Some(act) = &mut model.server_activity {
                act.scroll_left(4);
            }
        }
        Action::Move(Direction::Right) => {
            if let Some(act) = &mut model.server_activity {
                act.scroll_right(4);
            }
        }
        Action::Insert('l' | 'L') => {
            if let Some(act) = &mut model.server_activity {
                act.toggle_mode(unicode);
            }
        }
        Action::Insert('r' | 'R') | Action::ReloadObjects | Action::RefreshResult => {
            if model.connection.is_ready() {
                model.server_activity_request = model.server_activity_request.wrapping_add(1);
                let request = model.server_activity_request;
                if let Some(act) = &mut model.server_activity {
                    act.loading = true;
                    act.error = None;
                }
                return vec![Effect::LoadServerActivity { request }];
            }
        }
        Action::Insert('c' | 'C') | Action::Activate => {
            let sql = model
                .server_activity
                .as_ref()
                .and_then(|act| act.selected_process())
                .map(|proc| {
                    crate::activity::ServerActivitySnapshot::reviewed_backend_signal_sql(
                        proc, false,
                    )
                });
            if let Some(sql) = sql {
                edit_editor(model, |editor| {
                    editor.set_text(sql);
                });
                model.server_activity = None;
                model.focus = Focus::Editor;
            }
        }
        Action::Insert('x' | 'X') => {
            let sql = model
                .server_activity
                .as_ref()
                .and_then(|act| act.selected_process())
                .map(|proc| {
                    crate::activity::ServerActivitySnapshot::reviewed_backend_signal_sql(proc, true)
                });
            if let Some(sql) = sql {
                edit_editor(model, |editor| {
                    editor.set_text(sql);
                });
                model.server_activity = None;
                model.focus = Focus::Editor;
            }
        }
        _ => {}
    }
    Vec::new()
}

/// Opens the roles, privilege grants, and RLS auditor overlay.
fn security_audit_open(model: &mut Model) -> Vec<Effect> {
    if !model.connection.is_ready() {
        return Vec::new();
    }
    let unicode = !model.presentation.glyphs.is_ascii();
    model.security_audit_request = model.security_audit_request.wrapping_add(1);
    let request = model.security_audit_request;
    let mut state = crate::app::model::SecurityAuditState::new(
        crate::audit::SecurityAuditSnapshot::default(),
        unicode,
    );
    state.loading = true;
    model.security_audit = Some(state);
    vec![Effect::LoadSecurityAudit { request }]
}

/// Handles input while the roles, privilege grants, and RLS auditor overlay is on screen.
fn security_audit_action(model: &mut Model, action: Action) -> Vec<Effect> {
    let unicode = !model.presentation.glyphs.is_ascii();
    match action {
        Action::Dismiss | Action::SecurityAudit => {
            model.security_audit = None;
        }
        Action::Quit => {
            model.should_quit = true;
            return vec![Effect::Quit];
        }
        Action::Move(Direction::Up) => {
            if let Some(audit) = &mut model.security_audit {
                audit.select_prev();
            }
        }
        Action::Move(Direction::Down) => {
            if let Some(audit) = &mut model.security_audit {
                audit.select_next();
            }
        }
        Action::MovePage(Direction::Up) => {
            if let Some(audit) = &mut model.security_audit {
                audit.scroll_up(10);
            }
        }
        Action::MovePage(Direction::Down) => {
            if let Some(audit) = &mut model.security_audit {
                audit.scroll_down(10);
            }
        }
        Action::Move(Direction::Left) => {
            if let Some(audit) = &mut model.security_audit {
                audit.scroll_left(4);
            }
        }
        Action::Move(Direction::Right) => {
            if let Some(audit) = &mut model.security_audit {
                audit.scroll_right(4);
            }
        }
        Action::Insert('s' | 'S') | Action::FocusNext => {
            if let Some(audit) = &mut model.security_audit {
                audit.cycle_section(unicode);
            }
        }
        Action::Insert('1') => {
            if let Some(audit) = &mut model.security_audit {
                audit.set_section(crate::audit::AuditSection::Roles, unicode);
            }
        }
        Action::Insert('2') => {
            if let Some(audit) = &mut model.security_audit {
                audit.set_section(crate::audit::AuditSection::RlsPolicies, unicode);
            }
        }
        Action::Insert('3') => {
            if let Some(audit) = &mut model.security_audit {
                audit.set_section(crate::audit::AuditSection::Grants, unicode);
            }
        }
        Action::Insert('r' | 'R') | Action::ReloadObjects | Action::RefreshResult => {
            if model.connection.is_ready() {
                model.security_audit_request = model.security_audit_request.wrapping_add(1);
                let request = model.security_audit_request;
                if let Some(audit) = &mut model.security_audit {
                    audit.loading = true;
                    audit.error = None;
                }
                return vec![Effect::LoadSecurityAudit { request }];
            }
        }
        Action::Insert('e' | 'E') | Action::Activate => {
            let sql = model
                .security_audit
                .as_ref()
                .and_then(crate::app::model::SecurityAuditState::drafted_sql);
            if let Some(sql) = sql {
                edit_editor(model, |editor| {
                    editor.set_text(sql);
                });
                model.security_audit = None;
                model.focus = Focus::Editor;
            }
        }
        _ => {}
    }
    Vec::new()
}

fn schema_diff_open(model: &mut Model) -> Vec<Effect> {
    if model.schema_diff.is_some() {
        model.schema_diff = None;
        return Vec::new();
    }
    if !model.connection.is_ready() {
        return Vec::new();
    }
    let unicode = !model.presentation.glyphs.is_ascii();
    model.schema_diff_request = model.schema_diff_request.wrapping_add(1);
    let request = model.schema_diff_request;
    let mut state = crate::app::model::SchemaDiffState::new(
        None,
        "public".to_owned(),
        "public".to_owned(),
        unicode,
    );
    state.loading = true;
    model.schema_diff = Some(state);
    vec![Effect::LoadSchemaDiff {
        request,
        source_schema: "public".to_owned(),
        target_schema: "public".to_owned(),
    }]
}

/// Handles input while the schema diff, table designer, and migration generator overlay is on screen.
fn schema_diff_action(model: &mut Model, action: Action) -> Vec<Effect> {
    let unicode = !model.presentation.glyphs.is_ascii();
    match action {
        Action::Dismiss | Action::SchemaDiff => {
            model.schema_diff = None;
        }
        Action::Quit => {
            model.should_quit = true;
            return vec![Effect::Quit];
        }
        Action::Move(Direction::Up) | Action::Insert('k' | 'K') => {
            if let Some(diff) = &mut model.schema_diff {
                if diff.active_tab == crate::app::model::SchemaDiffTab::TableDesigner {
                    diff.prev_column();
                } else {
                    diff.scroll_up(1);
                }
            }
        }
        Action::Move(Direction::Down) | Action::Insert('j' | 'J') => {
            if let Some(diff) = &mut model.schema_diff {
                if diff.active_tab == crate::app::model::SchemaDiffTab::TableDesigner {
                    diff.next_column();
                } else {
                    diff.scroll_down(1);
                }
            }
        }
        Action::MovePage(Direction::Up) => {
            if let Some(diff) = &mut model.schema_diff {
                diff.scroll_up(10);
            }
        }
        Action::MovePage(Direction::Down) => {
            if let Some(diff) = &mut model.schema_diff {
                diff.scroll_down(10);
            }
        }
        Action::Move(Direction::Left) | Action::Insert('h' | 'H') => {
            if let Some(diff) = &mut model.schema_diff {
                diff.scroll_left(4);
            }
        }
        Action::Move(Direction::Right) | Action::Insert('l' | 'L') => {
            if let Some(diff) = &mut model.schema_diff {
                diff.scroll_right(4);
            }
        }
        Action::Insert('s' | 'S') | Action::FocusNext => {
            if let Some(diff) = &mut model.schema_diff {
                diff.cycle_tab(unicode);
            }
        }
        Action::Insert('1') => {
            if let Some(diff) = &mut model.schema_diff {
                diff.set_tab(crate::app::model::SchemaDiffTab::DiffTree, unicode);
            }
        }
        Action::Insert('2') => {
            if let Some(diff) = &mut model.schema_diff {
                diff.set_tab(crate::app::model::SchemaDiffTab::TableDesigner, unicode);
            }
        }
        Action::Insert('3') => {
            if let Some(diff) = &mut model.schema_diff {
                diff.set_tab(crate::app::model::SchemaDiffTab::MigrationSql, unicode);
            }
        }
        Action::Insert('r' | 'R') | Action::ReloadObjects | Action::RefreshResult => {
            if model.connection.is_ready() {
                model.schema_diff_request = model.schema_diff_request.wrapping_add(1);
                let request = model.schema_diff_request;
                let (src, tgt) = if let Some(diff) = &mut model.schema_diff {
                    diff.loading = true;
                    diff.error = None;
                    (diff.source_schema.clone(), diff.target_schema.clone())
                } else {
                    ("public".to_owned(), "public".to_owned())
                };
                return vec![Effect::LoadSchemaDiff {
                    request,
                    source_schema: src,
                    target_schema: tgt,
                }];
            }
        }
        Action::Insert('e' | 'E') | Action::Activate => {
            let sql = model
                .schema_diff
                .as_ref()
                .map(crate::app::model::SchemaDiffState::drafted_sql);
            if let Some(sql) = sql {
                edit_editor(model, |editor| {
                    editor.set_text(sql);
                });
                model.schema_diff = None;
                model.focus = Focus::Editor;
            }
        }
        _ => {}
    }
    Vec::new()
}

fn import_wizard_open(model: &mut Model) -> Vec<Effect> {
    if model.import_wizard.is_some() {
        model.import_wizard = None;
        return Vec::new();
    }
    if !model.connection.is_ready() {
        return Vec::new();
    }
    let table = if let Some(selected) = model.tree.selected_row() {
        selected.label
    } else {
        "orders".to_owned()
    };
    let schema = "public".to_owned();
    let mut state = crate::app::model::ImportWizardState::new(schema.clone(), table.clone());
    let file_path = format!("{table}.csv");
    state.file_path = file_path.clone();
    state.rebuild_sql();
    model.import_wizard_request = model.import_wizard_request.wrapping_add(1);
    let request = model.import_wizard_request;
    state.loading = true;
    model.import_wizard = Some(state);
    vec![Effect::LoadImportPreview {
        request,
        file_path,
        schema,
        table,
    }]
}

/// Handles input while the bulk data import wizard overlay is on screen.
fn import_wizard_action(model: &mut Model, action: Action) -> Vec<Effect> {
    match action {
        Action::Dismiss | Action::ImportData => {
            model.import_wizard = None;
        }
        Action::Quit => {
            model.should_quit = true;
            return vec![Effect::Quit];
        }
        Action::Move(Direction::Up) | Action::Insert('k' | 'K') => {
            if let Some(wizard) = &mut model.import_wizard {
                if wizard.step == crate::app::model::ImportWizardStep::ColumnMapping {
                    wizard.move_column_selection(-1);
                } else {
                    wizard.scroll_y = wizard.scroll_y.saturating_sub(1);
                }
            }
        }
        Action::Move(Direction::Down) | Action::Insert('j' | 'J') => {
            if let Some(wizard) = &mut model.import_wizard {
                if wizard.step == crate::app::model::ImportWizardStep::ColumnMapping {
                    wizard.move_column_selection(1);
                } else {
                    wizard.scroll_y = wizard.scroll_y.saturating_add(1);
                }
            }
        }
        Action::Insert(' ') => {
            if let Some(wizard) = model
                .import_wizard
                .as_mut()
                .filter(|w| w.step == crate::app::model::ImportWizardStep::ColumnMapping)
            {
                wizard.toggle_column_skip();
            }
        }
        Action::Insert('d' | 'D') => {
            if let Some(wizard) = &mut model.import_wizard {
                wizard.dry_run = !wizard.dry_run;
                wizard.rebuild_sql();
            }
        }
        Action::Insert('s' | 'S') | Action::FocusNext => {
            if let Some(wizard) = &mut model.import_wizard {
                wizard.step = wizard.step.next();
            }
        }
        Action::Insert('1') => {
            if let Some(wizard) = &mut model.import_wizard {
                wizard.step = crate::app::model::ImportWizardStep::SourceFile;
            }
        }
        Action::Insert('2') => {
            if let Some(wizard) = &mut model.import_wizard {
                wizard.step = crate::app::model::ImportWizardStep::ColumnMapping;
            }
        }
        Action::Insert('3') => {
            if let Some(wizard) = &mut model.import_wizard {
                wizard.step = crate::app::model::ImportWizardStep::Execute;
            }
        }
        Action::Insert('r' | 'R') | Action::ReloadObjects | Action::RefreshResult => {
            if model.connection.is_ready() {
                model.import_wizard_request = model.import_wizard_request.wrapping_add(1);
                let request = model.import_wizard_request;
                if let Some(wizard) = &mut model.import_wizard {
                    wizard.loading = true;
                    wizard.error = None;
                    return vec![Effect::LoadImportPreview {
                        request,
                        file_path: wizard.file_path.clone(),
                        schema: wizard.schema.clone(),
                        table: wizard.table.clone(),
                    }];
                }
            }
        }
        Action::Insert('e' | 'E') => {
            let sql = model.import_wizard.as_ref().map(|w| w.copy_sql.clone());
            if let Some(sql) = sql {
                edit_editor(model, |editor| {
                    editor.set_text(sql);
                });
                model.import_wizard = None;
                model.focus = Focus::Editor;
            }
        }
        Action::Activate => {
            if let Some(wizard) = &mut model.import_wizard {
                if wizard.step == crate::app::model::ImportWizardStep::Execute {
                    wizard.loading = true;
                    wizard.error = None;
                    model.import_wizard_request = model.import_wizard_request.wrapping_add(1);
                    let request = model.import_wizard_request;
                    let options = crate::transfer::ImportOptions {
                        schema: wizard.schema.clone(),
                        table: wizard.table.clone(),
                        file_path: wizard.file_path.clone(),
                        format: wizard.format,
                        delimiter: wizard.delimiter,
                        has_header: wizard.has_header,
                        null_string: wizard.null_string.clone(),
                        quote: wizard.quote,
                        escape: wizard.escape,
                        columns: wizard.preview.as_ref().map_or(Vec::new(), |p| {
                            p.column_mappings
                                .iter()
                                .filter(|m| !m.skip)
                                .map(|m| m.target_column.clone())
                                .collect()
                        }),
                        dry_run: wizard.dry_run,
                        transactional: wizard.transactional,
                    };
                    return vec![Effect::ExecuteImport {
                        request,
                        options,
                        data: Vec::new(),
                    }];
                } else {
                    wizard.step = wizard.step.next();
                }
            }
        }
        _ => {}
    }
    Vec::new()
}

fn routine_runner_open(model: &mut Model) -> Vec<Effect> {
    if model.routine_runner.is_some() {
        model.routine_runner = None;
        return Vec::new();
    }
    if !model.connection.is_ready() {
        return Vec::new();
    }

    let state = crate::app::model::RoutineRunnerState {
        rollback_savepoint: true,
        executing: true,
        ..Default::default()
    };

    model.routine_runner_request = model.routine_runner_request.wrapping_add(1);
    let request = model.routine_runner_request;
    model.routine_runner = Some(state);

    vec![Effect::LoadRoutines {
        request,
        schema: None,
    }]
}

/// Handles input while the PL/pgSQL routine runner and profiler overlay is on screen.
fn routine_runner_action(model: &mut Model, action: Action) -> Vec<Effect> {
    match action {
        Action::Dismiss | Action::RoutineRunner => {
            model.routine_runner = None;
        }
        Action::Quit => {
            model.should_quit = true;
            return vec![Effect::Quit];
        }
        Action::Move(Direction::Up) | Action::Insert('k' | 'K') => {
            if let Some(runner) = &mut model.routine_runner {
                match runner.step {
                    crate::app::model::RoutineRunnerStep::Routines => {
                        runner.move_routine_selection(-1);
                    }
                    crate::app::model::RoutineRunnerStep::Arguments => {
                        runner.move_arg_selection(-1);
                    }
                    crate::app::model::RoutineRunnerStep::Profile => {
                        runner.scroll_y = runner.scroll_y.saturating_sub(1);
                    }
                }
            }
        }
        Action::Move(Direction::Down) | Action::Insert('j' | 'J') => {
            if let Some(runner) = &mut model.routine_runner {
                match runner.step {
                    crate::app::model::RoutineRunnerStep::Routines => {
                        runner.move_routine_selection(1);
                    }
                    crate::app::model::RoutineRunnerStep::Arguments => {
                        runner.move_arg_selection(1);
                    }
                    crate::app::model::RoutineRunnerStep::Profile => {
                        runner.scroll_y = runner.scroll_y.saturating_add(1);
                    }
                }
            }
        }
        Action::Insert('s' | 'S') => {
            if let Some(runner) = &mut model.routine_runner {
                runner.rollback_savepoint = !runner.rollback_savepoint;
            }
        }
        Action::FocusNext | Action::Insert('\t') => {
            if let Some(runner) = &mut model.routine_runner {
                runner.step = runner.step.next();
            }
        }
        Action::Insert('1') => {
            if let Some(runner) = &mut model.routine_runner {
                runner.step = crate::app::model::RoutineRunnerStep::Routines;
            }
        }
        Action::Insert('2') => {
            if let Some(runner) = &mut model.routine_runner {
                runner.step = crate::app::model::RoutineRunnerStep::Arguments;
            }
        }
        Action::Insert('3') => {
            if let Some(runner) = &mut model.routine_runner {
                runner.step = crate::app::model::RoutineRunnerStep::Profile;
            }
        }
        Action::Insert('r' | 'R') | Action::Activate => {
            if let Some((schema, name, args, rollback)) =
                model.routine_runner.as_ref().and_then(|r| {
                    r.selected_routine().map(|s| {
                        (
                            s.schema.clone(),
                            s.name.clone(),
                            r.args.clone(),
                            r.rollback_savepoint,
                        )
                    })
                })
            {
                if let Some(runner) = &mut model.routine_runner {
                    runner.executing = true;
                }
                model.routine_runner_request = model.routine_runner_request.wrapping_add(1);
                let request = model.routine_runner_request;
                let options = crate::routine::RoutineExecutionOptions {
                    schema,
                    name,
                    args,
                    rollback_savepoint: rollback,
                    timeout_ms: Some(30_000),
                };
                return vec![Effect::ExecuteRoutine { request, options }];
            }
        }
        Action::Insert('p' | 'P') => {
            if let Some((schema, name)) = model.routine_runner.as_ref().and_then(|r| {
                r.selected_routine()
                    .map(|s| (s.schema.clone(), s.name.clone()))
            }) {
                if let Some(runner) = &mut model.routine_runner {
                    runner.executing = true;
                }
                model.routine_runner_request = model.routine_runner_request.wrapping_add(1);
                let request = model.routine_runner_request;
                return vec![Effect::ProfileRoutine {
                    request,
                    schema,
                    name,
                }];
            }
        }
        Action::Insert('e' | 'E') => {
            if let Some(sql) = model.routine_runner.as_ref().and_then(|runner| {
                runner
                    .selected_routine()
                    .map(|routine| crate::routine::build_routine_call_sql(routine, &runner.args))
            }) {
                edit_editor(model, |editor| {
                    editor.set_text(sql);
                });
                model.routine_runner = None;
                model.focus = Focus::Editor;
            }
        }
        _ => {}
    }
    Vec::new()
}

fn maintenance_ops_open(model: &mut Model) -> Vec<Effect> {
    if model.maintenance_ops.is_some() {
        model.maintenance_ops = None;
        return Vec::new();
    }
    if !model.connection.is_ready() {
        return Vec::new();
    }

    let state = crate::app::model::MaintenanceOpsState {
        executing: true,
        ..Default::default()
    };

    model.maintenance_ops_request = model.maintenance_ops_request.wrapping_add(1);
    let request = model.maintenance_ops_request;
    model.maintenance_ops = Some(state);

    vec![
        Effect::LoadMaintenanceProgress { request },
        Effect::PollNotifications { request },
    ]
}

/// Handles input while the maintenance operations and pub/sub monitor overlay is on screen.
fn maintenance_ops_action(model: &mut Model, action: Action) -> Vec<Effect> {
    match action {
        Action::Dismiss | Action::MaintenanceOps => {
            model.maintenance_ops = None;
        }
        Action::Quit => {
            model.should_quit = true;
            return vec![Effect::Quit];
        }
        Action::Move(Direction::Up) | Action::Insert('k' | 'K') => {
            if let Some(ops) = &mut model.maintenance_ops {
                ops.move_selection(-1);
            }
        }
        Action::Move(Direction::Down) | Action::Insert('j' | 'J') => {
            if let Some(ops) = &mut model.maintenance_ops {
                ops.move_selection(1);
            }
        }
        Action::FocusNext | Action::Insert('\t') => {
            if let Some(ops) = &mut model.maintenance_ops {
                ops.step = ops.step.next();
            }
        }
        Action::Insert('1') => {
            if let Some(ops) = &mut model.maintenance_ops {
                ops.step = crate::app::model::MaintenanceOpsStep::PubSub;
            }
        }
        Action::Insert('2') => {
            if let Some(ops) = &mut model.maintenance_ops {
                ops.step = crate::app::model::MaintenanceOpsStep::Maintain;
            }
        }
        Action::Insert('3') => {
            if let Some(ops) = &mut model.maintenance_ops {
                ops.step = crate::app::model::MaintenanceOpsStep::Progress;
            }
        }
        Action::Insert('v' | 'V') => {
            if let Some(ops) = &mut model.maintenance_ops {
                ops.cycle_maintenance_kind();
            }
        }
        Action::Insert('p' | 'P') => {
            if let Some(ops) = &mut model.maintenance_ops {
                ops.confirm_production = !ops.confirm_production;
            }
        }
        Action::Insert('c' | 'C') => {
            if let Some(ops) = &mut model.maintenance_ops {
                ops.notifications.clear();
                ops.selected_idx = 0;
                ops.status_message = Some("Notifications cleared".to_owned());
            }
        }
        Action::Insert('l' | 'L') => {
            if let Some(ops) = &mut model.maintenance_ops {
                let channel = ops.channel_input.trim().to_owned();
                if !channel.is_empty() {
                    ops.executing = true;
                    model.maintenance_ops_request = model.maintenance_ops_request.wrapping_add(1);
                    let request = model.maintenance_ops_request;
                    return vec![Effect::SubscribeChannel { request, channel }];
                }
            }
        }
        Action::Insert('u' | 'U') => {
            if let Some(ops) = &mut model.maintenance_ops {
                let channel = ops.channel_input.trim().to_owned();
                if !channel.is_empty() {
                    ops.executing = true;
                    ops.channels.retain(|c| c != &channel);
                    model.maintenance_ops_request = model.maintenance_ops_request.wrapping_add(1);
                    let request = model.maintenance_ops_request;
                    return vec![Effect::UnsubscribeChannel { request, channel }];
                }
            }
        }
        Action::Insert('n' | 'N') => {
            if let Some(ops) = &mut model.maintenance_ops {
                let channel = ops.channel_input.trim().to_owned();
                if !channel.is_empty() {
                    ops.executing = true;
                    let payload = if ops.payload_input.trim().is_empty() {
                        None
                    } else {
                        Some(ops.payload_input.clone())
                    };
                    model.maintenance_ops_request = model.maintenance_ops_request.wrapping_add(1);
                    let request = model.maintenance_ops_request;
                    return vec![Effect::DispatchNotification {
                        request,
                        channel,
                        payload,
                    }];
                }
            }
        }
        Action::Insert('r' | 'R') | Action::Activate => {
            if let Some(ops) = &mut model.maintenance_ops {
                ops.executing = true;
                model.maintenance_ops_request = model.maintenance_ops_request.wrapping_add(1);
                let request = model.maintenance_ops_request;
                match ops.step {
                    crate::app::model::MaintenanceOpsStep::Maintain => {
                        let target = if ops.target_relation.trim().is_empty() {
                            None
                        } else {
                            Some(ops.target_relation.trim().to_owned())
                        };
                        let options = crate::ops::MaintenanceOptions {
                            kind: ops.maintenance_kind.clone(),
                            target,
                            confirm_production: ops.confirm_production,
                        };
                        return vec![Effect::ExecuteMaintenance { request, options }];
                    }
                    crate::app::model::MaintenanceOpsStep::Progress => {
                        return vec![Effect::LoadMaintenanceProgress { request }];
                    }
                    crate::app::model::MaintenanceOpsStep::PubSub => {
                        return vec![Effect::PollNotifications { request }];
                    }
                }
            }
        }
        Action::Insert('e' | 'E') => {
            if let Some(ops) = model.maintenance_ops.as_ref() {
                let target = if ops.target_relation.trim().is_empty() {
                    None
                } else {
                    Some(ops.target_relation.trim().to_owned())
                };
                let options = crate::ops::MaintenanceOptions {
                    kind: ops.maintenance_kind.clone(),
                    target,
                    confirm_production: ops.confirm_production,
                };
                if let Ok(sql) = crate::ops::build_maintenance_sql(&options) {
                    edit_editor(model, |editor| {
                        editor.set_text(sql);
                    });
                    model.maintenance_ops = None;
                    model.focus = Focus::Editor;
                }
            }
        }
        _ => {}
    }
    Vec::new()
}

fn query_performance_open(model: &mut Model) -> Vec<Effect> {
    if model.query_performance.is_some() {
        model.query_performance = None;
        return Vec::new();
    }
    if !model.connection.is_ready() {
        return Vec::new();
    }

    let state = crate::app::model::QueryPerformanceState {
        loading: true,
        ..Default::default()
    };

    model.query_performance_request = model.query_performance_request.wrapping_add(1);
    let request = model.query_performance_request;
    let sort_order = state.sort_order;
    model.query_performance = Some(state);

    vec![Effect::LoadQueryPerformance {
        request,
        sort_order,
        limit: crate::perf::DEFAULT_PERFORMANCE_LIMIT,
    }]
}

/// Handles input while the query performance and stat statements overlay is on screen.
fn query_performance_action(model: &mut Model, action: Action) -> Vec<Effect> {
    match action {
        Action::Dismiss | Action::QueryPerformance => {
            model.query_performance = None;
        }
        Action::Quit => {
            model.should_quit = true;
            return vec![Effect::Quit];
        }
        Action::Move(Direction::Up) | Action::Insert('k' | 'K') => {
            if let Some(perf) = &mut model.query_performance {
                perf.move_selection(-1);
            }
        }
        Action::Move(Direction::Down) | Action::Insert('j' | 'J') => {
            if let Some(perf) = &mut model.query_performance {
                perf.move_selection(1);
            }
        }
        Action::Insert('m' | 'M') => {
            if let Some(perf) = &mut model.query_performance {
                perf.sort_order = crate::perf::PerformanceSortOrder::MeanTime;
                perf.loading = true;
                model.query_performance_request = model.query_performance_request.wrapping_add(1);
                let request = model.query_performance_request;
                return vec![Effect::LoadQueryPerformance {
                    request,
                    sort_order: perf.sort_order,
                    limit: crate::perf::DEFAULT_PERFORMANCE_LIMIT,
                }];
            }
        }
        Action::Insert('t' | 'T') => {
            if let Some(perf) = &mut model.query_performance {
                perf.sort_order = crate::perf::PerformanceSortOrder::TotalTime;
                perf.loading = true;
                model.query_performance_request = model.query_performance_request.wrapping_add(1);
                let request = model.query_performance_request;
                return vec![Effect::LoadQueryPerformance {
                    request,
                    sort_order: perf.sort_order,
                    limit: crate::perf::DEFAULT_PERFORMANCE_LIMIT,
                }];
            }
        }
        Action::Insert('c' | 'C') => {
            if let Some(perf) = &mut model.query_performance {
                perf.sort_order = crate::perf::PerformanceSortOrder::Calls;
                perf.loading = true;
                model.query_performance_request = model.query_performance_request.wrapping_add(1);
                let request = model.query_performance_request;
                return vec![Effect::LoadQueryPerformance {
                    request,
                    sort_order: perf.sort_order,
                    limit: crate::perf::DEFAULT_PERFORMANCE_LIMIT,
                }];
            }
        }
        Action::Insert('r' | 'R') => {
            if let Some(perf) = &mut model.query_performance {
                perf.sort_order = crate::perf::PerformanceSortOrder::Rows;
                perf.loading = true;
                model.query_performance_request = model.query_performance_request.wrapping_add(1);
                let request = model.query_performance_request;
                return vec![Effect::LoadQueryPerformance {
                    request,
                    sort_order: perf.sort_order,
                    limit: crate::perf::DEFAULT_PERFORMANCE_LIMIT,
                }];
            }
        }
        Action::Insert('h' | 'H') => {
            if let Some(perf) = &mut model.query_performance {
                perf.sort_order = crate::perf::PerformanceSortOrder::HitRatio;
                perf.loading = true;
                model.query_performance_request = model.query_performance_request.wrapping_add(1);
                let request = model.query_performance_request;
                return vec![Effect::LoadQueryPerformance {
                    request,
                    sort_order: perf.sort_order,
                    limit: crate::perf::DEFAULT_PERFORMANCE_LIMIT,
                }];
            }
        }
        Action::Insert('!') => {
            if let Some(perf) = &mut model.query_performance {
                perf.confirm_reset = !perf.confirm_reset;
            }
        }
        Action::Activate | Action::Insert('\n') => {
            if let Some(perf) = &mut model.query_performance
                && perf.confirm_reset
            {
                perf.loading = true;
                perf.confirm_reset = false;
                model.query_performance_request = model.query_performance_request.wrapping_add(1);
                let request = model.query_performance_request;
                return vec![Effect::ResetQueryPerformance { request }];
            }
        }
        Action::Insert('e' | 'E') => {
            if let Some(perf) = model.query_performance.as_ref()
                && let Some(entry) = perf.selected_entry()
            {
                let query = entry.query.clone();
                edit_editor(model, |editor| {
                    editor.set_text(query);
                });
                model.query_performance = None;
                model.focus = Focus::Editor;
            }
        }
        _ => {}
    }
    Vec::new()
}

/// Opens the index advisor and bloat forensics overlay.
fn index_advisor_open(model: &mut Model) -> Vec<Effect> {
    if model.index_advisor.is_some() {
        model.index_advisor = None;
        return Vec::new();
    }
    if !model.connection.is_ready() {
        return Vec::new();
    }

    let state = crate::app::model::IndexAdvisorState {
        loading: true,
        ..Default::default()
    };

    model.index_advisor_request = model.index_advisor_request.wrapping_add(1);
    let request = model.index_advisor_request;
    model.index_advisor = Some(state);

    vec![Effect::LoadIndexAdvisor {
        request,
        min_size_bytes: 0,
    }]
}

/// Handles input while the index advisor and bloat forensics overlay is on screen.
fn index_advisor_action(model: &mut Model, action: Action) -> Vec<Effect> {
    match action {
        Action::Dismiss | Action::IndexAdvisor => {
            model.index_advisor = None;
        }
        Action::Quit => {
            model.should_quit = true;
            return vec![Effect::Quit];
        }
        Action::Move(Direction::Up) | Action::Insert('k' | 'K') => {
            if let Some(advisor) = &mut model.index_advisor {
                advisor.move_selection(-1);
            }
        }
        Action::Move(Direction::Down) | Action::Insert('j' | 'J') => {
            if let Some(advisor) = &mut model.index_advisor {
                advisor.move_selection(1);
            }
        }
        Action::Insert('1') => {
            if let Some(advisor) = &mut model.index_advisor {
                advisor.set_tab(0);
            }
        }
        Action::Insert('2') => {
            if let Some(advisor) = &mut model.index_advisor {
                advisor.set_tab(1);
            }
        }
        Action::Insert('3') => {
            if let Some(advisor) = &mut model.index_advisor {
                advisor.set_tab(2);
            }
        }
        Action::Insert('r' | 'R') => {
            if let Some(advisor) = &mut model.index_advisor {
                advisor.loading = true;
            }
            model.index_advisor_request = model.index_advisor_request.wrapping_add(1);
            let request = model.index_advisor_request;
            return vec![Effect::LoadIndexAdvisor {
                request,
                min_size_bytes: 0,
            }];
        }
        Action::Insert('e' | 'E') => {
            if let Some(advisor) = &model.index_advisor
                && let Some(sql) = advisor.selected_sql()
            {
                edit_editor(model, |editor| {
                    editor.set_text(sql);
                });
                model.index_advisor = None;
                model.focus = Focus::Editor;
            }
        }
        Action::Insert('c' | 'C') => {
            if let Some(advisor) = &model.index_advisor
                && let Some(sql) = advisor.selected_sql()
                && let Ok(payload) = crate::clipboard::ClipboardPayload::try_new(sql)
            {
                return vec![Effect::CopyValue { payload }];
            }
        }
        _ => {}
    }
    Vec::new()
}

/// Opens the transaction, WAL, and checkpointer observability overlay.
fn wal_observability_open(model: &mut Model) -> Vec<Effect> {
    if model.wal_observability.is_some() {
        model.wal_observability = None;
        return Vec::new();
    }
    if !model.connection.is_ready() {
        return Vec::new();
    }

    let state = crate::app::model::WalObservabilityState {
        loading: true,
        ..Default::default()
    };

    model.wal_observability_request = model.wal_observability_request.wrapping_add(1);
    let request = model.wal_observability_request;
    model.wal_observability = Some(state);

    vec![Effect::LoadWalObservability { request }]
}

/// Handles input while the transaction, WAL, and checkpointer observability overlay is on screen.
fn wal_observability_action(model: &mut Model, action: Action) -> Vec<Effect> {
    match action {
        Action::Dismiss | Action::WalObservability => {
            model.wal_observability = None;
        }
        Action::Quit => {
            model.should_quit = true;
            return vec![Effect::Quit];
        }
        Action::Move(Direction::Up) | Action::Insert('k' | 'K') => {
            if let Some(wal) = &mut model.wal_observability {
                wal.move_selection(-1);
            }
        }
        Action::Move(Direction::Down) | Action::Insert('j' | 'J') => {
            if let Some(wal) = &mut model.wal_observability {
                wal.move_selection(1);
            }
        }
        Action::Insert('1') => {
            if let Some(wal) = &mut model.wal_observability {
                wal.set_tab(0);
            }
        }
        Action::Insert('2') => {
            if let Some(wal) = &mut model.wal_observability {
                wal.set_tab(1);
            }
        }
        Action::Insert('3') => {
            if let Some(wal) = &mut model.wal_observability {
                wal.set_tab(2);
            }
        }
        Action::Insert('r' | 'R') => {
            if let Some(wal) = &mut model.wal_observability {
                wal.loading = true;
            }
            model.wal_observability_request = model.wal_observability_request.wrapping_add(1);
            let request = model.wal_observability_request;
            return vec![Effect::LoadWalObservability { request }];
        }
        _ => {}
    }
    Vec::new()
}

/// Opens the connection pool and PgBouncer telemetry overlay.
fn connection_pool_open(model: &mut Model) -> Vec<Effect> {
    if model.connection_pool.is_some() {
        model.connection_pool = None;
        return Vec::new();
    }
    if !model.connection.is_ready() {
        return Vec::new();
    }

    let state = crate::app::model::ConnectionPoolState {
        loading: true,
        ..Default::default()
    };

    model.connection_pool_request = model.connection_pool_request.wrapping_add(1);
    let request = model.connection_pool_request;
    model.connection_pool = Some(state);

    vec![Effect::LoadConnectionPool { request }]
}

/// Handles input while the connection pool and PgBouncer telemetry overlay is on screen.
fn connection_pool_action(model: &mut Model, action: Action) -> Vec<Effect> {
    match action {
        Action::Dismiss | Action::ConnectionPool => {
            model.connection_pool = None;
        }
        Action::Quit => {
            model.should_quit = true;
            return vec![Effect::Quit];
        }
        Action::Move(Direction::Up) | Action::Insert('k' | 'K') => {
            if let Some(pool) = &mut model.connection_pool {
                pool.move_selection(-1);
            }
        }
        Action::Move(Direction::Down) | Action::Insert('j' | 'J') => {
            if let Some(pool) = &mut model.connection_pool {
                pool.move_selection(1);
            }
        }
        Action::Insert('1') => {
            if let Some(pool) = &mut model.connection_pool {
                pool.set_tab(0);
            }
        }
        Action::Insert('2') => {
            if let Some(pool) = &mut model.connection_pool {
                pool.set_tab(1);
            }
        }
        Action::Insert('3') => {
            if let Some(pool) = &mut model.connection_pool {
                pool.set_tab(2);
            }
        }
        Action::Insert('c' | 'C') => {
            if let Some(pool) = &model.connection_pool
                && let Some(sql) = pool.selected_query()
                && let Ok(payload) = crate::clipboard::ClipboardPayload::try_new(sql)
            {
                return vec![Effect::CopyValue { payload }];
            }
        }
        Action::Insert('r' | 'R') => {
            if let Some(pool) = &mut model.connection_pool {
                pool.loading = true;
            }
            model.connection_pool_request = model.connection_pool_request.wrapping_add(1);
            let request = model.connection_pool_request;
            return vec![Effect::LoadConnectionPool { request }];
        }
        _ => {}
    }
    Vec::new()
}

/// Opens the data masking and anonymization rules overlay.
fn data_masking_open(model: &mut Model) -> Vec<Effect> {
    if model.data_masking.is_some() {
        model.data_masking = None;
        return Vec::new();
    }
    if !model.connection.is_ready() {
        return Vec::new();
    }

    let state = crate::app::model::DataMaskingState {
        loading: true,
        ..Default::default()
    };

    model.data_masking_request = model.data_masking_request.wrapping_add(1);
    let request = model.data_masking_request;
    model.data_masking = Some(state);

    vec![Effect::LoadDataMasking {
        request,
        schema: None,
    }]
}

/// Handles input while the data masking and anonymization overlay is on screen.
fn data_masking_action(model: &mut Model, action: Action) -> Vec<Effect> {
    match action {
        Action::Dismiss | Action::DataMasking => {
            model.data_masking = None;
        }
        Action::Quit => {
            model.should_quit = true;
            return vec![Effect::Quit];
        }
        Action::Move(Direction::Up) | Action::Insert('k' | 'K') => {
            if let Some(mask) = &mut model.data_masking {
                mask.move_selection(-1);
            }
        }
        Action::Move(Direction::Down) | Action::Insert('j' | 'J') => {
            if let Some(mask) = &mut model.data_masking {
                mask.move_selection(1);
            }
        }
        Action::Insert('1') => {
            if let Some(mask) = &mut model.data_masking {
                mask.set_tab(0);
            }
        }
        Action::Insert('2') => {
            if let Some(mask) = &mut model.data_masking {
                mask.set_tab(1);
            }
        }
        Action::Insert(' ' | '\n') | Action::Activate => {
            if let Some(mask) = &mut model.data_masking {
                mask.cycle_rule();
            }
        }
        Action::Insert('c' | 'C') => {
            if let Some(mask) = &model.data_masking
                && let Some(sql) = mask.generated_sql()
                && let Ok(payload) = crate::clipboard::ClipboardPayload::try_new(sql)
            {
                return vec![Effect::CopyValue { payload }];
            }
        }
        Action::Insert('r' | 'R') => {
            if let Some(mask) = &mut model.data_masking {
                mask.loading = true;
            }
            model.data_masking_request = model.data_masking_request.wrapping_add(1);
            let request = model.data_masking_request;
            return vec![Effect::LoadDataMasking {
                request,
                schema: None,
            }];
        }
        _ => {}
    }
    Vec::new()
}

/// Opens the Cross-Database Copy and Sync Pipelines overlay.
fn data_sync_open(model: &mut Model) -> Vec<Effect> {
    if !model.connection.is_ready() {
        return Vec::new();
    }

    let state = crate::app::model::DataSyncState {
        loading: true,
        ..Default::default()
    };

    model.data_sync_request = model.data_sync_request.wrapping_add(1);
    let request = model.data_sync_request;
    model.data_sync = Some(state);

    vec![Effect::LoadDataSync {
        request,
        schema: None,
    }]
}

/// Handles input while the cross-database copy and sync overlay is on screen.
fn data_sync_action(model: &mut Model, action: Action) -> Vec<Effect> {
    match action {
        Action::Dismiss | Action::DataSync => {
            model.data_sync = None;
        }
        Action::Quit => {
            model.should_quit = true;
            return vec![Effect::Quit];
        }
        Action::Move(Direction::Up) | Action::Insert('k' | 'K') => {
            if let Some(sync) = &mut model.data_sync {
                sync.move_selection(-1);
            }
        }
        Action::Move(Direction::Down) | Action::Insert('j' | 'J') => {
            if let Some(sync) = &mut model.data_sync {
                sync.move_selection(1);
            }
        }
        Action::Insert('1') => {
            if let Some(sync) = &mut model.data_sync {
                sync.set_tab(0);
            }
        }
        Action::Insert('2') => {
            if let Some(sync) = &mut model.data_sync {
                sync.set_tab(1);
            }
        }
        Action::Insert('d' | 'D') => {
            if let Some(sync) = &mut model.data_sync {
                sync.toggle_dry_run();
            }
        }
        Action::Insert('t' | 'T') => {
            if let Some(sync) = &mut model.data_sync {
                sync.toggle_truncate();
            }
        }
        Action::Insert('r' | 'R') => {
            if let Some(sync) = &mut model.data_sync {
                sync.loading = true;
            }
            model.data_sync_request = model.data_sync_request.wrapping_add(1);
            let request = model.data_sync_request;
            return vec![Effect::LoadDataSync {
                request,
                schema: None,
            }];
        }
        _ => {}
    }
    Vec::new()
}

/// Opens the Synthetic Test Fixture Generator overlay.
fn synthetic_seed_open(model: &mut Model) -> Vec<Effect> {
    if !model.connection.is_ready() {
        return Vec::new();
    }

    let state = crate::app::model::SyntheticSeedState {
        loading: true,
        ..Default::default()
    };

    model.synthetic_seed_request = model.synthetic_seed_request.wrapping_add(1);
    let request = model.synthetic_seed_request;
    model.synthetic_seed = Some(state);

    vec![Effect::LoadSyntheticSeed {
        request,
        schema: None,
    }]
}

/// Handles input while the synthetic fixture generator overlay is on screen.
fn synthetic_seed_action(model: &mut Model, action: Action) -> Vec<Effect> {
    match action {
        Action::Dismiss | Action::SyntheticSeed => {
            model.synthetic_seed = None;
        }
        Action::Quit => {
            model.should_quit = true;
            return vec![Effect::Quit];
        }
        Action::Move(Direction::Up) | Action::Insert('k' | 'K') => {
            if let Some(seed) = &mut model.synthetic_seed {
                seed.move_selection(-1);
            }
        }
        Action::Move(Direction::Down) | Action::Insert('j' | 'J') => {
            if let Some(seed) = &mut model.synthetic_seed {
                seed.move_selection(1);
            }
        }
        Action::Insert('1') => {
            if let Some(seed) = &mut model.synthetic_seed {
                seed.set_tab(0);
            }
        }
        Action::Insert('2') => {
            if let Some(seed) = &mut model.synthetic_seed {
                seed.set_tab(1);
            }
        }
        Action::Insert('s' | 'S') => {
            if let Some(seed) = &mut model.synthetic_seed {
                seed.cycle_seed();
            }
        }
        Action::Insert('n' | 'N') => {
            if let Some(seed) = &mut model.synthetic_seed {
                seed.cycle_rows();
            }
        }
        Action::Insert('r' | 'R') => {
            if let Some(seed) = &mut model.synthetic_seed {
                seed.loading = true;
            }
            model.synthetic_seed_request = model.synthetic_seed_request.wrapping_add(1);
            let request = model.synthetic_seed_request;
            return vec![Effect::LoadSyntheticSeed {
                request,
                schema: None,
            }];
        }
        _ => {}
    }
    Vec::new()
}

/// Opens the persistent scratchpad and snippet library overlay.
fn snippet_library_open(model: &mut Model) -> Vec<Effect> {
    if model.snippet_library.is_none() {
        let config_dir = crate::config::Paths::resolve().config_dir;
        let snippets_dir = config_dir.join("snippets");
        model.snippet_library = Some(crate::app::model::SnippetLibraryState::with_directory(
            snippets_dir,
        ));
    }
    Vec::new()
}

/// Restores crash-recovered buffers into the workspace and clears the journal.
///
/// The banner is the only route in: without an unrecovered journal on screen
/// there is nothing to restore, and the action is a no-op rather than a guess.
fn restore_session_action(model: &mut Model) -> Vec<Effect> {
    let Some(banner) = model.recovery_banner.take() else {
        return Vec::new();
    };
    // Anything typed after the capsule appeared was never checkpointed. Refuse
    // rather than exceed the tab bound and silently drop either side.
    let in_flight = model.buffers.clone();
    let dirty_in_flight = in_flight
        .buffers
        .iter()
        .filter(|b| b.editor.is_modified() && !b.editor.text().trim().is_empty())
        .count();
    let restorable = banner
        .journal
        .buffers
        .iter()
        .filter(|b| !b.sql.trim().is_empty() || b.file_path.is_some())
        .count();
    if dirty_in_flight > 0 && dirty_in_flight + restorable > crate::buffer::MAX_EDITOR_BUFFERS {
        model.recovery_banner = Some(banner);
        model.error = Some(
            crate::diagnostics::Diagnostic::new(
                crate::diagnostics::DiagnosticKind::Usage,
                "restoring would exceed the 16-tab limit and lose an edit made after the crash prompt",
                "restoring a crash session",
            )
            .likely_cause("the recovered session and the current workspace together hold more than 16 edited tabs")
            .next_action("save or close the current edit, then press Ctrl+K r again"),
        );
        return Vec::new();
    }
    banner.journal.restore_into(&mut model.buffers);
    for mut buffer in in_flight.buffers {
        if model.buffers.buffers.len() >= crate::buffer::MAX_EDITOR_BUFFERS {
            break;
        }
        if !buffer.editor.is_modified() || buffer.editor.text().trim().is_empty() {
            continue;
        }
        buffer.id = model.buffers.next_id;
        model.buffers.next_id = model.buffers.next_id.wrapping_add(1);
        model.buffers.buffers.push(buffer);
    }
    // A journal can legitimately yield zero restorable buffers (every tab was
    // empty). The editor still has to follow whatever the workspace holds now,
    // or the screen would show text the buffers do not.
    model.sync_editor_from_active_buffer();
    // The buffers now live in the workspace; the journal has done its job.
    if let Err(diagnostic) = crate::recovery::RecoveryJournal::discard(&banner.recovery_dir) {
        model.error = Some(diagnostic);
    }
    Vec::new()
}

/// Discards the unrecovered crash journal without touching the workspace.
fn discard_recovery_action(model: &mut Model) -> Vec<Effect> {
    let Some(banner) = model.recovery_banner.take() else {
        return Vec::new();
    };
    if let Err(diagnostic) = crate::recovery::RecoveryJournal::discard(&banner.recovery_dir) {
        model.error = Some(diagnostic);
    }
    Vec::new()
}

/// Lints the active buffer and opens the findings overlay.
///
/// The linter is pure and local: this reducer never connects, writes a file or
/// executes SQL. An empty editor is a no-op because a clean run there says
/// nothing useful.
fn lint_buffer_open(model: &mut Model) -> Vec<Effect> {
    if model.editor.text().trim().is_empty() {
        return Vec::new();
    }
    let report = crate::lint::lint_source(None, model.editor.text());
    model.lint_findings = Some(crate::app::model::LintFindingsState::new(report.findings));
    Vec::new()
}

/// Handles input while the lint findings overlay is on screen.
fn lint_findings_action(model: &mut Model, action: Action) -> Vec<Effect> {
    let Some(mut state) = model.lint_findings.take() else {
        return Vec::new();
    };
    match action {
        Action::Move(Direction::Up) => {
            state.move_selection(-1);
            model.lint_findings = Some(state);
            Vec::new()
        }
        Action::Move(Direction::Down) => {
            state.move_selection(1);
            model.lint_findings = Some(state);
            Vec::new()
        }
        Action::Activate => {
            if let Some(finding) = state.selected_finding() {
                model.editor.set_position(finding.line, finding.column);
                model.sync_active_buffer_from_editor();
            }
            Vec::new()
        }
        Action::Insert('r') => lint_buffer_open(model),
        Action::Dismiss => Vec::new(),
        Action::Quit => {
            model.should_quit = true;
            vec![Effect::Quit]
        }
        _ => {
            // An unhandled key keeps the overlay open, like every other
            // browser-style overlay. Closing it here would hide findings on a
            // stray keystroke and then apply that key to the editor.
            model.lint_findings = Some(state);
            Vec::new()
        }
    }
}

/// Handles input while the snippet and scratchpad library overlay is on screen.
fn snippet_library_action(model: &mut Model, action: Action) -> Vec<Effect> {
    let Some(mut state) = model.snippet_library.take() else {
        return Vec::new();
    };

    match action {
        Action::Dismiss | Action::SnippetLibrary => {
            model.snippet_library = None;
            return Vec::new();
        }
        Action::Quit => {
            model.should_quit = true;
            return vec![Effect::Quit];
        }
        Action::Move(Direction::Up) | Action::Insert('k' | 'K') => {
            state.library.move_selection(-1);
        }
        Action::Move(Direction::Down) | Action::Insert('j' | 'J') => {
            state.library.move_selection(1);
        }
        Action::Insert('\t') | Action::FocusNext => {
            state.library.cycle_category();
        }
        Action::Activate | Action::Insert('\n') => {
            if let Some(snippet) = state.library.selected_snippet() {
                let sql = snippet.sql.clone();
                edit_editor(model, |editor| {
                    if editor.text().trim().is_empty() {
                        editor.set_text(sql);
                    } else {
                        editor.insert_paste(&format!("\n\n{sql}"));
                    }
                });
                model.snippet_library = None;
                model.focus = Focus::Editor;
                return Vec::new();
            }
        }
        Action::Insert('n' | 'N') => {
            if let Some(snippet) = state.library.selected_snippet() {
                let sql = snippet.sql.clone();
                let title = format!("{}.sql", snippet.name);
                model.sync_active_buffer_from_editor();
                model.buffers.new_buffer(Some(title), Some(sql));
                model.sync_editor_from_active_buffer();
                model.snippet_library = None;
                model.focus = Focus::Editor;
                return Vec::new();
            }
        }
        Action::Insert('a' | 'A') => {
            let text = model.editor.text().trim().to_string();
            if !text.is_empty() {
                let count = state.library.total_count() + 1;
                let name = format!("user-snippet-{count}");
                let _ = state.library.add_user_snippet(
                    &name,
                    &name,
                    "Saved from active editor buffer.",
                    crate::snippet::SnippetCategory::User,
                    &text,
                    state.directory.as_deref(),
                );
                state.status_message = Some(format!("Saved buffer as snippet '{name}'"));
            } else {
                state.status_message = Some("Active editor buffer is empty".into());
            }
        }
        Action::Insert('d' | 'D') => {
            if let Some(selected) = state.library.selected_snippet() {
                if !selected.is_builtin {
                    let id = selected.id.clone();
                    if let Some(path) = &selected.path {
                        let _ = std::fs::remove_file(path);
                    }
                    state.library.snippets.retain(|s| s.id != id);
                    state.status_message = Some(format!("Deleted user snippet '{id}'"));
                } else {
                    state.status_message = Some("Cannot delete built-in snippet".into());
                }
            }
        }
        _ => {}
    }

    model.snippet_library = Some(state);
    Vec::new()
}

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

/// Changes the session shape, restoring before a visible split adjustment.
fn adjust_panes(model: &mut Model, action: &Action) {
    use crate::ui::layout::{LayoutMode, layout_mode, pane_geometry};
    use ratatui::layout::Rect;
    let area = Rect::new(0, 0, model.size.0, model.size.1);
    if layout_mode(area) != LayoutMode::Full {
        model.layout_notice = Some(LayoutNotice::SinglePane);
        return;
    }
    if matches!(action, Action::ZoomPane) {
        if model.focus != Focus::Results && needs_safety_details(model) {
            model.layout_notice = Some(LayoutNotice::SafetyDetails);
            return;
        }
        model.splits.zoomed = !model.splits.zoomed;
        return;
    }
    model.splits.zoomed = false;
    let geometry = pane_geometry(model, area);
    let grow = matches!(action, Action::GrowPane);
    if model.focus == Focus::Objects {
        if let Some(sidebar) = geometry.objects {
            let desired = if grow {
                sidebar.width.saturating_add(2)
            } else {
                sidebar.width.saturating_sub(2)
            };
            model.splits.sidebar_cells =
                Some(desired.clamp(20, 60.min(geometry.body.width.saturating_sub(30))));
        }
    } else if let Some(editor) = geometry.editor {
        let desired = if grow {
            editor.height.saturating_add(1)
        } else {
            editor.height.saturating_sub(1)
        };
        model.splits.editor_rows = Some(desired.clamp(3, geometry.body.height.saturating_sub(3)));
    }
}

/// Re-clamps any explicit split positions to the current terminal geometry.
fn clamp_splits(model: &mut Model) {
    use crate::ui::layout::{LayoutMode, body_area, layout_mode};
    use ratatui::layout::Rect;
    let area = Rect::new(0, 0, model.size.0, model.size.1);
    if layout_mode(area) != LayoutMode::Full {
        return;
    }
    let body = body_area(area);
    if let Some(cells) = model.splits.sidebar_cells {
        let max_sidebar = 60.min(body.width.saturating_sub(30)).max(20);
        model.splits.sidebar_cells = Some(cells.clamp(20, max_sidebar));
    }
    if let Some(rows) = model.splits.editor_rows {
        let max_editor = body.height.saturating_sub(3).max(3);
        model.splits.editor_rows = Some(rows.clamp(3, max_editor));
    }
}

/// Dispatches a mouse gesture through the pure hit-region table (`contracts/mouse.md`).
fn handle_mouse(
    model: &mut Model,
    kind: crate::app::message::MouseKind,
    column: u16,
    row: u16,
    modifiers: crate::app::message::MouseModifiers,
) -> Vec<Effect> {
    use crate::app::message::{MouseButton, MouseKind};
    use crate::app::model::{DOUBLE_CLICK_WINDOW, DragTarget, LastClick};
    use crate::ui::layout::{LayoutMode, layout_mode, pane_geometry};
    use crate::ui::widgets::{Region, overlay_blocks_panes, pane_at, region_at};
    use ratatui::layout::Rect;

    let area = Rect::new(0, 0, model.size.0, model.size.1);

    // A drag ends on Up of any button, regardless of modifiers or position.
    if matches!(kind, MouseKind::Up(_)) {
        model.mouse.drag = None;
        return Vec::new();
    }

    // Hover and horizontal wheel events have no verb in this feature.
    if matches!(
        kind,
        MouseKind::Moved | MouseKind::ScrollLeft | MouseKind::ScrollRight
    ) {
        return Vec::new();
    }

    // Ctrl+click and Alt-modified mouse events are deliberately unbound.
    if modifiers.control || modifiers.alt {
        if matches!(kind, MouseKind::Down(_)) {
            model.mouse.drag = None;
            model.mouse.last_click = None;
        }
        return Vec::new();
    }

    // Shift-modified events are ignored except Shift + wheel over Results.
    if modifiers.shift {
        if matches!(kind, MouseKind::ScrollDown | MouseKind::ScrollUp) {
            if pane_at(model, area, column, row) == Some(Focus::Results) && !model.plan.is_visible()
            {
                model.layout_notice = None;
                let columns = model.visible_result().map_or_else(Vec::new, |set| {
                    model.result_grid.visible_columns(set.columns.len())
                });
                let delta = if matches!(kind, MouseKind::ScrollDown) {
                    1
                } else {
                    -1
                };
                move_result_column(model, &columns, delta);
            }
            return Vec::new();
        }
        if matches!(kind, MouseKind::Down(_)) {
            model.mouse.drag = None;
            model.mouse.last_click = None;
        }
        return Vec::new();
    }

    // Unmodified wheel scrolls the pane under the pointer three rows per notch
    // without moving keyboard focus.
    if matches!(kind, MouseKind::ScrollDown | MouseKind::ScrollUp) {
        if let Some(target_pane) = pane_at(model, area, column, row) {
            model.layout_notice = None;
            let direction = if matches!(kind, MouseKind::ScrollDown) {
                Direction::Down
            } else {
                Direction::Up
            };
            let saved_focus = model.focus;
            model.focus = target_pane;
            for _ in 0..3 {
                move_selection(model, direction);
            }
            model.focus = saved_focus;
        }
        return Vec::new();
    }

    if let MouseKind::Drag(button) = kind {
        if button != MouseButton::Left {
            return Vec::new();
        }
        let Some(drag) = model.mouse.drag else {
            return Vec::new();
        };
        if overlay_blocks_panes(model) {
            model.mouse.drag = None;
            return Vec::new();
        }
        match drag {
            DragTarget::VerticalSplit {
                start_column,
                start_width,
            } => {
                if layout_mode(area) != LayoutMode::Full {
                    model.mouse.drag = None;
                    return Vec::new();
                }
                model.layout_notice = None;
                model.splits.zoomed = false;
                let geometry = pane_geometry(model, area);
                let delta = i32::from(column) - i32::from(start_column);
                let desired = u16::try_from(i32::from(start_width).saturating_add(delta).max(0))
                    .unwrap_or(u16::MAX);
                model.splits.sidebar_cells =
                    Some(desired.clamp(20, 60.min(geometry.body.width.saturating_sub(30))));
            }
            DragTarget::HorizontalSplit {
                start_row,
                start_height,
            } => {
                if layout_mode(area) != LayoutMode::Full {
                    model.mouse.drag = None;
                    return Vec::new();
                }
                model.layout_notice = None;
                model.splits.zoomed = false;
                let geometry = pane_geometry(model, area);
                let delta = i32::from(row) - i32::from(start_row);
                let desired = u16::try_from(i32::from(start_height).saturating_add(delta).max(0))
                    .unwrap_or(u16::MAX);
                model.splits.editor_rows =
                    Some(desired.clamp(3, geometry.body.height.saturating_sub(3)));
            }
            DragTarget::ColumnEdge {
                column: col_idx,
                start_column,
                start_width,
            } => {
                if model.plan.is_visible() {
                    model.mouse.drag = None;
                    return Vec::new();
                }
                model.layout_notice = None;
                let delta =
                    isize::try_from(i32::from(column) - i32::from(start_column)).unwrap_or(0);
                let width = start_width.saturating_add_signed(delta).clamp(
                    crate::app::grid::MIN_COLUMN_WIDTH,
                    crate::app::grid::MAX_COLUMN_WIDTH,
                );
                model.result_grid.widths.insert(col_idx, width);
                model.result_grid.note = None;
            }
        }
        return Vec::new();
    }

    let MouseKind::Down(button) = kind else {
        return Vec::new();
    };
    model.mouse.drag = None;
    if button != MouseButton::Left {
        model.mouse.last_click = None;
        return Vec::new();
    }

    let region = region_at(model, area, column, row);
    let is_double_clickable = |candidate: Region| {
        matches!(
            candidate,
            Region::SidebarRow { .. } | Region::ResultsCell { .. } | Region::OverlayItem { .. }
        )
    };
    let is_double = region.is_some_and(|current| {
        is_double_clickable(current)
            && model.mouse.last_click.is_some_and(|previous| {
                previous.region == current
                    && model.mouse.clock.saturating_sub(previous.at) <= DOUBLE_CLICK_WINDOW
            })
    });
    if is_double {
        model.mouse.last_click = None;
    } else if let Some(current) = region.filter(|candidate| is_double_clickable(*candidate)) {
        model.mouse.last_click = Some(LastClick {
            region: current,
            at: model.mouse.clock,
        });
    } else {
        model.mouse.last_click = None;
    }

    match region {
        Some(Region::SidebarRow { index }) => {
            model.prefix_pending = false;
            model.layout_notice = None;
            model.completion.menu = None;
            model.result_filtering = false;
            model.focus = Focus::Objects;
            let max = model.tree.rows().len().saturating_sub(1);
            model.tree.selected = index.min(max);
            if is_double {
                model.tree.filtering = false;
                return apply_action(model, Action::Activate);
            }
            Vec::new()
        }
        Some(Region::EditorText {
            line,
            column: text_col,
        }) => {
            model.prefix_pending = false;
            model.layout_notice = None;
            model.completion.menu = None;
            model.tree.filtering = false;
            model.result_filtering = false;
            model.focus = Focus::Editor;
            model.editor.set_position(line, text_col);
            Vec::new()
        }
        Some(Region::ResultsCell {
            row: cell_row,
            column: cell_col,
        }) => {
            model.prefix_pending = false;
            model.layout_notice = None;
            model.completion.menu = None;
            model.tree.filtering = false;
            model.focus = Focus::Results;
            let max_row = model.displayed_rows().len().saturating_sub(1);
            model.selected_row = cell_row.min(max_row);
            if let Some(set) = model.visible_result() {
                let columns = model.result_grid.visible_columns(set.columns.len());
                if let Some(pos) = columns.iter().position(|&c| c == cell_col) {
                    model.selected_column = cell_col;
                    model.result_grid.horizontal_start = pos;
                }
            }
            if is_double {
                model.result_filtering = false;
                return apply_action(model, Action::ToggleInspector);
            }
            Vec::new()
        }
        Some(Region::ResultsHeaderCell { column: header_col }) => {
            model.prefix_pending = false;
            model.layout_notice = None;
            let selected_source_row = model.selected_source_row();
            model.result_grid.toggle_sort(header_col);
            restore_selected_source_row(model, selected_source_row);
            Vec::new()
        }
        Some(Region::ColumnEdge { column: edge_col }) => {
            model.prefix_pending = false;
            let automatic = model
                .visible_result()
                .map_or(crate::app::grid::MIN_COLUMN_WIDTH, |set| {
                    crate::app::grid::automatic_column_width(set, edge_col)
                });
            let start_width = model.result_grid.width_for(edge_col, automatic);
            model.mouse.drag = Some(DragTarget::ColumnEdge {
                column: edge_col,
                start_column: column,
                start_width,
            });
            Vec::new()
        }
        Some(Region::VerticalSplit) => {
            model.prefix_pending = false;
            let geometry = pane_geometry(model, area);
            if let Some(sidebar) = geometry.objects {
                model.mouse.drag = Some(DragTarget::VerticalSplit {
                    start_column: column,
                    start_width: sidebar.width,
                });
            }
            Vec::new()
        }
        Some(Region::HorizontalSplit) => {
            model.prefix_pending = false;
            let geometry = pane_geometry(model, area);
            if let Some(editor) = geometry.editor {
                model.mouse.drag = Some(DragTarget::HorizontalSplit {
                    start_row: row,
                    start_height: editor.height,
                });
            }
            Vec::new()
        }
        Some(Region::OverlayItem { index }) => {
            model.prefix_pending = false;
            model.layout_notice = None;
            if let Some(palette) = model.palette.as_mut() {
                let max = palette.matches().len().saturating_sub(1);
                palette.selected = index.min(max);
            }
            if is_double {
                return apply_action(model, Action::Activate);
            }
            Vec::new()
        }
        Some(Region::Header | Region::Footer) => Vec::new(),
        Some(Region::EditorGutter { .. } | Region::ResultsScrollbar) | None => {
            if let Some(pane) = pane_at(model, area, column, row) {
                model.prefix_pending = false;
                model.layout_notice = None;
                model.completion.menu = None;
                if pane != Focus::Objects {
                    model.tree.filtering = false;
                }
                if pane != Focus::Results {
                    model.result_filtering = false;
                }
                model.focus = pane;
            }
            Vec::new()
        }
    }
}

/// Moves by the current renderer-owned content capacity of the focused pane.
fn page(model: &mut Model, direction: Direction) {
    let lines = crate::ui::layout::editor_page(model);
    match model.focus {
        Focus::Editor => match direction {
            Direction::Up => model.editor.move_page_up(lines),
            Direction::Down => model.editor.move_page_down(lines),
            Direction::Left | Direction::Right => {}
        },
        Focus::Results => {
            if model.plan.is_visible() && model.plan.document().is_none() {
                return;
            }
            if model.plan.document().is_some() {
                let step =
                    isize::try_from(crate::ui::layout::results_page(model)).unwrap_or(isize::MAX);
                match direction {
                    Direction::Up => model.plan.move_selection(-step),
                    Direction::Down => model.plan.move_selection(step),
                    Direction::Left | Direction::Right => {}
                }
                return;
            }
            let rows = model.displayed_rows().len();
            let step = crate::ui::layout::results_page(model);
            match direction {
                Direction::Up => model.selected_row = model.selected_row.saturating_sub(step),
                Direction::Down => {
                    model.selected_row = (model.selected_row + step).min(rows.saturating_sub(1));
                }
                Direction::Left | Direction::Right => {}
            }
        }
        Focus::Objects => {
            let step =
                isize::try_from(crate::ui::layout::objects_page(model)).unwrap_or(isize::MAX);
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
    if refuse_unavailable_result(model) {
        return;
    }
    if model.plan.is_visible() {
        return;
    }
    if model.inspector.is_some() {
        model.inspector = None;
        return;
    }
    let (width, _) = crate::ui::layout::inspector_viewport(model.size, &model.presentation);
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
    let (width, height) = crate::ui::layout::inspector_viewport(model.size, &model.presentation);
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
            if model.help_open {
                model.help_scroll = 0;
            }
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
            let target = model
                .error
                .as_ref()
                .map_or_else(String::new, |diagnostic| diagnostic.attempted.clone());
            model.error = None;
            model.connection = ConnectionState::Connecting {
                step: ConnectingStep::ServerHandshake,
                target,
            };
            model.connecting_step = Some(ConnectingStep::ServerHandshake);
            model.running_for = None;
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

/// Consumes only review navigation or refused acceptance, before the owning
/// handler can consume a candidate, accept an answer or allocate a job.
fn review_action(model: &mut Model, kind: crate::ui::review::Kind, action: &Action) -> bool {
    if !matches!(
        action,
        Action::Activate
            | Action::RunBuffer
            | Action::Move(Direction::Up | Direction::Down)
            | Action::MovePage(_)
            | Action::MoveLineStart
            | Action::MoveLineEnd
            | Action::MoveBufferStart
            | Action::MoveBufferEnd
    ) {
        return false;
    }
    let Some(view) = crate::ui::review::build(
        model,
        kind,
        &model.presentation,
        ratatui::layout::Rect::new(0, 0, model.size.0, model.size.1),
    ) else {
        return false;
    };
    if let Some(scroll) = view.navigate(action) {
        model.review_scroll = scroll;
        return true;
    }
    !view.ready
}

/// Handles input while named parameter values are being collected.
fn parameter_action(model: &mut Model, action: Action) -> Vec<Effect> {
    if review_action(model, crate::ui::review::Kind::Parameter, &action) {
        return Vec::new();
    }
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
                Some(Ok(None)) => {
                    model.review_scroll = 0;
                    Vec::new()
                }
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
                    if refuse_unavailable_result(model) {
                        return Vec::new();
                    }
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
    if review_action(model, crate::ui::review::Kind::Production, &action) {
        return Vec::new();
    }
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
    if review_action(model, crate::ui::review::Kind::Analyze, &action) {
        return Vec::new();
    }
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
/// Typing is swallowed, review navigation stays local, and acceptance rechecks
/// the retained result before a payload exists.
fn copy_confirmation_action(model: &mut Model, action: Action) -> Vec<Effect> {
    if review_action(model, crate::ui::review::Kind::Copy, &action) {
        return Vec::new();
    }
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

/// A local refusal keeps PostgreSQL outcome and diagnostic priority untouched.
fn refuse_unavailable_result(model: &mut Model) -> bool {
    if !model
        .visible_result()
        .is_some_and(|set| !set.is_available())
    {
        return false;
    }
    model.result_grid.note = Some(format!(
        "{}. {}",
        crate::query::result::RETAINED_DATA_UNAVAILABLE,
        crate::query::result::RETAINED_DATA_GUIDANCE
    ));
    true
}

/// Opens a value-free copy confirmation for the selected retained cell.
fn begin_copy(model: &mut Model) -> Vec<Effect> {
    if refuse_unavailable_result(model) {
        return Vec::new();
    }
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
    model.review_scroll = 0;
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
    let set = model.visible_result().filter(|set| set.is_available())?;
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
                    model.review_scroll = 0;
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
    if review_action(model, crate::ui::review::Kind::Update, &action) {
        return Vec::new();
    }
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
    if refuse_unavailable_result(model) {
        return Vec::new();
    }
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
    if update_lookup_has_newer_owner(model) {
        model.cell_update_notice = Some(
            "Cell update metadata was discarded because another input or review is active. Finish or cancel it, then regenerate the cell update; nothing was sent."
                .to_owned(),
        );
        return Vec::new();
    }
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

/// An asynchronous lookup may not replace a newer input owner. Its candidate
/// identity can still be current even though the user opened another prompt.
fn update_lookup_has_newer_owner(model: &Model) -> bool {
    crate::ui::review::active_kind(model).is_some()
        || model.password_prompt.is_some()
        || model.name_prompt.is_some()
        || model.connection_details
        || model.prefix_pending
        || model.update_prompt.is_some()
        || model.palette.is_some()
        || model.help_open
        || pane_input_owned(model)
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
        model.review_scroll = 0;
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
        model.review_scroll = 0;
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
        model.review_scroll = 0;
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
            let step = isize::try_from(crate::app::grid::COLUMN_WIDTH_STEP).unwrap_or(2);
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
        Action::Insert(ch) => {
            // The recovery capsule owns `r` and `d` only while it is on screen;
            // every other chord keeps working. Without a banner those letters
            // keep their ordinary chord meaning too.
            let chord = model
                .recovery_banner
                .is_some()
                .then(|| crate::ui::keymap::recovery_chord_action(ch))
                .flatten()
                .or_else(|| crate::ui::keymap::chord_action(ch));
            match chord {
                Some(chord) => apply_action(model, chord),
                None => Vec::new(),
            }
        }
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
            let had_help = model.help_open;
            let had_notice_details = model.notice_details.is_some();
            if chosen.is_some() {
                model.help_open = false;
                model.notice_details = None;
            }
            match chosen.map(|entry| entry.command) {
                Some(crate::app::palette::PaletteCommand::Run(Action::ToggleHelp)) if had_help => {
                    Vec::new()
                }
                Some(crate::app::palette::PaletteCommand::Run(next)) => apply_action(model, next),
                Some(crate::app::palette::PaletteCommand::NoticeDetails) => {
                    model.notice_details = Some(crate::app::model::NoticeDetails {
                        snapshot: model.notice_state.snapshot(),
                        scroll: 0,
                    });
                    Vec::new()
                }
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
                    if had_help || had_notice_details {
                        reveal_palette_editor(model);
                    }
                    model.loaded_query = Some(name.clone());
                    vec![Effect::LoadQuery { name }]
                }
                Some(crate::app::palette::PaletteCommand::Insert(text)) => {
                    reveal_palette_editor(model);
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

/// Direct editor-target palette entries bypass action dispatch. Reveal that
/// target after selection, preserving covered reading state on cancellation.
fn reveal_palette_editor(model: &mut Model) {
    model.focus = Focus::Editor;
    model.inspector = None;
    model.definition = None;
    model.completion.menu = None;
    model.tree.filtering = false;
    model.result_filtering = false;
}

/// Read-only client accounting has no SQL/clipboard/file action route.
fn notice_details_action(model: &mut Model, action: Action) -> Vec<Effect> {
    let view = crate::ui::notices::build(
        model,
        &model.presentation,
        ratatui::layout::Rect::new(0, 0, model.size.0, model.size.1),
    );
    if let Some(offset) = view.as_ref().and_then(|view| view.navigate(&action)) {
        if let Some(details) = model.notice_details.as_mut() {
            details.scroll = offset;
        }
        return Vec::new();
    }
    match action {
        Action::Dismiss | Action::ToggleHelp => model.notice_details = None,
        Action::Cancel => {
            model.notice_details = None;
            return cancel_query(model);
        }
        Action::Activate if view.is_some_and(|view| view.usable) => {
            if let Some(details) = model.notice_details.take() {
                model.notice_state.acknowledge(&details.snapshot);
            }
        }
        Action::OpenPalette => {
            model.palette = Some(crate::app::palette::Palette::over_commands(
                palette_entries(model),
                crate::app::discovery::palette_note(model),
            ))
        }
        Action::BeginPrefix => model.prefix_pending = true,
        Action::Quit => {
            model.should_quit = true;
            return vec![Effect::Quit];
        }
        _ => {}
    }
    Vec::new()
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

/// Delivers a paste to the surface that owns input.
///
/// The peel order mirrors [`apply_action`] exactly, so a paste and a keystroke
/// can never disagree about who owns input. A text surface receives the text;
/// a surface that does not accept text says the paste was ignored instead of
/// dropping it. No route emits an effect, so a paste can never execute SQL.
fn paste_action(model: &mut Model, text: String) -> Vec<Effect> {
    if text.len() > crate::app::editor::PASTE_LIMIT_BYTES {
        model.paste_notice = Some(PasteNotice::Refused);
        model.format_notice = None;
        return Vec::new();
    }
    // Windows and old-Mac line endings become LF before anything stores them.
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    if text.is_empty() {
        model.paste_notice = None;
        return Vec::new();
    }

    if let Some(prompt) = model.password_prompt.as_mut() {
        for character in text.chars() {
            prompt.push(character);
        }
        model.paste_notice = None;
        return Vec::new();
    }
    if let Some(prompt) = model.parameter_prompt.as_mut() {
        for character in text.chars() {
            prompt.push(character);
        }
        model.paste_notice = None;
        return Vec::new();
    }
    if let Some(prompt) = model.name_prompt.as_mut() {
        prompt.typed.push_str(&text);
        model.paste_notice = None;
        return Vec::new();
    }
    if model.connection_details || model.prefix_pending {
        model.paste_notice = Some(PasteNotice::Ignored);
        model.format_notice = None;
        return Vec::new();
    }
    if model.pending_run.is_some() || model.pending_plan.is_some() || model.pending_copy.is_some() {
        model.paste_notice = Some(PasteNotice::Ignored);
        model.format_notice = None;
        return Vec::new();
    }
    if let Some(prompt) = model.update_prompt.as_mut() {
        for character in text.chars() {
            prompt.push(character);
        }
        model.paste_notice = None;
        return Vec::new();
    }
    if model.pending_update.is_some() {
        model.paste_notice = Some(PasteNotice::Ignored);
        model.format_notice = None;
        return Vec::new();
    }
    if let Some(palette) = model.palette.as_mut() {
        for character in text.chars() {
            palette.push(character);
        }
        model.paste_notice = None;
        return Vec::new();
    }
    if model.help_open || model.notice_details.is_some() {
        model.paste_notice = Some(PasteNotice::Ignored);
        model.format_notice = None;
        return Vec::new();
    }
    if model.tree.filtering {
        model.tree.filter.push_str(&text);
        model.tree.selected = 0;
        model.paste_notice = None;
        return Vec::new();
    }
    if model.result_filtering {
        model.result_filter.push_str(&text);
        model.selected_row = 0;
        model.paste_notice = None;
        return Vec::new();
    }
    if model.inspector.is_some() || model.definition.is_some() {
        model.paste_notice = Some(PasteNotice::Ignored);
        model.format_notice = None;
        return Vec::new();
    }

    // The editor is the client's SQL surface: a paste reaches it whenever no
    // higher surface owns input, even while the completion menu is open. It
    // goes through `edit_editor` so a stale server error marker and a format
    // notice are invalidated exactly as they would be for typed input.
    edit_editor(model, |editor| {
        editor.insert_paste(&text);
    });
    refresh_completion_menu(model);
    Vec::new()
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
    if model.phase.is_busy() || matches!(model.connection, ConnectionState::Connecting { .. }) {
        return Vec::new();
    }
    let summary = profile.as_deref().and_then(|name| {
        model
            .connection_profiles
            .iter()
            .find(|item| item.name == name)
    });
    model
        .notice_state
        .begin_retirement(model.connection_generation);
    model.connection_generation = model.connection_generation.wrapping_add(1).max(1);
    model.credential_provider = summary.and_then(|item| item.auth.clone());
    model.credential_presentation = summary.and_then(|item| item.provider_presentation.clone());
    model.connection = ConnectionState::Connecting {
        step: ConnectingStep::ResolvingProfile,
        target: profile.clone().unwrap_or_default(),
    };
    model.connecting_step = Some(ConnectingStep::ResolvingProfile);
    model.phase = QueryPhase::Idle;
    model.last_execution = None;
    model.retained_data_loss = crate::query::result::RetentionLossSummary::default();
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
    model.schema_diagram = None;
    model.schema_diagram_notice = None;
    model.server_activity = None;
    model.security_audit = None;
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
    if refuse_unavailable_result(model) {
        return Vec::new();
    }
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

    entries.push(PaletteEntry {
        label: "Server notice retention".into(),
        detail: format!(
            "{} omitted, {} shortened, {} abandoned; {}",
            model.notice_state.pending.omitted.words(),
            model.notice_state.pending.shortened.words(),
            model.notice_state.pending.abandoned.words(),
            if model.notice_state.pending.needs_report() {
                "pending"
            } else {
                "client accounting"
            }
        ),
        group: "Session",
        command: PaletteCommand::NoticeDetails,
    });

    // A command appears once even when several active keys invoke it. The
    // detail keeps every active direct key, with configured replacements first.
    let mut theme_keys = Vec::new();
    for binding in snapshot
        .bindings
        .iter()
        .filter(|binding| binding.action == Action::SwitchTheme)
    {
        if !theme_keys.contains(&binding.key) {
            theme_keys.push(binding.key.clone());
        }
    }
    for chord in snapshot
        .chords
        .iter()
        .filter(|chord| chord.action == Action::SwitchTheme)
    {
        let key = format!(
            "{} {}",
            snapshot.prefix.as_deref().unwrap_or("Ctrl+K"),
            chord.key
        );
        if !theme_keys.contains(&key) {
            theme_keys.push(key);
        }
    }
    entries.push(PaletteEntry {
        label: "Switch theme".into(),
        detail: if theme_keys.is_empty() {
            "Cycle theme for this session".into()
        } else {
            theme_keys.join(" / ")
        },
        group: "Session",
        command: PaletteCommand::Run(Action::SwitchTheme),
    });
    for action in crate::app::discovery::palette_actions(model) {
        if action == Action::SwitchTheme {
            continue;
        }
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
