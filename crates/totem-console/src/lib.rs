//! `totem-console`: the first Dioxus web console (ADV-CONSOLE-001) — a
//! read-only landscape dashboard and memory browser over `totem-gateway`'s
//! REST surface (docs/solution-intent.md §5, G5).
//!
//! `view_model` and `app` are target-agnostic: they compile and test on the
//! native host `cargo test`/`clippy` runs against, and are exercised here by
//! `dioxus-ssr` rendering rather than a browser. The wasm-only pieces
//! (`api`, the `main.rs` binary) join in the next commit.

pub mod app;
pub mod view_model;
