//! `totem-console`: the first Dioxus web console (ADV-CONSOLE-001) — a
//! read-only landscape dashboard and memory browser over `totem-gateway`'s
//! REST surface (docs/solution-intent.md §5, G5).
//!
//! Split so the wasm-only pieces (`api`, the `main.rs` binary) are the only
//! things gated to `wasm32-unknown-unknown`: `view_model` and `app` compile
//! and test on any target, including the native host `cargo test`/`clippy`
//! runs against ([`Cargo.toml`]'s per-target `dioxus-web`/`gloo-net`
//! dependencies keep those crates out of that build entirely).

pub mod app;
pub mod view_model;

#[cfg(target_arch = "wasm32")]
pub mod api;
pub mod auth;
