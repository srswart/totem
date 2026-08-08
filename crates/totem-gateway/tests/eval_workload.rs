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

    const INJECTED_MS: u64 = 30;
    let baseline = run_workload(&state, &WorkloadProfile::baseline()).await;
    let degraded = run_workload(
        &state,
        &WorkloadProfile::degraded(Duration::from_millis(INJECTED_MS)),
    )
    .await;

    // Two assertions, and neither compares two noisy runs for a verdict.
    //
    // 1. A hard causal floor. A per-operation sleep of INJECTED_MS puts a
    //    lower bound under the reported mean that no host slowness can push
    //    it beneath. If the injection ever leaves the timed region again —
    //    the defect that made this test look flaky for two days — this fails
    //    immediately and unambiguously, on every machine.
    let floor = INJECTED_MS as f64;
    assert!(
        degraded.recall.mean_ms >= floor,
        "a {INJECTED_MS}ms per-operation delay must reach the reported recall mean: \
         got {} ms, so the injection is outside the timed region",
        degraded.recall.mean_ms
    );
    assert!(
        degraded.save.mean_ms >= floor,
        "a {INJECTED_MS}ms per-operation delay must reach the reported save mean: got {} ms",
        degraded.save.mean_ms
    );

    // 2. Sensitivity: the report is not a fixed shape, it moves with input.
    //
    //    The tolerance is measured, not tuned. Five consecutive baseline runs
    //    on a workstation gave recall means of 106.4-110.0 ms — a spread of
    //    **3.6 ms**. Requiring the observed shift to be at least half the
    //    injection (15 ms) leaves roughly 4x that spread as headroom for a
    //    contended CI runner, while still failing loudly if the injection
    //    stops landing.
    //
    //    Note what this does NOT assert: `degraded.mean > baseline.mean`.
    //    That is a comparison of two independent measurements, and it is what
    //    the test used to do — which is how it managed to fail on noise while
    //    the thing it was meant to detect was broken the whole time.
    let observed_shift = degraded.recall.mean_ms - baseline.recall.mean_ms;
    assert!(
        observed_shift >= floor / 2.0,
        "injecting {INJECTED_MS}ms moved the reported recall mean by only {observed_shift:.1} ms \
         (baseline {:.1} -> degraded {:.1}); the report is not tracking its input",
        baseline.recall.mean_ms,
        degraded.recall.mean_ms
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
