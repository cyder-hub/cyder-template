# AGENTS.md

## Project

This repository contains a Rust backend in `server/` and a Vue 3 frontend in `front/`. The backend owns application configuration, HTTP behavior, persistence, migrations, and static asset serving. The frontend owns browser routes, pages, client state, and API calls.

## Working in the Repository

- Use the root `justfile` as the primary command interface.
- Run `just bootstrap` before the first local development session.
- Run `just dev` for the normal backend and frontend development loop.
- Use `just --list` before invoking lower-level commands directly.
- Keep local data and generated artifacts under ignored paths such as `.app/`, `target/`, `front/node_modules/`, and `front/dist/`.

## Change Boundaries

- Keep HTTP routing and shared application state in `server/src/app.rs`.
- Keep request parsing and response DTOs in `server/src/controller/`, business logic in `server/src/service/`, and persistence in `server/src/database/`.
- Update both SQLite and PostgreSQL migrations and schema modules when changing persistent data.
- Keep database IDs as `i64` in Rust and serialize them as strings at JSON boundaries.
- Keep frontend HTTP behavior in `front/src/services/` and page-level behavior in `front/src/pages/`.
- Preserve the same-origin production model: the Rust service serves the built frontend and API together.
- Add configuration only through the established loader and document user-facing settings in `docs/configuration.md`.

## Verification

- Run focused tests while iterating.
- Run `just test` for behavior changes.
- Run `just check` before handing off changes that affect application code, dependencies, build configuration, or shared tooling.
- Follow `docs/verification.md` for PostgreSQL, Docker, browser, security, and release-specific checks.

Keep code, tests, and documentation consistent. Do not commit secrets, local databases, generated assets, or machine-specific configuration.
