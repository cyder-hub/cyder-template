# Contributing

This repository is a GitHub template for a Rust backend and Vue frontend. Contributions should improve the template itself: correctness, documentation, local developer workflow, CI, Docker, and generic example resources.

## Local Setup

Install the versions listed in `README.md`, then install frontend dependencies:

```bash
npm --prefix front ci
```

Use `just --list` to inspect the local command surface.

## Verification

Run the checks that match your change. For most code, dependency, CI, or Docker changes, run the full local path:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
npm --prefix front ci
npm --prefix front test
npm --prefix front run build
just audit
docker compose -f docker-compose.yml config
docker build -t cyder-template:ci -f Dockerfile .
```

The shorter project shortcut is:

```bash
just check
```

`just check` covers Rust formatting, strict backend lints/tests, frontend tests, and frontend build. `just audit` separately validates security-exception metadata, Rust advisories/licenses/sources/bans, and high-or-critical npm advisories; it requires cargo-deny 0.20.2 and network access. The checked-in GitHub Actions workflows mirror the direct backend, frontend, security, compose, and Docker build checks above. Run the direct Docker commands when you change `Dockerfile`, `docker-compose.yml`, `.dockerignore`, runtime configuration, or release packaging.

When database changes affect persistence, include the relevant backend coverage in the pull request notes. SQLite changes should usually include `cargo test --workspace`, which covers file-backed SQLite migrations, readiness, and CRUD. PostgreSQL behavior is covered by the opt-in integration test against an isolated test database:

```bash
APP_TEST_POSTGRES_URL=postgres://cyder_template:cyder_template_dev@127.0.0.1:5432/cyder_template_test just test-postgres
```

Do not point `APP_TEST_POSTGRES_URL` at production, shared, or long-lived data.

## Pull Requests

Create a branch from the current `main` branch and keep the pull request focused on one change. Include a concise summary, the verification commands you ran, and any follow-up work that remains.

Before opening a pull request:

- Keep generated and local files out of the commit, including `front/node_modules/`, `front/dist/`, `target/`, `.app/`, `.env`, local databases, and logs.
- Do not commit real credentials, tokens, private endpoints, or machine-specific config.
- Keep the main README in English.
- Do not add product claims for features the template does not implement, such as authentication, authorization, teams, tenants, or production deployment automation.
- Update README and template rename guidance when changing `cyder-template`, `cyder_template`, Docker image names, database defaults, or automation commands.

## Code Style

Use `cargo fmt` for Rust formatting and the existing TypeScript/Vue toolchain for frontend checks. Prefer the project patterns already present in `server/`, `front/`, `Dockerfile`, and `docker-compose.yml`.

## Dependency Updates

Dependency update pull requests should include the relevant lockfile changes and should not bundle unrelated refactors. Before merging dependency updates, run the release validation set:

```bash
cargo fmt --manifest-path Cargo.toml --check
cargo clippy --manifest-path Cargo.toml --workspace --all-targets --all-features --locked -- -D warnings
cargo test --manifest-path Cargo.toml --workspace --locked
npm --prefix front ci
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

The owner must identify an accountable GitHub user or team, the justification must be meaningful, and the UTC expiry date must be in the future but no more than 90 days away. `just audit` and CI reject malformed, expired, overlong, duplicate, or unused exceptions. npm high/critical advisories do not have an ignore mechanism in this template.
