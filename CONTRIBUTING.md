# Contributing

## Local Setup

Install the pinned tools and prepare the checkout:

```bash
just bootstrap
```

Use `just --list` to inspect the available development commands. The complete setup and daily workflow are documented in [Development](docs/development.md).

<!-- template-init:start -->
When changing project initialization, example-removal boundaries, or identity replacement, follow [Template maintenance](docs/template-maintenance.md).
<!-- template-init:end -->

## Making Changes

- Keep changes focused and follow the existing patterns in `server/`, `front/`, and the project configuration files.
- Update tests when behavior changes and update documentation when commands, configuration, or operational contracts change.
- Keep generated and local files out of commits, including `.app/`, `target/`, `front/node_modules/`, `front/dist/`, local databases, and logs.
- Never commit credentials, tokens, private endpoints, or machine-specific configuration.

## Verification

Run checks that match the files and behavior you changed. For the standard local verification path:

```bash
just check
```

Database, Docker, browser, security, and release-specific checks are documented in [Verification](docs/verification.md).

## Pull Requests

Create a focused branch and include:

- a concise summary of the change;
- the verification commands you ran;
- any migration, compatibility, security, or follow-up considerations.

Do not claim support for features the project does not implement, such as authentication, authorization, teams, tenants, or deployment automation.

## Code Style

Use `cargo fmt` for Rust. Prettier is the frontend formatter, and ESLint runs with type-aware rules and zero warnings. Prefer tests that assert observable behavior and service interactions over large snapshots.

Dependency updates must include the corresponding lockfile changes. Fix vulnerable dependencies instead of suppressing advisories; any temporary Rust advisory exception must follow the ownership and expiry rules in [Verification](docs/verification.md).
