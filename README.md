# cyder-template

Rust + Vue foundation for a small backend-first web application.

The project provides an Axum service, Diesel persistence for SQLite and PostgreSQL, application-generated Snowflake-style IDs, health/readiness checks, and a Vue 3 operations UI. IDs are stored internally as `i64` and serialized as strings at JSON boundaries so browser clients do not lose 64-bit integer precision.

<!-- template-example:start -->
The included `items` and `users` CRUD resources demonstrate the full backend and frontend path. The `users` resource is sample data only; it is not an authentication, role, team, or tenant system.
<!-- template-example:end -->

<!-- template-init:start -->
## Use This Template

Create a new repository with GitHub's **Use this template** button, clone it, and run the interactive initializer before making project changes:

```bash
just init
```

The wizard validates a lowercase kebab-case project slug, derives the Rust, npm, database, Docker, and display identities, confirms the GitHub security-reporting link, and optionally removes the example resources. It requires a clean Git worktree and never changes the repository directory, remote, history, ignored build artifacts, or local databases.

You can prefill the slug while keeping the remaining confirmations interactive:

```bash
just init my-api
```

Automation can call `node scripts/init-project.mjs --answers-file <path>` with a reviewed JSON answer file. The human-facing `just init` command intentionally keeps these details inside the wizard.
<!-- template-init:end -->

## Requirements

- Rust 1.94 or newer with the rustfmt and Clippy components
- Node.js 24.11 or newer
- npm
- just
- cargo-deny 0.20.2 when running dependency security checks
- Diesel CLI when you want to regenerate schema files
- Docker when you want container builds or local PostgreSQL compose

## Quick Start

```bash
npm --prefix front ci
just dev
```

`just dev` starts the backend on `127.0.0.1:8000` and the Vite dev server for the Vue frontend. When `APP_DATA_DIR` is not set, the backend uses `.app/dev` and creates the SQLite database at `.app/dev/db/cyder-template.sqlite`.

Open the Vite URL printed by `npm run dev`. The frontend proxies `/api`, `/healthz`, and `/readyz` to the backend.

## Commands

Run `just --list` to see the command surface.

```bash
just dev                 # backend and frontend dev servers
just dev-backend         # backend only
just dev-front           # frontend only
just install-front-deps  # npm install when package files changed
just front-ci-deps       # npm ci
just build               # backend release binary and frontend dist
just test                # backend tests and frontend type checks
just test-postgres       # optional PostgreSQL integration tests
just lint-backend        # strict Rust lints for all targets/features
just check               # fmt, strict lint, tests, frontend build
just audit               # locked dependency advisories and policy
just docker-build        # local Docker image build
just test-container-shutdown # graceful SIGTERM test for the local image
```

<!-- template-example:start -->
Run `just strip-examples` from a clean, initialized project to remove the optional `items/users` resources without touching existing database data.
<!-- template-example:end -->

`just audit` requires `cargo-deny` 0.20.2 and registry access. Install the pinned version with:

```bash
cargo install --locked cargo-deny --version 0.20.2
```

Dependency security is intentionally separate from `just check` so normal local verification does not install tools or depend on advisory services. The Security workflow runs the same policies for every pull request and default-branch push, and once per day.

The `justfile` is for human development. CI and automation can call the same underlying Cargo, npm, and Docker commands directly.

## Configuration

The backend loads built-in defaults, then an optional YAML file, then `APP_*` environment variables. Set `APP_CONFIG_PATH` to choose a YAML file:

```bash
cp config.sample.yaml config.local.yaml
APP_CONFIG_PATH=config.local.yaml just dev-backend
```

Common environment overrides:

```bash
APP_HOST=127.0.0.1
APP_PORT=8000
APP_DATA_DIR=.app/dev
APP_DATABASE_URL=.app/dev/db/cyder-template.sqlite
APP_DATABASE_POOL_SIZE=1
APP_DATABASE_ACQUIRE_TIMEOUT_MS=30000
APP_SQLITE_BUSY_TIMEOUT_MS=5000
APP_ID_WORKER_ID=1
APP_LOG_LEVEL=info
APP_PUBLIC_DIR=front/dist
APP_SHUTDOWN_READINESS_DELAY_MS=1000
APP_SHUTDOWN_TIMEOUT_MS=8000
```

Copy `.env.example` to `.env` when you want `just` recipes to load local overrides automatically.

## Databases

The backend keeps Diesel as its default database layer and uses `diesel_async` for async connection pooling and query execution. This preserves Diesel schema files, embedded migrations, and typed query composition as the project grows. SQL-first libraries can still be a good choice for other projects; this repository defaults to Diesel because it already carries dual SQLite/PostgreSQL schema and migration structure.

SQLite is the default development database. No external service is required:

```bash
just dev-backend
```

The default SQLite pool size is `1` for a conservative local path. File-backed SQLite may use `APP_DATABASE_POOL_SIZE` greater than `1`; each pooled connection enables WAL mode, `APP_SQLITE_BUSY_TIMEOUT_MS`, and foreign keys. This helps read concurrency and short write-lock waits, but SQLite still has one writer at a time and should not be treated like PostgreSQL for parallel writes. Plain `:memory:` SQLite is kept to one effective pooled connection so migrations and queries see the same in-memory schema.

Generated IDs use a 43/8/12 Snowflake-style layout: 43 timestamp bits, 8 worker bits, and 12 sequence bits. Set `APP_ID_WORKER_ID` to a unique value from `0` to `255` for each running instance.

Use PostgreSQL by setting `APP_DATABASE_URL`:

```bash
APP_DATABASE_URL=postgres://cyder_template:cyder_template_dev@127.0.0.1:5432/cyder_template APP_DATABASE_POOL_SIZE=5 just dev-backend
```

The service detects the backend from the URL and runs the matching embedded Diesel migrations at startup. `APP_DATABASE_ACQUIRE_TIMEOUT_MS` controls how long a request waits for a pooled connection before failing readiness or database operations.

PostgreSQL integration tests are opt-in because they need a disposable database. Point `APP_TEST_POSTGRES_URL` at an isolated test database, then run:

```bash
APP_TEST_POSTGRES_URL=postgres://cyder_template:cyder_template_dev@127.0.0.1:5432/cyder_template_test just test-postgres
```

The PostgreSQL test uses a pool size greater than one and covers migrations and readiness. Without `APP_TEST_POSTGRES_URL`, the ignored PostgreSQL test is not part of the default `cargo test --workspace` path.

The compose setup creates `cyder_template_test` only when PostgreSQL initializes a fresh volume. If you already have a local compose volume, create a separate test database manually or recreate the local volume before running the PostgreSQL integration test.

Schema files are split by backend:

- `server/src/schema/sqlite.rs`
- `server/src/schema/postgres.rs`

See `server/diesel.toml` for the Diesel CLI commands used to regenerate each schema.

## API

Health endpoints:

- `GET /healthz` checks that the process is alive.
- `GET /readyz` checks that the service accepts traffic and that its database is connected.

<!-- template-example:start -->
Example resources:

- `GET /api/items`
- `POST /api/items`
- `GET /api/items/{id}`
- `DELETE /api/items/{id}`
- `GET /api/users`
- `POST /api/users`
- `GET /api/users/{id}`
- `DELETE /api/users/{id}`

ID boundary convention:

- Database and service structs keep generated IDs as `i64`.
- Controller response DTOs use `controller::api_id::ApiId` for `id` fields so JSON serializes IDs as strings.
- Controller path extractors can use `Path<ApiId>`, then call `into_i64()` before passing IDs to service/database functions.
- Frontend resource types use `string` for IDs and pass those strings back in URLs.

This keeps database indexes and backend arithmetic efficient while avoiding JavaScript 64-bit integer precision loss in browser clients.
<!-- template-example:end -->

## Graceful Shutdown

The backend handles SIGTERM on Unix and Ctrl-C on every supported platform. After receiving the first signal, it immediately makes `/readyz` return `503` while `/healthz` remains available. Normal requests continue during the default 1-second readiness propagation delay. The listener then stops accepting new connections and Axum waits for in-flight requests to finish.

`APP_SHUTDOWN_TIMEOUT_MS` is the total budget from signal receipt, including `APP_SHUTDOWN_READINESS_DELAY_MS`. The defaults are `8000` and `1000` milliseconds respectively. The readiness delay may be `0`, but it must remain shorter than the positive total timeout; invalid values fail startup. A second signal or an expired deadline forces shutdown and returns a non-zero process status.

Keep the container runtime or orchestrator termination deadline longer than `APP_SHUTDOWN_TIMEOUT_MS`. Docker sends SIGTERM and allows 10 seconds by default, so the built-in application values work without extra Dockerfile or Compose settings. Deployments with slower readiness propagation or longer requests can override the application values and their outer termination deadline together.

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

The production backend serves `front/dist` from `APP_PUBLIC_DIR` after `just build-front`.

## Docker And Compose

Build the local image:

```bash
just docker-build
```

The equivalent direct Docker command is:

```bash
docker build -t cyder-template:local -f Dockerfile .
```

Run the image with its default SQLite database:

```bash
docker run --rm -p 8000:8000 -v "$PWD/.app/docker:/data/app" cyder-template:local
```

The image runs the service as a non-root `app` user. Its entrypoint creates `config`, `db`, `storage`, and `tmp` directories under `APP_DATA_DIR`, which defaults to `/data/app` in the container.

Run PostgreSQL and the app together with compose:

```bash
cp .env.example .env
docker compose up --build
```

Compose builds the same `cyder-template:local` image, starts a local PostgreSQL service with a healthcheck, and points `APP_DATABASE_URL` at that service. Compose uses `COMPOSE_APP_DATABASE_POOL_SIZE`, defaulting to `5`, so PostgreSQL keeps a larger pool than the local SQLite default. The compose credentials in `.env.example` are local-development examples. Choose real credentials for shared or deployed environments.

## Automation

This repository includes `.github/workflows/ci.yml`. The workflow runs on pull requests, pushes to `main` or `master`, and manual dispatch:

- `Backend`: installs Rust 1.94 with rustfmt and Clippy plus native build dependencies, then runs Rust formatting, strict workspace linting across all targets/features, and workspace tests.
- `Frontend`: uses Node 24, runs locked npm install, type checks through `npm test`, and builds the Vite app.
- `Docker`: waits for backend and frontend jobs, validates `docker-compose.yml`, builds `cyder-template:ci`, then starts it and verifies graceful SIGTERM handling within Docker's default stop deadline.

Keep the workflow's Docker image tag aligned with the local Docker and compose names. If you use a different CI system, copy the same command set from the workflow. Node should stay on the 24.x line across local development and automation, with 24.11 or newer as the minimum.

## License

Licensed under the MIT License. See `LICENSE`.
