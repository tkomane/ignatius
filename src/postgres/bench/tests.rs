use super::*;
use std::sync::atomic::AtomicUsize;

#[tokio::test]
async fn pre_cancelled_benchmark_stops_before_running_any_iteration() {
    let cancel = BenchmarkCancelHandle::new();
    cancel
        .request_cancel()
        .await
        .expect("cancelling with no active query returns immediately");
    assert!(cancel.is_cancelled());

    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let err = collect_samples_cancellable(
        "Baseline (A)",
        "SELECT 1",
        &BenchConfig::new(2, 3, true),
        Some(&cancel),
        move || {
            let counter = Arc::clone(&counter);
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok((1.0, 1))
            }
        },
    )
    .await
    .expect_err("pre-cancelled benchmark stops cleanly");

    assert_eq!(err.kind, DiagnosticKind::Cancelled);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn cancelling_mid_warmup_stops_before_next_warmup_or_measured_iteration() {
    let cancel = BenchmarkCancelHandle::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let cancel_clone = cancel.clone();
    let counter = Arc::clone(&calls);

    let err = collect_samples_cancellable(
        "Baseline (A)",
        "SELECT 1",
        &BenchConfig::new(3, 5, true),
        Some(&cancel),
        move || {
            let cancel_clone = cancel_clone.clone();
            let counter = Arc::clone(&counter);
            async move {
                let n = counter.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    cancel_clone
                        .request_cancel()
                        .await
                        .expect("non-blocking cancel request");
                }
                Ok((1.0, 1))
            }
        },
    )
    .await
    .expect_err("cancel during warmup stops before next warmup");

    assert_eq!(err.kind, DiagnosticKind::Cancelled);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn cancelling_mid_measured_stops_before_next_measured_iteration() {
    let cancel = BenchmarkCancelHandle::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let cancel_clone = cancel.clone();
    let counter = Arc::clone(&calls);

    let err = collect_samples_cancellable(
        "Baseline (A)",
        "SELECT 1",
        &BenchConfig::new(1, 5, true),
        Some(&cancel),
        move || {
            let cancel_clone = cancel_clone.clone();
            let counter = Arc::clone(&counter);
            async move {
                let n = counter.fetch_add(1, Ordering::SeqCst);
                if n == 2 {
                    cancel_clone
                        .request_cancel()
                        .await
                        .expect("non-blocking cancel request");
                }
                Ok((1.0, 1))
            }
        },
    )
    .await
    .expect_err("cancel during measured stops before remaining runs");

    assert_eq!(err.kind, DiagnosticKind::Cancelled);
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}
