# cyder-template

Rust + Vue project template for a small backend-first web application.

The template provides an Axum service, Diesel persistence for SQLite and PostgreSQL, application-generated Snowflake-style IDs, health/readiness checks, and a Vue 3 operations UI for the example `items` and `users` resources. IDs are stored internally as `i64` and serialized as strings in JSON responses so browser clients do not lose 64-bit integer precision. The `users` resource is only CRUD sample data; it is not an authentication, role, team, or tenant system.

## Use This Template

Create a new repository from this template with GitHub's **Use this template** button, then clone your new repository and rename the project before the first release. Search for these template names after each rename pass:

- `cyder-template`
- `cyder_template`
- `cyder-template-front`

Rename checklist:

- `README.md`: update the title and project description.
- `server/Cargo.toml`: update `[package].name`, `default-run`, and `[[bin]].name`.
- `Cargo.lock`: regenerate it after changing the Rust package name.
- `server/src/app.rs`: update `APP_NAME`.
- `server/src/config.rs`, `server/src/database/mod.rs`, `config.sample.yaml`, and `.env.example`: update default SQLite paths and PostgreSQL database/user examples.
- `front/package.json` and `front/package-lock.json`: update the package name from `cyder-template-front`.
- `front/index.html`, `front/src/App.vue`, and `front/src/store/index.ts`: update visible app and service names.
- `Dockerfile`: update `cargo build -p cyder-template`, the copied release binary path, and `CMD ["cyder-template"]`.
- `docker-compose.yml` and `docker/postgres/init/`: update the `cyder-template:local` image name, `cyder-template-data` and `cyder-template-postgres-data` volumes, and the default `cyder_template` database, test database, user, and URL values.
- `justfile`: update the crate name used by Cargo recipes and the default Docker image name.
- `.github/workflows/ci.yml`: update the Docker image tag `cyder-template:ci`.
- `.github/PULL_REQUEST_TEMPLATE.md`, `.github/ISSUE_TEMPLATE/feature_request.yml`, and `CONTRIBUTING.md`: update verification command examples.
- `.github/ISSUE_TEMPLATE/config.yml`: update the private vulnerability reporting link when the GitHub owner or repository name changes.
- `.github/dependabot.yml`: update `target-branch` if the new repository does not use `main`.
- `LICENSE`: update the copyright holder if needed.

Shortest local verification path after renaming:

```bash
npm --prefix front ci
just check
docker compose -f docker-compose.yml config
docker build -t your-project:ci -f Dockerfile .
```

## Requirements

- Rust 1.94 or newer
- Node.js 24.11 or newer
- npm
- just
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
just check               # fmt, check, tests, frontend build
just docker-build        # local Docker image build
```

The template `justfile` is for human development. CI and automation can call the same underlying Cargo, npm, and Docker commands directly.

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
```

Copy `.env.example` to `.env` when you want `just` recipes to load local overrides automatically.

## Databases

The backend keeps Diesel as the template's default database layer and uses `diesel_async` for async connection pooling and query execution. This preserves Diesel schema files, embedded migrations, and typed query composition for projects that grow beyond the sample `items` and `users` resources. SQL-first libraries can still be a good choice for other templates; this template defaults to Diesel because it already carries dual SQLite/PostgreSQL schema and migration structure.

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

The PostgreSQL test uses a pool size greater than one and covers migrations, readiness, and example `items`/`users` CRUD. Without `APP_TEST_POSTGRES_URL`, the ignored PostgreSQL test is not part of the default `cargo test --workspace` path.

The compose setup creates `cyder_template_test` only when PostgreSQL initializes a fresh volume. If you already have a local compose volume, create a separate test database manually or recreate the local volume before running the PostgreSQL integration test.

Schema files are split by backend:

- `server/src/schema/sqlite.rs`
- `server/src/schema/postgres.rs`

See `server/diesel.toml` for the Diesel CLI commands used to regenerate each schema.

## API

Health endpoints:

- `GET /healthz` checks that the process is alive.
- `GET /readyz` checks database connectivity.

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

This template includes `.github/workflows/ci.yml`. The workflow runs on pull requests, pushes to `main` or `master`, and manual dispatch:

- `Backend`: installs Rust 1.94 and native build dependencies, then runs Rust formatting, workspace check, and workspace tests.
- `Frontend`: uses Node 24, runs locked npm install, type checks through `npm test`, and builds the Vite app.
- `Docker`: waits for backend and frontend jobs, validates `docker-compose.yml`, and builds `cyder-template:ci`.

When renaming the template, update the workflow's Docker image tag together with the local Docker and compose names. If you use a different CI system, copy the same command set from the workflow. Node should stay on the 24.x line across local development and automation, with 24.11 or newer as the minimum.

## License

Licensed under the MIT License. See `LICENSE`.
