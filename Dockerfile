# syntax=docker/dockerfile:1

FROM node:24-bookworm-slim AS frontend
WORKDIR /workspace/front

COPY front/package.json front/package-lock.json ./
RUN npm ci

COPY front/ ./
RUN npm run build

FROM rust:1.94-bookworm AS backend
WORKDIR /workspace

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl-dev \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY server/ server/
RUN cargo build -p cyder-template --release

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

COPY --from=backend /workspace/target/release/cyder-template /usr/local/bin/cyder-template
COPY --from=frontend /workspace/front/dist /app/public
COPY docker-entrypoint /usr/local/bin/docker-entrypoint

RUN chmod +x /usr/local/bin/docker-entrypoint \
    && mkdir -p /data/app \
    && chown -R app:app /app /data/app

ENV APP_HOST=0.0.0.0 \
    APP_PORT=8000 \
    APP_DATA_DIR=/data/app \
    APP_PUBLIC_DIR=/app/public \
    APP_CONFIG_PATH=/data/app/config/config.yaml

EXPOSE 8000

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS http://127.0.0.1:8000/readyz >/dev/null || exit 1

ENTRYPOINT ["docker-entrypoint"]
CMD ["cyder-template"]
