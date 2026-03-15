# syntax=docker/dockerfile:1
FROM rust:1-slim-bookworm AS builder
WORKDIR /build
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
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

FROM rust:1-slim-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    python3 \
    python3-pip \
    python3-venv \
    nodejs \
    npm \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/openfang /usr/local/bin/
COPY --from=builder /build/agents /opt/openfang/agents

# OCI labels — org.opencontainers.image.source links the GHCR package to this
# repository so the package can inherit the repo's public visibility.
LABEL org.opencontainers.image.source="https://github.com/RightNow-AI/openfang" \
      org.opencontainers.image.description="OpenFang — Open-Source Agent Operating System" \
      org.opencontainers.image.licenses="Apache-2.0 OR MIT"

EXPOSE 4200
VOLUME /data
ENV OPENFANG_HOME=/data
ENTRYPOINT ["openfang"]
CMD ["start"]
