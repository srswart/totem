//! Sensitivity proof for the workload driver (ADV-GATEWAY-008): a positive
//! control (the baseline profile reports clean, error-free numbers) and a
//! negative control (an injected-latency profile visibly worsens every
//! latency figure the report carries) — so a workload report can be trusted
//! to measure something real rather than a fixed shape regardless of input.

use std::time::Duration;

use totem_gateway::AppState;
use totem_gateway::eval::workload::{WorkloadProfile, run_workload};
use totem_store::Store;
use totem_store::corpus;

async fn seeded_state() -> AppState {
    let store = Store::in_memory().await.expect("embedded engine connects");
    store.migrate().await.expect("migrations apply");
    corpus::seed(&store).await.expect("corpus seeds");
    AppState::over(store)
}

#[tokio::test]
async fn baseline_profile_reports_clean_numbers() {
    let state = seeded_state().await;

    let report = run_workload(&state, &WorkloadProfile::baseline()).await;

    assert_eq!(
        report.error_count, 0,
        "no operation should fail against a freshly-seeded corpus"
    );
    assert!(report.recall.count > 0, "the baseline mix includes recalls");
    assert!(report.save.count > 0, "the baseline mix includes saves");
    assert!(report.throughput_ops_per_sec > 0.0);
    assert_eq!(report.profile_name, "baseline");
    assert_eq!(report.environment.os, std::env::consts::OS);
}

#[tokio::test]
async fn injected_latency_visibly_worsens_every_latency_figure() {
    let state = seeded_state().await;

    let baseline = run_workload(&state, &WorkloadProfile::baseline()).await;
    let degraded = run_workload(
        &state,
        &WorkloadProfile::degraded(Duration::from_millis(30)),
    )
    .await;

    assert!(
        degraded.recall.mean_ms > baseline.recall.mean_ms,
        "degraded recall mean {} should exceed baseline {}",
        degraded.recall.mean_ms,
        baseline.recall.mean_ms
    );
    assert!(
        degraded.save.mean_ms > baseline.save.mean_ms,
        "degraded save mean {} should exceed baseline {}",
        degraded.save.mean_ms,
        baseline.save.mean_ms
    );
    assert!(
        degraded.throughput_ops_per_sec < baseline.throughput_ops_per_sec,
        "an injected per-operation delay must reduce throughput: degraded {} vs baseline {}",
        degraded.throughput_ops_per_sec,
        baseline.throughput_ops_per_sec
    );
    assert_eq!(
        degraded.error_count, 0,
        "injected latency alone must not fail operations"
    );
}
