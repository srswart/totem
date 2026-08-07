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
FROM rust:1.96-slim-trixie AS builder

# RocksDB's build needs a C++ toolchain; SurrealDB's TLS needs pkg-config.
RUN apt-get update && apt-get install -y --no-install-recommends \
        clang libclang-dev pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY . .
# `fastembed` brings ONNX Runtime, which needs a C++ toolchain to link.
#
# Trixie rather than bookworm for every stage (ADV-STORE-008): the prebuilt
# ONNX Runtime `ort` links against libstdc++ 13+ symbols (`_M_replace_cold`),
# which bookworm's GCC 12 runtime does not define. The failure is a linker
# error deep in `ort-sys` that names a C++ mangled symbol and nothing about
# Debian releases. The runtime stage must match, since the binary needs that
# libstdc++ at run time too.
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev build-essential \
    && rm -rf /var/lib/apt/lists/*
# Two jobs, not one per core. Adding `fastembed` put ONNX Runtime alongside
# `surrealdb-core` — already the largest crate here — and the remote builder
# OOM-killed rustc (SIGKILL) partway through. The failure reads as
# "could not compile surrealdb-core", naming the victim rather than the cause,
# and it appeared only once this feature was added (ADV-STORE-008).
ENV CARGO_BUILD_JOBS=2
RUN cargo build --release -p totem-gateway --features rocksdb,fastembed

# Bake the model weights into the image (ADV-STORE-008). Cold construction is
# ~276s because of this download, against ~124ms warm (EMB-004) — a first boot
# paying that would fail its health check long before it served anything. Here
# a slow step is merely slow.
ENV FASTEMBED_CACHE_PATH=/models
RUN /build/target/release/totem-gateway --warm-embedder

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
ENV FASTEMBED_CACHE_PATH=/models
COPY --from=console /build/target/dx/totem-console/release/web/public /console
ENV TOTEM_CONSOLE_DIR=/console

# The volume mounts here; the gateway opens it exclusively (DEP-001).
ENV TOTEM_DATA_DIR=/data
EXPOSE 8787
CMD ["totem-gateway"]
