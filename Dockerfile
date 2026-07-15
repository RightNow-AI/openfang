# syntax=docker/dockerfile:1
FROM rust:1-slim-bookworm AS builder
WORKDIR /build
RUN apt-get update && apt-get install -y pkg-config libssl-dev perl make && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY xtask ./xtask
COPY agents ./agents
COPY packages ./packages
# Optional build args for dev environments to speed up compilation
# Example: docker build --build-arg LTO=false --build-arg CODEGEN_UNITS=16 .
ARG LTO=true
ARG CODEGEN_UNITS=1
ENV CARGO_PROFILE_RELEASE_LTO=${LTO} \
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS=${CODEGEN_UNITS}
RUN cargo build --release --bin openfang

# Runtime: plain Debian slim (NOT the rust image) so the production image ships
# no compiler/toolchain. libssl3 is the runtime half of the builder's libssl-dev
# (the binary links OpenSSL dynamically); python/node are needed by skills.
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    python3 \
    python3-pip \
    python3-venv \
    nodejs \
    npm \
    && rm -rf /var/lib/apt/lists/*

# Non-root runtime user (uid/gid 1000). The app writes everything under
# OPENFANG_HOME (/data) — provided by the PVC and made group-writable via the
# pod's fsGroup: 1000 — so no rootfs writes are needed. HOME=/data keeps pip/npm
# caches on the volume too.
RUN useradd --uid 1000 --user-group --home-dir /data --shell /usr/sbin/nologin openfang \
    && mkdir -p /data \
    && chown -R 1000:1000 /data

COPY --from=builder /build/target/release/openfang /usr/local/bin/
# Baked agents seeded to the data volume on first boot (Dewansh + assistant).
# Curated deploy set — NOT the repo's example agent gallery under ./agents.
COPY deploy/agents /opt/openfang/agents
# Baked default config (seeded to the data volume on first boot if absent).
COPY deploy/config.default.toml /opt/openfang/config.toml
# Entrypoint: seeds baked agents + default config onto the data volume on first
# boot (with diagnostic logging), then execs the daemon. See deploy/entrypoint.sh.
COPY deploy/entrypoint.sh /usr/local/bin/openfang-entrypoint.sh
RUN chmod +x /usr/local/bin/openfang-entrypoint.sh
EXPOSE 4200
VOLUME /data
ENV OPENFANG_HOME=/data \
    HOME=/data
USER 1000
ENTRYPOINT ["/usr/local/bin/openfang-entrypoint.sh"]
CMD ["start"]
