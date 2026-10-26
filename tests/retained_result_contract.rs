//! Real buffered ingestion and complete-stream controls for W10-013.
//!
//! Fixture opt-in is explicit. Cases share a process-local lock so ordinary CI
//! never runs the 40-MiB controls concurrently. The local evidence controller
//! additionally runs each named case in its own owned, deadline-bound child.
#![allow(clippy::print_stderr, clippy::print_stdout)]

use ignatius::config::Config;
use ignatius::connection::{ConnectionArgs, EnvSnapshot, SslMode, resolve};
use ignatius::postgres::{Session, StreamEvent, TlsState, session};
use ignatius::query::result::{
    Execution, ExecutionStatus, JobId, MAX_RETAINED_RESULT_DATA_BYTES, ResultSet,
    RetentionLossSummary, RetentionReason, TransactionState,
};
use ignatius::query::value::Cell;
use std::io::Read;
use std::mem::size_of;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

static LIVE_CASE: Mutex<()> = Mutex::new(());
const MIB: usize = 1024 * 1024;
const LARGE: &str = "SELECT repeat('x', 1048576) AS payload FROM generate_series(1, 40)";

struct Fixture {
    session: Session,
    runtime: tokio::runtime::Runtime,
    _gate: MutexGuard<'static, ()>,
}
impl Fixture {
    fn open(own_temp_write: bool) -> Option<Self> {
        let guard = LIVE_CASE.lock().expect("previous fixture case completed");
        let Ok(uri) = std::env::var("IGNATIUS_TEST_PG_URI") else {
            eprintln!("skipping: IGNATIUS_TEST_PG_URI is not set");
            return None;
        };
        if uri.is_empty() {
            eprintln!("skipping: IGNATIUS_TEST_PG_URI is empty");
            return None;
        }
        let mut target = resolve(
            Some(&uri),
            &ConnectionArgs::default(),
            &EnvSnapshot::from_process(),
            &Config::default().connection,
        )
        .expect("synthetic fixture target");
        // Only the explicitly named TEMP-table case admits session-local writes.
        target.read_only = !own_temp_write;
        let runtime = tokio::runtime::Runtime::new().expect("owned runtime");
        let opened = runtime.block_on(session::connect_reported(&target, Duration::from_secs(5)));
        let session = opened.value.expect("synthetic fixture session");
        match target.sslmode {
            SslMode::Disable => assert_eq!(session.info().tls, TlsState::Disabled),
            SslMode::Prefer => assert!(matches!(
                session.info().tls,
                TlsState::NotNegotiated
                    | TlsState::Active {
                        mode: SslMode::Prefer,
                        ..
                    }
            )),
            mode => assert!(
                matches!(session.info().tls, TlsState::Active { mode: observed, .. } if observed == mode)
            ),
        }
        let fixture = Self {
            session,
            runtime,
            _gate: guard,
        };
        fixture.health();
        Some(fixture)
    }
    fn execute(&self, sql: &str, cap: usize, job: u64) -> Execution {
        self.runtime
            .block_on(self.session.execute(sql, cap, JobId(job)))
    }
    fn health(&self) {
        let major = self
            .session
            .info()
            .server_version
            .split('.')
            .next()
            .unwrap();
        assert!(major.parse::<u32>().is_ok_and(|n| (14..=18).contains(&n)));
        if let Ok(expected) = std::env::var("IGNATIUS_PG_VERSION") {
            assert_eq!(major, expected, "configured fixture major");
        }
        let execution = self.execute(
            "SELECT current_setting('server_version'), count(*) FROM public.orders",
            1,
            9000,
        );
        assert_eq!(execution.status, ExecutionStatus::Succeeded);
        let set = execution.statements[0].result_set.as_ref().unwrap();
        assert_eq!(
            set.rows[0][0].raw(),
            Some(self.session.info().server_version.as_str())
        );
        assert_eq!(set.rows[0][1].raw(), Some("3"));
        println!(
            "health version={} orders=3",
            self.session.info().server_version
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            self.health();
        }
    }
}

fn occupied(execution: &Execution) -> usize {
    execution
        .statements
        .iter()
        .filter_map(|s| s.result_set.as_ref())
        .filter(|set| set.is_available())
        .map(|set| {
            size_of::<ResultSet>()
                + set.columns.len() * (size_of::<String>() + size_of::<Option<String>>())
                + set.columns.iter().map(String::len).sum::<usize>()
                + set
                    .column_types
                    .iter()
                    .flatten()
                    .map(String::len)
                    .sum::<usize>()
                + set
                    .rows
                    .iter()
                    .map(|row| {
                        size_of::<Vec<Cell>>()
                            + row.len() * size_of::<Cell>()
                            + row
                                .iter()
                                .filter_map(Cell::raw)
                                .map(str::len)
                                .sum::<usize>()
                    })
                    .sum::<usize>()
        })
        .sum()
}
fn assert_limited(execution: &Execution, expected_statements: usize) {
    assert_eq!(execution.status, ExecutionStatus::Succeeded);
    assert!(execution.error.is_none());
    assert_eq!(execution.transaction, TransactionState::Autocommit);
    assert_eq!(execution.statements.len(), expected_statements);
    let data = occupied(execution);
    assert!(data <= MAX_RETAINED_RESULT_DATA_BYTES);
    println!(
        "logical_occupied_bytes={data} native_result_set_bytes={}",
        size_of::<ResultSet>()
    );
}

#[test]
fn buffered_native_prefix_keeps_server_success_and_received_rows() {
    let Some(fixture) = Fixture::open(false) else {
        return;
    };
    let execution = fixture.execute(LARGE, 100, 1);
    assert_limited(&execution, 1);
    let statement = &execution.statements[0];
    assert_eq!(statement.rows_affected, Some(40));
    let set = statement.result_set.as_ref().unwrap();
    let description = size_of::<ResultSet>()
        + size_of::<String>()
        + size_of::<Option<String>>()
        + "payload".len();
    let row = size_of::<Vec<Cell>>() + size_of::<Cell>() + MIB;
    assert_eq!(
        set.retained(),
        (MAX_RETAINED_RESULT_DATA_BYTES - description) / row
    );
    assert_eq!(set.rows_seen, 40);
    assert_eq!(
        set.retention_reason(),
        Some(RetentionReason::RetainedResultLimit)
    );
    assert!(set.rows.iter().all(|row| {
        row.len() == 1
            && row[0]
                .raw()
                .is_some_and(|s| s.len() == MIB && s.bytes().all(|b| b == b'x'))
    }));
    println!(
        "retained={} received={} affected=40",
        set.retained(),
        set.rows_seen
    );
}

#[test]
fn cumulative_closure_counts_later_sets_without_fake_empty_or_replay() {
    let Some(fixture) = Fixture::open(false) else {
        return;
    };
    let execution = fixture.execute(
        &format!("SELECT 1 AS first; {LARGE}; SELECT 42 AS last"),
        100,
        2,
    );
    assert_limited(&execution, 3);
    assert_eq!(
        execution.statements[0].result_set.as_ref().unwrap().rows[0][0].raw(),
        Some("1")
    );
    assert_eq!(execution.statements[1].rows_affected, Some(40));
    let last = execution.statements[2].result_set.as_ref().unwrap();
    assert!(!last.is_available());
    assert_eq!(
        (last.source_column_count(), last.rows_seen, last.retained()),
        (1, 1, 0)
    );
    assert!(last.columns.is_empty() && last.column_types.is_empty());
    assert_eq!(execution.statements[2].rows_affected, Some(1));
    let loss = RetentionLossSummary::from_execution(&execution);
    assert_eq!(loss.limited_sets, 2);
    assert_eq!(loss.first.unwrap().ordinal, 2);
    println!(
        "limited_sets=2 first_limited_ordinal=2 later_columns=1 later_received=1 affected=40,1"
    );
}

#[test]
fn rowcap_zero_skips_oversized_values_without_closing_later_descriptions() {
    let Some(fixture) = Fixture::open(false) else {
        return;
    };
    let execution = fixture.execute(
        "SELECT repeat('x', 1048577) AS oversized; SELECT 42 AS later",
        0,
        3,
    );
    assert_limited(&execution, 2);
    for statement in &execution.statements {
        let set = statement.result_set.as_ref().unwrap();
        assert!(set.is_available());
        assert_eq!((set.retained(), set.rows_seen), (0, 1));
        assert_eq!(set.retention_reason(), Some(RetentionReason::RowCap));
        assert_eq!(statement.rows_affected, Some(1));
    }
    println!("rowcap=0 described_sets=2 received=1,1 no_per_cell_error=true");
}

#[test]
fn retention_keeps_completed_prefix_and_real_server_error() {
    let Some(fixture) = Fixture::open(false) else {
        return;
    };
    let execution = fixture.execute(&format!("{LARGE}; SELECT 1 / 0; SELECT 42"), 100, 4);
    assert_eq!(execution.status, ExecutionStatus::Failed);
    assert_eq!(execution.transaction, TransactionState::Autocommit);
    assert_eq!(execution.statements.len(), 1);
    assert_eq!(execution.statements[0].rows_affected, Some(40));
    assert!(
        execution
            .error
            .as_ref()
            .unwrap()
            .technical
            .iter()
            .any(|field| field.label == "SQLSTATE" && field.value == "22012")
    );
    assert!(occupied(&execution) <= MAX_RETAINED_RESULT_DATA_BYTES);
    assert_eq!(
        execution.statements[0]
            .result_set
            .as_ref()
            .unwrap()
            .rows_seen,
        40
    );
    println!("completed_prefix=1 affected=40 server_sqlstate=22012 later_statement_absent=true");
}

#[test]
fn temp_returning_limit_keeps_real_write_outcome_and_all_forty_rows() {
    let Some(fixture) = Fixture::open(true) else {
        return;
    };
    let created = fixture.execute(
        "CREATE TEMP TABLE ignatius_retention_owned (id integer, payload text)",
        1,
        5,
    );
    assert_eq!(created.status, ExecutionStatus::Succeeded);
    let execution = fixture.execute("INSERT INTO ignatius_retention_owned SELECT n, repeat('x', 1048576) FROM generate_series(1, 40) AS n RETURNING id, payload", 100, 6);
    assert_limited(&execution, 1);
    assert_eq!(execution.statements[0].rows_affected, Some(40));
    let set = execution.statements[0].result_set.as_ref().unwrap();
    assert_eq!(set.rows_seen, 40);
    assert!(set.retained() > 0 && set.retained() < 40);
    assert_eq!(
        set.retention_reason(),
        Some(RetentionReason::RetainedResultLimit)
    );
    for (index, row) in set.rows.iter().enumerate() {
        assert_eq!(row[0].raw().unwrap().parse::<usize>().unwrap(), index + 1);
        assert_eq!(row[1].raw().unwrap().len(), MIB);
    }
    drop(execution);
    let visible = fixture.execute("SELECT count(*) FROM ignatius_retention_owned", 1, 7);
    assert_eq!(visible.status, ExecutionStatus::Succeeded);
    assert_eq!(
        visible.statements[0].result_set.as_ref().unwrap().rows[0][0].raw(),
        Some("40")
    );
    println!("temp_returning_affected=40 subsequent_same_session_count=40 autocommit=true");
}

#[test]
fn streaming_control_keeps_complete_file_rows_and_bytes() {
    use ignatius::cli::output::{Format, OutputOptions, StreamWriter};
    let Some(fixture) = Fixture::open(false) else {
        return;
    };
    let dir = tempfile::tempdir().expect("owned output");
    // The task controller may retain an owned artifact on failure; ordinary CI
    // uses automatic temporary cleanup. This is a test-only output seam.
    let output = std::env::var_os("IGNATIUS_TEST_OUTPUT_DIR").map(std::path::PathBuf::from);
    let destination = output.as_deref().unwrap_or(dir.path()).join("complete.csv");
    let mut export = ignatius::query::Export::create(&destination, false).unwrap();
    let mut writer = StreamWriter::new(OutputOptions {
        format: Format::Csv,
        header: false,
        ..OutputOptions::default()
    })
    .unwrap();
    let mut accepted = 0usize;
    let mut sink = |event| -> std::io::Result<()> {
        match event {
            StreamEvent::Columns(columns) => writer.columns(export.writer(), columns),
            StreamEvent::Row(row) => {
                assert_eq!(row.len(), 1);
                assert_eq!(row[0].raw().unwrap().len(), MIB);
                // Fixed workload bound, independent from buffered ingestion.
                accepted = accepted.checked_add(MIB + 1).unwrap();
                assert!(accepted <= 64 * MIB);
                writer.row(export.writer(), &row)?;
                export.count_row();
                Ok(())
            }
        }
    };
    let reported = fixture
        .runtime
        .block_on(fixture.session.stream_reported(LARGE, &mut sink));
    assert_eq!(reported.value.unwrap(), 40);
    assert_eq!((writer.rows(), export.rows()), (40, 40));
    assert!(!reported.notice_report.counts.needs_report());
    let finished = export.finish().unwrap();
    assert_eq!(finished.rows, 40);
    assert_eq!(
        std::fs::metadata(&finished.path).unwrap().len(),
        40 * (MIB as u64 + 1)
    );
    let mut file = std::fs::File::open(&finished.path).unwrap();
    let mut block = [0u8; 8192];
    let mut position = 0usize;
    loop {
        let read = file.read(&mut block).unwrap();
        if read == 0 {
            break;
        }
        for byte in &block[..read] {
            assert_eq!(
                *byte,
                if position % (MIB + 1) == MIB {
                    b'\n'
                } else {
                    b'x'
                }
            );
            position += 1;
        }
    }
    assert_eq!(position, 40 * (MIB + 1));
    println!(
        "stream_complete_rows=40 complete_bytes={position} full_content_verified=true owned_temp_cleanup=true"
    );
}
