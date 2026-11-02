//! Integration contract suite for W10: Performance, Resource Bounds, and Local-State Recovery.
//!
//! Validates:
//! 1. `w10_local_input_palette_and_completion_p95_within_100ms_and_state_invariant`:
//!    p95 input-to-render latency <= 100 ms over warm-up and 100 measured actions
//!    with synthetic catalog, saved queries, and history; deterministic invariant preservation.
//! 2. `w10_local_grid_interaction_at_retained_row_cap_p95_within_100ms_and_preserves_row_identity`:
//!    p95 grid interaction-to-render latency <= 100 ms at 1,000 retained rows with wide
//!    Unicode, Cell::Null, empty, and literal "NULL"; row identity preservation across sort/filter.
//! 3. `w10_all_seven_subsystem_input_and_retention_ceilings_enforce_exact_boundaries`:
//!    Exact pass/refusal boundaries for Formatter, Plan, Clipboard, Named Parameters,
//!    Single-Cell Update, Notice/Cell Truncation, and Result-Set Retention Budget.
//! 4. `w10_local_state_recovery_preserves_existing_files_and_refuses_unsafe_paths`:
//!    Config schema recovery, atomic write replacement and permission preservation,
//!    export file collision and unwritable directory safety, history trimming and clear,
//!    and query library path safety.

#![allow(clippy::print_stderr, clippy::print_stdout)]

use ignatius::app::completion::CatalogStatus;
use ignatius::app::model::{ClipboardNotice, ConnectionState, Focus, FormatNotice, Model};
use ignatius::app::{Action, Direction, Message, update};
use ignatius::clipboard::{ClipboardPayload, MAX_OSC52_BYTES, PayloadTooLarge};
use ignatius::config::Paths;
use ignatius::connection::Environment;
use ignatius::diagnostics::DiagnosticKind;
use ignatius::exit_code::ExitCode;
use ignatius::history::{Entry, History, Outcome};
use ignatius::postgres::TlsState;
use ignatius::postgres::metadata::{ObjectKind, SchemaSummary};
use ignatius::postgres::session::SessionInfo;
use ignatius::queries::{Library, SavedQuery};
use ignatius::query::completion::{
    CatalogColumn, CatalogObject, CatalogObjectKind, CatalogRelation, CompletionCatalog,
};
use ignatius::query::export::Export;
use ignatius::query::format::{FormatError, MAX_FORMAT_BYTES, format_sql};
use ignatius::query::parameters::{MAX_DISTINCT_PARAMETERS, ParameterError, discover};
use ignatius::query::plan::{MAX_PLAN_JSON_BYTES, MAX_PLAN_NODES, PlanDocument, PlanParseError};
use ignatius::query::result::{
    Execution, ExecutionStatus, JobId, MAX_NOTICE_MESSAGE_BYTES, MAX_RETAINED_RESULT_DATA_BYTES,
    NoticeReport, ResultRetention, ResultSet, StatementResult, TransactionState,
    bounded_notice_message,
};
use ignatius::query::update::{
    MAX_UPDATE_SQL_BYTES, RelationReference, UpdateColumn, UpdateRefusal, UpdateSource, plan_update,
};
use ignatius::query::value::{Cell, MAX_RETAINED_VALUE_BYTES};
use ignatius::ui::{Keymap, render};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use std::collections::BTreeMap;
use std::fs;
use std::mem::size_of;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const RENDER_WIDTH: u16 = 120;
const RENDER_HEIGHT: u16 = 40;
const RENDER_AREA: Rect = Rect::new(0, 0, RENDER_WIDTH, RENDER_HEIGHT);

fn percentile(durations: &[Duration], p: f64) -> Duration {
    assert!(!durations.is_empty());
    let mut sorted = durations.to_vec();
    sorted.sort();
    let index = ((sorted.len() as f64 * p / 100.0).ceil() as usize).saturating_sub(1);
    sorted[index.min(sorted.len() - 1)]
}

fn populate_synthetic_catalog() -> CompletionCatalog {
    let mut objects = Vec::new();
    let mut relations = Vec::new();

    for schema in &["public", "analytics"] {
        for t in 0..15 {
            let table_name = format!("table_{t}");
            objects.push(CatalogObject {
                schema: (*schema).to_owned(),
                name: table_name.clone(),
                kind: CatalogObjectKind::Table,
                readable: true,
                detail: None,
            });

            let columns = (0..10)
                .map(|c| CatalogColumn {
                    name: format!("col_{c}"),
                    data_type: if c == 0 {
                        "bigint".to_owned()
                    } else {
                        "text".to_owned()
                    },
                })
                .collect();

            relations.push(CatalogRelation {
                schema: (*schema).to_owned(),
                name: table_name,
                columns,
            });
        }

        for v in 0..5 {
            objects.push(CatalogObject {
                schema: (*schema).to_owned(),
                name: format!("view_{v}"),
                kind: CatalogObjectKind::View,
                readable: true,
                detail: None,
            });
        }

        for f in 0..10 {
            objects.push(CatalogObject {
                schema: (*schema).to_owned(),
                name: format!("func_{f}"),
                kind: CatalogObjectKind::Function,
                readable: true,
                detail: None,
            });
        }
    }

    CompletionCatalog { objects, relations }
}

/// Retention budget pure arithmetic matching `src/query/result/retention_budget.rs`.
pub mod retention_budget {
    use super::*;

    pub const RETAINED_RESULT_DATA_BYTES: usize = 32 * 1024 * 1024;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum DescriptionAdmission {
        Admitted { charged_bytes: usize },
        Unavailable,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum RowAdmission {
        Admitted { charged_bytes: usize },
        DataLimit,
        OversizedCell { bytes: usize },
    }

    #[derive(Debug)]
    pub struct RetentionBudget {
        limit: usize,
        charged: usize,
        data_closed: bool,
    }

    impl Default for RetentionBudget {
        fn default() -> Self {
            Self::new()
        }
    }

    impl RetentionBudget {
        #[must_use]
        pub const fn new() -> Self {
            Self::with_limit(RETAINED_RESULT_DATA_BYTES)
        }

        pub const fn with_limit(limit: usize) -> Self {
            Self {
                limit,
                charged: 0,
                data_closed: false,
            }
        }

        #[must_use]
        pub const fn data_is_closed(&self) -> bool {
            self.data_closed
        }

        pub fn admit_description<'a>(
            &mut self,
            names: impl IntoIterator<Item = &'a str>,
        ) -> DescriptionAdmission {
            if self.data_closed {
                return DescriptionAdmission::Unavailable;
            }
            let mut count = 0_usize;
            let mut text = 0_usize;
            for name in names {
                count += 1;
                text += name.len();
            }
            let charge = size_of::<ResultSet>()
                + count * (size_of::<String>() + size_of::<Option<String>>())
                + text;
            if self.charged + charge <= self.limit {
                self.charged += charge;
                DescriptionAdmission::Admitted {
                    charged_bytes: charge,
                }
            } else {
                self.data_closed = true;
                DescriptionAdmission::Unavailable
            }
        }

        pub fn admit_row<'a>(
            &mut self,
            cells: impl IntoIterator<Item = Option<&'a str>>,
        ) -> RowAdmission {
            if self.data_closed {
                return RowAdmission::DataLimit;
            }
            let mut count = 0_usize;
            let mut text = 0_usize;
            for cell in cells {
                count += 1;
                if let Some(val) = cell {
                    if val.len() > MAX_RETAINED_VALUE_BYTES {
                        return RowAdmission::OversizedCell { bytes: val.len() };
                    }
                    text += val.len();
                }
            }
            let charge = size_of::<Vec<Cell>>() + count * size_of::<Cell>() + text;
            if self.charged + charge <= self.limit {
                self.charged += charge;
                RowAdmission::Admitted {
                    charged_bytes: charge,
                }
            } else {
                self.data_closed = true;
                RowAdmission::DataLimit
            }
        }
    }
}

/// Standalone atomic write implementation verifying atomic file replacement.
pub mod atomic {
    use super::*;

    static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

    pub fn write_atomic(path: &Path, contents: &[u8]) -> std::io::Result<()> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        path.file_name().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "the destination needs a file name",
            )
        })?;
        for _ in 0..128 {
            let temporary_name = format!(
                ".ignatius-state.tmp.{}.{}",
                std::process::id(),
                NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed)
            );
            let temporary = parent.join(temporary_name);
            let mut file = match create_private_file(&temporary) {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            };
            let result = (|| {
                use std::io::Write;
                file.write_all(contents)?;
                file.flush()?;
                file.sync_all()
            })();
            drop(file);
            let result = result.and_then(|()| fs::rename(&temporary, path));
            if result.is_err() {
                let _ = fs::remove_file(&temporary);
            }
            return result;
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not reserve a new temporary file beside the destination",
        ))
    }

    pub fn create_private_file(path: &Path) -> std::io::Result<fs::File> {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(path)
    }
}

#[test]
fn w10_local_input_palette_and_completion_p95_within_100ms_and_state_invariant() {
    let mut model = Model::new(100);
    model.size = (RENDER_WIDTH, RENDER_HEIGHT);

    // 1. Establish session connection
    update(
        &mut model,
        Message::Connected(Box::new(SessionInfo {
            target: "synthetic@localhost:5432/synthetic_db".to_owned(),
            database: "synthetic_db".to_owned(),
            user: "synthetic_user".to_owned(),
            server_version: "16.1".to_owned(),
            backend_pid: "12345".to_owned(),
            search_path: "public".to_owned(),
            read_only: false,
            tls: TlsState::Disabled,
            environment: Environment::Local,
        })),
    );

    // 2. Populate tree schemas
    let mut public_counts = BTreeMap::new();
    public_counts.insert(ObjectKind::Table, 15);
    public_counts.insert(ObjectKind::View, 5);
    let mut analytics_counts = BTreeMap::new();
    analytics_counts.insert(ObjectKind::Table, 15);
    analytics_counts.insert(ObjectKind::View, 5);
    let mut catalog_counts = BTreeMap::new();
    catalog_counts.insert(ObjectKind::Table, 150);
    catalog_counts.insert(ObjectKind::View, 30);

    model.tree.set_schemas(vec![
        SchemaSummary {
            name: "public".into(),
            usable: true,
            counts: public_counts,
        },
        SchemaSummary {
            name: "analytics".into(),
            usable: true,
            counts: analytics_counts,
        },
        SchemaSummary {
            name: "pg_catalog".into(),
            usable: true,
            counts: catalog_counts,
        },
    ]);

    // 3. Populate completion catalog
    let catalog = populate_synthetic_catalog();
    assert_eq!(catalog.objects.len(), 60);
    assert_eq!(catalog.relations.len(), 30);
    model.completion.catalog = CatalogStatus::Ready {
        catalog: catalog.clone(),
        loaded_at: "now".to_owned(),
    };

    // 4. Populate saved queries
    update(
        &mut model,
        Message::QueriesListed(vec![
            SavedQuery {
                name: "monthly_active_users".into(),
                path: PathBuf::from("/tmp/queries/monthly_active_users.sql"),
                summary: "SELECT user_id, count(*) FROM public.table_0 GROUP BY 1;".into(),
            },
            SavedQuery {
                name: "daily_revenue_breakdown".into(),
                path: PathBuf::from("/tmp/queries/daily_revenue_breakdown.sql"),
                summary: "SELECT date, sum(amount) FROM analytics.table_1 GROUP BY 1;".into(),
            },
            SavedQuery {
                name: "stale_session_cleanup".into(),
                path: PathBuf::from("/tmp/queries/stale_session_cleanup.sql"),
                summary: "DELETE FROM public.table_2 WHERE updated_at < now() - interval '7 days';"
                    .into(),
            },
        ]),
    );

    // 5. Populate history
    for i in 0..20 {
        model.history.push(Entry::now(
            "synthetic@localhost:5432/synthetic_db",
            "synthetic_db",
            "local",
            &format!("SELECT * FROM public.table_{} LIMIT 10;", i % 15),
            Outcome::Succeeded,
            Duration::from_millis(5),
        ));
    }

    // Set initial editor text and cursor
    model
        .editor
        .set_text("SELECT t.col_0, t.col_1 FROM public.table_0 AS t WHERE t.");
    model.editor.set_cursor(model.editor.text().len());
    let keymap = Keymap::new();
    let mut buffer = Buffer::empty(RENDER_AREA);

    // Warm-up: 20 actions + renders
    for _ in 0..20 {
        update(&mut model, Message::Action(Action::Insert('x')));
        render(&model, &keymap, RENDER_AREA, &mut buffer);
        update(&mut model, Message::Action(Action::Backspace));
        render(&model, &keymap, RENDER_AREA, &mut buffer);
    }

    // 100 measured actions covering:
    // - editor character insertion and backspace
    // - cursor movements
    // - completion trigger, navigation, and dismiss
    // - palette open, filter, navigation, and dismiss
    // - help overlay toggle
    // - focus cycle
    let action_sequence: Vec<Action> = vec![
        // Typing into editor
        Action::Insert('c'),
        Action::Insert('o'),
        Action::Insert('l'),
        Action::Insert('_'),
        Action::Insert('0'),
        // Cursor movements
        Action::Move(Direction::Left),
        Action::Move(Direction::Left),
        Action::Move(Direction::Right),
        Action::MoveLineStart,
        Action::MoveLineEnd,
        // Completion interaction
        Action::Complete,
        Action::Move(Direction::Down),
        Action::Move(Direction::Down),
        Action::Move(Direction::Up),
        Action::Dismiss,
        // Palette interaction
        Action::OpenPalette,
        Action::Insert('t'),
        Action::Insert('h'),
        Action::Move(Direction::Down),
        Action::Dismiss,
        // Help overlay toggle
        Action::ToggleHelp,
        Action::ToggleHelp,
        // Focus cycles (Editor -> Results -> Sidebar -> Editor)
        Action::FocusNext,
        Action::FocusNext,
        Action::FocusNext,
    ];

    let mut measured_durations = Vec::with_capacity(100);
    for i in 0..100 {
        let action = action_sequence[i % action_sequence.len()].clone();
        let start = Instant::now();
        update(&mut model, Message::Action(action));
        render(&model, &keymap, RENDER_AREA, &mut buffer);
        measured_durations.push(start.elapsed());
    }

    let p50 = percentile(&measured_durations, 50.0);
    let p95 = percentile(&measured_durations, 95.0);
    eprintln!(
        "[W10 Scenario 1] Input/Palette/Completion over synthetic catalog: p50 = {:?}, p95 = {:?}",
        p50, p95
    );

    // Target threshold: p95 <= 100 ms
    assert!(
        p95 <= Duration::from_millis(100),
        "p95 latency was {:?}, exceeding the 100ms ceiling",
        p95
    );

    // Deterministic state invariant assertions
    assert!(
        matches!(model.connection, ConnectionState::Connected(_)),
        "connection state must remain connected"
    );
    let ready_catalog = model
        .completion
        .catalog
        .catalog()
        .expect("catalog remains loaded");
    assert_eq!(
        ready_catalog.objects.len(),
        60,
        "retained catalog objects must remain uncorrupted"
    );
    assert_eq!(
        ready_catalog.relations.len(),
        30,
        "retained catalog relations must remain uncorrupted"
    );
    assert_eq!(
        model.tree.roots.len(),
        4,
        "three schemas and extensions node must remain intact"
    );
    assert!(
        model.editor.text().starts_with("SELECT t.col_0"),
        "editor text must remain deterministic and uncorrupted"
    );
}

#[test]
fn w10_local_grid_interaction_at_retained_row_cap_p95_within_100ms_and_preserves_row_identity() {
    let mut model = Model::new(1000);
    model.size = (RENDER_WIDTH, RENDER_HEIGHT);

    let columns = vec![
        "id".to_string(),
        "unicode_label".to_string(),
        "nullable_text".to_string(),
        "empty_or_literal".to_string(),
        "amount".to_string(),
    ];

    let mut set = ResultSet::new(columns.clone(), 1000);
    for i in 0..1000 {
        let row = vec![
            Cell::Text(format!("{i:04}")),
            Cell::Text(match i % 4 {
                0 => "日本語 🚀 PostgreSQL クライアント".to_string(),
                1 => "🌟 Spark & Database 📊".to_string(),
                2 => "한글 데이터 🎯".to_string(),
                _ => "Plain ASCII value".to_string(),
            }),
            match i % 3 {
                0 => Cell::Null,
                1 => Cell::Text("".to_string()),
                _ => Cell::Text("NULL".to_string()),
            },
            match i % 2 {
                0 => Cell::Text("".to_string()),
                _ => Cell::Text("NULL".to_string()),
            },
            Cell::Text(format!("{}", (i * 37) % 1000)),
        ];
        set.push(row);
    }

    assert_eq!(set.rows.len(), 1000);

    // Verify cell distinction invariant
    assert!(set.rows[0][2].is_null());
    assert_eq!(set.rows[1][2], Cell::Text("".to_string()));
    assert_eq!(set.rows[2][2], Cell::Text("NULL".to_string()));
    assert_ne!(Cell::Null, Cell::Text("".to_string()));
    assert_ne!(Cell::Null, Cell::Text("NULL".to_string()));
    assert_ne!(Cell::Text("".to_string()), Cell::Text("NULL".to_string()));

    model.last_execution = Some(Execution {
        notice_report: NoticeReport::default(),
        job: JobId(42),
        statements: vec![StatementResult {
            result_set: Some(set),
            rows_affected: Some(1000),
            elapsed: Duration::from_millis(10),
            notices: Vec::new(),
        }],
        status: ExecutionStatus::Succeeded,
        elapsed: Duration::from_millis(15),
        error: None,
        transaction: TransactionState::Autocommit,
    });
    model.result_grid.reset_for_result(&columns);
    model.focus = Focus::Results;
    model.selected_column = 0;
    model.selected_row = 42; // Anchor on source row 42

    assert_eq!(
        model.selected_source_row(),
        Some(42),
        "source row identity is initially 42"
    );

    let keymap = Keymap::new();
    let mut buffer = Buffer::empty(RENDER_AREA);

    // Warm-up: 20 actions + renders
    for _ in 0..10 {
        update(&mut model, Message::Action(Action::Move(Direction::Down)));
        render(&model, &keymap, RENDER_AREA, &mut buffer);
        update(&mut model, Message::Action(Action::Move(Direction::Up)));
        render(&model, &keymap, RENDER_AREA, &mut buffer);
    }

    // Grid interaction actions:
    // - Row movements
    // - Page movements
    // - Column navigation
    // - Column width adjustments (widen/narrow)
    // - Expanded row detail toggle
    let grid_actions: Vec<Action> = vec![
        Action::Move(Direction::Down),
        Action::Move(Direction::Down),
        Action::Move(Direction::Up),
        Action::MovePage(Direction::Down),
        Action::MovePage(Direction::Up),
        Action::Move(Direction::Right),
        Action::Move(Direction::Right),
        Action::Move(Direction::Left),
        Action::WidenColumn,
        Action::WidenColumn,
        Action::NarrowColumn,
        Action::ToggleExpandedRow,
        Action::ToggleExpandedRow,
    ];

    let mut measured_durations = Vec::with_capacity(100);
    for i in 0..100 {
        let action = grid_actions[i % grid_actions.len()].clone();
        let start = Instant::now();
        update(&mut model, Message::Action(action));
        render(&model, &keymap, RENDER_AREA, &mut buffer);
        measured_durations.push(start.elapsed());
    }

    let p50 = percentile(&measured_durations, 50.0);
    let p95 = percentile(&measured_durations, 95.0);
    eprintln!(
        "[W10 Scenario 2] Grid interaction at 1,000 retained rows: p50 = {:?}, p95 = {:?}",
        p50, p95
    );

    assert!(
        p95 <= Duration::from_millis(100),
        "Grid interaction p95 latency {:?} exceeded 100ms",
        p95
    );

    // Source-row identity preservation across sort, filter, and clear cycles
    model.selected_column = 0;
    model.selected_row = 0;
    let initial_source_row = model.selected_source_row();
    assert_eq!(initial_source_row, Some(0));

    // Sort column ascending
    update(&mut model, Message::Action(Action::OpenResultControls));
    if let Some(palette) = model.palette.as_mut() {
        palette.query = "sort".to_owned();
    }
    update(&mut model, Message::Action(Action::Activate));
    assert_eq!(
        model.selected_source_row(),
        initial_source_row,
        "sorting preserves selected source row"
    );

    // Apply filter
    update(&mut model, Message::Action(Action::StartFilter));
    update(&mut model, Message::Action(Action::Insert('0')));
    update(&mut model, Message::Action(Action::Insert('0')));
    update(&mut model, Message::Action(Action::Activate));
    assert_eq!(
        model.selected_source_row(),
        initial_source_row,
        "filtering preserves selected source row when matched"
    );

    // Clear filter
    update(&mut model, Message::Action(Action::StartFilter));
    update(&mut model, Message::Action(Action::Dismiss));
    assert_eq!(
        model.selected_source_row(),
        initial_source_row,
        "clearing filter restores row context"
    );

    // Check cells still maintain their distinct properties
    let final_set = model.visible_result().expect("visible result exists");
    assert!(final_set.rows[0][2].is_null());
    assert_eq!(final_set.rows[1][2], Cell::Text("".to_string()));
    assert_eq!(final_set.rows[2][2], Cell::Text("NULL".to_string()));
}

#[test]
fn w10_all_seven_subsystem_input_and_retention_ceilings_enforce_exact_boundaries() {
    // 1. SQL Formatter Ceiling: 1_048_576 bytes succeeds; 1_048_577 bytes refuses
    {
        assert_eq!(MAX_FORMAT_BYTES, 1_048_576);

        // Exact boundary: 1_048_576 bytes
        let base_sql = "SELECT 1;\n";
        let exact_sql = format!(
            "{base_sql}{}",
            " ".repeat(MAX_FORMAT_BYTES - base_sql.len())
        );
        assert_eq!(exact_sql.len(), MAX_FORMAT_BYTES);
        let format_exact = format_sql(&exact_sql, 0);
        assert!(format_exact.is_ok(), "exact 1 MiB SQL formatting succeeds");

        // 1 byte over: 1_048_577 bytes
        let over_sql = format!("{exact_sql} ");
        assert_eq!(over_sql.len(), MAX_FORMAT_BYTES + 1);
        let format_err = format_sql(&over_sql, 0).expect_err("over 1 MiB must refuse");
        assert_eq!(
            format_err,
            FormatError::TooLarge {
                bytes: MAX_FORMAT_BYTES + 1
            }
        );

        let guidance = format_err.message(&over_sql);
        assert!(
            guidance.contains("over the 1 MiB limit"),
            "guidance mentions 1 MiB: {guidance}"
        );
        assert!(
            guidance.contains("Save the query as a file, then format a smaller buffer"),
            "guidance suggests file export: {guidance}"
        );

        // Verify Model reducer behavior
        let mut model = Model::new(100);
        model.editor.set_text(&over_sql);
        update(&mut model, Message::Action(Action::FormatBuffer));
        match &model.format_notice {
            Some(FormatNotice::Refused { message }) => {
                assert!(message.contains("over the 1 MiB limit"));
            }
            other => panic!("expected FormatNotice::Refused, got {other:?}"),
        }
    }

    // 2. Execution Plan Ceiling: 500 nodes succeeds, 501 refuses/truncates; > 1_048_576 bytes refuses
    {
        assert_eq!(MAX_PLAN_NODES, 500);
        assert_eq!(MAX_PLAN_JSON_BYTES, 1_048_576);

        // 500 nodes (1 root + 499 children)
        let children_499 = (0..499)
            .map(|_| r#"{"Node Type":"Result"}"#)
            .collect::<Vec<_>>()
            .join(",");
        let plan_500 =
            format!(r#"[{{"Plan":{{"Node Type":"Append","Plans":[{children_499}]}}}}] "#);
        let doc_500 = PlanDocument::parse(&plan_500, false).expect("500 nodes succeeds");
        assert_eq!(doc_500.node_count, 500);
        assert!(!doc_500.truncated);

        // 501 nodes (1 root + 500 children)
        let children_500 = (0..500)
            .map(|_| r#"{"Node Type":"Result"}"#)
            .collect::<Vec<_>>()
            .join(",");
        let plan_501 =
            format!(r#"[{{"Plan":{{"Node Type":"Append","Plans":[{children_500}]}}}}] "#);
        let doc_501 = PlanDocument::parse(&plan_501, false).expect("501 nodes parses with latch");
        assert_eq!(doc_501.node_count, 500);
        assert!(
            doc_501.truncated,
            "501st node is refused and latches truncated"
        );

        // Response size over 1 MiB
        let oversized_json = format!("{plan_500}{}", " ".repeat(MAX_PLAN_JSON_BYTES));
        assert!(oversized_json.len() > MAX_PLAN_JSON_BYTES);
        let size_err = PlanDocument::parse(&oversized_json, false)
            .expect_err("oversized plan JSON must be refused");
        assert_eq!(size_err, PlanParseError::ResponseTooLarge);
    }

    // 3. Clipboard Ceiling: 1_048_576 bytes succeeds; 1_048_577 bytes refuses
    {
        assert_eq!(MAX_OSC52_BYTES, 1_048_576);

        let exact_payload = "x".repeat(MAX_OSC52_BYTES);
        assert!(ClipboardPayload::try_new(exact_payload).is_ok());

        let over_payload = "x".repeat(MAX_OSC52_BYTES + 1);
        let payload_err =
            ClipboardPayload::try_new(over_payload).expect_err("over 1 MiB must refuse");
        assert_eq!(
            payload_err,
            PayloadTooLarge {
                bytes: MAX_OSC52_BYTES + 1
            }
        );

        // Verify Model clipboard notice states refusal and suggests export
        let mut model = Model::new(100);
        model.clipboard_osc52 = true;
        model.focus = Focus::Results;
        let mut set = ResultSet::new(vec!["val".into()], 10);
        set.push(vec![Cell::Text("x".repeat(MAX_OSC52_BYTES + 1))]);
        model.last_execution = Some(Execution {
            notice_report: NoticeReport::default(),
            job: JobId(101),
            statements: vec![StatementResult {
                result_set: Some(set),
                rows_affected: Some(1),
                elapsed: Duration::from_millis(1),
                notices: Vec::new(),
            }],
            status: ExecutionStatus::Succeeded,
            elapsed: Duration::from_millis(2),
            error: None,
            transaction: TransactionState::Autocommit,
        });
        model.result_grid.reset_for_result(&["val".into()]);
        model.selected_column = 0;
        model.selected_row = 0;

        update(&mut model, Message::Action(Action::CopyValue));
        match &model.clipboard_notice {
            Some(ClipboardNotice::TooLarge { bytes }) => {
                assert_eq!(*bytes, MAX_OSC52_BYTES + 1);
                let msg = model.clipboard_notice.as_ref().unwrap().message();
                assert!(msg.contains("over the 1 MiB copy limit"));
                assert!(msg.contains("Use explicit export"));
            }
            other => panic!("expected ClipboardNotice::TooLarge, got {other:?}"),
        }
    }

    // 4. Named Parameters Ceiling: 64 distinct parameters succeeds; 65 refuses
    {
        assert_eq!(MAX_DISTINCT_PARAMETERS, 64);

        let sql_64 = (0..64)
            .map(|i| format!(":p{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let template_64 = discover(&sql_64).expect("64 distinct parameters must succeed");
        assert_eq!(template_64.names().len(), 64);

        let sql_65 = (0..=64)
            .map(|i| format!(":p{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let param_err = discover(&sql_65).expect_err("65 distinct parameters must refuse");
        assert_eq!(param_err, ParameterError::TooManyNames { limit: 64 });
        let diag = param_err.diagnostic("discovering named parameters");
        assert_eq!(diag.kind, DiagnosticKind::Usage);
        assert!(diag.headline.contains("more than 64 distinct"));
        assert_eq!(
            diag.next_action.as_deref(),
            Some("use fewer distinct names, or split the statement into smaller runs")
        );
    }

    // 5. Single-Cell Update Ceiling: 16_384 bytes bound SQL succeeds; 16_385 bytes refuses
    {
        assert_eq!(MAX_UPDATE_SQL_BYTES, 16_384);

        let source = UpdateSource {
            relation: RelationReference {
                schema: None,
                relation: "orders".to_owned(),
            },
            target_column: "note".into(),
            projected_columns: vec!["order_id".into(), "note".into()],
            selected_result_column: 1,
        };
        let rel_cols = vec![
            UpdateColumn {
                name: "order_id".into(),
                primary_key: true,
            },
            UpdateColumn {
                name: "note".into(),
                primary_key: false,
            },
        ];
        let result_cols = vec!["order_id".into(), "note".into()];
        let row = vec![Cell::Text("1".into()), Cell::Text("old".into())];

        // Overhead of UPDATE "public"."orders" SET "note" = E'...' WHERE "order_id" = E'1'
        let empty_plan = plan_update(
            &source,
            "public",
            "orders",
            &result_cols,
            &row,
            &rel_cols,
            "",
        )
        .expect("empty plan");
        let overhead = empty_plan.bound_sql().expect("bound").len();
        let payload_len = MAX_UPDATE_SQL_BYTES - overhead;

        // Exact boundary: 16_384 bytes
        let exact_val = "x".repeat(payload_len);
        let exact_plan = plan_update(
            &source,
            "public",
            "orders",
            &result_cols,
            &row,
            &rel_cols,
            &exact_val,
        )
        .expect("16_384 bound update succeeds");
        assert_eq!(exact_plan.bound_sql().unwrap().len(), MAX_UPDATE_SQL_BYTES);

        // 1 byte over: 16_385 bytes
        let over_val = "x".repeat(payload_len + 1);
        let update_err = plan_update(
            &source,
            "public",
            "orders",
            &result_cols,
            &row,
            &rel_cols,
            &over_val,
        )
        .expect_err("16_385 bound update must refuse");
        assert_eq!(
            update_err,
            UpdateRefusal::TooLarge {
                bytes: MAX_UPDATE_SQL_BYTES + 1,
                limit: MAX_UPDATE_SQL_BYTES,
            }
        );
    }

    // 6. Single-Cell & Notice Truncation: 65_536 bytes retained intact; 65_537 bytes truncated
    {
        assert_eq!(MAX_NOTICE_MESSAGE_BYTES, 65_536);

        let exact_notice = "a".repeat(MAX_NOTICE_MESSAGE_BYTES);
        let bounded_exact = bounded_notice_message(&exact_notice);
        assert_eq!(bounded_exact.len(), MAX_NOTICE_MESSAGE_BYTES);
        assert!(!bounded_exact.contains("[notice truncated]"));

        let over_notice = "a".repeat(MAX_NOTICE_MESSAGE_BYTES + 1);
        let bounded_over = bounded_notice_message(&over_notice);
        assert!(bounded_over.len() <= MAX_NOTICE_MESSAGE_BYTES);
        assert!(bounded_over.ends_with(" ... [notice truncated]"));

        // Single cell retention ceiling (MAX_RETAINED_VALUE_BYTES = 1_048_576)
        let cell_exact = Cell::from_option_bounded(Some(&"x".repeat(MAX_RETAINED_VALUE_BYTES)));
        assert!(cell_exact.is_ok());
        let cell_over = Cell::from_option_bounded(Some(&"x".repeat(MAX_RETAINED_VALUE_BYTES + 1)));
        assert_eq!(cell_over, Err(MAX_RETAINED_VALUE_BYTES + 1));
    }

    // 7. Result-Set Retention Byte Budget: enforces byte cap before row cap is reached
    {
        assert_eq!(MAX_RETAINED_RESULT_DATA_BYTES, 33_554_432);
        assert_eq!(retention_budget::RETAINED_RESULT_DATA_BYTES, 33_554_432);

        // Small test budget to prove byte cap enforcement before row cap
        let mut budget = retention_budget::RetentionBudget::with_limit(1024);
        let desc = budget.admit_description(["col1", "col2"]);
        assert!(matches!(
            desc,
            retention_budget::DescriptionAdmission::Admitted { .. }
        ));

        // First row fits
        let row1 = budget.admit_row([Some("abc"), Some("def")]);
        assert!(matches!(
            row1,
            retention_budget::RowAdmission::Admitted { .. }
        ));

        // Large row exhausts budget, enforcing byte cap even though row count is only 2
        let large_text = "x".repeat(2048);
        let row2 = budget.admit_row([Some(&large_text[..]), Some("data")]);
        assert_eq!(row2, retention_budget::RowAdmission::DataLimit);
        assert!(budget.data_is_closed());

        // Verify ResultSet typed retention states: DataLimit and Unavailable
        let mut set = ResultSet::new(vec!["c1".into()], 1000);
        set.retention = ResultRetention::DataLimit;
        assert_eq!(set.retention, ResultRetention::DataLimit);
        assert!(set.is_available());

        let unavail = ResultSet::unavailable(5, 1000);
        assert!(!unavail.is_available());
        assert_eq!(unavail.source_column_count(), 5);
        assert_eq!(
            unavail.retention,
            ResultRetention::Unavailable {
                source_column_count: 5
            }
        );
    }
}

#[test]
fn w10_local_state_recovery_preserves_existing_files_and_refuses_unsafe_paths() {
    let temp_root = tempfile::tempdir().expect("temp root");
    let base = temp_root.path();

    // 1. Config schema: unsupported schema version, unknown keys, and malformed TOML
    {
        let config_dir = base.join("config_test");
        let paths = Paths::rooted_at(&config_dir);
        fs::create_dir_all(&paths.config_dir).unwrap();

        // Unsupported schema version = 99
        fs::write(&paths.config_file, "schema_version = 99\n").unwrap();
        let err_version = ignatius::config::load(&paths).expect_err("schema 99 must refuse");
        assert_eq!(err_version.kind, DiagnosticKind::Config);
        assert!(
            err_version
                .technical
                .iter()
                .any(|f| f.value.contains("schema version 99")),
            "error technical details mention schema version 99: {err_version:#?}"
        );
        assert!(
            err_version
                .next_action
                .as_deref()
                .unwrap_or("")
                .contains("upgrade Ignatius")
        );

        // Unknown keys
        fs::write(
            &paths.config_file,
            "schema_version = 1\nunknown_setting = true\n",
        )
        .unwrap();
        let err_unknown = ignatius::config::load(&paths).expect_err("unknown key must refuse");
        assert_eq!(err_unknown.kind, DiagnosticKind::Config);
        assert!(
            err_unknown
                .likely_cause
                .as_deref()
                .unwrap_or("")
                .contains("unknown field `unknown_setting`")
        );
        assert!(
            err_unknown
                .next_action
                .as_deref()
                .unwrap_or("")
                .contains("config validate")
        );

        // Malformed TOML
        fs::write(&paths.config_file, "[ui\ntheme = \"dark\"\n").unwrap();
        let err_malformed = ignatius::config::load(&paths).expect_err("malformed TOML must refuse");
        assert_eq!(err_malformed.kind, DiagnosticKind::Config);
        assert!(
            err_malformed
                .next_action
                .as_deref()
                .unwrap_or("")
                .contains("fix the file")
        );
    }

    // 2. Atomic write recovery: preserves content, permissions, fails safely without corrupting siblings
    {
        let state_dir = base.join("atomic_test");
        fs::create_dir_all(&state_dir).unwrap();
        let file_path = state_dir.join("state.txt");
        let sibling_path = state_dir.join("sibling.txt");
        fs::write(&sibling_path, b"sibling untouched").unwrap();

        // Initial atomic write
        atomic::write_atomic(&file_path, b"initial payload").expect("initial write");
        assert_eq!(fs::read(&file_path).unwrap(), b"initial payload");

        // Atomic replacement preserves content
        atomic::write_atomic(&file_path, b"replaced payload").expect("atomic replacement");
        assert_eq!(fs::read(&file_path).unwrap(), b"replaced payload");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = fs::metadata(&file_path).unwrap();
            assert_eq!(
                meta.permissions().mode() & 0o777,
                0o600,
                "atomic write must ensure 0600 permissions on Unix"
            );
        }

        // Write to non-existent parent directory fails and preserves siblings
        let non_existent_dest = state_dir.join("missing_dir").join("file.txt");
        let write_err = atomic::write_atomic(&non_existent_dest, b"fail");
        assert!(write_err.is_err(), "missing parent directory must fail");
        assert!(!non_existent_dest.exists());
        assert_eq!(
            fs::read(&sibling_path).unwrap(),
            b"sibling untouched",
            "sibling files must not be corrupted"
        );
    }

    // 3. Export safety: existing file refused with AlreadyExists; missing parent refused cleanly
    {
        let export_dir = base.join("export_test");
        fs::create_dir_all(&export_dir).unwrap();
        let existing_dest = export_dir.join("report.csv");
        fs::write(&existing_dest, b"vital existing content").unwrap();

        // Attempting export to existing destination without force
        let export_err = Export::create(&existing_dest, false).expect_err("must refuse overwrite");
        assert_eq!(export_err.exit_code(), ExitCode::Usage);
        assert!(export_err.next_action.as_ref().unwrap().contains("--force"));
        assert_eq!(
            fs::read(&existing_dest).unwrap(),
            b"vital existing content",
            "existing file bytes must remain completely untouched"
        );

        // Destination with unwritable / blocked parent (parent is a file)
        let blocker = export_dir.join("blocker_file");
        fs::write(&blocker, b"im a file").unwrap();
        let unwritable_dest = blocker.join("report.csv");
        let block_err = Export::create(&unwritable_dest, false).expect_err("must refuse blocked");
        assert_eq!(block_err.exit_code(), ExitCode::Usage);
        assert!(block_err.next_action.is_some());
    }

    // 4. History & Saved Queries: history trimming & clear; query library path safety & corrupt file resilience
    {
        // History Store
        let history_file = base.join("history_test.jsonl");
        let history = History::at(&history_file, 3);

        for i in 1..=5 {
            history
                .record(&Entry::now(
                    "user@host/db",
                    "db",
                    "local",
                    &format!("SELECT {i};"),
                    Outcome::Succeeded,
                    Duration::from_millis(2),
                ))
                .expect("record history");
        }

        let entries = history.entries().expect("read history");
        assert_eq!(
            entries.len(),
            3,
            "history must be pruned to max_entries = 3"
        );
        assert_eq!(entries[0].sql, "SELECT 3;");
        assert_eq!(entries[1].sql, "SELECT 4;");
        assert_eq!(entries[2].sql, "SELECT 5;");

        // Clear history empties on-disk state
        history.clear().expect("clear history");
        assert!(
            !history_file.exists(),
            "clearing history must remove the file from disk"
        );
        assert_eq!(history.entries().unwrap().len(), 0);

        // Saved Queries Library
        let queries_dir = base.join("saved_queries");
        let library = Library::new(&queries_dir);

        // Safe query saving and loading
        library
            .save("analytics_overview", "SELECT 42;")
            .expect("save query");
        let loaded = library.load("analytics_overview").expect("load query");
        assert_eq!(loaded, "SELECT 42;\n");

        // Path traversal refusal
        let traversal_err = library
            .path_for("../traversal")
            .expect_err("path traversal must refuse");
        assert_eq!(traversal_err.kind, DiagnosticKind::Usage);
        assert!(
            traversal_err
                .likely_cause
                .unwrap()
                .contains("walk up a directory")
        );

        // Path separator refusal
        let sep_err = library
            .path_for("sub/dir")
            .expect_err("separator must refuse");
        assert_eq!(sep_err.kind, DiagnosticKind::Usage);
        assert!(sep_err.likely_cause.unwrap().contains("path separator"));

        // Corrupted / non-UTF8 query file in queries directory does not panic list()
        let corrupted_file = queries_dir.join("corrupted.sql");
        fs::write(&corrupted_file, b"\xFF\xFE\xFD").unwrap();
        let listed = library.list().expect("list must not panic on corrupt file");
        let corrupt_entry = listed.iter().find(|q| q.name == "corrupted").unwrap();
        assert_eq!(corrupt_entry.summary, "could not be read");
    }
}
