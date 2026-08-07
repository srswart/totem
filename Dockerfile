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
FROM rust:1.96-slim-bookworm AS builder

# RocksDB's build needs a C++ toolchain; SurrealDB's TLS needs pkg-config.
RUN apt-get update && apt-get install -y --no-install-recommends \
        clang libclang-dev pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY . .
RUN cargo build --release -p totem-gateway --features rocksdb

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/totem-gateway /usr/local/bin/totem-gateway

# The volume mounts here; the gateway opens it exclusively (DEP-001).
ENV TOTEM_DATA_DIR=/data
EXPOSE 8787
CMD ["totem-gateway"]
