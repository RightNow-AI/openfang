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

FROM rust:1-slim-bookworm
# Install ONNX Runtime shared library for Smart Turn inference
ARG ORT_VERSION=1.20.1
ADD https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VERSION}/onnxruntime-linux-x64-${ORT_VERSION}.tgz /tmp/ort.tgz
RUN tar -xzf /tmp/ort.tgz -C /tmp && \
    cp /tmp/onnxruntime-linux-x64-${ORT_VERSION}/lib/libonnxruntime.so.${ORT_VERSION} /usr/lib/ && \
    ln -s /usr/lib/libonnxruntime.so.${ORT_VERSION} /usr/lib/libonnxruntime.so && \
    ldconfig && \
    rm -rf /tmp/ort.tgz /tmp/onnxruntime-linux-x64-${ORT_VERSION}
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
EXPOSE 4200
VOLUME /data
ENV OPENFANG_HOME=/data
ENTRYPOINT ["openfang"]
CMD ["start"]
