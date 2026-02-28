# syntax=docker/dockerfile:1
# Stage 1: 编译环境 (Builder)
FROM rust:1-slim-bookworm AS builder
WORKDIR /build

# 安装编译所需的系统依赖
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

# 拷贝源码到容器内
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY xtask ./xtask
COPY agents ./agents
COPY packages ./packages

# 编译 Rust 二进制文件
RUN cargo build --release --bin openfang

# Stage 2: 最终运行环境 (包含完整的自动化依赖)
FROM debian:bookworm-slim

# 安装 CA 证书、FFmpeg、中文字体以及 Playwright 浏览器所需的所有底层库
RUN apt-get update && apt-get install -y \
    ca-certificates \
    ffmpeg \
    fonts-wqy-zenhei \
    libnss3 \
    libatk-bridge2.0-0 \
    libx11-xcb1 \
    libxcomposite1 \
    libxcursor1 \
    libxdamage1 \
    libxi6 \
    libxtst6 \
    libcups2 \
    libxss1 \
    libxrandr2 \
    libasound2 \
    libpangocairo-1.0-0 \
    libatk1.0-0 \
    libdrm2 \
    libgbm1 \
    && rm -rf /var/lib/apt/lists/*

# 从 builder 阶段提取编译好的文件
COPY --from=builder /build/target/release/openfang /usr/local/bin/
COPY --from=builder /build/agents /opt/openfang/agents

EXPOSE 4200
VOLUME /data
ENV OPENFANG_HOME=/data
ENTRYPOINT ["openfang"]
CMD ["start"]
