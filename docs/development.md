# Development

## Requirements

The repository pins the primary toolchain versions:

- Rust 1.97.1 through rustup, with rustfmt and Clippy from `rust-toolchain.toml`
- Node.js 24.19.0 from `.node-version`
- npm 11.17.0, bundled with the pinned Node.js release
- just 1.58.0 or newer on the 1.x release line

Docker and its Compose plugin are optional for the default SQLite workflow. Diesel CLI is only needed when regenerating schema files. `cargo-deny` 0.20.2 is required for dependency policy checks.

## Prepare the Checkout

```bash
just bootstrap
```

`just bootstrap` runs environment diagnostics, installs locked frontend dependencies with `npm ci`, and creates `.app/dev/{config,db,storage,logs}`. It does not install global tools, create or load `.env`, or start services.

Use `just doctor` for diagnostics without preparing the checkout. Rust, Node.js, npm, just, required Rust components, and the backend port are blocking checks. Docker, Compose, their daemon, and optional development ports are warnings unless you use those paths.

## Run the Application

```bash
just dev
```

This starts the Rust backend and Vite frontend together. The command asks the Rust configuration loader for the resolved backend endpoint and injects its HTTP origin into Vite, so changes to `APP_PORT` or the selected YAML configuration remain aligned automatically.

The default backend endpoint is `127.0.0.1:8000`. Open the Vite URL printed in the terminal; `/api`, `/healthz`, and `/readyz` are proxied to the backend.

For focused work:

```bash
just dev-backend
just dev-front
```

Use the `just` development commands instead of invoking Vite directly. The frontend deliberately requires an explicit resolved proxy target. To develop against a remote or containerized backend, provide a root HTTP or HTTPS origin without credentials, query, or fragment:

```bash
DEV_PROXY_TARGET=https://dev-api.example.com just dev-front
```

## Command Reference

Run `just --list` for the complete current command surface.

| Command | Purpose |
| --- | --- |
| `just doctor` | Diagnose tools, optional container support, and local ports |
| `just bootstrap` | Install locked frontend dependencies and create local data paths |
| `just dev` | Run backend and frontend development servers |
| `just dev-backend` | Run only the Rust service |
| `just dev-front` | Run only Vite with a resolved backend target |
| `just install-front-deps` | Refresh `node_modules` when package files change |
| `just build` | Build backend and frontend release artifacts |
| `just test` | Run backend and frontend tests |
| `just check` | Run the standard local verification suite |
| `just audit` | Check dependency advisories and policies |
| `just docker-build` | Build the local application image |

Commands for specific quality gates, PostgreSQL, containers, and browser tests are documented in [Verification](verification.md).

## Frontend Commands

The root `justfile` is the normal interface. Lower-level frontend commands remain available when debugging a specific tool:

```bash
npm --prefix front ci
npm --prefix front run dev
npm --prefix front run format:check
npm --prefix front run lint
npm --prefix front run typecheck
npm --prefix front test
npm --prefix front run build
```

Use `npm install` only when intentionally changing dependencies, and commit the resulting `package-lock.json` update.

## Local Data

When `APP_DATA_DIR` is not set, source development uses `.app/dev`. SQLite is created at `.app/dev/db/cyder-template.sqlite`, while `config/`, `storage/`, and `logs/` share the same ignored data root. The service currently logs to stdout and stderr; `logs/` is reserved for application use.

The project does not parse `.env` files. Export development overrides in your shell or configure them in your process manager.
