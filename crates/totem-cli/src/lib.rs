//! `totem`: the CLI for enrolling a repo with Totem, keeping its landscape
//! mirror in sync, and issuing local scoped credentials
//! (ADV-CLI-001; docs/solution-intent.md §3.3).
//!
//! This crate is lib+bin: `src/main.rs` is a thin `clap` dispatcher over the
//! functions here, so enrollment, sync, and credential issuance are testable
//! directly without spawning the binary.

#![warn(missing_docs)]

pub mod credential;
pub mod enroll;
pub mod error;
pub mod hook;
