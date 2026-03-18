# syntax=docker/dockerfile:1
FROM rust:1-slim-bookworm AS builder
WORKDIR /build
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY xtask ./xtask
COPY agents ./agents
COPY packages ./packages
COPY scripts ./scripts
# Optional build args for dev environments to speed up compilation
# Example: docker build --build-arg LTO=false --build-arg CODEGEN_UNITS=16 .
ARG LTO=true
ARG CODEGEN_UNITS=1
ENV CARGO_PROFILE_RELEASE_LTO=${LTO} \
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS=${CODEGEN_UNITS}
RUN cargo build --release --bin openfang

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    python3 \
    python3-pip \
    python3-venv \
    nodejs \
    npm \
    tini \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --system --create-home --home-dir /home/openfang --shell /usr/sbin/nologin openfang \
    && install -d -o openfang -g openfang /data /opt/openfang

COPY --from=builder --chown=openfang:openfang /build/target/release/openfang /usr/local/bin/openfang
COPY --from=builder --chown=openfang:openfang /build/scripts/healthcheck-openfang.py /usr/local/bin/healthcheck-openfang.py
COPY --from=builder --chown=openfang:openfang /build/agents /opt/openfang/agents
EXPOSE 4200
VOLUME /data
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
  CMD python3 /usr/local/bin/healthcheck-openfang.py
STOPSIGNAL SIGTERM
ENV HOME=/home/openfang \
    OPENFANG_HOME=/data
WORKDIR /data
USER openfang
ENTRYPOINT ["/usr/bin/tini", "--", "openfang"]
CMD ["start"]
