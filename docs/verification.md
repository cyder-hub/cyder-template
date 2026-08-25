# Verification

Run focused checks while iterating, then choose the broader gates that match the change.

## Standard Checks

```bash
just test
just check
```

`just test` runs backend and frontend tests. `just check` additionally covers toolchain and bootstrap contracts, Rust formatting and strict Clippy, frontend formatting, linting, type checking, coverage tests, and the production frontend build.

Useful focused commands include:

```bash
just fmt-check
just lint-backend
just test-backend
just format-front-check
just lint-front
just typecheck-front
just test-front
just check-front
just check-config
```

## Change-Based Guidance

| Change | Minimum relevant verification |
| --- | --- |
| Rust behavior | `just fmt-check`, `just lint-backend`, `just test-backend` |
| Vue or TypeScript behavior | `just check-front` |
| Shared development tooling | `just test-toolchain`, `just check` |
| Configuration loading | `just check-config`, relevant backend tests |
| SQLite persistence | `just test-backend` |
| PostgreSQL persistence | `just test-postgres` with an isolated database |
| Docker or runtime paths | build the image and run the affected container contracts |
| Browser navigation | `just test-container-e2e` against a freshly built image |
| Dependency policy | `just audit` |

## Frontend Quality Gates

Vitest and Vue Test Utils run application tests in jsdom. V8 coverage includes pages, services, and Store code. The gate requires 80% global lines, statements, and functions; 75% global branches; and at least 60% for every included file.

Use the watch mode for iterative unit testing:

```bash
npm --prefix front run test:unit:watch
```

Playwright uses one pinned Chromium worker against the complete application image. Failure traces, screenshots, and video are written under `front/playwright-report` and `front/test-results`.

## PostgreSQL

Point `DEV_POSTGRES_TEST_URL` at an isolated disposable database:

```bash
DEV_POSTGRES_TEST_URL=postgres://cyder_template:cyder_template_dev@127.0.0.1:5432/cyder_template_test just test-postgres
```

The test covers migrations, pooled connections, and readiness. Never use production or shared data.

## Container Contracts

Build one image, then test the required runtime paths against that exact tag:

```bash
just setup-e2e
just docker-build
just test-container-config
just test-container-http
just test-container-e2e
just test-container-shutdown
```

- The configuration contract checks the runtime identity, data layout, and side-effect-free configuration commands.
- The HTTP contract checks health, readiness, API boundaries, static assets, cache behavior, and compression.
- The browser contract checks the primary dashboard path.
- The shutdown contract sends SIGTERM and verifies a clean bounded exit.

## Dependency Security

```bash
just audit
```

This requires `cargo-deny` 0.20.2 and registry access:

```bash
cargo install --locked cargo-deny --version 0.20.2
```

Security checks remain separate from `just check` so normal local verification does not install extra global tools or depend on advisory services. The command validates Rust advisories, licenses, sources, bans, exception metadata, and high-or-critical npm advisories.

Fix a vulnerable dependency instead of suppressing it. A temporary Rust advisory exception must be a single inline table in `[advisories].ignore` in `deny.toml`:

```toml
{ id = "RUSTSEC-YYYY-NNNN", reason = "owner=@github-user-or-org/team; expires=YYYY-MM-DD; justification=why the temporary risk is accepted" },
```

The owner must be accountable, the UTC expiry must be in the future and no more than 90 days away, and the justification must describe the accepted risk. Duplicate, expired, overlong, malformed, and unused exceptions fail validation. npm high and critical advisories have no ignore mechanism.

## Continuous Integration

`.github/workflows/ci.yml` runs on pull requests, pushes to `main` or `master`, and manual dispatch. Its backend, frontend, and Docker jobs mirror the direct local checks with pinned Rust, Node.js, npm, Actions, and Chromium versions.

The security workflow runs dependency policies for pull requests, default-branch pushes, and its scheduled cadence. Keep toolchain metadata, Docker build arguments, package metadata, and workflow versions aligned; the toolchain contract rejects drift.
