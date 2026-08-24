# Contributing

<!-- template-init:start -->
This repository is a GitHub template for a Rust backend and Vue frontend. Contributions should improve the template itself: correctness, documentation, local developer workflow, CI, Docker, and generic example resources.
<!-- template-init:end -->

## Local Setup

Install the versions listed in `README.md`, then diagnose and prepare the checkout:

```bash
just doctor
just bootstrap
```

`just bootstrap` uses `npm ci` and creates the ignored `.app/dev/{config,db,storage,logs}` data layout. It does not create or load `.env`, install global tools, or start services. Use `just --list` to inspect the local command surface.

## Verification

Run the checks that match your change. For most code, dependency, CI, or Docker changes, run the full local path:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
node --test scripts/toolchain.test.mjs
npm --prefix front ci
npm --prefix front run format:check
npm --prefix front run lint
npm --prefix front run typecheck
npm --prefix front test
npm --prefix front run build
just audit
docker compose -f docker-compose.yml config
docker build -t cyder-template:ci -f Dockerfile .
bash scripts/test-container-config.sh cyder-template:ci
bash scripts/test-container-http.sh cyder-template:ci
bash scripts/test-container-e2e.sh cyder-template:ci
bash scripts/test-container-shutdown.sh cyder-template:ci
```

The shorter project shortcut is:

```bash
just check
```

`just check` covers toolchain/bootstrap contracts, Rust formatting, strict backend lints/tests, and the frontend formatting, strict lint, type, coverage-test, and build gates. `just audit` separately validates security-exception metadata, Rust advisories/licenses/sources/bans, and high-or-critical npm advisories; it requires cargo-deny 0.20.2 and network access. The checked-in GitHub Actions workflows mirror the direct backend, frontend, security, compose, and Docker build checks above. Run `just setup-e2e` once before the browser contract; it installs pinned Chromium but does not request administrator access for Linux host packages. Run the direct Docker commands when you change `Dockerfile`, `docker-compose.yml`, `.dockerignore`, runtime configuration, frontend navigation, or release packaging.

When database changes affect persistence, include the relevant backend coverage in the pull request notes. SQLite changes should usually include `cargo test --workspace`, which covers file-backed SQLite migrations, readiness, and CRUD. PostgreSQL behavior is covered by the opt-in integration test against an isolated test database:

```bash
DEV_POSTGRES_TEST_URL=postgres://cyder_template:cyder_template_dev@127.0.0.1:5432/cyder_template_test just test-postgres
```

Do not point `DEV_POSTGRES_TEST_URL` at production, shared, or long-lived data.

## Pull Requests

Create a branch from the current `main` branch and keep the pull request focused on one change. Include a concise summary, the verification commands you ran, and any follow-up work that remains.

Before opening a pull request:

- Keep generated and local files out of the commit, including `front/node_modules/`, `front/dist/`, `target/`, `.app/`, `.env`, local databases, and logs.
- Do not commit real credentials, tokens, private endpoints, or machine-specific config.
- Keep the main README in English.
- Do not add product claims for features the project does not implement, such as authentication, authorization, teams, tenants, or production deployment automation.
<!-- template-init:start -->
- Update README and initialization guidance when changing project identity fields, Docker image names, database defaults, or automation commands.
- Run `just test-template-init` when changing the one-time initializer, example-removal boundaries, or project identity touchpoints.
- Confirm the regular Backend, Frontend, Docker, and template frontend/E2E jobs pass when changing initialization behavior.
<!-- template-init:end -->

## Code Style

Use `cargo fmt` for Rust formatting. Use Prettier as the only frontend formatter and ESLint's zero-warning type-aware TypeScript/Vue rules for frontend linting. Tests should assert user-observable text, state, ARIA attributes, and service interactions instead of storing large HTML snapshots. Prefer the project patterns already present in `server/`, `front/`, `Dockerfile`, and `docker-compose.yml`.

## Dependency Updates

Dependency update pull requests should include the relevant lockfile changes and should not bundle unrelated refactors. Before merging dependency updates, run the release validation set:

```bash
cargo fmt --manifest-path Cargo.toml --check
cargo clippy --manifest-path Cargo.toml --workspace --all-targets --all-features --locked -- -D warnings
cargo test --manifest-path Cargo.toml --workspace --locked
npm --prefix front ci
npm --prefix front run format:check
npm --prefix front run lint
npm --prefix front run typecheck
npm --prefix front test
npm --prefix front run build
npm --prefix front outdated --json
just audit
docker compose -f docker-compose.yml config
docker build -t cyder-template:ci -f Dockerfile .
```

The direct commands above match the publishing checklist. In `npm --prefix front outdated --json`, every `current` version should match `wanted`; record intentionally deferred major-version upgrades when `latest` is newer.

### Security Exceptions

Fix or update a vulnerable dependency instead of suppressing its advisory. If a Rust advisory cannot be fixed immediately, add a single-line inline table to `[advisories].ignore` in `deny.toml`. Its `reason` must use this exact structure:

```toml
{ id = "RUSTSEC-YYYY-NNNN", reason = "owner=@github-user-or-org/team; expires=YYYY-MM-DD; justification=why the temporary risk is accepted" },
```

The owner must identify an accountable GitHub user or team, the justification must be meaningful, and the UTC expiry date must be in the future but no more than 90 days away. `just audit` and CI reject malformed, expired, overlong, duplicate, or unused exceptions. npm high/critical advisories do not have an ignore mechanism in this repository.
