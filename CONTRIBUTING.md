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
cargo check --workspace
cargo test --workspace
npm --prefix front ci
npm --prefix front test
npm --prefix front run build
docker compose -f docker-compose.yml config
docker build -t cyder-template:ci -f Dockerfile .
```

The shorter project shortcut is:

```bash
just check
```

`just check` covers Rust formatting, backend check/tests, frontend tests, and frontend build. Run the direct Docker commands when you change `Dockerfile`, `docker-compose.yml`, `.dockerignore`, runtime configuration, or release packaging.

## Pull Requests

Create a branch from the current `main` branch and keep the pull request focused on one change. Include a concise summary, the verification commands you ran, and any follow-up work that remains.

Before opening a pull request:

- Keep generated and local files out of the commit, including `front/node_modules/`, `front/dist/`, `target/`, `.app/`, `.env`, local databases, and logs.
- Do not commit real credentials, tokens, private endpoints, or machine-specific config.
- Keep the main README in English.
- Do not add product claims for features the template does not implement, such as authentication, authorization, teams, tenants, or production deployment automation.
- Update README and template rename guidance when changing `cyder-template`, `cyder_template`, Docker image names, database defaults, or CI tags.

## Code Style

Use `cargo fmt` for Rust formatting and the existing TypeScript/Vue toolchain for frontend checks. Prefer the project patterns already present in `server/`, `front/`, `Dockerfile`, and `.github/workflows/ci.yml`.

## Dependency Updates

Dependency update pull requests should include the relevant lockfile changes and should not bundle unrelated refactors. Before merging dependency updates, run the release validation set:

```bash
cargo fmt --manifest-path Cargo.toml --check
cargo check --manifest-path Cargo.toml --workspace
cargo test --manifest-path Cargo.toml --workspace
npm --prefix front ci
npm --prefix front test
npm --prefix front run build
npm --prefix front outdated --json
docker compose -f docker-compose.yml config
docker build -t cyder-template:ci -f Dockerfile .
```

The direct commands above match the publishing checklist. `npm --prefix front outdated --json` should print `{}` for a fully current frontend dependency set; otherwise record the difference in the pull request.
