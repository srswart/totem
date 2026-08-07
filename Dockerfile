# Totem gateway for Fly.io (ADV-INFRA-002).
#
# Built with the `rocksdb` feature: this image is the DEP-001 durable single
# instance, and the gateway refuses to start with TOTEM_DATA_DIR set unless
# the feature is compiled in.
FROM rust:1.85-slim-bookworm AS builder

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
