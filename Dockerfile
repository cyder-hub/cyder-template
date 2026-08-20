# syntax=docker/dockerfile:1

ARG NODE_VERSION=24.19.0
ARG RUST_VERSION=1.97.1

FROM node:${NODE_VERSION}-bookworm-slim AS frontend
WORKDIR /workspace/front

COPY front/package.json front/package-lock.json ./
RUN npm ci

COPY front/ ./
RUN npm run build

FROM rust:${RUST_VERSION}-bookworm AS backend
WORKDIR /workspace

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl-dev \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY server/ server/
RUN cargo build -p cyder-template --release --locked

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        gosu \
        libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 app \
    && useradd --uid 10001 --gid app --home-dir /app --shell /usr/sbin/nologin --no-create-home app

WORKDIR /app

COPY --from=backend /workspace/target/release/cyder-template /app/bin/cyder-template
COPY --from=frontend /workspace/front/dist /app/front/dist
COPY docker-entrypoint /app/bin/docker-entrypoint

RUN chmod +x /app/bin/docker-entrypoint \
    && mkdir -p /data/config /data/db /data/storage /data/logs /tmp/cyder-template \
    && chown -R app:app /app /data /tmp/cyder-template

ENV APP_HOST=0.0.0.0 \
    APP_DATA_DIR=/data

EXPOSE 8000

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["gosu", "app", "/app/bin/cyder-template", "healthcheck"]

ENTRYPOINT ["/app/bin/docker-entrypoint"]
CMD ["/app/bin/cyder-template"]
