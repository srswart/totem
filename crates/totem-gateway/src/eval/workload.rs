//! The workload driver (ADV-GATEWAY-008): replays a configurable recall/save
//! mix against a seeded store and reports latency, throughput, and error
//! counts with the environment stamped alongside, so a performance figure
//! never travels without saying what produced it (the performance protocol's
//! "missing provenance is indeterminate" rule).
//!
//! Driving [`ops::recall`] / [`ops::save`] directly, not either transport:
//! `ops.rs`'s own doc comment is explicit that both the REST and MCP surfaces
//! call exactly these two functions and add no scope-relevant work of their
//! own, so a single measurement here is representative of both without
//! doubling the harness. What this module does not measure is HTTP/MCP
//! marshalling overhead itself — a separate concern from the store/access-log
//! cost this module isolates.

use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::Serialize;
use totem_core::{ActorId, Author, Harness, MemoryCategory, RepoId, Scope, SessionId};
use totem_store::corpus;

use crate::Caller;
use crate::ops::{self, RecallInput, SaveInput};
use crate::state::AppState;

/// One workload run's configuration: the recall/save mix, its size, and how
/// concurrently it runs.
#[derive(Debug, Clone)]
pub struct WorkloadProfile {
    /// A short label carried into the report, so a comparison across runs
    /// says what was compared.
    pub name: &'static str,
    /// Total operations to run (recall + save combined).
    pub iterations: usize,
    /// How many operations run concurrently at once.
    pub concurrency: usize,
    /// Every `mix_period`-th operation (by index) is a save; the rest are
    /// recalls. `4` means a 3:1 recall:save mix.
    pub mix_period: usize,
    /// Extra delay injected before every operation, so the harness can prove
    /// itself sensitive to degradation without depending on the host's own
    /// performance characteristics.
    pub injected_latency: Duration,
}

impl WorkloadProfile {
    /// A small, deterministic default profile: 40 operations, 4-way
    /// concurrency, a 3:1 recall:save mix, no injected latency — the
    /// positive control.
    pub fn baseline() -> Self {
        Self {
            name: "baseline",
            iterations: 40,
            concurrency: 4,
            mix_period: 4,
            injected_latency: Duration::ZERO,
        }
    }

    /// [`Self::baseline`] with an injected per-operation delay — the
    /// negative control: a config that must visibly worsen every latency
    /// figure the report carries.
    pub fn degraded(extra: Duration) -> Self {
        Self {
            name: "degraded",
            injected_latency: extra,
            ..Self::baseline()
        }
    }
}

/// Where and when a report was produced, so a performance figure can be
/// compared against a later run instead of read in isolation.
#[derive(Debug, Clone, Serialize)]
pub struct EnvironmentStamp {
    /// `std::env::consts::OS`.
    pub os: &'static str,
    /// `std::env::consts::ARCH`.
    pub arch: &'static str,
    /// `std::thread::available_parallelism()`, or `1` if the host would not
    /// say.
    pub available_parallelism: usize,
    /// When this report was produced.
    pub captured_at: DateTime<Utc>,
}

impl EnvironmentStamp {
    fn capture() -> Self {
        Self {
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            available_parallelism: std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1),
            captured_at: Utc::now(),
        }
    }
}

/// Latency distribution over one operation kind (`recall` or `save`).
#[derive(Debug, Clone, Serialize)]
pub struct LatencyStats {
    /// How many samples this distribution summarizes.
    pub count: usize,
    /// The fastest observed operation, in milliseconds.
    pub min_ms: f64,
    /// The mean of every observed operation, in milliseconds.
    pub mean_ms: f64,
    /// The 95th-percentile observed operation, in milliseconds.
    pub p95_ms: f64,
    /// The slowest observed operation, in milliseconds.
    pub max_ms: f64,
}

impl LatencyStats {
    fn from_samples(mut samples: Vec<f64>) -> Self {
        if samples.is_empty() {
            return Self {
                count: 0,
                min_ms: 0.0,
                mean_ms: 0.0,
                p95_ms: 0.0,
                max_ms: 0.0,
            };
        }
        samples.sort_by(|a, b| a.partial_cmp(b).expect("latency samples are never NaN"));
        let count = samples.len();
        let sum: f64 = samples.iter().sum();
        let p95_index = (((count as f64) * 0.95).ceil() as usize)
            .saturating_sub(1)
            .min(count - 1);
        Self {
            count,
            min_ms: samples[0],
            mean_ms: sum / count as f64,
            p95_ms: samples[p95_index],
            max_ms: samples[count - 1],
        }
    }
}

/// One workload run's full report.
#[derive(Debug, Clone, Serialize)]
pub struct WorkloadReport {
    /// The profile's own label ([`WorkloadProfile::name`]).
    pub profile_name: &'static str,
    /// Where and when this report was produced.
    pub environment: EnvironmentStamp,
    /// Latency over every recall in this run.
    pub recall: LatencyStats,
    /// Latency over every save in this run.
    pub save: LatencyStats,
    /// Operations that returned an error instead of a result.
    pub error_count: usize,
    /// Wall-clock time the whole run took, in milliseconds.
    pub total_duration_ms: f64,
    /// `(recall.count + save.count + error_count) / total_duration`.
    pub throughput_ops_per_sec: f64,
}

/// Run `profile` against `state`'s store, replaying a recall/save mix through
/// [`ops::recall`] / [`ops::save`], and report latency, throughput, and error
/// counts with the environment stamped alongside.
///
/// Every operation reads/writes as [`corpus::NOVA`] against
/// [`corpus::ROCKET`] project scope — the same identity and project the
/// synthetic corpus's own golden queries use (ADV-STORE-005), so a run
/// against a freshly-seeded corpus has real records to recall and a writable
/// scope to save into.
pub async fn run_workload(state: &AppState, profile: &WorkloadProfile) -> WorkloadReport {
    let environment = EnvironmentStamp::capture();
    let caller = Caller::Trusted;
    let repo = RepoId::new(corpus::ROCKET).expect("corpus project id is valid");
    let actor = ActorId::new(corpus::NOVA).expect("corpus actor id is valid");

    let mut recall_samples = Vec::new();
    let mut save_samples = Vec::new();
    let mut error_count = 0usize;

    let start = Instant::now();
    let concurrency = profile.concurrency.max(1);
    let indices: Vec<usize> = (0..profile.iterations).collect();

    for chunk in indices.chunks(concurrency) {
        let mut set = tokio::task::JoinSet::new();
        for &i in chunk {
            let state = state.clone();
            let caller = caller.clone();
            let repo = repo.clone();
            let actor = actor.clone();
            let injected = profile.injected_latency;
            let is_save = profile.mix_period > 0 && i % profile.mix_period == 0;
            set.spawn(async move {
                // The timer opens BEFORE the injected delay, deliberately.
                //
                // It used to open after it, which meant the injected latency
                // reached no reported figure at all: `degraded` and `baseline`
                // measured the same quantity, and
                // `injected_latency_visibly_worsens_every_latency_figure`
                // passed or failed on scheduler noise. It failed in CI roughly
                // one run in three and was read as a flaky test for two days.
                //
                // It was not flaky. It was asserting something false and
                // passing by luck — which is worse, because a flaky test
                // eventually gets fixed and a lucky one gets merged past.
                //
                // This field's own contract is that the delay proves the
                // harness "sensitive to degradation without depending on the
                // host's own performance characteristics". Timing around the
                // sleep is what makes that true rather than aspirational.
                let op_start = Instant::now();
                if injected > Duration::ZERO {
                    tokio::time::sleep(injected).await;
                }
                let session = SessionId::new(format!("eval-workload-{i}"))
                    .expect("generated session id is valid");
                let ok = if is_save {
                    let input = SaveInput {
                        project: Some(repo.clone()),
                        teams: vec![],
                        category: MemoryCategory::Context,
                        scope: Scope::Project(repo),
                        subject: None,
                        body: format!("eval-workload synthetic note #{i}"),
                        tags: vec!["eval-workload".to_string()],
                        author: Author::Agent(actor),
                        harness: Harness::CloudAgent,
                        session,
                        turn: None,
                    };
                    ops::save(&state, input, &caller, "eval:workload")
                        .await
                        .is_ok()
                } else {
                    let input = RecallInput {
                        actor,
                        project: Some(repo),
                        teams: vec![],
                        query: None,
                        categories: vec![],
                        since: None,
                        limit: Some(10),
                        harness: Harness::CloudAgent,
                        session,
                        turn: None,
                    };
                    ops::recall(&state, input, &caller, "eval:workload")
                        .await
                        .is_ok()
                };
                let elapsed_ms = op_start.elapsed().as_secs_f64() * 1000.0;
                (is_save, elapsed_ms, ok)
            });
        }
        while let Some(result) = set.join_next().await {
            let (is_save, elapsed_ms, ok) = result.expect("workload task does not panic");
            if !ok {
                error_count += 1;
                continue;
            }
            if is_save {
                save_samples.push(elapsed_ms);
            } else {
                recall_samples.push(elapsed_ms);
            }
        }
    }

    let total_duration = start.elapsed();
    let total_ops = recall_samples.len() + save_samples.len() + error_count;
    let throughput_ops_per_sec = if total_duration.as_secs_f64() > 0.0 {
        total_ops as f64 / total_duration.as_secs_f64()
    } else {
        0.0
    };

    WorkloadReport {
        profile_name: profile.name,
        environment,
        recall: LatencyStats::from_samples(recall_samples),
        save: LatencyStats::from_samples(save_samples),
        error_count,
        total_duration_ms: total_duration.as_secs_f64() * 1000.0,
        throughput_ops_per_sec,
    }
}
