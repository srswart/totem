//! Evaluation tooling (ADV-GATEWAY-008): the harness the quality
//! (ADV-CORE-005) and performance (ADV-GATEWAY-007) evaluations run against.
//!
//! [`workload`] replays a configurable recall/save mix through the shared
//! `ops` layer and reports latency, throughput, and environment provenance.
//! [`quality`] scores recall ranking against the golden query set
//! ADV-STORE-005's synthetic corpus ships. Both are proven sensitive by their
//! own tests (`tests/eval_workload.rs`, `tests/eval_quality.rs`): a
//! deliberately degraded input visibly worsens the reported numbers, and the
//! corpus's own golden reader reports as expected.
//!
//! "Runnable locally and from CI" is `cargo test -p totem-gateway --test
//! eval_workload --test eval_quality` — a library call proven end-to-end by
//! its own tests, not a standalone terminal binary. Same scoping choice
//! ADV-STORE-005 made for its corpus generator (see that advance's
//! Corrections section): the immediate consumers are evaluation advances
//! that call this crate directly, so a CLI wrapper printing JSON is a small
//! addition any of them — or a future workstation advance — can add on top
//! without touching this module.

pub mod quality;
pub mod workload;
