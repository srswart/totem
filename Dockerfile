# Totem gateway for Fly.io (ADV-INFRA-002).
#
# Built with the `rocksdb` feature: this image is the DEP-001 durable single
# instance, and the gateway refuses to start with TOTEM_DATA_DIR set unless
# the feature is compiled in.
# Pinned to 1.96 (not the workspace's declared MSRV of 1.85): dependencies
# have moved past it — fastnum 0.7.5 requires 1.94 and darling 0.23 requires
# 1.88 — so the declared MSRV is already unachievable and only holds locally
# because workstations run a newer toolchain. Pinned rather than `latest` so a
# Rust release cannot break a deploy mid-trial, the same reasoning as the
# surrealdb pin.
# A shared base for the dependency-caching stages below (ADV-INFRA-006).
#
# Everything the compile needs is installed *here*, above any `COPY` of our
# own sources, so a source change cannot invalidate it: RocksDB's build needs
# a C++ toolchain, SurrealDB's TLS needs pkg-config, and `fastembed` brings
# ONNX Runtime, which needs a C++ toolchain to link.
#
# Trixie rather than bookworm for every stage (ADV-STORE-008): the prebuilt
# ONNX Runtime `ort` links against libstdc++ 13+ symbols (`_M_replace_cold`),
# which bookworm's GCC 12 runtime does not define. The failure is a linker
# error deep in `ort-sys` that names a C++ mangled symbol and nothing about
# Debian releases. The runtime stage must match, since the binary needs that
# libstdc++ at run time too.
FROM rust:1.96-slim-trixie AS chef
RUN apt-get update && apt-get install -y --no-install-recommends \
        clang libclang-dev pkg-config libssl-dev build-essential \
    && rm -rf /var/lib/apt/lists/*
# Pinned, for the same reason the toolchain and surrealdb are pinned: a
# release of a build tool must not be able to break a deploy mid-trial.
RUN cargo install cargo-chef --version 0.1.77 --locked
WORKDIR /build

# Reduce the whole workspace to a dependency recipe: a small JSON file
# describing what to build, with none of our source in it. This stage still
# rebuilds on every source change — it is seconds — but its *output* only
# changes when a manifest does, which is what makes the expensive layer below
# cacheable.
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
# How many crates compile at once. Computed, not chosen (ADV-INFRA-008).
#
# **The constraint is memory, not cores.** Each concurrent `rustc` holds its
# own working set, and adding `fastembed` put ONNX Runtime alongside
# `surrealdb-core` — already the largest crate here — until the remote builder
# OOM-killed one (SIGKILL). That read as "could not compile surrealdb-core",
# naming the victim rather than the cause (ADV-STORE-008).
#
# The fix then was `CARGO_BUILD_JOBS=2`, which held for months and cost a
# great deal: measured at 646s of dependency compile locally against 262s at
# 6 jobs — **2.47x** — on the step that is ~94% of a cold build.
#
# So: `min(cores, RAM_in_GB)`, floored at 2. One gigabyte per job is a rule of
# thumb, but a measured one: ten concurrent jobs completed this graph on
# 7.7 GB, so ~0.8 GB/job sufficed and 1.0 leaves headroom.
#
# Computed rather than pinned because the number belongs to the *builder*, and
# we do not control that machine. Fly's is a managed Depot builder whose shape
# is not visible from here and can change without notice; a literal would
# encode an assumption about it that is wrong in one direction or the other.
# Override with `--build-arg CARGO_BUILD_JOBS=2` if a build ever misbehaves.
ARG CARGO_BUILD_JOBS=""
RUN if [ -n "$CARGO_BUILD_JOBS" ]; then \
        echo "$CARGO_BUILD_JOBS" > /jobs; \
    else \
        awk -v c="$(nproc)" '/MemTotal/ {g=int($2/1048576); j=(g<c?g:c); print (j<2?2:j)}' \
            /proc/meminfo > /jobs; \
    fi; \
    echo "cargo build jobs: $(cat /jobs) (nproc=$(nproc), \
$(awk '/MemTotal/{printf "%.1fGB", $2/1048576}' /proc/meminfo))"

# The expensive layer, and the whole point of this advance: compile every
# dependency — SurrealDB, RocksDB, ONNX Runtime — from the recipe alone.
# Docker caches it against `recipe.json`, so it is reused until a dependency
# actually changes, and a source-only change skips it entirely.
#
# The flags MUST match the real build below. `cook` compiles with whatever it
# is given, so a mismatched feature set produces a cache that is silently
# useless: the layer is reused, then `cargo build` rebuilds everything anyway
# because the fingerprints differ. That failure costs the full build time
# while looking like a cache hit, which is why the two lines are kept
# adjacent.
COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --jobs "$(cat /jobs)" --recipe-path recipe.json \
        -p totem-gateway --features rocksdb,fastembed

COPY . .
RUN cargo build --release --jobs "$(cat /jobs)" -p totem-gateway --features rocksdb,fastembed

# Bake the model weights into the image (ADV-STORE-008). Cold construction is
# ~276s because of this download, against ~124ms warm (EMB-004) — a first boot
# paying that would fail its health check long before it served anything. Here
# a slow step is merely slow.
# FASTEMBED_CACHE_DIR, not _PATH. The wrong name is silently ignored — the
# library falls back to `.fastembed_cache` relative to the working directory,
# the warm step succeeds, and the failure surfaces three steps later as a
# COPY that finds nothing. Hence the check below: it fails at the step that
# caused it, saying which directory is empty.
ENV FASTEMBED_CACHE_DIR=/models
# Grouped, not `A && B || C`: with the flat form, a warm step that fails for
# any other reason still falls into the `||` branch and is misreported as an
# empty cache directory. Here a warming failure fails on its own message.
# Retried, because this step reaches the public internet and nothing else in
# the build does (ADV-INFRA-008). Observed 2026-08-08: four builds in two
# hours and the fifth failed instantly with "Failed to retrieve
# onnx/model.onnx" — a rate limit, not a timeout. The compile had already
# succeeded; a deploy died at a step unrelated to anything that changed.
#
# Three attempts with a widening pause. This does not survive a real outage —
# for that the weights want vendoring into a base image — but it turns the
# common transient into a slow build rather than a failed one.
RUN for attempt in 1 2 3; do \
        /build/target/release/totem-gateway --warm-embedder && break; \
        if [ "$attempt" = 3 ]; then \
            echo "FATAL: could not fetch the model weights after 3 attempts."; \
            echo "This step is the build's only network dependency; a rate"; \
            echo "limit or an upstream outage looks exactly like this."; \
            exit 1; \
        fi; \
        echo "warm-embedder attempt $attempt failed; retrying in $((attempt * 20))s"; \
        sleep $((attempt * 20)); \
    done \
    && { test -n "$(ls -A /models 2>/dev/null)" \
         || { echo "FATAL: /models is empty after warming — the weights went elsewhere."; \
              echo "Found instead:"; \
              find / -name '*.onnx' -not -path '*/target/*' 2>/dev/null | head; \
              exit 1; }; }

# The console bundle (ADV-GATEWAY-010), built in its own stage so a console
# change does not invalidate the gateway's (much longer) compile.
FROM rust:1.96-slim-trixie AS console
# dioxus-cli links against OpenSSL; without these its build fails with
# "Could not find directory of OpenSSL installation". This layer sits before
# `COPY . .` so Docker caches the CLI build across deploys — a source change
# does not recompile it.
RUN apt-get update && apt-get install -y --no-install-recommends \
        curl ca-certificates pkg-config libssl-dev build-essential \
    && rm -rf /var/lib/apt/lists/*
RUN rustup target add wasm32-unknown-unknown
RUN cargo install dioxus-cli --version 0.7.3 --locked
WORKDIR /build
COPY . .
RUN cd crates/totem-console && dx build --platform web --release

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/totem-gateway /usr/local/bin/totem-gateway
# The baked weights, and the variable that makes the runtime look for them
# here rather than trying to download to a working directory it cannot write.
COPY --from=builder /models /models
ENV FASTEMBED_CACHE_DIR=/models
COPY --from=console /build/target/dx/totem-console/release/web/public /console
ENV TOTEM_CONSOLE_DIR=/console

# The volume mounts here; the gateway opens it exclusively (DEP-001).
ENV TOTEM_DATA_DIR=/data
EXPOSE 8787
CMD ["totem-gateway"]
