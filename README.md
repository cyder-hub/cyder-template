# cyder-template

Rust + Vue project template for a small backend-first web application.

The template provides an Axum service, Diesel persistence for SQLite and PostgreSQL, application-generated Snowflake-style `i64` IDs, health/readiness checks, and a Vue 3 operations UI for the example `items` and `users` resources. The `users` resource is only CRUD sample data; it is not an authentication, role, team, or tenant system.

## Use This Template

Create a new repository from this template with GitHub's **Use this template** button, then clone your new repository and rename the project before the first release. Search for these template names after each rename pass:

- `cyder-template`
- `cyder_template`
- `cyder-template-front`

Rename checklist:

- `README.md`: update the title and project description.
- `server/Cargo.toml`: update `[package].name`, `default-run`, and `[[bin]].name`.
- `Cargo.lock`: regenerate it after changing the Rust package name.
- `front/package.json`: update the `name` field from `cyder-template-front`.
- `Dockerfile`: update `cargo build -p cyder-template`, the copied release binary path, and `CMD ["cyder-template"]`.
- `docker-compose.yml`: update the `cyder-template:local` image name, `cyder-template-data` and `cyder-template-postgres-data` volumes, and the default `cyder_template` database, user, and URL values.
- `.env.example`: update `APP_DATABASE_URL`, `POSTGRES_DB`, `POSTGRES_USER`, and `POSTGRES_PASSWORD`.
- `config.sample.yaml`: update the default SQLite path and PostgreSQL URL examples.
- `justfile`: update the crate name used by Cargo recipes and the default Docker image name.
- `.github/workflows/ci.yml`: update the CI Docker tag `cyder-template:ci`.

Shortest local verification path after renaming:

```bash
npm --prefix front ci
just check
docker compose -f docker-compose.yml config
docker build -t cyder-template:ci -f Dockerfile .
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
APP_DATABASE_POOL_SIZE=5
APP_ID_WORKER_ID=1
APP_LOG_LEVEL=info
APP_PUBLIC_DIR=front/dist
```

Copy `.env.example` to `.env` when you want `just` recipes to load local overrides automatically.

## Databases

SQLite is the default development database. No external service is required:

```bash
just dev-backend
```

Use PostgreSQL by setting `APP_DATABASE_URL`:

```bash
APP_DATABASE_URL=postgres://cyder_template:cyder_template_dev@127.0.0.1:5432/cyder_template just dev-backend
```

The service detects the backend from the URL and runs the matching embedded Diesel migrations at startup.

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

Compose builds the same `cyder-template:local` image, starts a local PostgreSQL service with a healthcheck, and points `APP_DATABASE_URL` at that service. The compose credentials in `.env.example` are local-development examples. Choose real credentials for shared or deployed environments.

## CI

The GitHub Actions workflow at `.github/workflows/ci.yml` runs:

- Rust formatting, check, and tests
- Frontend locked install, type checks, and build
- Docker compose config validation and Docker build smoke

Node should stay on the 24.x line across local development and CI, with 24.11 or newer as the minimum.

## License

Licensed under the MIT License. See `LICENSE`.
