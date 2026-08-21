# Cyder Music

> [!NOTE]
> This repository starts as a project template. Create a repository with GitHub's **Use this template** button, clone it, and run `just init` before making project-specific changes. The initializer removes its one-time tooling after it finishes; you may delete this notice once initialization is complete.

Rust + Vue foundation for a small backend-first web application.

This project provides an Axum service, Diesel persistence for SQLite and PostgreSQL, application-generated Snowflake-style IDs, health/readiness checks, and a Vue 3 operations UI. IDs are stored internally as `i64` and serialized as strings at JSON boundaries so browser clients do not lose 64-bit integer precision.

## Requirements

- Rust 1.97.1 through rustup, with the rustfmt and Clippy components declared in `rust-toolchain.toml`
- Node.js 24.19.0, declared in `.node-version`
- npm 11.17.0, bundled with the pinned Node.js release
- just 1.58.0 or newer on the 1.x release line
- cargo-deny 0.20.2 when running dependency security checks
- Diesel CLI when you want to regenerate schema files
- Docker when you want container builds or local PostgreSQL compose

## Quick Start

```bash
just bootstrap
just dev
```

`just bootstrap` checks the required toolchain and local ports, installs the locked frontend dependencies with `npm ci`, and creates `.app/dev/{config,db,storage,logs}`. It does not create or load `.env`; export development overrides in your shell or configure them in your process manager. Docker and its Compose plugin are optional for the default SQLite path, so unavailable container tooling is reported as a warning.

`just dev` asks the Rust configuration loader for the backend's resolved endpoint, starts the backend and Vite dev server in parallel, and injects the corresponding HTTP origin into Vite. The default remains `127.0.0.1:8000`; changing `APP_PORT` or the selected YAML configuration keeps the proxy aligned automatically. When `APP_DATA_DIR` is not set, the backend uses `.app/dev` and creates the SQLite database at `.app/dev/db/cyder-music.sqlite`.

Open the Vite URL printed by `npm run dev`. The frontend proxies `/api`, `/healthz`, and `/readyz` to the backend.

## Commands

Run `just --list` to see the command surface.

```bash
just doctor              # toolchain, optional Docker, and port diagnostics
just bootstrap           # locked frontend deps and local files
just dev                 # backend and frontend dev servers
just dev-backend         # backend only
just dev-front           # frontend only
just install-front-deps  # npm ci when package files changed
just front-ci-deps       # npm ci
just build               # backend release binary and frontend dist
just test                # backend and frontend tests/type checks
just test-postgres       # optional PostgreSQL integration tests
just check-config        # side-effect-free deployment configuration validation
just lint-backend        # strict Rust lints for all targets/features
just check               # toolchain contracts, fmt, lint, tests, build
just audit               # locked dependency advisories and policy
just docker-build        # local Docker image build
just test-container-config # configuration and data-layout image contract
just test-container-http # HTTP and static-asset image contract
just test-container-shutdown # graceful SIGTERM test for the local image
```

`just audit` requires `cargo-deny` 0.20.2 and registry access. Install the pinned version with:

```bash
cargo install --locked cargo-deny --version 0.20.2
```

Dependency security is intentionally separate from `just check` so normal local verification does not install tools or depend on advisory services. The Security workflow runs the same policies for every pull request and default-branch push, and once per day.

`just doctor` never installs tools. Rust, Node.js, npm, just, and the backend port are blocking checks. Docker/Compose availability and the default Vite and PostgreSQL ports are advisory because they are not required by the default SQLite development path. cargo-deny and Diesel CLI retain checks in the commands that actually need them.

The `justfile` is for human development. CI and automation can call the same underlying Cargo, npm, and Docker commands directly.

Use the `just` development commands instead of invoking `npm run dev` directly. The Vite development server deliberately fails when its resolved target has not been injected; it does not scan backend `.env`, YAML, or TOML files. If you intentionally want the frontend to use a remote or containerized backend, provide a strict HTTP(S) origin explicitly:

```bash
DEV_PROXY_TARGET=https://dev-api.example.com just dev-front
```

The explicit target takes precedence and skips backend configuration lookup. It must not contain credentials, a non-root path, a query, or a fragment. TLS verification remains enabled.

## Configuration

The backend is designed to start with no configuration. It resolves supported values in this order, from highest to lowest priority:

- One of the six explicitly supported `APP_*` environment variables.
- An optional YAML configuration file.
- Built-in defaults.

`APP_DATA_DIR` is an environment-only bootstrap setting. It defaults to `.app/dev` for source checkouts; the image sets it to `/data`. The backend automatically reads `<APP_DATA_DIR>/config/config.yaml` when that file exists. If `APP_CONFIG_PATH` is explicitly set, that exact file is required and a missing, unreadable, or non-file path fails immediately.

The application does not parse `.env` files, and `just` does not load them. Environment injection belongs to the shell, container runtime, systemd, Kubernetes, or another deployment tool. `config.example.yaml` is a comment-only reference and is never loaded automatically.

The complete environment-variable surface is intentionally small:

| Setting | Default |
| --- | --- |
| `APP_DATA_DIR` | `.app/dev` from source; `/data` in the image |
| `APP_CONFIG_PATH` | `<data-dir>/config/config.yaml`, optional |
| `APP_HOST` | `127.0.0.1` from source; `0.0.0.0` in the image |
| `APP_PORT` | `8000` |
| `APP_DATABASE_URL` | `<data-dir>/db/cyder-music.sqlite` |
| `APP_LOG_LEVEL` | `info` |

`host`, `port`, `database_url`, and `log_level` use the same defaults in YAML and may be overridden by their corresponding common environment variables. All operational tuning is YAML-only:

| YAML setting | Default | Valid range or rule |
| --- | --- | --- |
| `database_pool_size` | SQLite `1`; PostgreSQL `5` | Greater than `0`; in-memory SQLite requires `1` |
| `database_acquire_timeout_ms` | `30000` | Greater than `0` |
| `sqlite_busy_timeout_ms` | `5000` | SQLite only; `0` disables lock waiting |
| `shutdown_readiness_delay_ms` | `1000` | At least `0` and less than the shutdown timeout |
| `shutdown_timeout_ms` | `8000` | Greater than the readiness delay |
| `http_request_timeout_ms` | `30000` | `1..=300000` |
| `http_max_concurrent_requests` | `64` | `1..=4096` |
| `http_max_request_body_bytes` | `1048576` | `1..=67108864` |

For example:

```bash
APP_PORT=9000 just dev
APP_CONFIG_PATH=/path/to/config.yaml just dev-backend
```

Validate deployment configuration without connecting to a database, running migrations, creating files, or opening a listener:

```bash
just check-config
cargo run -p cyder-music -- config check --format json
```

Recognized values are always validated. During normal startup, unknown YAML fields and unsupported `APP_*` names are ignored with a warning that names the key but never its value. This behavior lets this project tolerate temporary configuration skew during its own rolling updates. `config check` rejects every ignored setting and is the deployment/CI gate. YAML-only settings have no corresponding environment-variable name. PostgreSQL summaries report only the backend kind and effective pool settings, so database URLs and passwords are never included.

The application binary exposes the non-sensitive resolved listen endpoint for local development tooling:

```bash
cargo run -p cyder-music -- config endpoint --format json
# {"host":"127.0.0.1","port":8000}
```

Rust remains the only application configuration parser. `just dev` consumes this JSON contract and converts unspecified bind addresses to usable local connection addresses: `0.0.0.0` becomes `127.0.0.1`, and `::` becomes `::1`. Other hosts and the resolved port are preserved. Port `0` cannot be used for automatic proxy derivation.

`DEV_PROXY_TARGET` is a development orchestration variable, not an `APP_*` backend setting. Production does not use this proxy contract: the Rust process loads its configuration directly and serves the built frontend on the same origin.

The frontend location is not configurable. Source and release runs use `front/dist`; the image stores the same immutable artifact at `/app/front/dist`. Persisted state belongs under the one data root: `config/`, `db/`, `storage/`, and the reserved `logs/` directory. The service currently logs to stdout/stderr, while temporary files belong under `/tmp/cyder-music`.

## Databases

The backend keeps Diesel as its default database layer and uses `diesel_async` for async connection pooling and query execution. This preserves Diesel schema files, embedded migrations, and typed query composition as the project grows. SQL-first libraries can still be a good choice for other projects; this repository defaults to Diesel because it already carries dual SQLite/PostgreSQL schema and migration structure.

SQLite is the default development database. No external service is required:

```bash
just dev-backend
```

The default SQLite pool size is `1` for a conservative local path. File-backed SQLite may set `database_pool_size` greater than `1` in YAML; each pooled connection enables WAL mode, the YAML `sqlite_busy_timeout_ms`, and foreign keys. This helps read concurrency and short write-lock waits, but SQLite still has one writer at a time and should not be treated like PostgreSQL for parallel writes. Plain `:memory:` SQLite requires an effective pool size of `1` so migrations and queries see the same in-memory schema.

Generated IDs retain the 43/8/12 Snowflake-style layout: 43 timestamp bits, 8 worker bits, and 12 sequence bits. This project deliberately supports one application instance and uses one internal worker ID. Do not scale the application horizontally without first replacing the ID and migration ownership strategy.

Use PostgreSQL by setting `APP_DATABASE_URL`:

```bash
APP_DATABASE_URL=postgres://cyder_music:cyder_music_dev@127.0.0.1:5432/cyder_music just dev-backend
```

The service detects the backend from the URL, defaults PostgreSQL to a pool of `5`, and runs the matching embedded Diesel migrations at startup. The YAML `database_acquire_timeout_ms` controls how long a request waits for a pooled connection before failing readiness or database operations.

PostgreSQL integration tests are opt-in because they need a disposable database. Point `DEV_POSTGRES_TEST_URL` at an isolated test database, then run:

```bash
DEV_POSTGRES_TEST_URL=postgres://cyder_music:cyder_music_dev@127.0.0.1:5432/cyder_music_test just test-postgres
```

The PostgreSQL test uses a pool size greater than one and covers migrations and readiness. Without `DEV_POSTGRES_TEST_URL`, the ignored PostgreSQL test is not part of the default `cargo test --workspace` path.

The compose setup creates `cyder_music_test` only when PostgreSQL initializes a fresh volume. If you already have a local compose volume, create a separate test database manually or recreate the local volume before running the PostgreSQL integration test.

Schema files are split by backend:

- `server/src/schema/sqlite.rs`
- `server/src/schema/postgres.rs`

See `server/diesel.toml` for the Diesel CLI commands used to regenerate each schema.

## API

Health endpoints:

- `GET /healthz` checks that the process is alive.
- `GET /readyz` checks that the service accepts traffic and that its database is connected.

## HTTP Boundary

Every response carries `X-Request-ID`. An inbound ID is retained only when it is 1–128 ASCII letters, digits, dots, underscores, or hyphens; otherwise the server generates a UUID v4. API and readiness errors use one JSON shape:

```json
{"error":"internal_error","message":"internal server error","request_id":"..."}
```

Client-actionable failures keep distinct codes such as `invalid_request`, `unsupported_media_type`, `payload_too_large`, `method_not_allowed`, `request_timeout`, `service_overloaded`, and `readiness_failed`. Internal failures and request panics collapse to `500 internal_error`; details and the complete causal chain stay in server logs. The frontend shows the request ID as a support reference for 5xx and 408 responses.

The request timeout, immediate concurrency rejection, and body-size limit apply only to `/api`, `/api/`, `/api/*`, and `/readyz`. `/healthz` and static files do not consume this concurrency budget. A timeout returns 408 and an overloaded service returns 503; neither response carries `Retry-After` because the server has no recovery time to advertise. Panic containment covers HTTP requests only, so an individual request becomes a logged 500 while a panic outside the HTTP boundary still terminates the process.

One structured access event is written for every request with the request ID, method, matched route, status, latency in milliseconds, and streamed response-body byte count. It does not record query strings, bodies, cookies, authorization values, user agents, or forwarded client addresses. Internal errors and panics are logged at error level; readiness failures, timeouts, and overload are warnings.

All responses include a same-origin CSP plus `X-Content-Type-Options`, `X-Frame-Options`, `Referrer-Policy`, `Permissions-Policy`, COOP, and CORP. The application does not emit HSTS because TLS termination belongs to the deployment edge, and it does not enable CORS because the production frontend and API share one origin.

The frontend build creates Brotli quality-11 and gzip level-9 sidecars for compressible HTML, CSS, JavaScript, JSON, SVG, XML, text, and WASM files of at least 1 KiB, retaining a sidecar only when it is smaller. Static serving negotiates these files, emits representation-aware strong ETags and `Vary: accept-encoding`, and honors `If-None-Match`. Successful `/assets/*` responses and their 304s use `public, max-age=31536000, immutable`; the SPA index and history fallback use `no-cache`; API, health, readiness, and error responses use `no-store` unless a successful dynamic handler explicitly sets its own cache policy.

## Graceful Shutdown

The backend handles SIGTERM on Unix and Ctrl-C on every supported platform. After receiving the first signal, it immediately makes `/readyz` return `503` while `/healthz` remains available. Normal requests continue during the default 1-second readiness propagation delay. The listener then stops accepting new connections and Axum waits for in-flight requests to finish.

The YAML `shutdown_timeout_ms` is the total budget from signal receipt, including `shutdown_readiness_delay_ms`. The defaults are `8000` and `1000` milliseconds respectively. The readiness delay may be `0`, but it must remain shorter than the positive total timeout; invalid values fail startup. A second signal or an expired deadline forces shutdown and returns a non-zero process status.

Keep the container runtime or orchestrator termination deadline longer than `shutdown_timeout_ms`. Docker sends SIGTERM and allows 10 seconds by default, so the built-in application values work without extra Dockerfile or Compose settings. Deployments with slower readiness propagation or longer requests can override the YAML values and their outer termination deadline together.

Shutdown logs expose stable `shutdown_signal_received`, `shutdown_readiness_disabled`, `shutdown_drain_started`, `shutdown_completed`, and `shutdown_forced` event fields. To verify an already-built local image with a real SIGTERM:

```bash
just test-container-shutdown
```

## Frontend

The frontend lives in `front/` and uses Vue 3, Vite, TypeScript, Pinia, and Vue Router.

```bash
npm --prefix front ci
npm --prefix front run dev
npm --prefix front run build
npm --prefix front test
```

The production backend serves the fixed `front/dist` artifact after `just build-front`, including its generated Brotli and gzip sidecars.

## Docker And Compose

Build the local image:

```bash
just docker-build
```

The equivalent direct Docker command is:

```bash
docker build -t cyder-music:local -f Dockerfile .
```

Run the image with its default SQLite database:

```bash
docker run --rm -p 8000:8000 -v "$PWD/.app/docker:/data" cyder-music:local
```

The image keeps versioned, immutable artifacts under `/app` and runs commands as the non-root UID/GID `10001`. When starting a runtime command, its entrypoint creates missing `config`, `db`, `storage`, and `logs` directories under `/data` without recursively changing an existing volume's ownership. Configuration and help commands skip this preparation, so validation also works against an empty read-only `/data` mount without changing it. Mounting `/data` is the simplest way to preserve every application-owned resource; advanced deployments may mount individual subdirectories. Ensure mounted paths are writable by UID/GID `10001` when starting the service.

The image healthcheck resolves configuration through the application binary before requesting `/readyz`, so a port supplied by the mounted YAML file or `APP_PORT` is probed consistently with the running service.

Run PostgreSQL and the app together with compose:

```bash
docker compose up --build
```

Compose builds the same `cyder-music:local` image, starts a local PostgreSQL service with a healthcheck, points `APP_DATABASE_URL` at that service, and mounts the application `/data` root. Its fixed credentials and exposed PostgreSQL port are development conveniences, not production defaults. PostgreSQL automatically receives the pool default of `5`.

## Automation

This repository includes `.github/workflows/ci.yml`. The workflow runs on pull requests, pushes to `main` or `master`, and manual dispatch:

- `Backend`: activates Rust 1.97.1 from `rust-toolchain.toml` with rustfmt and Clippy plus native build dependencies, then runs Rust formatting, strict workspace linting across all targets/features, and workspace tests.
- `Frontend`: reads Node 24.19.0 from `.node-version`, checks the applicable tooling contracts, runs locked npm install, type checks through `npm test`, and builds the Vite app.
- `Docker`: validates `docker-compose.yml`, builds `cyder-music:ci`, checks its safe configuration/data-layout and HTTP/static-asset contracts, then verifies graceful SIGTERM handling within Docker's default stop deadline.

Keep the workflow's Docker image tag aligned with the local Docker and compose names. If you use a different CI system, copy the same command set from the workflow. Update `.node-version`, `rust-toolchain.toml`, package metadata, and Docker build arguments together; `just test-toolchain` rejects version drift.

## License

Licensed under the MIT License. See `LICENSE`.
