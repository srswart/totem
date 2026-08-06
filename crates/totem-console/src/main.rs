//! The console binary. Only meaningful for `wasm32-unknown-unknown` (build
//! with `cargo build --target wasm32-unknown-unknown -p totem-console`);
//! the wasm-only launch path lives entirely in this file so the native
//! `cargo build`/`clippy --all-targets` runs never need `dioxus-web` or
//! `gloo-net` in their dependency graph (see `Cargo.toml`'s per-target
//! `[target.'cfg(target_arch = "wasm32")'.dependencies]`).

#[cfg(target_arch = "wasm32")]
fn main() {
    dioxus_web::launch::launch_cfg(totem_console::api::RootApp, dioxus_web::Config::default());
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!(
        "totem-console targets wasm32-unknown-unknown; build it with \
         `cargo build --target wasm32-unknown-unknown -p totem-console`."
    );
}
